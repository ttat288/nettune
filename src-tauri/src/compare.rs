//! Before/after comparison of two diagnostic reports. Pure and deterministic.
//!
//! A change is only called "better" or "worse" when it exceeds a noise margin;
//! everything else is reported as "no clear change".

use crate::models::{DiagnosticReport, LatencyStats, ProbeMethod, ResolverRole};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Change {
    Better,
    Worse,
    NoClearChange,
    /// One side is missing, so no comparison is possible.
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricDelta {
    pub key: String,
    pub label: String,
    pub unit: String,
    pub before: Option<f64>,
    pub after: Option<f64>,
    pub change: Change,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Comparison {
    pub metrics: Vec<MetricDelta>,
    pub warnings: Vec<String>,
    pub summary: String,
}

/// Lower is better for every metric. `abs` and `rel` define the noise margin.
fn classify(before: Option<f64>, after: Option<f64>, abs: f64, rel: f64) -> Change {
    match (before, after) {
        (Some(b), Some(a)) => {
            let margin = abs.max(rel * b);
            if a < b - margin {
                Change::Better
            } else if a > b + margin {
                Change::Worse
            } else {
                Change::NoClearChange
            }
        }
        _ => Change::Unavailable,
    }
}

/// Aggregate Internet ICMP targets measured in both runs (same targets → comparable).
fn internet_metric(
    before: &DiagnosticReport,
    after: &DiagnosticReport,
    f: impl Fn(&LatencyStats) -> Option<f64>,
) -> (Option<f64>, Option<f64>) {
    let pick = |r: &DiagnosticReport, other: &DiagnosticReport| -> Option<f64> {
        let vals: Vec<f64> = r
            .internet
            .iter()
            .filter(|s| s.method == ProbeMethod::Icmp && s.error.is_none())
            .filter(|s| {
                other.internet.iter().any(|o| {
                    o.target == s.target && o.method == ProbeMethod::Icmp && o.error.is_none()
                })
            })
            .filter_map(&f)
            .collect();
        (!vals.is_empty()).then(|| vals.iter().sum::<f64>() / vals.len() as f64)
    };
    (pick(before, after), pick(after, before))
}

fn dns_median(r: &DiagnosticReport, role: ResolverRole) -> Option<f64> {
    let v: Vec<f64> = r
        .dns
        .iter()
        .filter(|d| d.role == role)
        .filter_map(|d| d.median_ms)
        .collect();
    (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64)
}

pub fn compare(before: &DiagnosticReport, after: &DiagnosticReport) -> Comparison {
    let mut warnings = Vec::new();
    if before.interface.name != after.interface.name {
        warnings.push(format!(
            "Different interfaces ({} vs {}); results are not comparable.",
            before.interface.name, after.interface.name
        ));
    }
    if before.interface.connection_uuid != after.interface.connection_uuid {
        warnings.push("The active connection changed between runs.".into());
    }
    if before.config != after.config {
        warnings.push("Test settings differ between runs.".into());
    }
    fn gw_ok(r: &DiagnosticReport) -> Option<&LatencyStats> {
        r.gateway
            .as_ref()
            .filter(|g| g.error.is_none() && g.received > 0)
    }
    let (gb, ga) = (gw_ok(before), gw_ok(after));
    if before.gateway.as_ref().map(|g| &g.target) != after.gateway.as_ref().map(|g| &g.target) {
        warnings.push("The router address changed between runs.".into());
    }

    let mut m = Vec::new();
    let mut push =
        |key: &str, label: &str, unit: &str, b: Option<f64>, a: Option<f64>, abs: f64, rel: f64| {
            m.push(MetricDelta {
                key: key.into(),
                label: label.into(),
                unit: unit.into(),
                before: b,
                after: a,
                change: classify(b, a, abs, rel),
            })
        };

    push(
        "gwLatency",
        "Router latency (median)",
        "ms",
        gb.and_then(|g| g.median_ms),
        ga.and_then(|g| g.median_ms),
        2.0,
        0.15,
    );
    push(
        "gwJitter",
        "Router jitter",
        "ms",
        gb.and_then(|g| g.jitter_ms),
        ga.and_then(|g| g.jitter_ms),
        1.0,
        0.25,
    );
    push(
        "gwLoss",
        "Router packet loss",
        "%",
        gb.and_then(|g| g.loss_pct),
        ga.and_then(|g| g.loss_pct),
        5.0,
        0.0,
    );
    let (b, a) = internet_metric(before, after, |s| s.median_ms);
    push(
        "netLatency",
        "Internet latency (median)",
        "ms",
        b,
        a,
        2.0,
        0.15,
    );
    let (b, a) = internet_metric(before, after, |s| s.jitter_ms);
    push("netJitter", "Internet jitter", "ms", b, a, 1.0, 0.25);
    let (b, a) = internet_metric(before, after, |s| s.loss_pct);
    push("netLoss", "Internet packet loss", "%", b, a, 5.0, 0.0);
    let role = if dns_median(before, ResolverRole::Configured).is_some() {
        ResolverRole::Configured
    } else {
        ResolverRole::SystemStub
    };
    push(
        "dns",
        "DNS lookup (median)",
        "ms",
        dns_median(before, role),
        dns_median(after, role),
        5.0,
        0.2,
    );

    let better = m.iter().filter(|d| d.change == Change::Better).count();
    let worse = m.iter().filter(|d| d.change == Change::Worse).count();
    let summary = if !warnings.is_empty() && before.interface.name != after.interface.name {
        "Not comparable.".to_string()
    } else if better == 0 && worse == 0 {
        "No clear change beyond normal variation.".to_string()
    } else if worse == 0 {
        format!("{better} metric(s) improved beyond normal variation; none got worse.")
    } else if better == 0 {
        format!("{worse} metric(s) got worse beyond normal variation.")
    } else {
        format!("Mixed: {better} improved, {worse} got worse.")
    };
    Comparison {
        metrics: m,
        warnings,
        summary,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{InterfaceKind, TargetRole};
    use crate::rules::tests::{dns, healthy, lat};

    fn with_gateway(rtts: Vec<f64>) -> DiagnosticReport {
        let mut r = healthy(InterfaceKind::Wifi);
        r.gateway = Some(lat("192.168.1.1", TargetRole::Gateway, 20, rtts));
        r
    }

    #[test]
    fn identical_runs_show_no_change() {
        let r = healthy(InterfaceKind::Wifi);
        let c = compare(&r, &r);
        assert!(c.metrics.iter().all(|d| d.change == Change::NoClearChange));
        assert!(c.warnings.is_empty());
        assert_eq!(c.summary, "No clear change beyond normal variation.");
    }

    #[test]
    fn small_changes_are_noise() {
        let b = with_gateway(vec![3.0; 20]);
        let a = with_gateway(vec![4.0; 20]); // +1 ms < 2 ms floor
        let c = compare(&b, &a);
        assert_eq!(c.metrics[0].change, Change::NoClearChange);
    }

    #[test]
    fn jitter_improvement_detected() {
        let spiky: Vec<f64> = (0..20)
            .map(|i| if i % 4 == 0 { 120.0 } else { 3.0 })
            .collect();
        let b = with_gateway(spiky);
        let a = with_gateway([3.0, 3.5].repeat(10));
        let c = compare(&b, &a);
        let j = c.metrics.iter().find(|d| d.key == "gwJitter").unwrap();
        assert_eq!(j.change, Change::Better);
        assert!(c.summary.contains("improved"));
    }

    #[test]
    fn worse_loss_detected() {
        let b = with_gateway(vec![3.0; 20]);
        let a = with_gateway(vec![3.0; 16]); // 20% loss
        let c = compare(&b, &a);
        let l = c.metrics.iter().find(|d| d.key == "gwLoss").unwrap();
        assert_eq!(
            (l.before, l.after, l.change),
            (Some(0.0), Some(20.0), Change::Worse)
        );
    }

    #[test]
    fn missing_side_is_unavailable() {
        let b = with_gateway(vec![]);
        let a = with_gateway(vec![3.0; 20]);
        let c = compare(&b, &a);
        assert_eq!(c.metrics[0].change, Change::Unavailable);
    }

    #[test]
    fn different_interface_not_comparable() {
        let b = healthy(InterfaceKind::Wifi);
        let mut a = healthy(InterfaceKind::Wifi);
        a.interface.name = "enp2s0".into();
        let c = compare(&b, &a);
        assert_eq!(c.summary, "Not comparable.");
        assert!(!c.warnings.is_empty());
    }

    #[test]
    fn dns_uses_configured_resolver() {
        let b = healthy(InterfaceKind::Wifi);
        let mut a = healthy(InterfaceKind::Wifi);
        a.dns[0] = dns("192.168.1.1", ResolverRole::Configured, &[Some(60.0); 6]);
        let d = compare(&b, &a)
            .metrics
            .into_iter()
            .find(|d| d.key == "dns")
            .unwrap();
        assert_eq!(
            (d.before, d.after, d.change),
            (Some(12.0), Some(60.0), Change::Worse)
        );
    }
}
