// Pure row logic for the Optimize screen. The defaults come from
// recommendation.ts — the same function the App List and Health call — so
// mobile's Recommended tab and desktop's Optimize wizard pre-select the same
// apps for the same TV.

import type { OptimizeMode, OptimizePlanItem, Safety } from "./types";
import { catalogItem, type AppItem } from "./appsList";
import { isBlocked, type SafetyStatus } from "./safety";
import { isReinstallable, recommendation, type PackageState, type RemovalMethod } from "./recommendation";

export type RowAction = RemovalMethod | "enable";

/// On-device state read off the plan's skip reason, exactly as desktop's
/// OptimizeTab does: not installed ⇒ missing, already disabled ⇒ disabled,
/// everything else ⇒ enabled.
export function planRowState(item: OptimizePlanItem): PackageState {
  if (item.action.kind === "skip") {
    if (item.action.reason === "not_installed") return "missing";
    if (item.action.reason === "already_disabled") return "disabled";
  }
  return "enabled";
}

/// The action an actionable row runs, or null for a skip row. The engine
/// already downgrades an uninstall the store can't give back; this repeats it
/// so no plan can make an unrecoverable uninstall the action.
export function naturalAction(item: OptimizePlanItem): RowAction | null {
  if (item.action.kind === "skip") return null;
  if (item.action.kind === "uninstall" && !isReinstallable(item.entry)) return "disable";
  return item.action.kind;
}

/// Lookup state for one row from the screen's verdict map and load flags.
export function rowSafety(
  verdict: Safety | undefined,
  loading: boolean,
  failed: boolean,
): SafetyStatus {
  if (verdict) return { status: "ready", verdict };
  if (loading) return { status: "checking" };
  return { status: "unavailable", reason: failed ? "lookup failed" : "no verdict" };
}

/// Whether a row belongs on the Recommended tab and starts selected.
/// Optimize: exactly the rows desktop pre-selects (`recommendation().kind ===
/// "act"`). Restore: the catalog's `default_restore` enables.
export function isPlanRecommended(
  item: OptimizePlanItem,
  mode: OptimizeMode,
  safety: SafetyStatus | undefined,
): boolean {
  const action = naturalAction(item);
  if (mode === "restore") return action === "enable" && item.entry.default_restore;
  if (action !== "disable" && action !== "uninstall") return false;
  return recommendation(item.entry, planRowState(item), safety).kind === "act";
}

/// A "remove if unused" row whose safety is resolved and not protected —
/// desktop's `isReviewRow`, which earns the amber "Review: … if unused" pill.
export function isReviewRow(item: OptimizePlanItem, safety: SafetyStatus | undefined): boolean {
  const action = naturalAction(item);
  return (
    !!item.entry.review &&
    planRowState(item) === "enabled" &&
    (action === "disable" || action === "uninstall") &&
    safety?.status === "ready" &&
    !isBlocked(safety.verdict)
  );
}

/// Case-insensitive match on the app's name or package id. An empty query
/// matches everything.
export function matchesPlanQuery(item: OptimizePlanItem, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  return (
    item.entry.name.toLowerCase().includes(q) || item.entry.package.toLowerCase().includes(q)
  );
}

/// The row as AppDetailSheet reads it. The plan's state is what the TV said
/// when the plan was built; the catalog does not say whether it is a system
/// app, so that stays unknown.
export function planItemApp(item: OptimizePlanItem): AppItem {
  return catalogItem(item.entry, planRowState(item));
}
