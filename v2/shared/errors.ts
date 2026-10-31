/// One error vocabulary for both apps: a short code, a plain-language
/// sentence, and the raw text kept for Copy details.
///
/// The code is what a user sees next to the message and what the anonymous
/// error report carries (`error_code`), so a support conversation and the
/// report talk about the same thing. Commands keep returning
/// `Result<T, String>`; codes are derived here from the text, never parsed
/// out of a changed error type.

import { errorTierHint, type SupportInfo } from "./support";

export type ErrorCode =
  | "adb_missing"
  | "app_quitting"
  | "connection_lost"
  | "not_connected"
  | "device_unauthorized"
  | "device_offline"
  | "wrong_pin"
  | "pairing_expired"
  | "connection_refused"
  | "timeout"
  | "unreachable"
  | "protected_package"
  | "permission_denied"
  | "package_not_found"
  | "storage_full"
  | "pm_failure"
  | "unsupported"
  | "locked"
  | "partial_failure"
  | "unknown";

/// First match wins, so the specific shapes come before the broad ones.
const RULES: ReadonlyArray<readonly [RegExp, ErrorCode]> = [
  [/^LOCKED:/, "locked"],
  [/could not locate an adb binary|adb binary at .* is not a regular file/i, "adb_missing"],
  [/the app is quitting/i, "app_quitting"],
  [/Connection to the TV was lost/i, "connection_lost"],
  [/in state 'unauthorized'|\bunauthori[sz]ed\b/i, "device_unauthorized"],
  [/in state 'offline'|device offline|\boffline\b/i, "device_offline"],
  [/Not connected to a device|no devices\/emulators found|device .* not found/i, "not_connected"],
  [/wrong password|incorrect (pin|code|password)|bad (pin|code)/i, "wrong_pin"],
  [
    /pairing (was )?(cancel|aborted|expired)|unable to start pairing client|connection was dropped/i,
    "pairing_expired",
  ],
  [/connection refused/i, "connection_refused"],
  [/timed out|timeout/i, "timeout"],
  [
    /no route to host|network is unreachable|host is (down|unreachable)|protocol fault|couldn't read status|could not connect|cannot connect|failed to connect|unable to connect|no such host|nodename nor servname|name or service not known/i,
    "unreachable",
  ],
  [/^Refusing to|last enabled home|keeps? a home screen|protected/i, "protected_package"],
  [/Permission Denial|SecurityException|permission denied|not allowed/i, "permission_denied"],
  [/Unknown package|not installed for|package .* not found|NameNotFoundException/i, "package_not_found"],
  [/No space left|INSUFFICIENT_STORAGE/i, "storage_full"],
  [/ADB operation unsupported/i, "unsupported"],
  [/Failure \[|^Failure\b|\bFailure:/, "pm_failure"],
];

const PLAIN: Record<ErrorCode, string> = {
  adb_missing: "ADB isn't installed. Install it from the Devices screen, then try again.",
  app_quitting: "The app is closing, so it didn't start this.",
  connection_lost: "The connection to the TV dropped. Reconnect and try again.",
  not_connected: "The TV isn't connected. Connect it again and retry.",
  device_unauthorized:
    "The TV hasn't allowed this computer or phone yet. Accept the \"Allow debugging\" prompt on the TV.",
  device_offline: "The TV is listed but not responding. Check it's awake and on the same network.",
  wrong_pin: "The pairing code didn't match. Open the pairing screen on the TV again for a fresh code.",
  pairing_expired: "The pairing screen on the TV closed or expired. Open it again and use the new code.",
  connection_refused:
    "The TV refused the connection. Check the IP and port, and that debugging is still on.",
  timeout: "The TV took too long to answer. Check it's awake and on the same network, then retry.",
  unreachable: "Couldn't reach the TV. Check its IP address and that both are on the same network.",
  protected_package: "The app refused this to keep the TV usable.",
  permission_denied: "The TV refused this change. Some system apps can't be changed over ADB.",
  package_not_found: "That app isn't on this TV.",
  storage_full: "The TV is out of storage space.",
  pm_failure: "The TV's package manager reported a failure.",
  unsupported: "This connection type can't do that.",
  locked: "This needs ATV Optimizer Pro.",
  partial_failure: "Some changes didn't apply.",
  unknown: "Something went wrong.",
};

export function errorText(raw: unknown): string {
  if (raw instanceof Error) return raw.message;
  if (typeof raw === "string") return raw;
  try {
    return JSON.stringify(raw) ?? String(raw);
  } catch {
    return String(raw);
  }
}

export function classifyError(raw: unknown): ErrorCode {
  const text = errorText(raw);
  for (const [pattern, code] of RULES) {
    if (pattern.test(text)) return code;
  }
  return "unknown";
}

export interface PresentedError {
  code: ErrorCode;
  /// What happened, in plain language.
  message: string;
  /// The TV's support tier, when it could explain the failure.
  tierHint: string | null;
  /// The backend's own words, for Copy details.
  raw: string;
}

export function presentError(raw: unknown, support?: SupportInfo | null): PresentedError {
  const text = errorText(raw).trim();
  const code = classifyError(text);
  // Connection problems aren't about the model, so the tier says nothing.
  const deviceSide = !["adb_missing", "app_quitting", "locked", "not_connected"].includes(code);
  // The backend's own errors are mostly written for people ("not an IP
  // address…"). When none of the known shapes match, show that sentence
  // rather than hiding it behind a generic one.
  const message =
    code === "unknown" && text ? (text.length > 300 ? `${text.slice(0, 297)}…` : text) : PLAIN[code];
  return {
    code,
    message,
    tierHint: deviceSide ? errorTierHint(support) : null,
    raw: text,
  };
}

/// What Copy details puts on the clipboard. Local only: nothing here is
/// uploaded, and the user decides where to paste it.
export function errorDetails(
  e: PresentedError,
  context: { action?: string; app?: string; version?: string; device?: string; tier?: string } = {},
): string {
  const lines = [
    `Error: ${e.code}`,
    `Message: ${e.message}`,
    context.action ? `Action: ${context.action}` : null,
    context.device ? `TV: ${context.device}` : null,
    context.tier ? `Support: ${context.tier}` : null,
    context.app ? `App: ${context.app}${context.version ? ` ${context.version}` : ""}` : null,
    e.tierHint ? `Note: ${e.tierHint}` : null,
    `Details: ${e.raw || "(none)"}`,
  ];
  return lines.filter((l): l is string => l !== null).join("\n");
}
