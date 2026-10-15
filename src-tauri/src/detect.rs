//! Interface detection from unprivileged sources: `ip -j`, `/sys/class/net`,
//! `nmcli` (optional) and `iw` / `ethtool` (optional).

use crate::cmd::{self, split_nmcli_terse, unescape_nmcli};
use crate::models::{InterfaceKind, NetInterface, WifiInfo};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::Duration;

const T: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq)]
pub struct IpLink {
    pub name: String,
    pub operstate: String,
    pub link_type: String,
    pub mac: Option<String>,
    pub mtu: Option<u32>,
    pub ipv4: Vec<String>,
    pub ipv6: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DefaultRoute {
    pub dev: String,
    pub gateway: Option<String>,
    pub metric: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NmDevice {
    pub device: String,
    pub dev_type: String,
    pub state: String,
    pub uuid: Option<String>,
    pub connection: Option<String>,
}

pub fn parse_ip_addr(json: &str) -> Vec<IpLink> {
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|it| {
            let name = it.get("ifname")?.as_str()?.to_string();
            let mut ipv4 = Vec::new();
            let mut ipv6 = Vec::new();
            if let Some(Value::Array(addrs)) = it.get("addr_info") {
                for a in addrs {
                    let (Some(fam), Some(local)) = (
                        a.get("family").and_then(Value::as_str),
                        a.get("local").and_then(Value::as_str),
                    ) else {
                        continue;
                    };
                    let prefix = a.get("prefixlen").and_then(Value::as_u64);
                    let scope = a.get("scope").and_then(Value::as_str).unwrap_or("");
                    let s = match prefix {
                        Some(p) => format!("{local}/{p}"),
                        None => local.to_string(),
                    };
                    match fam {
                        "inet" => ipv4.push(s),
                        // Link-local IPv6 is noise for diagnosis; keep global/site addresses.
                        "inet6" if scope != "link" && scope != "host" => ipv6.push(s),
                        _ => {}
                    }
                }
            }
            Some(IpLink {
                name,
                operstate: it
                    .get("operstate")
                    .and_then(Value::as_str)
                    .unwrap_or("UNKNOWN")
                    .to_lowercase(),
                link_type: it
                    .get("link_type")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                mac: it
                    .get("address")
                    .and_then(Value::as_str)
                    .filter(|m| *m != "00:00:00:00:00:00")
                    .map(str::to_string),
                mtu: it.get("mtu").and_then(Value::as_u64).map(|v| v as u32),
                ipv4,
                ipv6,
            })
        })
        .collect()
}

pub fn parse_default_routes(json: &str) -> Vec<DefaultRoute> {
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    let mut routes: Vec<DefaultRoute> = items
        .iter()
        .filter_map(|r| {
            Some(DefaultRoute {
                dev: r.get("dev")?.as_str()?.to_string(),
                gateway: r.get("gateway").and_then(Value::as_str).map(str::to_string),
                metric: r.get("metric").and_then(Value::as_u64).unwrap_or(0),
            })
        })
        .collect();
    routes.sort_by_key(|r| r.metric);
    routes
}

pub fn parse_nmcli_devices(text: &str) -> Vec<NmDevice> {
    text.lines()
        .filter(|l| !l.is_empty())
        .filter_map(|l| {
            let f = split_nmcli_terse(l);
            if f.len() < 5 {
                return None;
            }
            let opt = |s: &str| (!s.is_empty() && s != "--").then(|| s.to_string());
            Some(NmDevice {
                device: f[0].clone(),
                dev_type: f[1].clone(),
                state: f[2].clone(),
                uuid: opt(&f[3]),
                // Connection names may themselves contain ':' (already unescaped and split);
                // re-join any trailing fields.
                connection: opt(&f[4..].join(":")),
            })
        })
        .collect()
}

/// Parse `iw dev <if> link`.
pub fn parse_iw_link(text: &str) -> Option<WifiInfo> {
    if text.trim_start().starts_with("Not connected") {
        return None;
    }
    let mut w = WifiInfo::default();
    for line in text.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("signal:") {
            w.signal_dbm = v.split_whitespace().next().and_then(|n| n.parse().ok());
        } else if let Some(v) = line.strip_prefix("freq:") {
            w.freq_mhz = v
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|f| *f > 0.0)
                .map(|f| f.round() as u32);
        } else if let Some(v) = line.strip_prefix("rx bitrate:") {
            w.rx_bitrate_mbps = v.split_whitespace().next().and_then(|n| n.parse().ok());
        } else if let Some(v) = line.strip_prefix("tx bitrate:") {
            w.tx_bitrate_mbps = v.split_whitespace().next().and_then(|n| n.parse().ok());
        }
    }
    Some(w)
}

/// Parse `iw dev <if> get power_save` ("Power save: on").
pub fn parse_iw_power_save(text: &str) -> Option<bool> {
    let v = text.split_once("Power save:")?.1.trim();
    match v {
        s if s.starts_with("on") => Some(true),
        s if s.starts_with("off") => Some(false),
        _ => None,
    }
}

/// Parse `ethtool <if>` for speed / duplex / auto-negotiation.
pub fn parse_ethtool(text: &str) -> (Option<u32>, Option<String>, Option<bool>) {
    let mut speed = None;
    let mut duplex = None;
    let mut autoneg = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(v) = line.strip_prefix("Speed:") {
            speed = v.trim().trim_end_matches("Mb/s").trim().parse().ok();
        } else if let Some(v) = line.strip_prefix("Duplex:") {
            let v = v.trim().to_lowercase();
            if v == "full" || v == "half" {
                duplex = Some(v);
            }
        } else if let Some(v) = line.strip_prefix("Auto-negotiation:") {
            autoneg = match v.trim() {
                "on" => Some(true),
                "off" => Some(false),
                _ => None,
            };
        }
    }
    (speed, duplex, autoneg)
}

/// Parse `nmcli -g IP4.DNS,IP6.DNS device show <if>`: one line per field, multiple values ` | `.
pub fn parse_nmcli_dns(text: &str) -> Vec<String> {
    text.lines()
        .flat_map(|l| l.split(" | "))
        .map(|v| unescape_nmcli(v.trim()))
        .filter(|v| !v.is_empty())
        .collect()
}

fn read_trim(p: &Path) -> Option<String> {
    fs::read_to_string(p).ok().map(|s| s.trim().to_string())
}

fn sysfs_kind(name: &str) -> InterfaceKind {
    let base = Path::new("/sys/class/net").join(name);
    if base.join("wireless").exists() || base.join("phy80211").exists() {
        return InterfaceKind::Wifi;
    }
    match read_trim(&base.join("type")).as_deref() {
        Some("772") => return InterfaceKind::Loopback,
        Some("1") => {}
        Some(_) => return InterfaceKind::Other,
        None => {}
    }
    if !base.join("device").exists() {
        InterfaceKind::Virtual
    } else {
        InterfaceKind::Ethernet
    }
}

fn sysfs_driver(name: &str) -> Option<String> {
    let link = fs::read_link(Path::new("/sys/class/net").join(name).join("device/driver")).ok()?;
    link.file_name().map(|s| s.to_string_lossy().into_owned())
}

/// Kernel reports -1 (unknown) or EINVAL (down / not applicable); both become `None`.
fn sysfs_speed(name: &str) -> Option<u32> {
    read_trim(&Path::new("/sys/class/net").join(name).join("speed"))?
        .parse::<i64>()
        .ok()
        .filter(|v| *v > 0 && *v < 1_000_000)
        .map(|v| v as u32)
}

fn wifi_signal_percent(name: &str) -> Option<u8> {
    let out = cmd::run(
        "nmcli",
        &[
            "-t",
            "-f",
            "IN-USE,SIGNAL",
            "device",
            "wifi",
            "list",
            "ifname",
            name,
            "--rescan",
            "no",
        ],
        T,
    )
    .ok()?;
    out.stdout.lines().find_map(|l| {
        let f = split_nmcli_terse(l);
        (f.first().map(String::as_str) == Some("*"))
            .then(|| f.get(1)?.parse().ok())
            .flatten()
    })
}

pub fn iw_power_save(name: &str) -> Option<bool> {
    let out = cmd::run("iw", &["dev", name, "get", "power_save"], T).ok()?;
    parse_iw_power_save(&out.stdout)
}

fn wifi_info(name: &str, ssid: Option<String>) -> WifiInfo {
    let mut w = cmd::run("iw", &["dev", name, "link"], T)
        .ok()
        .and_then(|o| parse_iw_link(&o.stdout))
        .unwrap_or_default();
    // nmcli gives the SSID as UTF-8 (iw prints byte escapes).
    w.ssid = ssid;
    w.signal_percent = wifi_signal_percent(name);
    w.power_save = iw_power_save(name);
    w
}

pub fn list_interfaces() -> Result<Vec<NetInterface>, String> {
    let addr = cmd::run("ip", &["-j", "addr", "show"], T).map_err(|e| e.to_string())?;
    if !addr.success() {
        return Err(format!("`ip -j addr` failed: {}", addr.stderr.trim()));
    }
    let links = parse_ip_addr(&addr.stdout);

    let mut routes = cmd::run("ip", &["-j", "route", "show", "default"], T)
        .ok()
        .map(|o| parse_default_routes(&o.stdout))
        .unwrap_or_default();
    routes.extend(
        cmd::run("ip", &["-6", "-j", "route", "show", "default"], T)
            .ok()
            .map(|o| parse_default_routes(&o.stdout))
            .unwrap_or_default(),
    );
    let primary_dev = routes.first().map(|r| r.dev.clone());

    let nm: HashMap<String, NmDevice> = cmd::run(
        "nmcli",
        &[
            "-t",
            "-f",
            "DEVICE,TYPE,STATE,CON-UUID,CONNECTION",
            "device",
        ],
        T,
    )
    .ok()
    .filter(|o| o.success())
    .map(|o| parse_nmcli_devices(&o.stdout))
    .unwrap_or_default()
    .into_iter()
    .map(|d| (d.device.clone(), d))
    .collect();

    let mut out = Vec::new();
    for l in links {
        let kind = sysfs_kind(&l.name);
        if kind == InterfaceKind::Loopback || l.link_type == "loopback" {
            continue;
        }
        let nmd = nm.get(&l.name);
        let nm_managed = nmd.is_some_and(|d| d.state != "unmanaged");
        let state = nmd
            .map(|d| d.state.clone())
            .unwrap_or_else(|| l.operstate.clone());
        let has_addr = !l.ipv4.is_empty() || !l.ipv6.is_empty();
        let connected = match nmd {
            Some(d) if d.state != "unmanaged" => d.state.starts_with("connected") && has_addr,
            _ => l.operstate == "up" && has_addr,
        };
        let gateway = routes
            .iter()
            .find(|r| r.dev == l.name && r.gateway.is_some())
            .and_then(|r| r.gateway.clone());

        let (mut speed, mut duplex, mut autoneg) = (None, None, None);
        if kind == InterfaceKind::Ethernet {
            if let Ok(o) = cmd::run("ethtool", &[&l.name], T) {
                (speed, duplex, autoneg) = parse_ethtool(&o.stdout);
            }
            speed = speed.or_else(|| sysfs_speed(&l.name));
            duplex = duplex.or_else(|| {
                read_trim(&Path::new("/sys/class/net").join(&l.name).join("duplex"))
                    .filter(|d| d == "full" || d == "half")
            });
            if !connected {
                // Values reported for a link without carrier are meaningless.
                speed = None;
                duplex = None;
            }
        }

        let dns_servers = if nm_managed {
            cmd::run(
                "nmcli",
                &["-g", "IP4.DNS,IP6.DNS", "device", "show", &l.name],
                T,
            )
            .ok()
            .filter(|o| o.success())
            .map(|o| parse_nmcli_dns(&o.stdout))
            .unwrap_or_default()
        } else {
            Vec::new()
        };

        let connection_name = nmd.and_then(|d| d.connection.clone());
        let wifi = (kind == InterfaceKind::Wifi).then(|| {
            if connected {
                wifi_info(&l.name, connection_name.clone())
            } else {
                WifiInfo {
                    power_save: iw_power_save(&l.name),
                    ..Default::default()
                }
            }
        });

        out.push(NetInterface {
            is_default_route: primary_dev.as_deref() == Some(l.name.as_str()),
            driver: sysfs_driver(&l.name),
            name: l.name,
            kind,
            state,
            connected,
            ipv4: l.ipv4,
            ipv6: l.ipv6,
            mac: l.mac,
            mtu: l.mtu,
            speed_mbps: speed,
            duplex,
            autoneg,
            gateway,
            nm_managed,
            connection_name,
            connection_uuid: nmd.and_then(|d| d.uuid.clone()),
            dns_servers,
            wifi,
        });
    }
    out.sort_by_key(|i| {
        (
            !i.is_default_route,
            !i.connected,
            i.kind == InterfaceKind::Virtual,
            i.name.clone(),
        )
    });
    Ok(out)
}

/// Whether /etc/resolv.conf points at the systemd-resolved stub.
pub fn uses_resolved_stub() -> bool {
    fs::read_to_string("/etc/resolv.conf")
        .map(|s| {
            s.lines()
                .any(|l| l.trim().starts_with("nameserver") && l.contains("127.0.0.53"))
        })
        .unwrap_or(false)
}

pub fn dns_over_tls_enabled() -> bool {
    cmd::run("resolvectl", &["status"], T)
        .map(|o| o.stdout.contains("+DNSOverTLS"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    const IP_ADDR: &str = include_str!("../tests/fixtures/ip_addr.json");

    #[test]
    fn parses_ip_addr() {
        let links = parse_ip_addr(IP_ADDR);
        assert_eq!(links.len(), 3);
        let w = links.iter().find(|l| l.name == "wlo1").unwrap();
        assert_eq!(w.ipv4, vec!["172.20.10.4/28"]);
        assert_eq!(w.ipv6, vec!["2001:db8::5/64"]); // link-local dropped
        assert_eq!(w.operstate, "up");
        assert_eq!(w.mtu, Some(1500));
        let lo = links.iter().find(|l| l.name == "lo").unwrap();
        assert_eq!(lo.mac, None);
        let e = links.iter().find(|l| l.name == "enp2s0").unwrap();
        assert!(e.ipv4.is_empty());
        assert_eq!(e.operstate, "down");
    }

    #[test]
    fn parses_garbage_as_empty() {
        assert!(parse_ip_addr("not json").is_empty());
        assert!(parse_default_routes("").is_empty());
    }

    #[test]
    fn default_routes_sorted_by_metric() {
        let r = parse_default_routes(
            r#"[{"dst":"default","gateway":"10.0.0.1","dev":"wlo1","metric":600},
                {"dst":"default","gateway":"192.168.1.1","dev":"enp2s0","metric":100}]"#,
        );
        assert_eq!(r[0].dev, "enp2s0");
        assert_eq!(r[1].gateway.as_deref(), Some("10.0.0.1"));
    }

    #[test]
    fn parses_nmcli_devices_with_colon_in_name() {
        let d = parse_nmcli_devices(
            "wlo1:wifi:connected:6bf4bd07-e5d2:Cafe\\: 2nd floor\nenp2s0:ethernet:unavailable::\n",
        );
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].connection.as_deref(), Some("Cafe: 2nd floor"));
        assert_eq!(d[0].uuid.as_deref(), Some("6bf4bd07-e5d2"));
        assert_eq!(d[1].uuid, None);
        assert_eq!(d[1].connection, None);
    }

    #[test]
    fn parses_iw_link() {
        let w = parse_iw_link(
            "Connected to 4e:45:7c:fa:7f:db (on wlo1)\n\tSSID: x\n\tfreq: 2437.0\n\tsignal: -29 dBm\n\trx bitrate: 130.0 MBit/s MCS 15\n\ttx bitrate: 26.0 MBit/s MCS 9\n",
        )
        .unwrap();
        assert_eq!(w.signal_dbm, Some(-29));
        assert_eq!(w.freq_mhz, Some(2437));
        assert_eq!(w.rx_bitrate_mbps, Some(130.0));
        assert_eq!(w.tx_bitrate_mbps, Some(26.0));
        assert!(parse_iw_link("Not connected.\n").is_none());
    }

    #[test]
    fn parses_power_save() {
        assert_eq!(parse_iw_power_save("Power save: on\n"), Some(true));
        assert_eq!(parse_iw_power_save("Power save: off\n"), Some(false));
        assert_eq!(
            parse_iw_power_save("command failed: Operation not supported (-95)"),
            None
        );
    }

    #[test]
    fn parses_ethtool() {
        let (s, d, a) = parse_ethtool(
            "Settings for enp2s0:\n\tSpeed: 100Mb/s\n\tDuplex: Half\n\tAuto-negotiation: on\nnetlink error: Operation not permitted\n",
        );
        assert_eq!((s, d.as_deref(), a), (Some(100), Some("half"), Some(true)));
        let (s, d, _) = parse_ethtool("\tSpeed: Unknown!\n\tDuplex: Unknown! (255)\n");
        assert_eq!((s, d), (None, None));
    }

    #[test]
    fn parses_nmcli_dns() {
        assert_eq!(
            parse_nmcli_dns("192.168.1.1 | 9.9.9.9\nfe80\\:\\:1\n"),
            vec!["192.168.1.1", "9.9.9.9", "fe80::1"]
        );
        assert!(parse_nmcli_dns("\n\n").is_empty());
    }
}
