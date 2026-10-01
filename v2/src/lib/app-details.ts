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
  /// The device context "Report this app" may share. Absent, the panel
  /// offers no report.
  report?: AppReportDevice;
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
