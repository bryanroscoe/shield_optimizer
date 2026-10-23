// The one safety vocabulary for the desktop UI.
//
// Core's `safety_info` returns exactly four kinds
// (crates/core/src/engine/safety.rs: NeverDisable / Caution / Safe / Unknown).
// `safe` only ever comes from the reviewed app list, never as a fallback. This
// maps each to the label, blurb and class every screen renders. Screens must
// not invent tiers and must not classify packages themselves, so one state
// reads the same on every screen.
//
// Deliberately not modelled here: the app-list catalog's `risk` field, which
// is hand-written editorial metadata saying "how much will you miss this",
// not "is this safe to remove". The engine's own `safe` verdict, sourced from
// the reviewed list, is the answer to that second question.

import type { Safety } from "../../shared/safety";

export type { Safety };
export type SafetyKind = Safety["kind"];

/// Resolution state of an async `safety_info` lookup.
export type SafetyStatus =
  | { status: "checking" }
  | { status: "ready"; verdict: Safety }
  | { status: "unavailable"; reason: string };

export interface SafetyTier {
  kind: SafetyKind;
  label: string;
  /// One-line explanation of the tier itself — never a claim about a package.
  description: string;
  /// Full class for the rendered chip. Spelled out rather than a suffix so a
  /// grep for the class name finds both the CSS and every call site.
  cls: string;
}

export const SAFETY_TIERS: Record<SafetyKind, SafetyTier> = {
  safe: {
    kind: "safe",
    label: "Safe to remove",
    description:
      "The reason says what the app is and what you lose. Only ever comes from the reviewed list, never as a fallback.",
    cls: "safety-tier safety-tier--safe",
  },
  unknown: {
    kind: "unknown",
    label: "Unknown",
    description:
      "No protection or caution rule matched. That does not prove removing this package is safe, so review its purpose and confirm explicitly.",
    cls: "safety-tier safety-tier--unknown",
  },
  caution: {
    kind: "caution",
    label: "Caution",
    description:
      "Removing it has a known consequence — the reason says exactly what can stop working. Review that warning first.",
    cls: "safety-tier safety-tier--caution",
  },
  never_disable: {
    kind: "never_disable",
    label: "Protected",
    description:
      "Disabling would break the TV or cut this app's connection to it. Every disable and uninstall path refuses these.",
    cls: "safety-tier safety-tier--protected",
  },
};

export const SAFETY_TIER_LIST: SafetyTier[] = [
  SAFETY_TIERS.safe,
  SAFETY_TIERS.unknown,
  SAFETY_TIERS.caution,
  SAFETY_TIERS.never_disable,
];

/// Tier for a resolved verdict. Null in, null out — an unresolved lookup must
/// render as "Checking" or "Unavailable", never fall back to Unknown.
export function tierOf(safety: Safety | null | undefined): SafetyTier | null {
  return safety ? SAFETY_TIERS[safety.kind] : null;
}

/// The verdict inside a lookup state, or null while it is unresolved.
export function verdictOf(status: SafetyStatus | undefined): Safety | null {
  return status?.status === "ready" ? status.verdict : null;
}

export const CHECKING_LABEL = "Checking…";
export const UNAVAILABLE_LABEL = "Safety unavailable";

/// Label for a resolved verdict. Confirm prompts hold a bare `Safety`, not a
/// lookup state, so this is the entry point for them.
export function verdictLabel(safety: Safety): string {
  return SAFETY_TIERS[safety.kind].label;
}

/// Short label for a table cell or chip — the only place a screen gets one.
/// tests/memory-safety.mjs proves resolution by reading `data-verdict`, not
/// these words, so the wording is free to change.
export function safetyLabel(status: SafetyStatus | undefined): string {
  if (!status || status.status === "unavailable") return UNAVAILABLE_LABEL;
  if (status.status === "checking") return CHECKING_LABEL;
  return verdictLabel(status.verdict);
}

/// Build a lookup state from the split props a row component receives.
export function statusOf(
  status: SafetyStatus["status"] | undefined,
  verdict: Safety | null | undefined,
  reason?: string,
): SafetyStatus {
  if (status === "ready" && verdict) return { status: "ready", verdict };
  if (status === "checking") return { status: "checking" };
  return { status: "unavailable", reason: reason?.trim() || "Safety lookup has not completed." };
}

/// Chip class for a lookup state.
///
/// Unavailable is a warning, not an absence: it *blocks* removal, so it reads
/// amber rather than the grey that would make it look like no information.
export function safetyClass(status: SafetyStatus | undefined): string {
  if (!status || status.status === "unavailable") {
    return "safety-tier safety-tier--unavailable";
  }
  if (status.status === "checking") return "safety-tier safety-tier--checking";
  return SAFETY_TIERS[status.verdict.kind].cls;
}

/// The core-supplied reason, or why there isn't one yet.
export function safetyReason(status: SafetyStatus | undefined): string {
  if (!status) return "Safety lookup has not completed.";
  if (status.status === "checking") return "Safety lookup is in progress.";
  if (status.status === "unavailable") {
    return `Safety lookup failed: ${status.reason}`;
  }
  return status.verdict.reason;
}

/// Host layer refuses these outright — hard-block, never offer a confirm.
export function isBlocked(safety: Safety | null | undefined): boolean {
  return safety?.kind === "never_disable";
}

/// Needs an explicit confirm carrying the reason before any removal.
export function needsConfirm(safety: Safety | null | undefined): boolean {
  return safety?.kind === "caution" || safety?.kind === "unknown";
}

/// The Safety/Reason lines shared by every removal confirm prompt.
export function confirmVerdictLine(safety: Safety): string {
  return `Safety: ${verdictLabel(safety)}\nReason: ${safety.reason}`;
}
