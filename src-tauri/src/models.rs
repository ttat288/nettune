//! Measurement and analysis data model shared with the UI (serialized as camelCase JSON).
//!
//! Convention: `Option::None` always means "not available / not measured", never zero.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InterfaceKind {
    Ethernet,
    Wifi,
    Loopback,
    Virtual,
    Other,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WifiInfo {
    pub ssid: Option<String>,
    pub signal_dbm: Option<i32>,
    /// NetworkManager's 0–100 signal estimate.
    pub signal_percent: Option<u8>,
    pub freq_mhz: Option<u32>,
    pub rx_bitrate_mbps: Option<f64>,
    pub tx_bitrate_mbps: Option<f64>,
    /// Runtime power-save state as reported by `iw` (true = on).
    pub power_save: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetInterface {
    pub name: String,
    pub kind: InterfaceKind,
    /// NetworkManager state (e.g. "connected", "disconnected", "unavailable") or kernel operstate.
    pub state: String,
    pub connected: bool,
    pub ipv4: Vec<String>,
    pub ipv6: Vec<String>,
    pub mac: Option<String>,
    pub mtu: Option<u32>,
    pub driver: Option<String>,
    pub speed_mbps: Option<u32>,
    pub duplex: Option<String>,
    pub autoneg: Option<bool>,
    pub gateway: Option<String>,
    pub is_default_route: bool,
    pub nm_managed: bool,
    pub connection_name: Option<String>,
    pub connection_uuid: Option<String>,
    pub dns_servers: Vec<String>,
    pub wifi: Option<WifiInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticConfig {
    /// Echo requests per target (clamped to 5..=50).
    pub samples: u32,
    /// Per-reply / per-query timeout in milliseconds (clamped to 500..=5000).
    pub timeout_ms: u32,
}

impl Default for DiagnosticConfig {
    fn default() -> Self {
        Self {
            samples: 20,
            timeout_ms: 1000,
        }
    }
}

impl DiagnosticConfig {
    pub fn clamped(self) -> Self {
        Self {
            samples: self.samples.clamp(5, 50),
            timeout_ms: self.timeout_ms.clamp(500, 5000),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TargetRole {
    Gateway,
    Internet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProbeMethod {
    Icmp,
    /// TCP handshake timing, used only when ICMP gets no replies.
    Tcp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LatencyStats {
    pub target: String,
    pub label: String,
    pub role: TargetRole,
    pub method: ProbeMethod,
    pub sent: u32,
    pub received: u32,
    /// Raw round-trip samples, in order.
    pub rtts_ms: Vec<f64>,
    pub loss_pct: Option<f64>,
    pub min_ms: Option<f64>,
    pub avg_ms: Option<f64>,
    pub median_ms: Option<f64>,
    pub max_ms: Option<f64>,
    /// Mean absolute difference between consecutive samples (RFC 3550-style, unsmoothed).
    pub jitter_ms: Option<f64>,
    /// Set when the probe itself could not run (missing tool, permission, timeout...).
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DnsSample {
    pub name: String,
    pub ms: Option<f64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResolverRole {
    /// Resolver configured for the interface (via DHCP / NetworkManager).
    Configured,
    /// Local systemd-resolved stub (includes its cache and any DNS-over-TLS upstream).
    SystemStub,
    /// Public resolver used for comparison only.
    Public,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DnsResult {
    pub resolver: String,
    pub label: String,
    pub role: ResolverRole,
    pub samples: Vec<DnsSample>,
    pub median_ms: Option<f64>,
    pub max_ms: Option<f64>,
    pub failures: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PowerSaveState {
    /// True when NetTune can change this setting automatically on this interface.
    pub supported: bool,
    /// Why automatic changes are unavailable (when `supported` is false).
    pub unsupported_reason: Option<String>,
    /// Value stored in the NetworkManager profile: default | ignore | disable | enable.
    pub profile_value: Option<String>,
    /// Runtime state reported by the driver via `iw`.
    pub runtime_on: Option<bool>,
    pub connection_uuid: Option<String>,
    /// polkit result for modifying the profile: "yes" or "auth" (password prompt).
    pub auth: Option<String>,
    /// A NetTune-made change is recorded and can be reverted.
    pub backup: Option<PowerSaveBackup>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PowerSaveBackup {
    pub interface: String,
    pub connection_uuid: String,
    pub previous_value: String,
    pub applied_value: String,
    pub applied_at_unix: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticReport {
    pub started_at_unix: u64,
    pub duration_ms: u64,
    pub config: DiagnosticConfig,
    pub interface: NetInterface,
    pub gateway: Option<LatencyStats>,
    /// Kernel neighbor-table state for the gateway (REACHABLE, STALE, FAILED...), when checked.
    pub gateway_neighbor: Option<String>,
    pub internet: Vec<LatencyStats>,
    pub dns: Vec<DnsResult>,
    pub dns_over_tls: bool,
    pub power_save: Option<PowerSaveState>,
    /// Non-fatal problems encountered while measuring.
    pub notes: Vec<String>,
    pub analysis: Analysis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OverallState {
    /// No problem detected by the rules.
    Healthy,
    /// Something degraded but connectivity works.
    Degraded,
    /// Connectivity is broken.
    Failing,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub id: String,
    pub severity: Severity,
    pub title: String,
    pub evidence: Vec<String>,
    pub explanation: String,
    pub confidence: Confidence,
    pub limitations: String,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "type"
)]
pub enum RecommendationAction {
    /// Applied by NetTune with backup and rollback.
    WifiPowerSave {
        current: String,
        proposed: String,
        side_effects: Vec<String>,
    },
    /// User follows the steps themselves.
    Manual { steps: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recommendation {
    pub id: String,
    pub title: String,
    pub rationale: String,
    /// Finding ids this recommendation responds to.
    pub related_findings: Vec<String>,
    pub action: RecommendationAction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Analysis {
    pub overall: OverallState,
    pub summary: String,
    pub findings: Vec<Finding>,
    pub recommendations: Vec<Recommendation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeResult {
    pub ok: bool,
    pub message: String,
    pub state: PowerSaveState,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The UI depends on these exact JSON keys (see src/types.ts).
    #[test]
    fn json_shape_matches_frontend_types() {
        let a = RecommendationAction::WifiPowerSave {
            current: "default".into(),
            proposed: "disable".into(),
            side_effects: vec!["x".into()],
        };
        let v = serde_json::to_value(&a).unwrap();
        assert_eq!(v["type"], "wifiPowerSave");
        assert_eq!(v["sideEffects"][0], "x");
        let m = serde_json::to_value(RecommendationAction::Manual { steps: vec![] }).unwrap();
        assert_eq!(m["type"], "manual");
        assert!(m["steps"].is_array());
        let c = serde_json::to_value(DiagnosticConfig::default()).unwrap();
        assert_eq!(c["timeoutMs"], 1000);
        assert_eq!(serde_json::to_value(InterfaceKind::Wifi).unwrap(), "wifi");
        assert_eq!(
            serde_json::to_value(ResolverRole::SystemStub).unwrap(),
            "systemStub"
        );
    }
}
