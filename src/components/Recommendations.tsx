import { useState } from "react";
import type { PowerSaveState, Recommendation } from "../types";

async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    // WebKitGTK may refuse the async clipboard API; fall back to a selection copy.
    const ta = document.createElement("textarea");
    ta.value = text;
    ta.setAttribute("readonly", "");
    ta.style.position = "fixed";
    ta.style.opacity = "0";
    document.body.appendChild(ta);
    ta.select();
    let ok = false;
    try {
      ok = document.execCommand("copy");
    } catch {
      ok = false;
    }
    ta.remove();
    return ok;
  }
}

function Steps({ steps }: { steps: string[] }) {
  const [copied, setCopied] = useState<"idle" | "ok" | "fail">("idle");
  const commands = steps.filter((s) => /^(nmcli|ethtool|ip|iw)\s/.test(s));
  return (
    <>
      <ol className="steps">
        {steps.map((s) =>
          /^(nmcli|ethtool|ip|iw)\s/.test(s) ? (
            <li key={s}>
              <code>{s}</code>
            </li>
          ) : (
            <li key={s}>{s}</li>
          ),
        )}
      </ol>
      {commands.length > 0 && (
        <div className="inline">
          <button
            type="button"
            className="btn small"
            onClick={async () => setCopied((await copyText(commands.join("\n"))) ? "ok" : "fail")}
          >
            Copy commands
          </button>
          <span role="status" className="muted small">
            {copied === "ok" ? "Copied." : copied === "fail" ? "Copy failed; select the text manually." : ""}
          </span>
        </div>
      )}
    </>
  );
}

interface PowerSaveProps {
  rec: Recommendation & { action: { type: "wifiPowerSave" } };
  state: PowerSaveState | null;
  busy: boolean;
  onApply: () => void;
}

function PowerSaveCard({ rec, state, busy, onApply }: PowerSaveProps) {
  const [confirming, setConfirming] = useState(false);
  const a = rec.action;
  if (state?.backup) {
    return <p className="muted small">This change has been applied. See “Applied change” above to compare, keep or revert.</p>;
  }
  if (!state?.supported) {
    return <p className="muted small">Automatic change unavailable: {state?.unsupportedReason ?? "state unknown"}.</p>;
  }
  return (
    <>
      <dl className="metrics change">
        <div className="row">
          <dt>Setting</dt>
          <dd>
            <code>802-11-wireless.powersave</code> on this connection
          </dd>
        </div>
        <div className="row">
          <dt>Current</dt>
          <dd>
            {a.current}
            {state.runtimeOn !== null && <span className="muted"> (driver: power save {state.runtimeOn ? "on" : "off"})</span>}
          </dd>
        </div>
        <div className="row">
          <dt>Proposed</dt>
          <dd>{a.proposed}</dd>
        </div>
      </dl>
      {!confirming ? (
        <button type="button" className="btn" onClick={() => setConfirming(true)} disabled={busy}>
          Review change…
        </button>
      ) : (
        <div className="confirm" role="group" aria-labelledby={`${rec.id}-confirm`}>
          <p id={`${rec.id}-confirm`}>
            <strong>Before applying:</strong>
          </p>
          <ul>
            {a.sideEffects.map((s) => (
              <li key={s}>{s}</li>
            ))}
            <li>The previous value ({a.current}) is saved so you can revert.</li>
          </ul>
          <div className="inline">
            <button
              type="button"
              className="btn primary"
              disabled={busy}
              onClick={() => {
                setConfirming(false);
                onApply();
              }}
            >
              Disable power saving
            </button>
            <button type="button" className="btn" onClick={() => setConfirming(false)} disabled={busy}>
              Cancel
            </button>
          </div>
        </div>
      )}
    </>
  );
}

interface Props {
  recommendations: Recommendation[];
  powerSave: PowerSaveState | null;
  busy: boolean;
  onApplyPowerSave: () => void;
}

export function Recommendations({ recommendations, powerSave, busy, onApplyPowerSave }: Props) {
  if (recommendations.length === 0) return null;
  return (
    <section className="panel" aria-labelledby="rec-h">
      <h2 id="rec-h">Recommended actions</h2>
      <ul className="list">
        {recommendations.map((r) => (
          <li key={r.id} className="rec">
            <div className="rec-head">
              <h3>{r.title}</h3>
              <span className="tag">{r.action.type === "manual" ? "Manual" : "Reversible"}</span>
            </div>
            <p className="muted">{r.rationale}</p>
            {r.action.type === "manual" ? (
              <Steps steps={r.action.steps} />
            ) : (
              <PowerSaveCard
                rec={r as PowerSaveProps["rec"]}
                state={powerSave}
                busy={busy}
                onApply={onApplyPowerSave}
              />
            )}
          </li>
        ))}
      </ul>
    </section>
  );
}
