// The anonymous-report setting as the backend last confirmed it. Shared so the
// header notice, the Report a bug dialog and Settings never disagree about it.
import { api } from "$lib/api";
import type { TelemetrySettings } from "../../shared/telemetry";

export const reports = $state({
  /// `null` until the backend answers, and after a read fails.
  settings: null as TelemetrySettings | null,
  /// Whether a read has finished at all, so "not yet" and "failed" differ.
  read: false,
  noticeOpen: false,
});

export async function loadReports(): Promise<void> {
  try {
    reports.settings = await api.telemetrySettings();
    reports.noticeOpen = reports.settings.available && !reports.settings.notice_seen;
  } catch {
    reports.settings = null;
  } finally {
    reports.read = true;
  }
}

export async function dismissReportsNotice(): Promise<void> {
  reports.noticeOpen = false;
  try {
    reports.settings = await api.dismissTelemetryNotice();
  } catch {
    /* the notice stays closed for this session either way */
  }
}

/// Believe the backend, not the click. Rethrows the failure for the caller to
/// show, after re-reading what the setting actually is.
export async function setReportsEnabled(wanted: boolean): Promise<void> {
  try {
    reports.settings = await api.setTelemetryEnabled(wanted);
  } catch (err) {
    reports.settings = await api.telemetrySettings().catch(() => reports.settings);
    throw err;
  }
}
