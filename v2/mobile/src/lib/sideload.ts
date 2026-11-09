// Pure presentation helpers for Install APK and Upload to TV. No I/O.

import type { ApkSetInspection } from "./types";

export function formatBytes(n: number): string {
  if (!Number.isFinite(n) || n < 1024) return `${Math.max(0, Math.round(n || 0))} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

/// The headline for an install the TV did not confirm, the same reading desktop's Install APK
/// gives. The package manager's own text is shown below it.
export function installFailureSummary(raw: string): string {
  const m = raw.match(/INSTALL_FAILED_[A-Z_]+|INSTALL_PARSE_FAILED[A-Z_]*/);
  if (m) {
    if (m[0].includes("ALREADY_EXISTS")) return "Already installed (same version).";
    if (m[0].includes("VERSION_DOWNGRADE")) return "A newer version is already installed.";
    if (m[0].includes("NO_MATCHING_ABIS")) return "Wrong CPU architecture for this TV.";
    if (m[0].includes("OLDER_SDK")) return "Needs a newer Android version than this TV.";
    if (m[0].includes("UPDATE_INCOMPATIBLE")) return "The installed copy is signed differently.";
    return `Install failed (${m[0]}).`;
  }
  return "Installation not confirmed. Check the TV's apps before retrying.";
}

export type AbiVerdict = "mismatch" | "unknown" | "ok";

/// Only claim a mismatch the backend established; unreadable sides are unknown, never a match.
export function abiVerdict(inspection: ApkSetInspection): AbiVerdict {
  if (inspection.abi_compatible === false) return "mismatch";
  if (inspection.abi_compatible === null) return "unknown";
  return "ok";
}

export function nativeCodeLabel(inspection: ApkSetInspection): string {
  if (!inspection.abis_known) return "couldn't be read";
  return inspection.abis.length === 0 ? "none (runs on any architecture)" : inspection.abis.join(", ");
}

export function deviceAbiLabel(inspection: ApkSetInspection): string {
  return inspection.device_abis.length ? inspection.device_abis.join(", ") : "not reported";
}

export function plural(n: number, one: string, many = `${one}s`): string {
  return `${n} ${n === 1 ? one : many}`;
}
