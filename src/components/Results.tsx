import type { DiagnosticReport, DnsResult, Finding, LatencyStats } from "../types";
import { NA, fmtMs, fmtPct, fmtTime } from "../format";

const overallLabel = { healthy: "No problems found", degraded: "Degraded", failing: "Connectivity problem" };
const sevLabel = { info: "Info", warning: "Warning", critical: "Critical" };
const sevGlyph = { info: "i", warning: "!", critical: "×" };

function lossText(s: LatencyStats | null | undefined): string {
  if (!s) return NA;
  if (s.error) return NA;
  return `${fmtPct(s.lossPct)} (${s.received}/${s.sent})`;
}

function bestInternet(r: DiagnosticReport): LatencyStats | null {
  const ok = r.internet.filter((s) => !s.error && s.received > 0);
  const icmp = ok.filter((s) => s.method === "icmp");
  const pool = icmp.length > 0 ? icmp : ok;
  return pool.reduce<LatencyStats | null>(
    (best, s) => (best === null || (s.medianMs ?? Infinity) < (best.medianMs ?? Infinity) ? s : best),
    null,
  );
}

function primaryDns(r: DiagnosticReport): DnsResult | null {
  return (
    r.dns.find((d) => d.role === "configured" && d.medianMs !== null) ??
    r.dns.find((d) => d.role === "systemStub" && d.medianMs !== null) ??
    r.dns.find((d) => d.role !== "public") ??
    null
  );
}

export function Summary({ report }: { report: DiagnosticReport }) {
  const a = report.analysis;
  const gw = report.gateway;
  const net = bestInternet(report);
  const dns = primaryDns(report);
  const connected = report.interface.connected;
  return (
    <section className="panel" aria-labelledby="sum-h">
      <div className={`status status-${a.overall}`}>
        <h2 id="sum-h">{overallLabel[a.overall]}</h2>
        <p>{a.summary}</p>
        <p className="muted small">
          Measured at {fmtTime(report.startedAtUnix)} in {(report.durationMs / 1000).toFixed(1)} s ·{" "}
          {report.config.samples} samples per target. A short test is a snapshot; conditions fluctuate.
        </p>
      </div>

      {connected && (
        <div className="table-wrap">
          <table className="summary">
            <thead>
              <tr>
                <th scope="col">Path</th>
                <th scope="col">Latency (median)</th>
                <th scope="col">Jitter</th>
                <th scope="col">Packet loss</th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <th scope="row">
                  Router <span className="muted">{gw?.target ?? ""}</span>
                </th>
                <td>{fmtMs(gw?.medianMs)}</td>
                <td>{fmtMs(gw?.jitterMs)}</td>
                <td>{lossText(gw)}</td>
              </tr>
              <tr>
                <th scope="row">
                  Internet <span className="muted">{net ? `${net.label}${net.method === "tcp" ? ", TCP" : ""}` : ""}</span>
                </th>
                <td>{fmtMs(net?.medianMs)}</td>
                <td>{fmtMs(net?.jitterMs)}</td>
                <td>{net ? lossText(net) : report.internet.length ? "100%" : NA}</td>
              </tr>
              <tr>
                <th scope="row">
                  DNS <span className="muted">{dns ? dns.label : ""}</span>
                </th>
                <td>{fmtMs(dns?.medianMs)}</td>
                <td className="na">{NA}</td>
                <td>{dns ? `${dns.failures}/${dns.samples.length} failed` : NA}</td>
              </tr>
            </tbody>
          </table>
        </div>
      )}

      {connected && <Details report={report} />}

      {report.notes.length > 0 && (
        <ul className="notes muted small">
          {report.notes.map((n) => (
            <li key={n}>{n}</li>
          ))}
        </ul>
      )}
    </section>
  );
}

function Details({ report }: { report: DiagnosticReport }) {
  const rows = [report.gateway, ...report.internet].filter((s): s is LatencyStats => s !== null);
  return (
    <details className="more">
      <summary>All measurements</summary>
      <div className="table-wrap">
        <table>
          <caption className="sr-only">Latency per target</caption>
          <thead>
            <tr>
              <th scope="col">Target</th>
              <th scope="col">Method</th>
              <th scope="col">Min</th>
              <th scope="col">Median</th>
              <th scope="col">Max</th>
              <th scope="col">Jitter</th>
              <th scope="col">Loss</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((s) => (
              <tr key={`${s.target}-${s.method}`}>
                <th scope="row">
                  {s.label} <span className="muted">{s.target}</span>
                </th>
                <td>{s.method === "icmp" ? "ping" : "TCP 443"}</td>
                {s.error ? (
                  <td colSpan={5} className="muted">
                    Not measured: {s.error}
                  </td>
                ) : (
                  <>
                    <td>{fmtMs(s.minMs)}</td>
                    <td>{fmtMs(s.medianMs)}</td>
                    <td>{fmtMs(s.maxMs)}</td>
                    <td>{fmtMs(s.jitterMs)}</td>
                    <td>{lossText(s)}</td>
                  </>
                )}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {report.dns.length > 0 && (
        <div className="table-wrap">
          <table>
            <caption className="sr-only">DNS lookup time per resolver</caption>
            <thead>
              <tr>
                <th scope="col">DNS resolver</th>
                <th scope="col">Role</th>
                <th scope="col">Median</th>
                <th scope="col">Max</th>
                <th scope="col">Failed</th>
              </tr>
            </thead>
            <tbody>
              {report.dns.map((d) => (
                <tr key={d.resolver}>
                  <th scope="row">
                    {d.label} <span className="muted">{d.resolver}</span>
                  </th>
                  <td>{d.role === "configured" ? "Your network" : d.role === "systemStub" ? "Local cache" : "Comparison"}</td>
                  <td>{fmtMs(d.medianMs)}</td>
                  <td>{fmtMs(d.maxMs)}</td>
                  <td>
                    {d.failures}/{d.samples.length}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      <p className="muted small">
        Jitter is the mean difference between consecutive round trips. DNS uses {report.dns[0]?.samples.length ?? 0}{" "}
        popular names per resolver; the local cache answers repeated names instantly.
      </p>
    </details>
  );
}

export function Findings({ findings }: { findings: Finding[] }) {
  if (findings.length === 0) return null;
  return (
    <section className="panel" aria-labelledby="find-h">
      <h2 id="find-h">Diagnosis</h2>
      <ul className="list">
        {findings.map((f) => (
          <li key={f.id}>
            <details className={`finding sev-${f.severity}`} open={f.severity !== "info"}>
              <summary>
                <span className={`sev sev-${f.severity}`}>
                  <span aria-hidden="true" className="glyph">
                    {sevGlyph[f.severity]}
                  </span>
                  {sevLabel[f.severity]}
                </span>
                <span className="title">{f.title}</span>
              </summary>
              <div className="body">
                <h3 className="label">Observed</h3>
                <ul className="evidence">
                  {f.evidence.map((e) => (
                    <li key={e}>{e}</li>
                  ))}
                </ul>
                <p>{f.explanation}</p>
                <p className="muted small">
                  Confidence: {f.confidence}
                  {f.limitations ? ` · ${f.limitations}` : ""}
                </p>
                <p>
                  <strong>Next:</strong> {f.nextAction}
                </p>
              </div>
            </details>
          </li>
        ))}
      </ul>
    </section>
  );
}
