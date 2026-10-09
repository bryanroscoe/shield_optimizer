// The ONE safety vocabulary for the mobile UI. Core's `safety_info` returns
// exactly four kinds (crates/core/src/engine/safety.rs: NeverDisable /
// Caution / Safe / Unknown); this maps each to the label, blurb and CSS class
// every screen renders. Screens must not invent tiers and must not classify
// packages themselves. Labels and descriptions match desktop's
// v2/src/lib/safety.ts word for word, so one TV reads the same on both apps.
//
// `Safe` only ever comes from the reviewed app list, never as a fallback —
// that is what separates "we looked at this and rated it" from "we have no
// idea what this is", and both were previously collapsed into Unknown.

import type { Safety } from "./types";

export type { Safety };
export type SafetyKind = Safety["kind"];

/// Resolution state of an async `safety_info` lookup — same shape as desktop.
export type SafetyStatus =
  | { status: "checking" }
  | { status: "ready"; verdict: Safety }
  | { status: "unavailable"; reason: string };

export interface SafetyTier {
  kind: SafetyKind;
  label: string;
  /// One-line explanation of the tier itself — never a claim about a package.
  description: string;
  /// Suffix for the `.tier-*` / `.risk-*` class rules in each consumer.
  cls: "unknown" | "caution" | "blocked" | "safe";
  icon: string;
}

export const SAFETY_TIERS: Record<SafetyKind, SafetyTier> = {
  safe: {
    kind: "safe",
    label: "Safe to remove",
    description:
      "The reason says what the app is and what you lose. Only ever comes from the reviewed list, never as a fallback.",
    cls: "safe",
    icon: "check_circle",
  },
  unknown: {
    kind: "unknown",
    label: "Unknown",
    description:
      "No protection or caution rule matched. That does not prove removing this package is safe, so review its purpose and confirm explicitly.",
    cls: "unknown",
    icon: "help",
  },
  caution: {
    kind: "caution",
    label: "Caution",
    description:
      "Removing it has a known consequence — the reason says exactly what can stop working. Review that warning first.",
    cls: "caution",
    icon: "warning",
  },
  never_disable: {
    kind: "never_disable",
    label: "Protected",
    description:
      "Disabling would break the TV or cut this app's connection to it. Every disable and uninstall path refuses these.",
    cls: "blocked",
    icon: "shield",
  },
};

/// The legend, ordered least to most restricted. Every kind in the union
/// belongs here — a tier the user can be shown but cannot look up reads as a
/// bug in the app.
export const SAFETY_TIER_LIST: SafetyTier[] = [
  SAFETY_TIERS.safe,
  SAFETY_TIERS.unknown,
  SAFETY_TIERS.caution,
  SAFETY_TIERS.never_disable,
];

/// Tier for a resolved `safety_info` result. Null in, null out — an unresolved
/// classification must render as "checking" or an error, never as Unknown.
export function tierOf(safety: Safety | null | undefined): SafetyTier | null {
  return safety ? SAFETY_TIERS[safety.kind] : null;
}

/// The verdict inside a lookup state, or null while it is unresolved.
export function verdictOf(status: SafetyStatus | undefined): Safety | null {
  return status?.status === "ready" ? status.verdict : null;
}

export const CHECKING_LABEL = "Checking…";
export const UNAVAILABLE_LABEL = "Safety unavailable";

/// Label for a resolved verdict.
export function verdictLabel(safety: Safety): string {
  return SAFETY_TIERS[safety.kind].label;
}

/// Short label for a lookup state — never falls back to Unknown while the
/// lookup is unresolved or failed.
export function safetyLabel(status: SafetyStatus | undefined): string {
  if (!status || status.status === "unavailable") return UNAVAILABLE_LABEL;
  if (status.status === "checking") return CHECKING_LABEL;
  return verdictLabel(status.verdict);
}

/// The core-supplied reason, or why there isn't one yet.
export function safetyReason(status: SafetyStatus | undefined): string {
  if (!status) return "Safety lookup has not completed.";
  if (status.status === "checking") return "Safety lookup is in progress.";
  if (status.status === "unavailable") return `Safety lookup failed: ${status.reason}`;
  return status.verdict.reason;
}

/// The core-supplied reason for every canonical verdict.
export function reasonOf(safety: Safety | null | undefined): string {
  return safety?.reason ?? "";
}

/// One sentence stating what the engine said about a package, for the confirm
/// dialog the user reads before a removal. Derived from the kind rather than
/// written per call site: a two-way `unknown ? … : "Caution"` ternary over this
/// four-valued union told the user the engine had "marked this Caution" about
/// packages it had actually rated Safe — a false claim about our own verdict,
/// at the moment of consent.
export function verdictSummary(safety: Safety | null | undefined): string {
  switch (safety?.kind) {
    case "safe":
      return `The safety engine rated this ${SAFETY_TIERS.safe.label}.`;
    case "caution":
      return "The safety engine marked this Caution.";
    case "never_disable":
      return "The safety engine marked this Protected.";
    default:
      return "The removal impact is Unknown.";
  }
}

/// Host layer refuses these outright — the UI must hard-block, not confirm.
export function isBlocked(safety: Safety | null | undefined): boolean {
  return safety?.kind === "never_disable";
}

/// Needs a loud confirm carrying `reasonOf(safety)` before any removal action.
export function needsConfirm(safety: Safety | null | undefined): boolean {
  return safety?.kind === "caution" || safety?.kind === "unknown";
}

/// The Safety/Reason lines shared by every removal confirm prompt.
export function confirmVerdictLine(safety: Safety): string {
  return `Safety: ${verdictLabel(safety)}\nReason: ${safety.reason}`;
}
