/// Frontend half of the anonymous reports. Both apps route every backend
/// call through one wrapper; that wrapper hands each finished call to
/// `observeCall`, which tells Rust what happened. Rust decides whether to
/// send anything (the user's setting, a configured endpoint), builds the
/// payload and redacts it, so nothing here can leak more than Rust allows.
///
/// Every report is fire-and-forget: it never awaits on the user's path and
/// swallows its own failures.

import { classifyError, errorText } from "./errors";

export type TelemetryAction =
  | "connect"
  | "scan"
  | "disable"
  | "enable"
  | "uninstall"
  | "optimize"
  | "snapshot_apply"
  | "launcher"
  | "settings_write"
  | "pairing"
  | "other";

/// Backend commands that are a tracked user action.
export const ACTION_COMMANDS: Readonly<Record<string, TelemetryAction>> = {
  connect_device: "connect",
  connect_paired: "connect",
  wireless_connect: "connect",
  scan_network: "scan",
  wireless_discover: "scan",
  disable_package: "disable",
  enable_package: "enable",
  reinstall_existing: "enable",
  uninstall_package: "uninstall",
  // An Optimize run reports its own "optimize" summary; this is one step.
  apply_performance_settings: "settings_write",
  apply_snapshot: "snapshot_apply",
  set_default_launcher: "launcher",
  disable_launcher: "launcher",
  set_home_any: "launcher",
  disable_stock_launcher: "launcher",
  disable_setup_helper: "launcher",
  write_setting: "settings_write",
  set_display_scaling: "settings_write",
  set_private_dns: "settings_write",
  pair_device: "pairing",
  wireless_pair: "pairing",
};

/// Calls whose failure is routine (polling, probing, a status read while
/// disconnected) and says nothing about whether the app works. Their errors
/// still reach the UI; they just aren't reported.
const QUIET_COMMANDS = new Set([
  "adb_status",
  "check_for_update",
  "local_address_for",
  "probe_paired_connect",
  "resource_sample",
  "read_device_logs",
  "read_debug_log",
  "wireless_status",
  "wireless_cancel_connect",
  "remote_warm",
  "find_remote",
  "send_key",
  "send_text",
  "log_ui_event",
]);

/// Codes that are expected states, not failures worth reporting.
const QUIET_CODES = new Set(["locked", "app_quitting"]);

export type Send = (command: string, args: Record<string, unknown>) => Promise<unknown>;

function isTelemetryCommand(command: string): boolean {
  return command.startsWith("telemetry_") || command.endsWith("_telemetry_enabled") ||
    command === "dismiss_telemetry_notice";
}

function fire(send: Send, command: string, args: Record<string, unknown>): void {
  try {
    void send(command, args).catch(() => {});
  } catch {
    // A missing backend (tests, the browser demo) is not the user's problem.
  }
}

export interface Outcome {
  ok: boolean;
  code?: string;
  message?: string;
}

/// Did a command that resolved actually succeed? Several report failure in
/// their result (`{ ok: false, message }`) rather than by rejecting.
export function resolvedOutcome(command: string, value: unknown): Outcome {
  if (value && typeof value === "object") {
    const v = value as Record<string, unknown>;
    if (command === "apply_snapshot") {
      const failed =
        (Array.isArray(v.packages_failed) ? v.packages_failed.length : 0) +
        (Array.isArray(v.settings_failed) ? v.settings_failed.length : 0);
      // Counts only: the names are the user's apps and settings.
      return failed > 0
        ? { ok: false, code: "partial_failure", message: `${failed} item(s) failed to apply` }
        : { ok: true };
    }
    if (v.ok === false || v.success === false) {
      const message = typeof v.message === "string" ? v.message : undefined;
      return { ok: false, code: classifyError(message ?? ""), message };
    }
  }
  return { ok: true };
}

function serialOf(args: Record<string, unknown> | undefined): string | undefined {
  const s = args?.serial;
  return typeof s === "string" && s ? s : undefined;
}

/// Report one finished backend call. `error` is set when it rejected.
export function observeCall(
  send: Send,
  command: string,
  args: Record<string, unknown> | undefined,
  value: unknown,
  error?: unknown,
): void {
  if (isTelemetryCommand(command)) return;
  const action = ACTION_COMMANDS[command];
  const serial = serialOf(args);
  if (action) {
    const outcome: Outcome =
      error !== undefined
        ? { ok: false, code: classifyError(error), message: errorText(error) }
        : resolvedOutcome(command, value);
    if (!outcome.ok && QUIET_CODES.has(outcome.code ?? "")) return;
    fire(send, "telemetry_action", {
      action,
      serial: serial ?? null,
      ok: outcome.ok,
      errorCode: outcome.ok ? null : (outcome.code ?? "unknown"),
      message: outcome.ok ? null : (outcome.message ?? null),
    });
    return;
  }
  if (error === undefined || QUIET_COMMANDS.has(command)) return;
  const code = classifyError(error);
  if (QUIET_CODES.has(code)) return;
  fire(send, "telemetry_error", {
    errorCode: code,
    message: `${command}: ${errorText(error)}`,
    action: "other",
    serial: serial ?? null,
  });
}

/// A summary for a multi-step flow (an Optimize run), on top of the
/// per-package results the wrapper already reported.
export function recordAction(
  send: Send,
  action: TelemetryAction,
  serial: string | null,
  outcome: Outcome,
): void {
  fire(send, "telemetry_action", {
    action,
    serial,
    ok: outcome.ok,
    errorCode: outcome.ok ? null : (outcome.code ?? "unknown"),
    message: outcome.ok ? null : (outcome.message ?? null),
  });
}

/// Forward uncaught errors and unhandled rejections. Returns a disposer.
/// Rust rate-limits these, so a render loop that throws can't flood anything.
export function installGlobalErrorReporting(send: Send, target: Window = window): () => void {
  const onError = (event: ErrorEvent) => {
    const where = event.filename ? ` (${event.filename.split("/").pop()}:${event.lineno})` : "";
    fire(send, "telemetry_error", {
      errorCode: "uncaught_error",
      message: `${errorText(event.error ?? event.message)}${where}`,
      action: "other",
      serial: null,
    });
  };
  const onRejection = (event: PromiseRejectionEvent) => {
    fire(send, "telemetry_error", {
      errorCode: "unhandled_rejection",
      message: errorText(event.reason),
      action: "other",
      serial: null,
    });
  };
  target.addEventListener("error", onError);
  target.addEventListener("unhandledrejection", onRejection);
  return () => {
    target.removeEventListener("error", onError);
    target.removeEventListener("unhandledrejection", onRejection);
  };
}

export interface TelemetrySettings {
  enabled: boolean;
  available: boolean;
  notice_seen: boolean;
}

export const NOTICE_TEXT =
  "ATV Optimizer sends anonymous reports: your TV's make, model and Android version, " +
  "which actions worked or failed, and error messages with addresses, names and file paths removed. " +
  "No serial numbers, IP addresses, TV names or app lists. You can turn this off at any time.";

export const TOGGLE_LABEL = "Share anonymous device and error reports";
