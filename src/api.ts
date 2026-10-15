import { invoke } from "@tauri-apps/api/core";
import type {
  ChangeResult,
  Comparison,
  DiagnosticConfig,
  DiagnosticReport,
  NetInterface,
  PowerSaveState,
} from "./types";

/** Backend errors arrive as plain strings; normalise anything else. */
export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}

export const api = {
  listInterfaces: () => invoke<NetInterface[]>("list_interfaces"),
  estimateDuration: (config: DiagnosticConfig) => invoke<number>("estimate_duration", { config }),
  runDiagnostics: (iface: string, config: DiagnosticConfig) =>
    invoke<DiagnosticReport>("run_diagnostics", { interface: iface, config }),
  powerSaveState: (iface: string) => invoke<PowerSaveState>("power_save_state", { interface: iface }),
  applyPowerSave: (iface: string) => invoke<ChangeResult>("apply_power_save", { interface: iface }),
  revertPowerSave: (iface: string) => invoke<ChangeResult>("revert_power_save", { interface: iface }),
  compareReports: (before: DiagnosticReport, after: DiagnosticReport) =>
    invoke<Comparison>("compare_reports", { before, after }),
};
