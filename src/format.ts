import type { InterfaceKind } from "./types";

/** Placeholder for values that were not measured / not reported. Never rendered as 0. */
export const NA = "—";

export function fmtMs(v: number | null | undefined): string {
  if (v === null || v === undefined) return NA;
  return v < 10 ? `${v.toFixed(1)} ms` : `${Math.round(v)} ms`;
}

export function fmtPct(v: number | null | undefined): string {
  if (v === null || v === undefined) return NA;
  return `${v < 10 && v !== 0 ? v.toFixed(1) : Math.round(v)}%`;
}

export function fmtValue(v: number | null, unit: string): string {
  if (unit === "ms") return fmtMs(v);
  if (unit === "%") return fmtPct(v);
  return v === null ? NA : `${v} ${unit}`;
}

export const kindLabel: Record<InterfaceKind, string> = {
  ethernet: "Ethernet",
  wifi: "Wi-Fi",
  loopback: "Loopback",
  virtual: "Virtual",
  other: "Other",
};

export function band(freqMhz: number | null): string {
  if (freqMhz === null) return NA;
  const b = freqMhz < 3000 ? "2.4 GHz" : freqMhz < 5925 ? "5 GHz" : "6 GHz";
  return `${b} (${freqMhz} MHz)`;
}

export function fmtTime(unix: number): string {
  return new Date(unix * 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
}

export function fmtDateTime(unix: number): string {
  return new Date(unix * 1000).toLocaleString([], { dateStyle: "medium", timeStyle: "short" });
}
