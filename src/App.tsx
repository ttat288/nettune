import { useCallback, useEffect, useState } from "react";
import { api, errorText } from "./api";
import type { Comparison, DiagnosticConfig, DiagnosticReport, NetInterface, PowerSaveState } from "./types";
import { InterfacePanel } from "./components/InterfacePanel";
import { Findings, Summary } from "./components/Results";
import { Recommendations } from "./components/Recommendations";
import { AppliedChange } from "./components/AppliedChange";

type Message = { kind: "ok" | "warn" | "error"; text: string } | null;

const SAMPLE_OPTIONS = [10, 20, 50];
const TIMEOUT_OPTIONS = [1000, 2000, 3000];

export default function App() {
  const [interfaces, setInterfaces] = useState<NetInterface[]>([]);
  const [detectError, setDetectError] = useState<string | null>(null);
  const [selectedName, setSelectedName] = useState<string | null>(null);
  const [config, setConfig] = useState<DiagnosticConfig>({ samples: 20, timeoutMs: 1000 });
  const [estimate, setEstimate] = useState<number | null>(null);
  const [running, setRunning] = useState(false);
  const [changing, setChanging] = useState<"apply" | "revert" | null>(null);
  const [report, setReport] = useState<DiagnosticReport | null>(null);
  const [runError, setRunError] = useState<string | null>(null);
  const [powerSave, setPowerSave] = useState<PowerSaveState | null>(null);
  const [baseline, setBaseline] = useState<DiagnosticReport | null>(null);
  const [comparison, setComparison] = useState<Comparison | null>(null);
  const [message, setMessage] = useState<Message>(null);

  const busy = running || changing !== null;
  const selected = interfaces.find((i) => i.name === selectedName) ?? null;

  const refreshPowerSave = useCallback(async (iface: NetInterface | null) => {
    if (!iface || iface.kind !== "wifi") {
      setPowerSave(null);
      return;
    }
    try {
      setPowerSave(await api.powerSaveState(iface.name));
    } catch {
      setPowerSave(null);
    }
  }, []);

  const loadInterfaces = useCallback(async () => {
    try {
      const list = await api.listInterfaces();
      setInterfaces(list);
      setDetectError(null);
      setSelectedName((cur) => (cur && list.some((i) => i.name === cur) ? cur : (list[0]?.name ?? null)));
      return list;
    } catch (e) {
      setDetectError(errorText(e));
      return [];
    }
  }, []);

  useEffect(() => {
    void loadInterfaces();
  }, [loadInterfaces]);

  useEffect(() => {
    void refreshPowerSave(selected);
    // Only when the selection changes, not on every interface refresh.
  }, [selectedName]);

  useEffect(() => {
    api.estimateDuration(config).then(setEstimate, () => setEstimate(null));
  }, [config]);

  function selectInterface(name: string) {
    setSelectedName(name);
    setReport(null);
    setRunError(null);
    setComparison(null);
    setBaseline(null);
    setMessage(null);
  }

  async function runDiagnostics() {
    if (!selected) return;
    setRunning(true);
    setRunError(null);
    setMessage(null);
    try {
      const r = await api.runDiagnostics(selected.name, config);
      setReport(r);
      setInterfaces((list) => list.map((i) => (i.name === r.interface.name ? r.interface : i)));
      if (r.powerSave) setPowerSave(r.powerSave);
      if (baseline && baseline.interface.name === r.interface.name && r.powerSave?.backup) {
        setComparison(await api.compareReports(baseline, r));
      }
    } catch (e) {
      setRunError(errorText(e));
    } finally {
      setRunning(false);
    }
  }

  async function applyPowerSave() {
    if (!selected || !report) return;
    setChanging("apply");
    setMessage(null);
    setComparison(null);
    try {
      const res = await api.applyPowerSave(selected.name);
      setPowerSave(res.state);
      setBaseline(res.state.backup ? report : null);
      setMessage({ kind: res.ok ? "ok" : "warn", text: res.message });
    } catch (e) {
      setBaseline(null);
      setMessage({ kind: "error", text: errorText(e) });
      await refreshPowerSave(selected);
    } finally {
      setChanging(null);
      await loadInterfaces();
    }
  }

  async function revertPowerSave() {
    if (!selected) return;
    setChanging("revert");
    setMessage(null);
    try {
      const res = await api.revertPowerSave(selected.name);
      setPowerSave(res.state);
      setBaseline(null);
      setComparison(null);
      setMessage({
        kind: res.ok ? "ok" : "warn",
        text: `${res.message}${res.ok ? " Run diagnostics again to see current results." : ""}`,
      });
    } catch (e) {
      setMessage({ kind: "error", text: errorText(e) });
      await refreshPowerSave(selected);
    } finally {
      setChanging(null);
      await loadInterfaces();
    }
  }

  function keepChange() {
    setBaseline(null);
    setComparison(null);
    setMessage({ kind: "ok", text: "Change kept. You can still revert it later from this window." });
  }

  const statusText = running
    ? `Running diagnostics${estimate ? ` (about ${estimate} s)` : ""}…`
    : changing === "apply"
      ? "Applying change: waiting for NetworkManager. Your system may ask for a password; the Wi-Fi reconnects briefly."
      : changing === "revert"
        ? "Reverting change: waiting for NetworkManager…"
        : "";

  return (
    <main>
      <header className="app-head">
        <h1>NetTune</h1>
        <p className="muted small">Local network diagnostics. Nothing leaves this computer except the test probes.</p>
      </header>

      {detectError && (
        <p className="msg msg-error" role="alert">
          Could not detect network interfaces: {detectError}
        </p>
      )}

      <InterfacePanel
        interfaces={interfaces}
        selected={selected}
        onSelect={selectInterface}
        onRefresh={() => void loadInterfaces()}
        disabled={busy}
      />

      <section className="panel run" aria-label="Run diagnostics">
        <button
          type="button"
          className="btn primary large"
          onClick={() => void runDiagnostics()}
          disabled={busy || !selected}
        >
          {running ? "Running…" : report ? "Run again" : "Run diagnostics"}
        </button>
        <label>
          Samples
          <select
            value={config.samples}
            disabled={busy}
            onChange={(e) => setConfig({ ...config, samples: Number(e.target.value) })}
          >
            {SAMPLE_OPTIONS.map((n) => (
              <option key={n} value={n}>
                {n} per target
              </option>
            ))}
          </select>
        </label>
        <label>
          Timeout
          <select
            value={config.timeoutMs}
            disabled={busy}
            onChange={(e) => setConfig({ ...config, timeoutMs: Number(e.target.value) })}
          >
            {TIMEOUT_OPTIONS.map((t) => (
              <option key={t} value={t}>
                {t / 1000} s
              </option>
            ))}
          </select>
        </label>
        <span className="muted small" role="status" aria-live="polite">
          {statusText}
        </span>
      </section>

      {message && (
        <p className={`msg msg-${message.kind}`} role={message.kind === "error" ? "alert" : "status"}>
          {message.text}
        </p>
      )}
      {runError && (
        <p className="msg msg-error" role="alert">
          Diagnostics failed: {runError}
        </p>
      )}

      {powerSave?.backup && selected && powerSave.backup.interface === selected.name && (
        <AppliedChange
          backup={powerSave.backup}
          hasBaseline={baseline !== null}
          comparison={comparison}
          busy={busy}
          onKeep={keepChange}
          onRevert={() => void revertPowerSave()}
        />
      )}

      {report && (
        <>
          <Summary report={report} />
          <Findings findings={report.analysis.findings} />
          <Recommendations
            recommendations={report.analysis.recommendations}
            powerSave={powerSave}
            busy={busy}
            onApplyPowerSave={() => void applyPowerSave()}
          />
        </>
      )}

      {!report && !running && selected && (
        <p className="muted empty">
          Run diagnostics to measure latency, jitter, packet loss and DNS on {selected.name}. Tests send small ping and
          DNS requests to your router and to public servers (Cloudflare, Google, Quad9).
        </p>
      )}
    </main>
  );
}
