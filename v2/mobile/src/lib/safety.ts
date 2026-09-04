// The ONE safety vocabulary for the mobile UI. Core's `safety_info` returns
// exactly three kinds (crates/core/src/engine/safety.rs: NeverDisable /
// Caution / Safe); this maps each to the label, blurb and CSS class every
// screen renders. Screens must not invent tiers (there is no "Advanced" tier)
// and must not classify packages themselves.

import type { Safety } from "./types";

export type SafetyKind = Safety["kind"];

export interface SafetyTier {
  kind: SafetyKind;
  label: string;
  /// One-line explanation of the tier itself — never a claim about a package.
  description: string;
  /// Suffix for the `.tier-*` / `.risk-*` class rules in each consumer.
  cls: "safe" | "caution" | "blocked";
  icon: string;
}

export const SAFETY_TIERS: Record<SafetyKind, SafetyTier> = {
  safe: {
    kind: "safe",
    label: "Safe",
    description:
      "No system role the classifier knows about. Disabling is reversible and shouldn't affect the rest of the TV.",
    cls: "safe",
    icon: "check_circle",
  },
  caution: {
    kind: "caution",
    label: "Caution",
    description:
      "Recoverable, but disabling it visibly degrades the TV — the reason says exactly what stops working. You get a confirm first.",
    cls: "caution",
    icon: "warning",
  },
  never_disable: {
    kind: "never_disable",
    label: "Blocked",
    description:
      "Disabling would brick the TV or cut this app's connection to it. Every disable and uninstall path refuses these.",
    cls: "blocked",
    icon: "shield",
  },
};

export const SAFETY_TIER_LIST: SafetyTier[] = [
  SAFETY_TIERS.safe,
  SAFETY_TIERS.caution,
  SAFETY_TIERS.never_disable,
];

/// Tier for a resolved `safety_info` result. Null in, null out — an unresolved
/// classification must render as "checking" or an error, never as Safe.
export function tierOf(safety: Safety | null | undefined): SafetyTier | null {
  return safety ? SAFETY_TIERS[safety.kind] : null;
}

/// The core-supplied reason, or the tier blurb for Safe (which carries none).
export function reasonOf(safety: Safety | null | undefined): string {
  if (!safety) return "";
  return safety.kind === "safe"
    ? SAFETY_TIERS.safe.description
    : safety.reason;
}

/// Host layer refuses these outright — the UI must hard-block, not confirm.
export function isBlocked(safety: Safety | null | undefined): boolean {
  return safety?.kind === "never_disable";
}

/// Needs a loud confirm carrying `reasonOf(safety)` before any disable.
export function needsConfirm(safety: Safety | null | undefined): boolean {
  return safety?.kind === "caution";
}
