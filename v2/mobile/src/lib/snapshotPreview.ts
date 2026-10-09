// Rows for the snapshot "Preview restore" sheet — the mobile port of
// desktop's plan table (routes/snapshots/+page.svelte): Item, Now, What
// happens. "Now" is what the TV reported when the plan was computed; a
// setting already at the snapshot's value is listed as "Already set" rather
// than counted as work.

import type { SnapshotApplyPlan } from "./types";

export type PreviewRowKind = "disable" | "launcher" | "setting" | "reset";

export interface PreviewRow {
  item: string;
  now: string;
  /// What happens — the action for acting rows, the reason for the rest.
  result: string;
  kind: PreviewRowKind;
}

export interface PreviewRows {
  acting: PreviewRow[];
  unchanged: PreviewRow[];
}

export function previewRows(plan: SnapshotApplyPlan, snapshotLauncher: string | null): PreviewRows {
  const now = plan.current_values ?? {};
  const currentLauncher = plan.current_launcher ?? "—";
  const acting: PreviewRow[] = [];
  const unchanged: PreviewRow[] = [];

  for (const pkg of plan.packages_to_disable) {
    acting.push({ item: pkg, now: "enabled", result: "Disable", kind: "disable" });
  }
  if (plan.launcher_to_set) {
    acting.push({
      item: "Home app",
      now: currentLauncher,
      result: `Set Home → ${plan.launcher_to_set}`,
      kind: "launcher",
    });
  }
  for (const [key, value] of Object.entries(plan.settings_to_write)) {
    acting.push({ item: key, now: now[key] ?? "unset", result: `Set → ${value}`, kind: "setting" });
  }
  for (const key of plan.settings_to_delete) {
    acting.push({
      item: key,
      now: now[key] ?? "—",
      result: "Reset → device default",
      kind: "reset",
    });
  }

  if (plan.launcher_not_installed) {
    unchanged.push({
      item: "Home app",
      now: currentLauncher,
      result: `${plan.launcher_not_installed} isn't installed; skipped`,
      kind: "launcher",
    });
  } else if (!plan.launcher_to_set && snapshotLauncher !== null) {
    unchanged.push({ item: "Home app", now: currentLauncher, result: "Already Home", kind: "launcher" });
  }
  for (const key of plan.settings_already_set) {
    unchanged.push({ item: key, now: now[key] ?? "unset", result: "Already set", kind: "setting" });
  }
  for (const pkg of plan.packages_already_disabled) {
    unchanged.push({ item: pkg, now: "disabled", result: "Already disabled", kind: "disable" });
  }
  for (const pkg of plan.packages_not_installed) {
    unchanged.push({ item: pkg, now: "not installed", result: "Not on device", kind: "disable" });
  }
  return { acting, unchanged };
}

/// Desktop's one-line plan summary.
export function previewSummary(plan: SnapshotApplyPlan): string {
  const settings = Object.keys(plan.settings_to_write).length;
  const parts = [
    `${plan.packages_to_disable.length} to disable`,
    `${plan.packages_already_disabled.length} already disabled`,
    `${plan.packages_not_installed.length} not on device`,
    `${settings} setting${settings === 1 ? "" : "s"} to write`,
    `${plan.settings_to_delete.length} to reset`,
  ];
  if (plan.launcher_to_set) parts.push("Home app");
  return parts.join(" · ");
}
