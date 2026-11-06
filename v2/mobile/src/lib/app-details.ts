// What the app detail sheet needs to say about each figure: whether it was
// read, when, and why not. A figure is only ever shown next to the time it was
// measured, and a failed read is "unavailable", never zero. Ported from
// desktop's v2/src/lib/app-details.ts; keep the two in step.

import type { AppStorage, AppUsage, Device } from "./types";
import { reportFamily, type AppReportDevice, type AppReportState } from "./app-report";
import { normalizeHardwareId } from "./identity";

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

// Internal receipt only: identity and timing fields never enter report JSON.
export interface ApkReportMeasurement {
  package: string;
  serial: string;
  generation: number;
  measurementId: number;
  appBytes: number;
  at: number;
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
/// the catalog (undefined) defers to its own state, unless a run has changed
/// the device since that row's list was read (`othersStale`). When a run
/// changed the device while the page had no inventory to re-read
/// (`unreadAfterRun`), every row's state may predate it, and none is reported
/// until the inventory is read.
export function reportLiveState(
  inCatalog: boolean,
  state: "enabled" | "disabled" | "missing" | undefined,
  resyncing: boolean,
  unreadAfterRun = false,
  othersStale = false,
): "enabled" | "disabled" | "missing" | null | undefined {
  if (unreadAfterRun) return null;
  if (!inCatalog) return othersStale ? null : undefined;
  return resyncing ? null : (state ?? null);
}

/// The device context "Report this app" may share, or null with no device.
export function reportDeviceOf(device: Device | null, sessionSerial: string): AppReportDevice | null {
  if (!device) return null;
  return {
    family: reportFamily(device.device_type, device.tv_evidence),
    androidVersion: device.properties?.android_release || null,
    // A placeholder ro.serialno ("unknown") identifies nothing, and
    // scrubbing it would eat that word from the user's note.
    redact: [sessionSerial, device.serial, device.properties?.serial_number].filter(
      (id): id is string => !!normalizeHardwareId(id),
    ),
  };
}

/// The measured figures a report may carry. A read that failed or has not
/// answered is null, never zero. `apk` is the detail sheet's APK-size
/// receipt, passed only once the caller has matched it to this package,
/// connection and measurement batch.
export function reportFigures(
  measures: AppMeasurements,
  memoryMb: number | undefined,
  storage: AppStorage | undefined,
  apk: ApkReportMeasurement | null,
): Omit<AppReportState, "status"> {
  const running = measures.memory.status === "ready" ? memoryMb !== undefined && memoryMb > 0 : null;
  const storageSettled = measures.storage.status === "ready" || measures.storage.status === "unavailable";
  const apkUsable =
    apk && storageSettled && Number.isFinite(apk.appBytes) && apk.appBytes >= 0 && Number.isFinite(apk.at)
      ? apk
      : null;
  return {
    running,
    ramMb: running ? (memoryMb ?? null) : null,
    storage:
      measures.storage.status === "ready" && hasStorage(storage)
        ? { source: "diskstats", ...storage }
        : apkUsable
          ? { source: "apk_files", app_bytes: apkUsable.appBytes, data_bytes: null, cache_bytes: null }
          : null,
  };
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
