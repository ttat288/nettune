// Mirrors src-tauri/src/models.rs and compare.rs (serde camelCase). `null` = not available.

export type InterfaceKind = "ethernet" | "wifi" | "loopback" | "virtual" | "other";

export interface WifiInfo {
  ssid: string | null;
  signalDbm: number | null;
  signalPercent: number | null;
  freqMhz: number | null;
  rxBitrateMbps: number | null;
  txBitrateMbps: number | null;
  powerSave: boolean | null;
}

export interface NetInterface {
  name: string;
  kind: InterfaceKind;
  state: string;
  connected: boolean;
  ipv4: string[];
  ipv6: string[];
  mac: string | null;
  mtu: number | null;
  driver: string | null;
  speedMbps: number | null;
  duplex: string | null;
  autoneg: boolean | null;
  gateway: string | null;
  isDefaultRoute: boolean;
  nmManaged: boolean;
  connectionName: string | null;
  connectionUuid: string | null;
  dnsServers: string[];
  wifi: WifiInfo | null;
}

export interface DiagnosticConfig {
  samples: number;
  timeoutMs: number;
}

export interface LatencyStats {
  target: string;
  label: string;
  role: "gateway" | "internet";
  method: "icmp" | "tcp";
  sent: number;
  received: number;
  rttsMs: number[];
  lossPct: number | null;
  minMs: number | null;
  avgMs: number | null;
  medianMs: number | null;
  maxMs: number | null;
  jitterMs: number | null;
  error: string | null;
}

export interface DnsSample {
  name: string;
  ms: number | null;
  error: string | null;
}

export interface DnsResult {
  resolver: string;
  label: string;
  role: "configured" | "systemStub" | "public";
  samples: DnsSample[];
  medianMs: number | null;
  maxMs: number | null;
  failures: number;
}

export interface PowerSaveBackup {
  interface: string;
  connectionUuid: string;
  previousValue: string;
  appliedValue: string;
  appliedAtUnix: number;
}

export interface PowerSaveState {
  supported: boolean;
  unsupportedReason: string | null;
  profileValue: string | null;
  runtimeOn: boolean | null;
  connectionUuid: string | null;
  auth: string | null;
  backup: PowerSaveBackup | null;
}

export type Severity = "info" | "warning" | "critical";
export type Confidence = "low" | "medium" | "high";
export type OverallState = "healthy" | "degraded" | "failing";

export interface Finding {
  id: string;
  severity: Severity;
  title: string;
  evidence: string[];
  explanation: string;
  confidence: Confidence;
  limitations: string;
  nextAction: string;
}

export type RecommendationAction =
  | { type: "wifiPowerSave"; current: string; proposed: string; sideEffects: string[] }
  | { type: "manual"; steps: string[] };

export interface Recommendation {
  id: string;
  title: string;
  rationale: string;
  relatedFindings: string[];
  action: RecommendationAction;
}

export interface Analysis {
  overall: OverallState;
  summary: string;
  findings: Finding[];
  recommendations: Recommendation[];
}

export interface DiagnosticReport {
  startedAtUnix: number;
  durationMs: number;
  config: DiagnosticConfig;
  interface: NetInterface;
  gateway: LatencyStats | null;
  gatewayNeighbor: string | null;
  internet: LatencyStats[];
  dns: DnsResult[];
  dnsOverTls: boolean;
  powerSave: PowerSaveState | null;
  notes: string[];
  analysis: Analysis;
}

export interface ChangeResult {
  ok: boolean;
  message: string;
  state: PowerSaveState;
}

export type Change = "better" | "worse" | "noClearChange" | "unavailable";

export interface MetricDelta {
  key: string;
  label: string;
  unit: string;
  before: number | null;
  after: number | null;
  change: Change;
}

export interface Comparison {
  metrics: MetricDelta[];
  warnings: string[];
  summary: string;
}
