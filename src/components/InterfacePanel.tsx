import type { NetInterface } from "../types";
import { NA, band, kindLabel } from "../format";

interface Props {
  interfaces: NetInterface[];
  selected: NetInterface | null;
  onSelect: (name: string) => void;
  onRefresh: () => void;
  disabled: boolean;
}

function Row({ label, value, title }: { label: string; value: string; title?: string }) {
  return (
    <div className="row">
      <dt>{label}</dt>
      <dd className={value === NA ? "na" : undefined} title={value === NA ? (title ?? "Not reported") : undefined}>
        {value}
      </dd>
    </div>
  );
}

function optionLabel(i: NetInterface): string {
  const parts = [kindLabel[i.kind], i.state];
  if (i.isDefaultRoute) parts.push("default route");
  return `${i.name} — ${parts.join(" · ")}`;
}

function linkRate(i: NetInterface): string {
  if (i.kind === "wifi" && i.wifi) {
    const { rxBitrateMbps: rx, txBitrateMbps: tx } = i.wifi;
    if (rx === null && tx === null) return NA;
    return `↓ ${rx ?? NA} / ↑ ${tx ?? NA} Mb/s`;
  }
  if (i.speedMbps === null) return NA;
  return `${i.speedMbps} Mb/s${i.duplex ? `, ${i.duplex} duplex` : ""}`;
}

function signal(i: NetInterface): string {
  const w = i.wifi;
  if (!w || (w.signalDbm === null && w.signalPercent === null)) return NA;
  const parts = [];
  if (w.signalDbm !== null) parts.push(`${w.signalDbm} dBm`);
  if (w.signalPercent !== null) parts.push(`${w.signalPercent}%`);
  return parts.join(" · ");
}

export function InterfacePanel({ interfaces, selected, onSelect, onRefresh, disabled }: Props) {
  return (
    <section className="panel" aria-labelledby="iface-h">
      <div className="panel-head">
        <h2 id="iface-h">Network</h2>
        <div className="inline">
          <label htmlFor="iface-select" className="sr-only">
            Interface
          </label>
          <select
            id="iface-select"
            value={selected?.name ?? ""}
            onChange={(e) => onSelect(e.target.value)}
            disabled={disabled || interfaces.length === 0}
          >
            {interfaces.map((i) => (
              <option key={i.name} value={i.name}>
                {optionLabel(i)}
              </option>
            ))}
          </select>
          <button type="button" className="btn" onClick={onRefresh} disabled={disabled}>
            Refresh
          </button>
        </div>
      </div>

      {selected && (
        <>
          <dl className="metrics cols">
            <Row label="Type" value={kindLabel[selected.kind]} />
            <Row label="Status" value={selected.connected ? "Connected" : `Not connected (${selected.state})`} />
            {selected.kind === "wifi" && <Row label="Network" value={selected.wifi?.ssid ?? selected.connectionName ?? NA} />}
            <Row label="IPv4" value={selected.ipv4.join(", ") || NA} title="No IPv4 address" />
            <Row label="Gateway" value={selected.gateway ?? NA} title="No default gateway on this interface" />
            <Row label={selected.kind === "wifi" ? "Bit rate" : "Link speed"} value={linkRate(selected)} title="Not reported by the driver" />
            {selected.kind === "wifi" && <Row label="Signal" value={signal(selected)} />}
            {selected.kind === "wifi" && <Row label="Band" value={band(selected.wifi?.freqMhz ?? null)} />}
          </dl>
          <details className="more">
            <summary>Adapter details</summary>
            <dl className="metrics cols">
              <Row label="Driver" value={selected.driver ?? NA} />
              <Row label="MAC" value={selected.mac ?? NA} />
              <Row label="MTU" value={selected.mtu?.toString() ?? NA} />
              <Row label="IPv6" value={selected.ipv6.join(", ") || NA} title="No global IPv6 address" />
              <Row label="DNS servers" value={selected.dnsServers.join(", ") || NA} />
              <Row label="Managed by" value={selected.nmManaged ? "NetworkManager" : "Not NetworkManager"} />
              {selected.kind === "ethernet" && (
                <Row label="Auto-negotiation" value={selected.autoneg === null ? NA : selected.autoneg ? "On" : "Off"} />
              )}
              {selected.kind === "wifi" && (
                <Row
                  label="Power saving"
                  value={selected.wifi?.powerSave === null || !selected.wifi ? NA : selected.wifi.powerSave ? "On" : "Off"}
                  title="Driver does not report power-save state"
                />
              )}
            </dl>
          </details>
        </>
      )}
    </section>
  );
}
