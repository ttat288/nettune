//! Deterministic diagnosis rules. Pure functions over a measured report — no I/O.
//!
//! Thresholds are deliberately conservative and documented next to each rule.
//! Findings describe what was observed; they never claim a confirmed root cause.

use crate::models::*;

// Local link (router) thresholds.
const GW_LOSS_WARN: f64 = 5.0;
const GW_LOSS_CRIT: f64 = 20.0;
const GW_MEDIAN_WIFI_MS: f64 = 20.0;
const GW_MEDIAN_WIRED_MS: f64 = 5.0;
const GW_JITTER_WIFI_MS: f64 = 15.0;
const GW_JITTER_WIRED_MS: f64 = 5.0;
/// A spike is a sample this far above the median (and at least SPIKE_MIN_MS above it).
const SPIKE_FACTOR: f64 = 4.0;
const SPIKE_MIN_MS: f64 = 50.0;
// Internet path thresholds.
const NET_LOSS_WARN: f64 = 5.0;
const NET_JITTER_MS: f64 = 30.0;
const NET_MEDIAN_MS: f64 = 150.0;
// DNS thresholds.
const DNS_SLOW_MS: f64 = 150.0;
const DNS_FASTER_RATIO: f64 = 0.6;
const DNS_FASTER_MIN_MS: f64 = 20.0;
// Wi-Fi signal (dBm).
const WIFI_WEAK_DBM: i32 = -75;
const WIFI_VERY_WEAK_DBM: i32 = -82;
const WIFI_FAIR_DBM: i32 = -67;

fn ms(v: f64) -> String {
    if v < 10.0 {
        format!("{v:.1} ms")
    } else {
        format!("{v:.0} ms")
    }
}

fn sample_confidence(sent: u32, base: Confidence) -> Confidence {
    if sent < 15 {
        Confidence::Low
    } else {
        base
    }
}

fn reachable(s: &LatencyStats) -> bool {
    s.error.is_none() && s.received > 0
}

fn has_spikes(s: &LatencyStats) -> bool {
    match (s.median_ms, s.max_ms) {
        (Some(med), Some(max)) => max > med * SPIKE_FACTOR && max - med >= SPIKE_MIN_MS,
        _ => false,
    }
}

struct Ctx<'a> {
    r: &'a DiagnosticReport,
    findings: Vec<Finding>,
    recs: Vec<Recommendation>,
}

impl Ctx<'_> {
    #[allow(clippy::too_many_arguments)]
    fn find(
        &mut self,
        id: &str,
        severity: Severity,
        title: impl Into<String>,
        evidence: Vec<String>,
        explanation: impl Into<String>,
        confidence: Confidence,
        limitations: impl Into<String>,
        next_action: impl Into<String>,
    ) {
        self.findings.push(Finding {
            id: id.into(),
            severity,
            title: title.into(),
            evidence,
            explanation: explanation.into(),
            confidence,
            limitations: limitations.into(),
            next_action: next_action.into(),
        });
    }

    fn has(&self, id: &str) -> bool {
        self.findings.iter().any(|f| f.id == id)
    }

    fn ids(&self, ids: &[&str]) -> Vec<String> {
        ids.iter()
            .filter(|i| self.has(i))
            .map(|i| i.to_string())
            .collect()
    }

    fn manual(
        &mut self,
        id: &str,
        title: &str,
        rationale: &str,
        related: &[&str],
        steps: Vec<String>,
    ) {
        let related_findings = self.ids(related);
        self.recs.push(Recommendation {
            id: id.into(),
            title: title.into(),
            rationale: rationale.into(),
            related_findings,
            action: RecommendationAction::Manual { steps },
        });
    }
}

pub fn analyze(r: &DiagnosticReport) -> Analysis {
    let mut c = Ctx {
        r,
        findings: Vec::new(),
        recs: Vec::new(),
    };
    let iface = &r.interface;
    let wifi = iface.kind == InterfaceKind::Wifi;

    if !iface.connected {
        c.find(
            "not-connected",
            Severity::Critical,
            format!("{} is not connected", iface.name),
            vec![
                format!("State: {}", iface.state),
                format!(
                    "Addresses: {}",
                    if iface.ipv4.is_empty() && iface.ipv6.is_empty() {
                        "none".to_string()
                    } else {
                        "present".to_string()
                    }
                ),
            ],
            "Without an active link and an IP address no network tests can run on this interface.",
            Confidence::High,
            "Only this interface was checked.",
            if wifi {
                "Connect to a Wi-Fi network, then run diagnostics again."
            } else {
                "Check the cable and that the network is enabled, then run diagnostics again."
            },
        );
        c.manual(
            "connect",
            "Bring the interface up",
            "The interface has no active connection.",
            &["not-connected"],
            vec![
                format!("nmcli device status   # check the state of {}", iface.name),
                format!("nmcli device connect {}", iface.name),
            ],
        );
        return finish(c);
    }

    gateway_rules(&mut c);
    internet_rules(&mut c);
    dns_rules(&mut c);
    if wifi {
        wifi_rules(&mut c);
    }
    if iface.kind == InterfaceKind::Ethernet {
        ethernet_rules(&mut c);
    }
    finish(c)
}

fn gateway_rules(c: &mut Ctx) {
    gateway_findings(c);
    if c.has("gateway-loss")
        || c.has("gateway-latency")
        || c.has("gateway-jitter")
        || c.has("gateway-unreachable")
    {
        c.manual(
            "router-check",
            "Check the router and local network",
            "The problem was measured between this computer and the router, so the Internet provider is less likely to be the cause.",
            &["gateway-loss", "gateway-latency", "gateway-jitter", "gateway-unreachable"],
            vec![
                "Check whether other devices on the same network show the same problem.".into(),
                "Restart the router and run diagnostics again.".into(),
                "If possible, test with a wired connection to separate Wi-Fi issues from router issues.".into(),
            ],
        );
    }
}

fn gateway_findings(c: &mut Ctx) {
    let r = c.r;
    let iface = &r.interface;
    let wifi = iface.kind == InterfaceKind::Wifi;
    let internet_ok = r.internet.iter().any(reachable);

    let Some(gw) = &r.gateway else {
        if iface.is_default_route {
            c.find(
                "no-gateway",
                Severity::Critical,
                "No default gateway",
                vec![format!("{} has no gateway in the routing table", iface.name)],
                "Traffic to the Internet needs a gateway (usually your router). DHCP may have failed or the network provides only local access.",
                Confidence::High,
                "Static configurations without a gateway are sometimes intentional.",
                "Reconnect to the network; if it persists, check the router's DHCP service.",
            );
        } else {
            c.find(
                "not-default-route",
                Severity::Info,
                "This interface does not carry Internet traffic",
                vec![format!("{} has no default route; another interface is used", iface.name)],
                "Internet and DNS tests use the system's default route, so they do not describe this interface.",
                Confidence::High,
                "",
                "Select the interface marked as default route for Internet diagnostics.",
            );
        }
        return;
    };

    if let Some(err) = &gw.error {
        c.find(
            "gateway-probe-failed",
            Severity::Info,
            "Router latency could not be measured",
            vec![format!("ping {}: {err}", gw.target)],
            "The measurement tool failed, so nothing is known about the local link from this run.",
            Confidence::High,
            "This is a tool or permission problem, not a network measurement.",
            "Ensure the `ping` utility (iputils-ping) is installed and usable without root.",
        );
        return;
    }

    let neigh = r
        .gateway_neighbor
        .as_deref()
        .map(|s| format!("Router neighbor (ARP/NDP) state: {s}"));
    if gw.received == 0 {
        let mut ev = vec![format!("0 of {} echo replies from {}", gw.sent, gw.target)];
        ev.extend(neigh.clone());
        if internet_ok {
            c.find(
                "gateway-icmp-filtered",
                Severity::Info,
                "Router does not answer ping",
                ev,
                "Internet servers replied, so traffic passes through the router; it most likely ignores ping requests. Local latency cannot be measured on this network.",
                Confidence::Medium,
                "Some routers and hotspots deliberately drop ICMP.",
                "Nothing to fix; rely on the Internet measurements.",
            );
        } else {
            let l2_dead = matches!(
                r.gateway_neighbor.as_deref(),
                Some("FAILED") | Some("INCOMPLETE") | None
            );
            c.find(
                "gateway-unreachable",
                Severity::Critical,
                "Router is unreachable",
                ev,
                "Neither the router nor any Internet server replied. The problem is most likely between this computer and the router (link, Wi-Fi association, or the router itself).",
                if l2_dead { Confidence::High } else { Confidence::Medium },
                "A router that drops ping combined with a separate upstream outage would look the same.",
                "Check other devices on the same network; restart the router if they are affected too.",
            );
        }
        return;
    }

    let loss = gw.loss_pct.unwrap_or(0.0);
    if loss >= GW_LOSS_WARN {
        c.find(
            "gateway-loss",
            if loss >= GW_LOSS_CRIT { Severity::Critical } else { Severity::Warning },
            "Packet loss on the local link",
            vec![format!(
                "{:.0}% loss to the router ({} of {} replies)",
                loss, gw.received, gw.sent
            )],
            if wifi {
                "Packets are being lost before they leave your local network. On Wi-Fi this usually points to weak signal, interference or a congested channel."
            } else {
                "Packets are being lost before they leave your local network, which points to the cable, port, adapter or router."
            },
            sample_confidence(gw.sent, Confidence::Medium),
            "Routers may rate-limit ping replies under load, which inflates loss.",
            "Repeat the test; if loss persists, follow the router and signal recommendations.",
        );
    }

    let (med_t, jit_t) = if wifi {
        (GW_MEDIAN_WIFI_MS, GW_JITTER_WIFI_MS)
    } else {
        (GW_MEDIAN_WIRED_MS, GW_JITTER_WIRED_MS)
    };
    if let Some(med) = gw.median_ms {
        if med > med_t {
            c.find(
                "gateway-latency",
                Severity::Warning,
                "High latency to the router",
                vec![format!("Median {} to the router (expected under {})", ms(med), ms(med_t))],
                "Latency inside your own network should be small. High values add delay to everything you do online.",
                sample_confidence(gw.sent, Confidence::Medium),
                "Some routers answer ping slowly by design (low priority for ICMP); a phone hotspot is usually slower.",
                "Compare with another device; check signal strength and router load.",
            );
        }
    }
    let jit = gw.jitter_ms.unwrap_or(0.0);
    let spikes = has_spikes(gw);
    if jit > jit_t || spikes {
        let mut ev = Vec::new();
        if let Some(j) = gw.jitter_ms {
            ev.push(format!(
                "Jitter {} to the router (threshold {})",
                ms(j),
                ms(jit_t)
            ));
        }
        if let (true, Some(med), Some(max)) = (spikes, gw.median_ms, gw.max_ms) {
            ev.push(format!("Spikes: max {} vs median {}", ms(max), ms(med)));
        }
        c.find(
            "gateway-jitter",
            Severity::Warning,
            "Unstable latency on the local link",
            ev,
            "Round-trip times to the router vary a lot. This is felt as occasional lag in calls and games even when average latency looks fine.",
            sample_confidence(gw.sent, Confidence::Medium),
            "A short test captures a moment; background traffic on this computer also causes variation.",
            if wifi {
                "Check Wi-Fi power saving and signal recommendations below."
            } else {
                "Check for other heavy traffic on the network and the router's load."
            },
        );
    }
}

fn internet_rules(c: &mut Ctx) {
    internet_findings(c);
    let gw = c.r.gateway.as_ref().filter(|g| reachable(g));
    let gw_loss_clean = gw
        .and_then(|g| g.loss_pct)
        .is_some_and(|l| l < GW_LOSS_WARN);
    let gw_jit_high = gw
        .and_then(|g| g.jitter_ms)
        .is_some_and(|j| j > NET_JITTER_MS / 2.0);
    if (c.has("internet-loss") && gw_loss_clean)
        || (c.has("internet-jitter") && !gw_jit_high)
        || c.has("internet-latency")
        || c.has("internet-unreachable")
    {
        c.manual(
            "isp-path",
            "Investigate the connection beyond your router",
            "Measurements point past the local network, which NetTune cannot change.",
            &[
                "internet-loss",
                "internet-jitter",
                "internet-latency",
                "internet-unreachable",
            ],
            vec![
                "Run diagnostics again at a different time of day to see whether it is congestion."
                    .into(),
                "Check the router's status page for WAN errors or reconnects.".into(),
                "If the problem persists, share these measurements with your provider.".into(),
            ],
        );
    }
}

fn internet_findings(c: &mut Ctx) {
    let r = c.r;
    let icmp: Vec<&LatencyStats> = r
        .internet
        .iter()
        .filter(|s| s.method == ProbeMethod::Icmp)
        .collect();
    let tcp: Vec<&LatencyStats> = r
        .internet
        .iter()
        .filter(|s| s.method == ProbeMethod::Tcp)
        .collect();
    if icmp.is_empty() || c.has("gateway-unreachable") || c.has("no-gateway") {
        return;
    }
    if icmp.iter().all(|s| s.error.is_some()) {
        c.find(
            "internet-probe-failed",
            Severity::Info,
            "Internet latency could not be measured",
            icmp.iter()
                .filter_map(|s| s.error.as_ref().map(|e| format!("{}: {e}", s.target)))
                .collect(),
            "The measurement tool failed for every target.",
            Confidence::High,
            "This is a tool or permission problem, not a network measurement.",
            "Ensure `ping` (iputils-ping) is installed.",
        );
        return;
    }
    let gw_ok = r.gateway.as_ref().is_some_and(reachable) || c.has("gateway-icmp-filtered");
    let icmp_ok: Vec<&LatencyStats> = icmp.iter().copied().filter(|s| reachable(s)).collect();

    if icmp_ok.is_empty() {
        let tcp_ok: Vec<&&LatencyStats> = tcp.iter().filter(|s| reachable(s)).collect();
        if !tcp_ok.is_empty() {
            c.find(
                "icmp-blocked",
                Severity::Info,
                "Ping is blocked beyond the router",
                vec![
                    format!("0 ping replies from {} test servers", icmp.len()),
                    format!("{} of {} servers accepted TCP connections", tcp_ok.len(), tcp.len()),
                ],
                "The network filters ping but normal connections work. Latency below is measured with TCP handshakes, which run slightly slower than ping.",
                Confidence::Medium,
                "TCP timing includes connection setup and is not directly comparable to ping.",
                "Nothing to fix unless applications also fail.",
            );
        } else {
            c.find(
                "internet-unreachable",
                Severity::Critical,
                "No connectivity beyond the router",
                vec![
                    format!("0 replies from {} Internet test servers (ping)", icmp.len()),
                    if tcp.is_empty() {
                        "TCP fallback not run".to_string()
                    } else {
                        format!("0 of {} TCP connection attempts succeeded", tcp.len())
                    },
                    if gw_ok {
                        "Router answered".to_string()
                    } else {
                        "Router reachability unknown".to_string()
                    },
                ],
                "Your local network works but traffic does not get through to the Internet. The router's upstream link, the provider, or a captive portal is the likely cause.",
                if gw_ok { Confidence::High } else { Confidence::Medium },
                "Strict firewalls that block both ping and outbound port 443 would look the same.",
                "Open a web page to check for a captive portal (hotel/café login); otherwise check the router's WAN status or contact your provider.",
            );
        }
        return;
    }

    if icmp_ok.len() < icmp.len() {
        c.find(
            "partial-targets",
            Severity::Info,
            "Some test servers did not reply",
            icmp.iter()
                .filter(|s| !reachable(s))
                .map(|s| format!("No replies from {} ({})", s.label, s.target))
                .collect(),
            "Other servers replied, so general connectivity works. A specific route or server is filtered or unreachable.",
            Confidence::Medium,
            "Public servers can rate-limit ping.",
            "No action needed unless specific services fail.",
        );
    }

    // Use the best-performing reachable target to describe the path (least pessimistic).
    let best = icmp_ok
        .iter()
        .min_by(|a, b| {
            a.median_ms
                .unwrap_or(f64::MAX)
                .total_cmp(&b.median_ms.unwrap_or(f64::MAX))
        })
        .copied()
        .expect("non-empty");
    let min_loss = icmp_ok
        .iter()
        .filter_map(|s| s.loss_pct)
        .fold(f64::MAX, f64::min);
    let min_jit = icmp_ok
        .iter()
        .filter_map(|s| s.jitter_ms)
        .fold(f64::MAX, f64::min);
    let gw = r.gateway.as_ref().filter(|g| reachable(g));
    let gw_loss = gw.and_then(|g| g.loss_pct);
    let gw_jit = gw.and_then(|g| g.jitter_ms);

    if min_loss != f64::MAX && min_loss >= NET_LOSS_WARN {
        let local_clean = gw_loss.is_some_and(|l| l < GW_LOSS_WARN);
        let mut ev = vec![format!(
            "At least {:.0}% loss to every reachable Internet server",
            min_loss
        )];
        if let Some(l) = gw_loss {
            ev.push(format!("{:.0}% loss to the router", l));
        }
        c.find(
            "internet-loss",
            Severity::Warning,
            if local_clean {
                "Packet loss beyond your local network"
            } else {
                "Packet loss on the Internet path"
            },
            ev,
            if local_clean {
                "The router path is clean but packets to the Internet are lost, which points to the router's upstream link or the provider."
            } else {
                "Loss to Internet servers may simply be the local loss seen above; fix local issues first."
            },
            sample_confidence(best.sent, if local_clean { Confidence::Medium } else { Confidence::Low }),
            "Public servers may deprioritise ping; a short test can catch a transient burst.",
            "Repeat at different times. If it persists with a clean local link, contact your provider with these results.",
        );
    }

    if min_jit != f64::MAX && min_jit > NET_JITTER_MS {
        let local = gw_jit.is_some_and(|j| j > NET_JITTER_MS / 2.0);
        let mut ev = vec![format!(
            "Jitter of at least {} on every reachable Internet server",
            ms(min_jit)
        )];
        if let Some(j) = gw_jit {
            ev.push(format!("Jitter to the router: {}", ms(j)));
        }
        c.find(
            "internet-jitter",
            Severity::Warning,
            "High jitter on the Internet path",
            ev,
            if local {
                "Much of the variation is already present on the local link, so it likely originates there."
            } else {
                "The local link is steady, so the variation is added further upstream (router uplink or provider)."
            },
            sample_confidence(best.sent, Confidence::Medium),
            "Jitter changes with load; one run is a snapshot.",
            "Repeat while the network is idle to see whether other traffic causes it.",
        );
    }

    if let Some(med) = best.median_ms {
        if med > NET_MEDIAN_MS {
            c.find(
                "internet-latency",
                Severity::Warning,
                "High latency to Internet servers",
                vec![format!("Best median {} ({})", ms(med), best.label)],
                "Even the closest test server is far in network terms. This makes interactive applications feel slow.",
                Confidence::Medium,
                "Cellular, satellite and remote locations have inherently higher latency; this may be normal for your connection type.",
                "Compare with the latency your provider advertises.",
            );
        }
    }
}

fn dns_rules(c: &mut Ctx) {
    let r = c.r;
    if r.dns.is_empty() || c.has("gateway-unreachable") || c.has("internet-unreachable") {
        return;
    }
    let configured: Vec<&DnsResult> = r
        .dns
        .iter()
        .filter(|d| d.role == ResolverRole::Configured)
        .collect();
    let stub = r.dns.iter().find(|d| d.role == ResolverRole::SystemStub);
    let publics: Vec<&DnsResult> = r
        .dns
        .iter()
        .filter(|d| d.role == ResolverRole::Public)
        .collect();
    let dot_note = if r.dns_over_tls {
        " This system uses DNS-over-TLS, so applications may not use the configured servers directly."
    } else {
        ""
    };

    // "Primary" = what the system actually uses: configured upstream if measurable, else the stub.
    let all_failed = |d: &&DnsResult| d.median_ms.is_none();
    if !configured.is_empty() && configured.iter().all(all_failed) {
        let public_ok = publics.iter().any(|p| p.median_ms.is_some());
        let stub_ok = stub.is_some_and(|s| s.median_ms.is_some());
        c.find(
            "dns-configured-down",
            if stub_ok { Severity::Warning } else { Severity::Critical },
            "Configured DNS server does not respond",
            configured
                .iter()
                .map(|d| format!("{}: {} of {} queries failed", d.resolver, d.failures, d.samples.len()))
                .chain(stub.map(|s| {
                    format!(
                        "System resolver (127.0.0.53): {}",
                        if s.median_ms.is_some() { "answers" } else { "fails" }
                    )
                }))
                .collect(),
            if public_ok {
                "Public resolvers answered, so the Internet works but the DNS server handed out by your network does not reply to direct queries."
            } else {
                "No DNS server replied to direct queries."
            },
            Confidence::Medium,
            format!("Some networks block direct DNS to the router while still serving the system resolver.{dot_note}"),
            "If websites fail to load by name, use a different DNS server (see recommendations).",
        );
    }

    let primary = configured
        .iter()
        .filter(|d| d.median_ms.is_some())
        .min_by(|a, b| a.median_ms.unwrap().total_cmp(&b.median_ms.unwrap()))
        .copied()
        .or(stub.filter(|s| s.median_ms.is_some()));

    if let Some(p) = primary {
        let med = p.median_ms.unwrap_or_default();
        if med > DNS_SLOW_MS {
            c.find(
                "dns-slow",
                Severity::Warning,
                "Slow DNS responses",
                vec![format!("Median {} from {} ({})", ms(med), p.label, p.resolver)],
                "Every new connection waits for a name lookup first, so slow DNS makes pages and apps feel slow to start.",
                Confidence::Medium,
                format!("Results mix cached and uncached names; 6 queries per resolver.{dot_note}"),
                "Compare against public resolvers below.",
            );
        }
        if p.failures > 0 && (p.failures as usize) < p.samples.len() {
            c.find(
                "dns-intermittent",
                Severity::Warning,
                "Some DNS queries failed",
                vec![format!(
                    "{} of {} queries to {} failed",
                    p.failures,
                    p.samples.len(),
                    p.resolver
                )],
                "Occasional lookup failures cause pages or apps to hang briefly before retrying.",
                Confidence::Low,
                "A single timeout can be caused by a transient Wi-Fi drop.",
                "Repeat the test; persistent failures suggest changing DNS server.",
            );
        }
        if let Some(best) = publics
            .iter()
            .filter(|d| d.median_ms.is_some() && d.failures == 0)
            .min_by(|a, b| a.median_ms.unwrap().total_cmp(&b.median_ms.unwrap()))
        {
            let bm = best.median_ms.unwrap_or_default();
            if p.role == ResolverRole::Configured
                && bm < med * DNS_FASTER_RATIO
                && med - bm >= DNS_FASTER_MIN_MS
            {
                c.find(
                    "dns-public-faster",
                    Severity::Info,
                    format!("{} answered faster than your configured DNS", best.label),
                    vec![
                        format!("Configured {}: median {}", p.resolver, ms(med)),
                        format!("{} {}: median {}", best.label, best.resolver, ms(bm)),
                    ],
                    "A different resolver was faster in this test. It may or may not be faster for the sites you use.",
                    Confidence::Low,
                    format!("Short benchmark with popular names, which favours large public resolvers' caches.{dot_note}"),
                    "Consider the DNS recommendation; switching changes who sees your lookups.",
                );
            }
        }
    }

    if c.has("dns-slow")
        || c.has("dns-intermittent")
        || c.has("dns-public-faster")
        || c.has("dns-configured-down")
    {
        let alt = publics
            .iter()
            .filter(|d| d.median_ms.is_some())
            .min_by(|a, b| a.median_ms.unwrap().total_cmp(&b.median_ms.unwrap()))
            .map(|d| d.resolver.clone())
            .unwrap_or_else(|| "9.9.9.9".into());
        let id = r
            .interface
            .connection_uuid
            .clone()
            .unwrap_or_else(|| "<connection>".into());
        c.manual(
            "dns-compare",
            "Try a different DNS resolver",
            "DNS measurements suggest your current resolver is slow or unreliable. This changes who processes your DNS queries, so NetTune leaves it to you.",
            &["dns-slow", "dns-intermittent", "dns-public-faster", "dns-configured-down"],
            vec![
                format!("nmcli connection modify {id} ipv4.dns \"{alt}\" ipv4.ignore-auto-dns yes"),
                format!("nmcli connection up {id}"),
                "Run diagnostics again and compare DNS results.".into(),
                format!("To undo: nmcli connection modify {id} ipv4.dns \"\" ipv4.ignore-auto-dns no && nmcli connection up {id}"),
            ],
        );
    }
}

fn wifi_rules(c: &mut Ctx) {
    let r = c.r;
    let Some(w) = &r.interface.wifi else { return };

    let weak = match (w.signal_dbm, w.signal_percent) {
        (Some(d), _) if d <= WIFI_WEAK_DBM => Some((
            if d <= WIFI_VERY_WEAK_DBM {
                Severity::Critical
            } else {
                Severity::Warning
            },
            format!("Signal {d} dBm"),
        )),
        (None, Some(p)) if p < 35 => Some((
            Severity::Warning,
            format!("Signal {p}% (NetworkManager estimate)"),
        )),
        _ => None,
    };
    if let Some((sev, ev)) = weak {
        c.find(
            "wifi-weak",
            sev,
            "Weak Wi-Fi signal",
            vec![ev],
            "A weak signal forces slower transmission rates and retransmissions, causing loss, jitter and lower throughput.",
            Confidence::High,
            "Signal is measured at this moment and changes as you move.",
            "Move closer to the access point or remove obstructions.",
        );
    } else if let Some(d) = w.signal_dbm.filter(|d| *d <= WIFI_FAIR_DBM) {
        c.find(
            "wifi-fair",
            Severity::Info,
            "Moderate Wi-Fi signal",
            vec![format!("Signal {d} dBm")],
            "Usable, but there is little margin; interference can push it into unstable territory.",
            Confidence::Medium,
            "",
            "No action needed unless you see loss or jitter.",
        );
    }
    if c.has("wifi-weak")
        || (c.has("wifi-fair") && (c.has("gateway-loss") || c.has("gateway-jitter")))
    {
        let band = match w.freq_mhz {
            Some(f) if f < 3000 => "You are on 2.4 GHz: it reaches further but is more crowded.",
            Some(_) => "You are on 5/6 GHz: faster, but range is shorter.",
            None => "",
        };
        c.manual(
            "wifi-signal",
            "Improve Wi-Fi signal",
            format!("Signal quality limits what any software setting can achieve. {band}").trim(),
            &["wifi-weak", "wifi-fair", "gateway-loss", "gateway-jitter"],
            vec![
                "Move closer to the access point, or move it to a central, elevated spot.".into(),
                "Avoid metal, mirrors, microwaves and thick walls between you and the router."
                    .into(),
                "If the router offers 5 GHz and you are close to it, prefer that network.".into(),
            ],
        );
    }

    let Some(ps) = &r.power_save else { return };
    let gw_unstable = c.has("gateway-jitter") || c.has("gateway-latency");
    if ps.runtime_on == Some(true) {
        if gw_unstable {
            c.find(
                "wifi-powersave",
                Severity::Warning,
                "Wi-Fi power saving may be adding latency",
                vec![
                    "Driver reports power saving: on".into(),
                    "Unstable or high latency measured to the router".into(),
                ],
                "With power saving on, the adapter sleeps between beacons and delivers queued packets late, which shows up as latency spikes. Both are present here, but that does not prove one causes the other.",
                Confidence::Low,
                "Effect depends on driver and access point; on many systems it is negligible.",
                "Try disabling power saving for this connection and re-measure.",
            );
        } else {
            c.find(
                "wifi-powersave",
                Severity::Info,
                "Wi-Fi power saving is on",
                vec!["Driver reports power saving: on".into()],
                "No latency problem was measured in this run, so power saving is not shown to be a problem here.",
                Confidence::Medium,
                "Spikes can be intermittent; a longer test may reveal them.",
                "Only change this if you experience lag.",
            );
        }
    }
    if ps.supported && (ps.runtime_on == Some(true) || ps.backup.is_some()) {
        let current = ps.profile_value.clone().unwrap_or_else(|| "unknown".into());
        let mut side_effects = vec![
            "The Wi-Fi connection is re-activated: expect a short disconnect (a few seconds)."
                .to_string(),
            "Slightly higher battery use while connected to this network.".to_string(),
            "Applies only to this saved connection; other networks are unchanged.".to_string(),
        ];
        if ps.auth.as_deref() == Some("auth") {
            side_effects.push("Your system will ask for an administrator password.".into());
        }
        let related_findings = c.ids(&["wifi-powersave"]);
        c.recs.push(Recommendation {
            id: "wifi-powersave-off".into(),
            title: "Disable Wi-Fi power saving for this connection".into(),
            rationale: if gw_unstable {
                "Unstable latency was measured while power saving is on. Disabling it is a common, reversible fix, but measure again to confirm it helps.".into()
            } else {
                "Optional: power saving can cause intermittent latency spikes on some drivers. Measure before and after.".into()
            },
            related_findings,
            action: RecommendationAction::WifiPowerSave {
                current,
                proposed: "disable".into(),
                side_effects,
            },
        });
    }
}

fn ethernet_rules(c: &mut Ctx) {
    let i = &c.r.interface;
    let name = i.name.clone();
    if let Some(s) = i.speed_mbps.filter(|s| *s < 1000) {
        c.find(
            "eth-slow-link",
            Severity::Warning,
            format!("Ethernet link negotiated at {s} Mb/s"),
            vec![format!("Link speed {s} Mb/s")],
            "Most modern adapters and routers support 1000 Mb/s. A lower rate often means a damaged cable, a cable with missing pairs, or an old switch port.",
            Confidence::Medium,
            "Your router or switch may only support this speed.",
            "Try another cable (Cat5e or better) and another port.",
        );
    }
    if i.duplex.as_deref() == Some("half") {
        c.find(
            "eth-half-duplex",
            Severity::Warning,
            "Ethernet link is half duplex",
            vec!["Duplex: half".into()],
            "Half duplex causes collisions and poor throughput; it usually results from a negotiation mismatch.",
            Confidence::Medium,
            "",
            "Make sure auto-negotiation is enabled on both ends.",
        );
    }
    if i.autoneg == Some(false) {
        c.find(
            "eth-autoneg-off",
            Severity::Info,
            "Auto-negotiation is disabled",
            vec!["Auto-negotiation: off".into()],
            "Forced link settings must match exactly on both ends, otherwise duplex mismatches occur.",
            Confidence::Medium,
            "This may be intentional on managed networks.",
            "Review the setting with ethtool.",
        );
    }
    if c.has("eth-slow-link") || c.has("eth-half-duplex") || c.has("eth-autoneg-off") {
        c.manual(
            "ethernet-link",
            "Review the Ethernet link",
            "The negotiated link is below what is typical for current hardware.",
            &["eth-slow-link", "eth-half-duplex", "eth-autoneg-off"],
            vec![
                format!("ethtool {name}   # shows supported and advertised link modes"),
                "Replace the cable, or try a different router/switch port.".into(),
                format!("nmcli connection modify <connection> 802-3-ethernet.auto-negotiate yes   # only if it was disabled"),
            ],
        );
    }
}

fn finish(c: Ctx) -> Analysis {
    let mut findings = c.findings;
    findings.sort_by(|a, b| b.severity.cmp(&a.severity));
    let overall = match findings.iter().map(|f| f.severity).max() {
        Some(Severity::Critical) => OverallState::Failing,
        Some(Severity::Warning) => OverallState::Degraded,
        _ => OverallState::Healthy,
    };
    let summary = match overall {
        OverallState::Healthy => "No problems detected in this run.".to_string(),
        _ => {
            let top = &findings[0];
            let more = findings
                .iter()
                .filter(|f| f.severity >= Severity::Warning)
                .count()
                - 1;
            if more > 0 {
                format!("{} (+{} more)", top.title, more)
            } else {
                top.title.clone()
            }
        }
    };
    Analysis {
        overall,
        summary,
        findings,
        recommendations: c.recs,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::probe::build_stats;

    pub fn iface(kind: InterfaceKind) -> NetInterface {
        NetInterface {
            name: "wlo1".into(),
            kind,
            state: "connected".into(),
            connected: true,
            ipv4: vec!["192.168.1.20/24".into()],
            ipv6: vec![],
            mac: None,
            mtu: Some(1500),
            driver: Some("iwlwifi".into()),
            speed_mbps: None,
            duplex: None,
            autoneg: None,
            gateway: Some("192.168.1.1".into()),
            is_default_route: true,
            nm_managed: true,
            connection_name: Some("Home".into()),
            connection_uuid: Some("6bf4bd07-e5d2-475c-bb29-2bec7a5da3dd".into()),
            dns_servers: vec!["192.168.1.1".into()],
            wifi: (kind == InterfaceKind::Wifi).then(|| WifiInfo {
                signal_dbm: Some(-50),
                freq_mhz: Some(5180),
                ..Default::default()
            }),
        }
    }

    pub fn lat(target: &str, role: TargetRole, sent: u32, rtts: Vec<f64>) -> LatencyStats {
        build_stats(target, target, role, ProbeMethod::Icmp, sent, rtts, None)
    }

    fn steady(base: f64, n: usize) -> Vec<f64> {
        (0..n).map(|i| base + (i % 2) as f64 * 0.4).collect()
    }

    pub fn dns(resolver: &str, role: ResolverRole, times: &[Option<f64>]) -> DnsResult {
        let samples: Vec<DnsSample> = times
            .iter()
            .map(|t| DnsSample {
                name: "example.com".into(),
                ms: *t,
                error: t.is_none().then(|| "timed out".into()),
            })
            .collect();
        let ok: Vec<f64> = times.iter().flatten().copied().collect();
        DnsResult {
            resolver: resolver.into(),
            label: resolver.into(),
            role,
            failures: samples.iter().filter(|s| s.ms.is_none()).count() as u32,
            median_ms: crate::probe::median(&ok),
            max_ms: ok.iter().copied().reduce(f64::max),
            samples,
        }
    }

    pub fn healthy(kind: InterfaceKind) -> DiagnosticReport {
        DiagnosticReport {
            started_at_unix: 0,
            duration_ms: 0,
            config: DiagnosticConfig::default(),
            interface: iface(kind),
            gateway: Some(lat("192.168.1.1", TargetRole::Gateway, 20, steady(2.0, 20))),
            gateway_neighbor: None,
            internet: vec![
                lat("1.1.1.1", TargetRole::Internet, 20, steady(15.0, 20)),
                lat("8.8.8.8", TargetRole::Internet, 20, steady(18.0, 20)),
            ],
            dns: vec![
                dns("192.168.1.1", ResolverRole::Configured, &[Some(12.0); 6]),
                dns("1.1.1.1", ResolverRole::Public, &[Some(14.0); 6]),
            ],
            dns_over_tls: false,
            power_save: None,
            notes: vec![],
            analysis: Analysis {
                overall: OverallState::Healthy,
                summary: String::new(),
                findings: vec![],
                recommendations: vec![],
            },
        }
    }

    fn ids(a: &Analysis) -> Vec<&str> {
        a.findings.iter().map(|f| f.id.as_str()).collect()
    }

    #[test]
    fn healthy_network_has_no_findings() {
        let a = analyze(&healthy(InterfaceKind::Wifi));
        assert_eq!(a.overall, OverallState::Healthy);
        assert!(a.findings.is_empty(), "{:?}", ids(&a));
        assert!(a.recommendations.is_empty());
    }

    #[test]
    fn disconnected_short_circuits() {
        let mut r = healthy(InterfaceKind::Ethernet);
        r.interface.connected = false;
        r.interface.state = "unavailable".into();
        let a = analyze(&r);
        assert_eq!(ids(&a), vec!["not-connected"]);
        assert_eq!(a.overall, OverallState::Failing);
        assert_eq!(a.recommendations[0].id, "connect");
    }

    #[test]
    fn gateway_filtered_but_internet_ok_is_info() {
        let mut r = healthy(InterfaceKind::Wifi);
        r.gateway = Some(lat("192.168.1.1", TargetRole::Gateway, 20, vec![]));
        let a = analyze(&r);
        assert_eq!(ids(&a), vec!["gateway-icmp-filtered"]);
        assert_eq!(a.overall, OverallState::Healthy);
    }

    #[test]
    fn gateway_and_internet_down_is_local_problem() {
        let mut r = healthy(InterfaceKind::Wifi);
        r.gateway = Some(lat("192.168.1.1", TargetRole::Gateway, 20, vec![]));
        r.gateway_neighbor = Some("FAILED".into());
        r.internet = vec![lat("1.1.1.1", TargetRole::Internet, 20, vec![])];
        r.dns = vec![dns("192.168.1.1", ResolverRole::Configured, &[None; 6])];
        let a = analyze(&r);
        assert_eq!(ids(&a), vec!["gateway-unreachable"]);
        assert_eq!(a.findings[0].confidence, Confidence::High);
        assert!(a.recommendations.iter().any(|x| x.id == "router-check"));
        // DNS rules are suppressed when nothing is reachable (no misleading DNS finding).
        assert!(!ids(&a).iter().any(|i| i.starts_with("dns")));
    }

    #[test]
    fn gateway_ok_internet_down() {
        let mut r = healthy(InterfaceKind::Ethernet);
        r.internet = vec![
            lat("1.1.1.1", TargetRole::Internet, 20, vec![]),
            build_stats(
                "1.1.1.1",
                "x",
                TargetRole::Internet,
                ProbeMethod::Tcp,
                5,
                vec![],
                None,
            ),
        ];
        let a = analyze(&r);
        assert!(ids(&a).contains(&"internet-unreachable"));
        assert_eq!(a.overall, OverallState::Failing);
        assert!(a.recommendations.iter().any(|x| x.id == "isp-path"));
    }

    #[test]
    fn icmp_blocked_but_tcp_works() {
        let mut r = healthy(InterfaceKind::Ethernet);
        r.internet = vec![
            lat("1.1.1.1", TargetRole::Internet, 20, vec![]),
            build_stats(
                "1.1.1.1",
                "x",
                TargetRole::Internet,
                ProbeMethod::Tcp,
                5,
                vec![20.0; 5],
                None,
            ),
        ];
        let a = analyze(&r);
        assert_eq!(ids(&a), vec!["icmp-blocked"]);
        assert_eq!(a.overall, OverallState::Healthy);
    }

    #[test]
    fn local_loss_detected_and_upstream_not_blamed() {
        let mut r = healthy(InterfaceKind::Wifi);
        r.gateway = Some(lat("192.168.1.1", TargetRole::Gateway, 20, steady(2.0, 17)));
        r.internet = vec![lat("1.1.1.1", TargetRole::Internet, 20, steady(15.0, 17))];
        let a = analyze(&r);
        let f = a.findings.iter().find(|f| f.id == "gateway-loss").unwrap();
        assert_eq!(f.severity, Severity::Warning);
        let inet = a.findings.iter().find(|f| f.id == "internet-loss").unwrap();
        assert_eq!(inet.title, "Packet loss on the Internet path");
        assert_eq!(inet.confidence, Confidence::Low);
        assert!(!a.recommendations.iter().any(|x| x.id == "isp-path"));
    }

    #[test]
    fn upstream_loss_with_clean_local_link() {
        let mut r = healthy(InterfaceKind::Ethernet);
        r.internet = vec![lat("1.1.1.1", TargetRole::Internet, 20, steady(15.0, 16))];
        let a = analyze(&r);
        let f = a.findings.iter().find(|f| f.id == "internet-loss").unwrap();
        assert_eq!(f.title, "Packet loss beyond your local network");
        assert!(a.recommendations.iter().any(|x| x.id == "isp-path"));
    }

    #[test]
    fn spikes_trigger_jitter_and_powersave_warning() {
        let mut r = healthy(InterfaceKind::Wifi);
        let mut rtts = steady(3.0, 18);
        rtts.insert(5, 130.0);
        rtts.insert(12, 90.0);
        r.gateway = Some(lat("192.168.1.1", TargetRole::Gateway, 20, rtts));
        r.power_save = Some(PowerSaveState {
            supported: true,
            unsupported_reason: None,
            profile_value: Some("default".into()),
            runtime_on: Some(true),
            connection_uuid: r.interface.connection_uuid.clone(),
            auth: Some("yes".into()),
            backup: None,
        });
        let a = analyze(&r);
        assert!(ids(&a).contains(&"gateway-jitter"));
        let ps = a
            .findings
            .iter()
            .find(|f| f.id == "wifi-powersave")
            .unwrap();
        assert_eq!(ps.severity, Severity::Warning);
        assert_eq!(ps.confidence, Confidence::Low);
        let rec = a
            .recommendations
            .iter()
            .find(|x| x.id == "wifi-powersave-off")
            .unwrap();
        assert!(
            matches!(rec.action, RecommendationAction::WifiPowerSave { ref proposed, .. } if proposed == "disable")
        );
        assert_eq!(rec.related_findings, vec!["wifi-powersave"]);
    }

    #[test]
    fn powersave_unsupported_hides_auto_action() {
        let mut r = healthy(InterfaceKind::Wifi);
        r.power_save = Some(PowerSaveState {
            supported: false,
            unsupported_reason: Some("driver does not report power-save state".into()),
            profile_value: None,
            runtime_on: Some(true),
            connection_uuid: None,
            auth: None,
            backup: None,
        });
        let a = analyze(&r);
        assert!(ids(&a).contains(&"wifi-powersave"));
        assert!(a
            .recommendations
            .iter()
            .all(|x| x.id != "wifi-powersave-off"));
    }

    #[test]
    fn powersave_off_without_backup_has_no_recommendation() {
        let mut r = healthy(InterfaceKind::Wifi);
        r.power_save = Some(PowerSaveState {
            supported: true,
            unsupported_reason: None,
            profile_value: Some("disable".into()),
            runtime_on: Some(false),
            connection_uuid: None,
            auth: None,
            backup: None,
        });
        let a = analyze(&r);
        assert!(a.findings.is_empty());
        assert!(a.recommendations.is_empty());
    }

    #[test]
    fn weak_signal() {
        let mut r = healthy(InterfaceKind::Wifi);
        r.interface.wifi.as_mut().unwrap().signal_dbm = Some(-84);
        let a = analyze(&r);
        let f = a.findings.iter().find(|f| f.id == "wifi-weak").unwrap();
        assert_eq!(f.severity, Severity::Critical);
        assert!(a.recommendations.iter().any(|x| x.id == "wifi-signal"));
    }

    #[test]
    fn slow_dns_and_faster_public() {
        let mut r = healthy(InterfaceKind::Wifi);
        r.dns = vec![
            dns("192.168.1.1", ResolverRole::Configured, &[Some(220.0); 6]),
            dns("1.1.1.1", ResolverRole::Public, &[Some(20.0); 6]),
        ];
        let a = analyze(&r);
        assert!(ids(&a).contains(&"dns-slow"));
        assert!(ids(&a).contains(&"dns-public-faster"));
        let rec = a
            .recommendations
            .iter()
            .find(|x| x.id == "dns-compare")
            .unwrap();
        let RecommendationAction::Manual { steps } = &rec.action else {
            panic!()
        };
        assert!(steps[0].contains("1.1.1.1"));
        assert!(steps.iter().any(|s| s.starts_with("To undo")));
    }

    #[test]
    fn small_dns_difference_is_not_reported() {
        let mut r = healthy(InterfaceKind::Wifi);
        r.dns = vec![
            dns("192.168.1.1", ResolverRole::Configured, &[Some(30.0); 6]),
            dns("1.1.1.1", ResolverRole::Public, &[Some(15.0); 6]),
        ];
        assert!(analyze(&r).findings.is_empty());
    }

    #[test]
    fn configured_dns_down() {
        let mut r = healthy(InterfaceKind::Wifi);
        r.dns = vec![
            dns("192.168.1.1", ResolverRole::Configured, &[None; 6]),
            dns("127.0.0.53", ResolverRole::SystemStub, &[Some(1.0); 6]),
            dns("1.1.1.1", ResolverRole::Public, &[Some(14.0); 6]),
        ];
        let a = analyze(&r);
        let f = a
            .findings
            .iter()
            .find(|f| f.id == "dns-configured-down")
            .unwrap();
        assert_eq!(f.severity, Severity::Warning);
    }

    #[test]
    fn intermittent_dns() {
        let mut r = healthy(InterfaceKind::Wifi);
        r.dns[0] = dns(
            "192.168.1.1",
            ResolverRole::Configured,
            &[
                Some(10.0),
                None,
                Some(11.0),
                Some(12.0),
                Some(10.0),
                Some(9.0),
            ],
        );
        assert!(ids(&analyze(&r)).contains(&"dns-intermittent"));
    }

    #[test]
    fn ethernet_negotiation() {
        let mut r = healthy(InterfaceKind::Ethernet);
        r.interface.speed_mbps = Some(100);
        r.interface.duplex = Some("half".into());
        let a = analyze(&r);
        assert!(ids(&a).contains(&"eth-slow-link"));
        assert!(ids(&a).contains(&"eth-half-duplex"));
        assert!(a.recommendations.iter().any(|x| x.id == "ethernet-link"));
    }

    #[test]
    fn probe_tool_failure_is_not_a_network_finding() {
        let mut r = healthy(InterfaceKind::Ethernet);
        r.gateway = Some(build_stats(
            "192.168.1.1",
            "x",
            TargetRole::Gateway,
            ProbeMethod::Icmp,
            0,
            vec![],
            Some("`ping` is not installed".into()),
        ));
        let a = analyze(&r);
        assert_eq!(ids(&a), vec!["gateway-probe-failed"]);
        assert_eq!(a.overall, OverallState::Healthy);
    }

    #[test]
    fn high_internet_latency() {
        let mut r = healthy(InterfaceKind::Ethernet);
        r.internet = vec![lat("1.1.1.1", TargetRole::Internet, 20, steady(400.0, 20))];
        let a = analyze(&r);
        assert!(ids(&a).contains(&"internet-latency"));
    }

    #[test]
    fn findings_sorted_by_severity() {
        let mut r = healthy(InterfaceKind::Wifi);
        r.interface.wifi.as_mut().unwrap().signal_dbm = Some(-70); // info
        r.internet = vec![lat("1.1.1.1", TargetRole::Internet, 20, steady(400.0, 20))]; // warning
        let a = analyze(&r);
        assert_eq!(a.findings[0].severity, Severity::Warning);
        assert_eq!(a.overall, OverallState::Degraded);
    }
}
