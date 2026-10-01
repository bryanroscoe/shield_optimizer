// What the shared app detail panel needs to say about each figure: whether it
// was read, when, and why not. A figure is only ever shown next to the time
// it was measured, and a failed read is "unavailable", never zero.

import type { AppStorage, AppUsage } from "./types";
import type { AppReportDevice } from "./app-report";

export type MeasureStatus =
  | { status: "idle" }
  | { status: "loading" }
  | { status: "ready"; at: number }
  | { status: "unavailable"; at: number; reason: string };

export interface AppMeasurements {
  /// `dumpsys meminfo` — Total PSS by process.
  memory: MeasureStatus;
  /// `dumpsys usagestats` — last foreground use.
  usage: MeasureStatus;
  /// `dumpsys diskstats` — installed storage per package.
  storage: MeasureStatus;
}

/// One row's slice of the page's measurements, as the detail panel reads it.
export interface AppDetailInputs {
  serial: string;
  measures: AppMeasurements;
  memoryMb?: number;
  usage?: AppUsage;
  storage?: AppStorage;
  onRemeasure?: () => void;
  /// The page's current package state, resynced after every action. A row
  /// can carry an older one (the Optimize plan is not reloaded after a run),
  /// so a report uses this for a catalog package: null means the page has no
  /// current state, and undefined (a non-catalog row) defers to the row.
  liveState?: "enabled" | "disabled" | "missing" | null;
  /// The device context "Report this app" may share. Absent, the panel
  /// offers no report.
  report?: AppReportDevice;
}

/// The state a report may carry for a row. A catalog row takes the page's
/// current state, and none while that state is being re-read: an Optimize or
/// bulk change has just run, so the cached value may predate it. A row outside
/// the catalog (undefined) defers to its own state, unless such a run changed
/// the device while the page had no inventory to re-read: then every row's
/// state may predate it, and none is reported until the inventory is read.
export function reportLiveState(
  inCatalog: boolean,
  state: "enabled" | "disabled" | "missing" | undefined,
  resyncing: boolean,
  unreadAfterRun = false,
): "enabled" | "disabled" | "missing" | null | undefined {
  if (unreadAfterRun) return null;
  if (!inCatalog) return undefined;
  return resyncing ? null : (state ?? null);
}

export function idleMeasurements(): AppMeasurements {
  return { memory: { status: "idle" }, usage: { status: "idle" }, storage: { status: "idle" } };
}

export function loadingMeasurements(): AppMeasurements {
  return {
    memory: { status: "loading" },
    usage: { status: "loading" },
    storage: { status: "loading" },
  };
}

/// Settle one read into a status, stamped with when the answer came back.
export function settled<T>(result: PromiseSettledResult<T>, at: number): MeasureStatus {
  return result.status === "fulfilled"
    ? { status: "ready", at }
    : { status: "unavailable", at, reason: String(result.reason) };
}

export function measuredAt(at: number): string {
  return new Date(at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
}

/// Bytes as a short size, or null when the device did not report it. Null is
/// deliberately not "0 B": an unreported figure is unknown, not empty.
export function formatBytes(bytes: number | null | undefined): string | null {
  if (bytes === null || bytes === undefined || !Number.isFinite(bytes) || bytes < 0) return null;
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value >= 100 ? value.toFixed(0) : value.toFixed(1)} ${units[unit]}`;
}

/// True when at least one storage figure was actually reported.
export function hasStorage(s: AppStorage | undefined): s is AppStorage {
  return !!s && (s.app_bytes !== null || s.data_bytes !== null || s.cache_bytes !== null);
}
