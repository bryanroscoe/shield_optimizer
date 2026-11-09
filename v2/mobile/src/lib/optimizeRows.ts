// Pure row logic for the Optimize screen. The defaults come from
// recommendation.ts — the same function the App List and Health call — so
// mobile's Recommended tab and desktop's Optimize wizard pre-select the same
// apps for the same TV.

import type { OptimizeMode, OptimizePlanItem, Safety } from "./types";
import { catalogItem, type AppItem } from "./appsList";
import { SAFETY_TIERS, isBlocked, type SafetyKind, type SafetyStatus } from "./safety";
import { isReinstallable, recommendation, type PackageState, type RemovalMethod } from "./recommendation";

export type RowAction = RemovalMethod | "enable";

/// Enable plans come only from installed, disabled apps. Skip reasons retain
/// the state the planner observed; other actionable plans start enabled.
export function planRowState(item: OptimizePlanItem): PackageState {
  if (item.action.kind === "enable") return "disabled";
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

/// The row's own description, or nothing when the safety reason shown above
/// it already carries that sentence. Core builds a reviewed app's reason from
/// the same catalog text (all of it for Safe, its tail for Caution), so
/// printing both would show the sentence twice.
export function rowPurpose(description: string | null | undefined, shownReason: string): string {
  const purpose = (description ?? "").trim();
  if (!purpose) return "";
  return shownReason.includes(purpose) ? "" : purpose;
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

/// One change the user is about to confirm, with the verdict they saw (null
/// only for an enable).
export interface ConfirmItem {
  item: OptimizePlanItem;
  verdict: Safety | null;
}

const CONFIRM_TIER_ORDER: SafetyKind[] = ["never_disable", "unknown", "caution", "safe"];

function sentence(text: string): string {
  return /[.!?…]$/.test(text) ? text : `${text}.`;
}

/// The Apply confirmation. Every removal keeps its own verb, name and safety
/// reason (informed consent), stated once: grouped under its tier, most
/// serious first, with apps that share a reason listed together. What
/// uninstall means and the animation pass are said once, not per app.
export function applyConfirmation(
  items: ConfirmItem[],
  mode: OptimizeMode,
): { warning: string; message: string } {
  const tiers = new Map<SafetyKind, Map<string, string[]>>();
  const enables: string[] = [];
  let uninstalls = 0;
  for (const { item, verdict } of items) {
    const action = naturalAction(item);
    if (!action) continue;
    if (action === "enable") {
      enables.push(item.entry.name);
      continue;
    }
    if (action === "uninstall") uninstalls += 1;
    const kind = verdict?.kind ?? "unknown";
    const reason = verdict?.reason.trim() ?? "";
    const byReason = tiers.get(kind) ?? new Map<string, string[]>();
    tiers.set(kind, byReason);
    const names = byReason.get(reason) ?? [];
    byReason.set(reason, names);
    names.push(`${action === "uninstall" ? "Uninstall" : "Disable"} ${item.entry.name}`);
  }
  const warning = CONFIRM_TIER_ORDER.filter((kind) => tiers.has(kind))
    .map((kind) => {
      const entries = [...tiers.get(kind)!].map(([reason, names]) =>
        sentence(reason ? `${names.join(", ")} — ${reason}` : names.join(", ")),
      );
      return `${SAFETY_TIERS[kind].label}: ${entries.join(" ")}`;
    })
    .join(" ");
  const message = [
    enables.length > 0 ? `Re-enable ${enables.join(", ")}.` : "",
    uninstalls > 0
      ? "Uninstall removes an app for this TV's user; Reinstall works only while its APK remains on the TV, otherwise use the Play Store."
      : "",
    mode === "optimize"
      ? "Afterward, animation scales are set to 0.5×."
      : "Afterward, animation scales are reset to 1×.",
  ]
    .filter(Boolean)
    .join(" ");
  return { warning, message };
}
