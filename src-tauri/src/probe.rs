//! Active measurements: ICMP latency via the system `ping`, TCP handshake timing
//! as a fallback when ICMP is filtered, and direct UDP DNS queries.

use crate::cmd::{self, CmdError};
use crate::models::{DnsResult, DnsSample, LatencyStats, ProbeMethod, ResolverRole, TargetRole};
use std::net::{IpAddr, SocketAddr, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Interval between echo requests (gentle; well above the 2 ms unprivileged floor).
pub const PING_INTERVAL_S: f64 = 0.2;

pub fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut v = values.to_vec();
    v.sort_by(|a, b| a.total_cmp(b));
    let n = v.len();
    Some(if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    })
}

/// Mean absolute difference of consecutive samples. Needs at least two samples.
pub fn jitter(values: &[f64]) -> Option<f64> {
    if values.len() < 2 {
        return None;
    }
    let sum: f64 = values.windows(2).map(|w| (w[1] - w[0]).abs()).sum();
    Some(sum / (values.len() - 1) as f64)
}

pub fn build_stats(
    target: &str,
    label: &str,
    role: TargetRole,
    method: ProbeMethod,
    sent: u32,
    rtts: Vec<f64>,
    error: Option<String>,
) -> LatencyStats {
    let received = rtts.len() as u32;
    let loss_pct = (sent > 0 && error.is_none())
        .then(|| (sent.saturating_sub(received)) as f64 * 100.0 / sent as f64);
    let min = rtts.iter().copied().reduce(f64::min);
    let max = rtts.iter().copied().reduce(f64::max);
    let avg = (!rtts.is_empty()).then(|| rtts.iter().sum::<f64>() / rtts.len() as f64);
    LatencyStats {
        target: target.to_string(),
        label: label.to_string(),
        role,
        method,
        sent,
        received,
        loss_pct,
        min_ms: min,
        avg_ms: avg,
        median_ms: median(&rtts),
        max_ms: max,
        jitter_ms: jitter(&rtts),
        rtts_ms: rtts,
        error,
    }
}

#[derive(Debug, PartialEq)]
pub struct PingParse {
    pub transmitted: Option<u32>,
    pub rtts: Vec<f64>,
}

/// Parse iputils `ping -n` output (C locale).
pub fn parse_ping(stdout: &str) -> PingParse {
    let mut rtts = Vec::new();
    let mut transmitted = None;
    for line in stdout.lines() {
        if line.contains("bytes from") && !line.contains("(DUP!)") {
            if let Some(t) = line
                .split_once("time=")
                .and_then(|(_, r)| r.split_whitespace().next())
                .and_then(|n| n.parse::<f64>().ok())
            {
                rtts.push(t);
            }
        } else if let Some((n, _)) = line.split_once(" packets transmitted") {
            transmitted = n.trim().parse().ok();
        }
    }
    PingParse { transmitted, rtts }
}

/// Run `ping` against one target with bounded duration.
pub fn ping(
    target: &str,
    label: &str,
    role: TargetRole,
    samples: u32,
    timeout_ms: u32,
) -> LatencyStats {
    // Only literal IP addresses are ever passed to ping (never option-like strings or names).
    if target.parse::<IpAddr>().is_err() {
        return build_stats(
            target,
            label,
            role,
            ProbeMethod::Icmp,
            0,
            vec![],
            Some("not an IP address".into()),
        );
    }
    let wait_s = timeout_ms as f64 / 1000.0;
    // `-c` alone sends exactly `samples` requests and waits `-W` for the last reply.
    // (`-w` is avoided: with it ping keeps sending until it has `samples` replies.)
    let budget = Duration::from_secs_f64(samples as f64 * PING_INTERVAL_S + wait_s + 3.0);
    let (count, interval, wait) = (
        samples.to_string(),
        format!("{PING_INTERVAL_S}"),
        format!("{wait_s:.1}"),
    );
    let args = ["-n", "-c", &count, "-i", &interval, "-W", &wait, target];
    let res = cmd::run("ping", &args, budget);
    match res {
        Ok(out) => {
            let p = parse_ping(&out.stdout);
            let sent = p.transmitted.unwrap_or(samples);
            // Exit 1 = no/partial replies (a measurement). Other non-zero = the probe failed.
            let error = match out.status {
                Some(0) | Some(1) if p.transmitted.is_some() => None,
                _ => Some(
                    first_line(&out.stderr)
                        .unwrap_or_else(|| format!("ping exited with status {:?}", out.status)),
                ),
            };
            build_stats(target, label, role, ProbeMethod::Icmp, sent, p.rtts, error)
        }
        Err(e) => build_stats(
            target,
            label,
            role,
            ProbeMethod::Icmp,
            0,
            Vec::new(),
            Some(match e {
                CmdError::NotFound(_) => "`ping` (iputils-ping) is not installed".to_string(),
                other => other.to_string(),
            }),
        ),
    }
}

fn first_line(s: &str) -> Option<String> {
    s.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_string)
}

/// Time TCP handshakes. A refused connection still proves the host answered, so it counts.
pub fn tcp_probe(
    target: &str,
    label: &str,
    role: TargetRole,
    port: u16,
    attempts: u32,
    timeout_ms: u32,
) -> LatencyStats {
    let Ok(ip) = target.parse::<IpAddr>() else {
        return build_stats(
            target,
            label,
            role,
            ProbeMethod::Tcp,
            0,
            vec![],
            Some("invalid address".into()),
        );
    };
    let addr = SocketAddr::new(ip, port);
    let mut rtts = Vec::new();
    for _ in 0..attempts {
        let t0 = Instant::now();
        match TcpStream::connect_timeout(&addr, Duration::from_millis(timeout_ms as u64)) {
            Ok(_) => rtts.push(t0.elapsed().as_secs_f64() * 1000.0),
            Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => {
                rtts.push(t0.elapsed().as_secs_f64() * 1000.0)
            }
            Err(_) => {}
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    build_stats(target, label, role, ProbeMethod::Tcp, attempts, rtts, None)
}

// ---------------------------------------------------------------- DNS

static QUERY_SEQ: AtomicU16 = AtomicU16::new(0);

fn query_id() -> u16 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    (nanos as u16) ^ QUERY_SEQ.fetch_add(0x9E37, Ordering::Relaxed)
}

/// Build a standard recursive query for an A record.
pub fn build_dns_query(id: u16, name: &str) -> Option<Vec<u8>> {
    let mut p = Vec::with_capacity(32 + name.len());
    p.extend_from_slice(&id.to_be_bytes());
    p.extend_from_slice(&[0x01, 0x00]); // RD
    p.extend_from_slice(&[0, 1, 0, 0, 0, 0, 0, 0]); // QD=1
    for label in name.trim_end_matches('.').split('.') {
        if label.is_empty() || label.len() > 63 {
            return None;
        }
        p.push(label.len() as u8);
        p.extend_from_slice(label.as_bytes());
    }
    p.push(0);
    p.extend_from_slice(&[0, 1, 0, 1]); // QTYPE=A, QCLASS=IN
    Some(p)
}

#[derive(Debug, PartialEq, Eq)]
pub enum DnsReply {
    /// Answered (NOERROR or NXDOMAIN — both mean the resolver did its job).
    Answered,
    /// Resolver replied with an error code (SERVFAIL, REFUSED...).
    Rcode(u8),
    /// Not a reply to our query.
    Unrelated,
}

pub fn check_dns_reply(id: u16, buf: &[u8]) -> DnsReply {
    if buf.len() < 12 || u16::from_be_bytes([buf[0], buf[1]]) != id || buf[2] & 0x80 == 0 {
        return DnsReply::Unrelated;
    }
    match buf[3] & 0x0f {
        0 | 3 => DnsReply::Answered,
        rc => DnsReply::Rcode(rc),
    }
}

fn rcode_name(rc: u8) -> String {
    match rc {
        1 => "format error".into(),
        2 => "server failure (SERVFAIL)".into(),
        4 => "not implemented".into(),
        5 => "query refused".into(),
        n => format!("response code {n}"),
    }
}

pub fn dns_query(resolver: IpAddr, name: &str, timeout_ms: u32) -> DnsSample {
    let fail = |e: String| DnsSample {
        name: name.to_string(),
        ms: None,
        error: Some(e),
    };
    let id = query_id();
    let Some(pkt) = build_dns_query(id, name) else {
        return fail("invalid name".into());
    };
    let bind: SocketAddr = match resolver {
        IpAddr::V4(_) => "0.0.0.0:0".parse().unwrap(),
        IpAddr::V6(_) => "[::]:0".parse().unwrap(),
    };
    let sock = match UdpSocket::bind(bind) {
        Ok(s) => s,
        Err(e) => return fail(format!("socket error: {e}")),
    };
    let deadline = Instant::now() + Duration::from_millis(timeout_ms as u64);
    if let Err(e) = sock.connect(SocketAddr::new(resolver, 53)) {
        return fail(format!("no route to resolver: {e}"));
    }
    let t0 = Instant::now();
    if let Err(e) = sock.send(&pkt) {
        return fail(format!("send failed: {e}"));
    }
    let mut buf = [0u8; 1500];
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return fail("timed out".into());
        }
        let _ = sock.set_read_timeout(Some(left));
        match sock.recv(&mut buf) {
            Ok(n) => match check_dns_reply(id, &buf[..n]) {
                DnsReply::Answered => {
                    return DnsSample {
                        name: name.to_string(),
                        ms: Some(t0.elapsed().as_secs_f64() * 1000.0),
                        error: None,
                    }
                }
                DnsReply::Rcode(rc) => return fail(rcode_name(rc)),
                DnsReply::Unrelated => continue,
            },
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                return fail("timed out".into())
            }
            Err(e) => return fail(format!("receive failed: {e}")),
        }
    }
}

/// Real, popular names. Results mix cold and warm cache entries; see README limitations.
pub const DNS_NAMES: &[&str] = &[
    "example.com",
    "wikipedia.org",
    "github.com",
    "ubuntu.com",
    "debian.org",
    "mozilla.org",
];

pub fn dns_benchmark(
    resolver: IpAddr,
    label: &str,
    role: ResolverRole,
    timeout_ms: u32,
) -> DnsResult {
    let samples: Vec<DnsSample> = DNS_NAMES
        .iter()
        .map(|n| dns_query(resolver, n, timeout_ms))
        .collect();
    let times: Vec<f64> = samples.iter().filter_map(|s| s.ms).collect();
    DnsResult {
        resolver: resolver.to_string(),
        label: label.to_string(),
        role,
        failures: samples.iter().filter(|s| s.ms.is_none()).count() as u32,
        median_ms: median(&times),
        max_ms: times.iter().copied().reduce(f64::max),
        samples,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PING_OK: &str = "PING 172.20.10.1 (172.20.10.1) 56(84) bytes of data.
64 bytes from 172.20.10.1: icmp_seq=1 ttl=64 time=38.6 ms
64 bytes from 172.20.10.1: icmp_seq=2 ttl=64 time=5.29 ms
64 bytes from 172.20.10.1: icmp_seq=2 ttl=64 time=5.30 ms (DUP!)
64 bytes from 172.20.10.1: icmp_seq=4 ttl=64 time=130 ms

--- 172.20.10.1 ping statistics ---
4 packets transmitted, 3 received, +1 duplicates, 25% packet loss, time 803ms
rtt min/avg/max/mdev = 5.287/58.165/129.894/42.341 ms
";

    #[test]
    fn parses_ping_output() {
        let p = parse_ping(PING_OK);
        assert_eq!(p.transmitted, Some(4));
        assert_eq!(p.rtts, vec![38.6, 5.29, 130.0]);
    }

    #[test]
    fn parses_total_loss() {
        let p = parse_ping(
            "PING 10.9.9.9 (10.9.9.9) 56(84) bytes of data.\n\n--- 10.9.9.9 ping statistics ---\n5 packets transmitted, 0 received, 100% packet loss, time 4087ms\n",
        );
        assert_eq!(p.transmitted, Some(5));
        assert!(p.rtts.is_empty());
        let s = build_stats(
            "x",
            "x",
            TargetRole::Internet,
            ProbeMethod::Icmp,
            5,
            p.rtts,
            None,
        );
        assert_eq!(s.loss_pct, Some(100.0));
        assert_eq!(s.avg_ms, None);
        assert_eq!(s.jitter_ms, None);
    }

    #[test]
    fn stats_math() {
        let s = build_stats(
            "t",
            "t",
            TargetRole::Gateway,
            ProbeMethod::Icmp,
            5,
            vec![10.0, 20.0, 10.0, 30.0],
            None,
        );
        assert_eq!(s.loss_pct, Some(20.0));
        assert_eq!(s.min_ms, Some(10.0));
        assert_eq!(s.max_ms, Some(30.0));
        assert_eq!(s.avg_ms, Some(17.5));
        assert_eq!(s.median_ms, Some(15.0));
        // |20-10| + |10-20| + |30-10| = 40 / 3
        assert!((s.jitter_ms.unwrap() - 40.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn errored_probe_has_no_loss_value() {
        let s = build_stats(
            "t",
            "t",
            TargetRole::Gateway,
            ProbeMethod::Icmp,
            0,
            vec![],
            Some("x".into()),
        );
        assert_eq!(s.loss_pct, None);
    }

    #[test]
    fn median_and_jitter_edges() {
        assert_eq!(median(&[]), None);
        assert_eq!(median(&[3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(jitter(&[5.0]), None);
    }

    #[test]
    fn dns_query_encoding() {
        let q = build_dns_query(0xabcd, "example.com").unwrap();
        assert_eq!(&q[..4], &[0xab, 0xcd, 0x01, 0x00]);
        assert_eq!(&q[12..25], b"\x07example\x03com\x00");
        assert_eq!(&q[25..], &[0, 1, 0, 1]);
        assert!(build_dns_query(1, "bad..name").is_none());
    }

    #[test]
    fn dns_reply_validation() {
        let mut r = vec![0u8; 12];
        r[0] = 0x12;
        r[1] = 0x34;
        r[2] = 0x81; // QR + RD
        r[3] = 0x80; // RA, NOERROR
        assert_eq!(check_dns_reply(0x1234, &r), DnsReply::Answered);
        assert_eq!(check_dns_reply(0x9999, &r), DnsReply::Unrelated);
        r[3] = 0x83; // NXDOMAIN
        assert_eq!(check_dns_reply(0x1234, &r), DnsReply::Answered);
        r[3] = 0x82; // SERVFAIL
        assert_eq!(check_dns_reply(0x1234, &r), DnsReply::Rcode(2));
        r[2] = 0x01; // not a response
        assert_eq!(check_dns_reply(0x1234, &r), DnsReply::Unrelated);
        assert_eq!(check_dns_reply(0x1234, &r[..5]), DnsReply::Unrelated);
    }

    #[test]
    fn dns_to_unreachable_resolver_times_out_within_bound() {
        // TEST-NET-1 address: guaranteed not to answer.
        let t0 = Instant::now();
        let s = dns_query("192.0.2.1".parse().unwrap(), "example.com", 500);
        assert!(s.ms.is_none());
        assert!(t0.elapsed() < Duration::from_millis(1500));
    }
}
