// The ONE safety vocabulary for the mobile UI. Core's `safety_info` returns
// exactly four kinds (crates/core/src/engine/safety.rs: NeverDisable /
// Caution / Safe / Unknown); this maps each to the label, blurb and CSS class
// every screen renders. Screens must not invent tiers and must not classify
// packages themselves.
//
// `Safe` only ever comes from the reviewed app list, never as a fallback —
// that is what separates "we looked at this and rated it" from "we have no
// idea what this is", and both were previously collapsed into Unknown.

import type { Safety } from "./types";

export type SafetyKind = Safety["kind"];

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
    label: "Safe",
    description:
      "Reviewed for Android TV and rated safe to remove. The reason says what it is and what you lose.",
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
      "Removing it has a known consequence — the reason says exactly what can stop working. You must review that warning first.",
    cls: "caution",
    icon: "warning",
  },
  never_disable: {
    kind: "never_disable",
    label: "Protected",
    description:
      "Disabling would brick the TV or cut this app's connection to it. Every disable and uninstall path refuses these.",
    cls: "blocked",
    icon: "shield",
  },
};

export const SAFETY_TIER_LIST: SafetyTier[] = [
  SAFETY_TIERS.unknown,
  SAFETY_TIERS.caution,
  SAFETY_TIERS.never_disable,
];

/// Tier for a resolved `safety_info` result. Null in, null out — an unresolved
/// classification must render as "checking" or an error, never as Unknown.
export function tierOf(safety: Safety | null | undefined): SafetyTier | null {
  return safety ? SAFETY_TIERS[safety.kind] : null;
}

/// The core-supplied reason for every canonical verdict.
export function reasonOf(safety: Safety | null | undefined): string {
  return safety?.reason ?? "";
}

/// Host layer refuses these outright — the UI must hard-block, not confirm.
export function isBlocked(safety: Safety | null | undefined): boolean {
  return safety?.kind === "never_disable";
}

/// Needs a loud confirm carrying `reasonOf(safety)` before any removal action.
export function needsConfirm(safety: Safety | null | undefined): boolean {
  return safety?.kind === "caution" || safety?.kind === "unknown";
}
