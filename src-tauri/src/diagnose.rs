//! Runs one complete diagnostic pass. Probes run concurrently on scoped threads,
//! each with its own hard time bound, so a pass always terminates.

use crate::cmd::{self, SystemRunner};
use crate::models::*;
use crate::powersave::{self, valid_ifname, BackupStore};
use crate::probe::{self, dns_benchmark, ping, tcp_probe};
use crate::{detect, rules};
use serde_json::Value;
use std::net::IpAddr;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const INTERNET_V4: &[(&str, &str)] = &[
    ("1.1.1.1", "Cloudflare"),
    ("8.8.8.8", "Google"),
    ("9.9.9.9", "Quad9"),
];
const INTERNET_V6: &[(&str, &str)] = &[
    ("2606:4700:4700::1111", "Cloudflare"),
    ("2001:4860:4860::8888", "Google"),
    ("2620:fe::fe", "Quad9"),
];

pub fn parse_neighbor_state(json: &str) -> Option<String> {
    let v: Value = serde_json::from_str(json).ok()?;
    let first = v.as_array()?.first()?;
    match first.get("state")? {
        Value::Array(a) => a.first()?.as_str().map(str::to_string),
        Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

fn neighbor_state(gw: &str, ifname: &str) -> Option<String> {
    if gw.parse::<IpAddr>().is_err() || !valid_ifname(ifname) {
        return None;
    }
    let o = cmd::run(
        "ip",
        &["-j", "neigh", "show", gw, "dev", ifname],
        Duration::from_secs(3),
    )
    .ok()?;
    parse_neighbor_state(&o.stdout)
}

fn resolvers(iface: &NetInterface, ipv4: bool) -> Vec<(IpAddr, String, ResolverRole)> {
    let mut out: Vec<(IpAddr, String, ResolverRole)> = Vec::new();
    for s in &iface.dns_servers {
        // Link-local IPv6 resolvers need a scope id; skip rather than guess.
        if let Ok(ip) = s.parse::<IpAddr>() {
            let link_local = matches!(ip, IpAddr::V6(v6) if (v6.segments()[0] & 0xffc0) == 0xfe80);
            if !link_local && !out.iter().any(|(i, _, _)| *i == ip) {
                out.push((ip, "Network DNS".into(), ResolverRole::Configured));
            }
        }
    }
    if detect::uses_resolved_stub() {
        out.push((
            "127.0.0.53".parse().unwrap(),
            "System resolver".into(),
            ResolverRole::SystemStub,
        ));
    }
    let publics = if ipv4 { INTERNET_V4 } else { INTERNET_V6 };
    for (ip, name) in publics {
        let ip: IpAddr = ip.parse().unwrap();
        if !out.iter().any(|(i, _, _)| *i == ip) {
            out.push((ip, name.to_string(), ResolverRole::Public));
        }
    }
    out
}

pub fn run(
    interface: &str,
    config: DiagnosticConfig,
    store: &BackupStore,
) -> Result<DiagnosticReport, String> {
    let config = config.clamped();
    let t0 = Instant::now();
    let started_at_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let ifaces = detect::list_interfaces()?;
    let iface = ifaces
        .into_iter()
        .find(|i| i.name == interface)
        .ok_or_else(|| format!("Interface {interface} is no longer present."))?;

    let mut notes = Vec::new();
    let power_save =
        (iface.kind == InterfaceKind::Wifi).then(|| powersave::state(&iface, &SystemRunner, store));

    let mut report = DiagnosticReport {
        started_at_unix,
        duration_ms: 0,
        config,
        gateway: None,
        gateway_neighbor: None,
        internet: Vec::new(),
        dns: Vec::new(),
        dns_over_tls: false,
        power_save,
        notes: Vec::new(),
        analysis: Analysis {
            overall: OverallState::Healthy,
            summary: String::new(),
            findings: Vec::new(),
            recommendations: Vec::new(),
        },
        interface: iface,
    };

    if report.interface.connected {
        let iface = &report.interface;
        if !iface.is_default_route {
            notes.push(format!(
                "{} is not the default route: Internet and DNS tests follow the system's default route.",
                iface.name
            ));
        }
        let ipv4 = !iface.ipv4.is_empty();
        let targets = if ipv4 { INTERNET_V4 } else { INTERNET_V6 };
        let resolvers = resolvers(iface, ipv4);
        let (n, tmo) = (config.samples, config.timeout_ms);
        let gw_addr = iface
            .gateway
            .clone()
            .filter(|g| g.parse::<IpAddr>().is_ok());
        if iface.gateway.is_some() && gw_addr.is_none() {
            notes.push(
                "The gateway address is IPv6 link-local and cannot be probed directly.".into(),
            );
        }

        let (gateway, internet, dns, dot) = thread::scope(|s| {
            let gw = gw_addr
                .as_deref()
                .map(|g| s.spawn(move || ping(g, "Router", TargetRole::Gateway, n, tmo)));
            let inet: Vec<_> = targets
                .iter()
                .map(|(ip, name)| s.spawn(move || ping(ip, name, TargetRole::Internet, n, tmo)))
                .collect();
            let dns: Vec<_> = resolvers
                .iter()
                .map(|(ip, label, role)| s.spawn(move || dns_benchmark(*ip, label, *role, tmo)))
                .collect();
            let dot = s.spawn(detect::dns_over_tls_enabled);
            (
                gw.map(|h| h.join().expect("gateway probe panicked")),
                inet.into_iter()
                    .map(|h| h.join().expect("probe panicked"))
                    .collect::<Vec<_>>(),
                dns.into_iter()
                    .map(|h| h.join().expect("dns probe panicked"))
                    .collect::<Vec<_>>(),
                dot.join().unwrap_or(false),
            )
        });

        let mut internet = internet;
        let all_silent = internet
            .iter()
            .all(|s| s.error.is_none() && s.received == 0);
        if all_silent {
            notes.push("No ping replies from Internet servers; measured TCP handshakes to port 443 instead.".into());
            let attempts = n.min(5);
            let tcp: Vec<LatencyStats> = thread::scope(|s| {
                let hs: Vec<_> = targets
                    .iter()
                    .map(|(ip, name)| {
                        s.spawn(move || {
                            tcp_probe(ip, name, TargetRole::Internet, 443, attempts, tmo)
                        })
                    })
                    .collect();
                hs.into_iter()
                    .map(|h| h.join().expect("tcp probe panicked"))
                    .collect()
            });
            internet.extend(tcp);
        }
        if let Some(g) = &gateway {
            if g.error.is_none() && g.received == 0 {
                report.gateway_neighbor = neighbor_state(&g.target, &report.interface.name);
            }
        }
        for s in gateway.iter().chain(internet.iter()) {
            if let Some(e) = &s.error {
                notes.push(format!("{} ({}): {e}", s.label, s.target));
            }
        }
        if dot {
            notes.push("DNS-over-TLS is enabled: direct DNS timings may not match the path applications use.".into());
        }
        report.gateway = gateway;
        report.internet = internet;
        report.dns = dns;
        report.dns_over_tls = dot;
    }

    report.notes = notes;
    report.analysis = rules::analyze(&report);
    report.duration_ms = t0.elapsed().as_millis() as u64;
    Ok(report)
}

/// Rough upper bound of a run's duration, shown in the UI before starting.
pub fn estimated_seconds(config: DiagnosticConfig) -> u64 {
    let c = config.clamped();
    let ping = c.samples as f64 * probe::PING_INTERVAL_S + c.timeout_ms as f64 / 1000.0 + 1.0;
    let dns = probe::DNS_NAMES.len() as f64 * c.timeout_ms as f64 / 1000.0;
    ping.max(dns).ceil() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neighbor_state_parsing() {
        assert_eq!(
            parse_neighbor_state(
                r#"[{"dst":"172.20.10.1","dev":"wlo1","lladdr":"aa:bb","state":["REACHABLE"]}]"#
            )
            .as_deref(),
            Some("REACHABLE")
        );
        assert_eq!(parse_neighbor_state("[]"), None);
        assert_eq!(parse_neighbor_state(""), None);
    }

    #[test]
    fn estimate_is_bounded() {
        assert!(
            estimated_seconds(DiagnosticConfig {
                samples: 50,
                timeout_ms: 5000
            }) <= 31
        );
        assert!(estimated_seconds(DiagnosticConfig::default()) >= 5);
    }
}
