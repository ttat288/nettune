import { useState } from "react";
import type { Comparison, PowerSaveBackup } from "../types";
import { fmtDateTime, fmtValue } from "../format";

const changeLabel = {
  better: "Better",
  worse: "Worse",
  noClearChange: "No clear change",
  unavailable: "Not available",
};

interface Props {
  backup: PowerSaveBackup;
  hasBaseline: boolean;
  comparison: Comparison | null;
  busy: boolean;
  onKeep: () => void;
  onRevert: () => void;
}

export function AppliedChange({ backup, hasBaseline, comparison, busy, onKeep, onRevert }: Props) {
  const [confirmRevert, setConfirmRevert] = useState(false);
  return (
    <section className="panel applied" aria-labelledby="applied-h">
      <h2 id="applied-h">Applied change</h2>
      <p>
        Wi-Fi power saving set to <strong>{backup.appliedValue}</strong> on {backup.interface} (was{" "}
        <strong>{backup.previousValue}</strong>) · {fmtDateTime(backup.appliedAtUnix)}
      </p>

      {comparison ? (
        <>
          <div className="table-wrap">
            <table>
              <caption className="sr-only">Before and after comparison</caption>
              <thead>
                <tr>
                  <th scope="col">Metric</th>
                  <th scope="col">Before</th>
                  <th scope="col">After</th>
                  <th scope="col">Result</th>
                </tr>
              </thead>
              <tbody>
                {comparison.metrics.map((m) => (
                  <tr key={m.key}>
                    <th scope="row">{m.label}</th>
                    <td>{fmtValue(m.before, m.unit)}</td>
                    <td>{fmtValue(m.after, m.unit)}</td>
                    <td className={`chg chg-${m.change}`}>{changeLabel[m.change]}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <p>
            <strong>{comparison.summary}</strong>
          </p>
          {comparison.warnings.map((w) => (
            <p key={w} className="warn-text small">
              {w}
            </p>
          ))}
          <p className="muted small">
            Two short runs can differ by chance. If the result is unclear, run diagnostics again a few times before
            deciding.
          </p>
        </>
      ) : (
        hasBaseline && (
          <p className="muted">Run diagnostics again to compare with the measurement taken before the change.</p>
        )
      )}

      {!confirmRevert ? (
        <div className="inline">
          {comparison && (
            <button type="button" className="btn" onClick={onKeep} disabled={busy}>
              Keep change
            </button>
          )}
          <button type="button" className="btn" onClick={() => setConfirmRevert(true)} disabled={busy}>
            Revert…
          </button>
        </div>
      ) : (
        <div className="confirm" role="group" aria-label="Confirm revert">
          <p>
            Restore power saving to <strong>{backup.previousValue}</strong>? The Wi-Fi connection is re-activated (short
            disconnect).
          </p>
          <div className="inline">
            <button
              type="button"
              className="btn primary"
              disabled={busy}
              onClick={() => {
                setConfirmRevert(false);
                onRevert();
              }}
            >
              Revert change
            </button>
            <button type="button" className="btn" onClick={() => setConfirmRevert(false)} disabled={busy}>
              Cancel
            </button>
          </div>
        </div>
      )}
    </section>
  );
}
