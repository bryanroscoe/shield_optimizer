// Rows for the snapshot "Preview restore" sheet — the mobile port of
// desktop's plan table (routes/snapshots/+page.svelte): Item, Now, What
// happens. "Now" is what the TV reported when the plan was computed; a
// setting already at the snapshot's value is listed as "Already set" rather
// than counted as work.

import type { ApplyResult, SnapshotApplyPlan, SnapshotFile } from "./types";

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

/// What a restore leaves alone so the TV keeps a Home screen. Older cores send
/// neither field, so both are read as optional.
type HomeSafety = {
  /// The snapshot's launcher when it is installed but disabled here.
  launcher_disabled?: string | null;
  /// Listed as disabled in the snapshot but kept on; `reason` is display-ready.
  packages_kept?: { package: string; reason: string }[];
};

function homeSafety(plan: SnapshotApplyPlan): Required<HomeSafety> {
  const extra = plan as SnapshotApplyPlan & HomeSafety;
  return { launcher_disabled: extra.launcher_disabled ?? null, packages_kept: extra.packages_kept ?? [] };
}

/// Why the restore won't change the Home app, shown beside Restore rather than
/// only inside the collapsed list. Null when Home will be set or already matches.
export function launcherSkipNote(plan: SnapshotApplyPlan): string | null {
  if (plan.launcher_to_set) return null;
  if (plan.launcher_not_installed) return `${plan.launcher_not_installed} isn't installed on this TV.`;
  const disabled = homeSafety(plan).launcher_disabled;
  if (disabled) {
    return `${disabled} is disabled on this TV, and a restore never re-enables anything. Enable it first, then restore again.`;
  }
  return null;
}

/// Packages the snapshot lists as disabled that the restore keeps on, with
/// the core's reason, so they can be named beside Restore.
export function keptPackages(plan: SnapshotApplyPlan): { package: string; reason: string }[] {
  return homeSafety(plan).packages_kept;
}

export function previewRows(plan: SnapshotApplyPlan, snapshotLauncher: string | null): PreviewRows {
  const now = plan.current_values ?? {};
  const currentLauncher = plan.current_launcher ?? "—";
  const acting: PreviewRow[] = [];
  const unchanged: PreviewRow[] = [];

  for (const pkg of plan.packages_to_disable) {
    acting.push({
      item: pkg,
      now: "enabled",
      result: "Disable",
      kind: "disable",
    });
  }
  for (const pkg of plan.packages_absent_skipped ?? []) {
    unchanged.push({ item: pkg, now: "—", result: "Recorded absent; unchanged because saved-TV hardware identity is unverified", kind: "disable" });
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
  } else if (!plan.launcher_to_set && homeSafety(plan).launcher_disabled) {
    unchanged.push({
      item: "Home app",
      now: currentLauncher,
      result: `${homeSafety(plan).launcher_disabled} is disabled; skipped`,
      kind: "launcher",
    });
  } else if (!plan.launcher_to_set && snapshotLauncher !== null) {
    unchanged.push({ item: "Home app", now: currentLauncher, result: "Already Home", kind: "launcher" });
  }
  for (const kept of homeSafety(plan).packages_kept) {
    unchanged.push({ item: kept.package, now: "enabled", result: `Kept on: ${kept.reason}`, kind: "disable" });
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
  const absent = plan.packages_absent_skipped?.length ?? 0;
  if (absent > 0) parts.push(`${absent} recorded absent; unchanged`);
  const kept = homeSafety(plan).packages_kept.length;
  if (kept > 0) parts.push(`${kept} kept on for Home`);
  if (plan.launcher_to_set) parts.push("Home app");
  return parts.join(" · ");
}

/// The name a snapshot goes by everywhere on screen: its label, else the TV
/// it was taken from.
export function snapshotName(snap: Pick<SnapshotFile, "label" | "device_name">): string {
  return snap.label?.trim() || snap.device_name;
}

export interface RestoreOutcomeRow {
  item: string;
  /// The backend's own reason, shown as given.
  reason: string;
}

/// Everything a finished restore skipped, kept or failed, each with its reason.
/// Package entries come as `pkg (reason)` and settings as `key: reason`; an
/// entry in neither shape is shown whole rather than guessed at.
export function restoreOutcome(
  res: ApplyResult,
  plan: Pick<SnapshotApplyPlan, "launcher_to_set"> | null = null,
): RestoreOutcomeRow[] {
  const rows: RestoreOutcomeRow[] = [];
  for (const entry of res.packages_failed) {
    const m = /^(\S+) \(([\s\S]+)\)$/.exec(entry.trim());
    rows.push(m ? { item: m[1], reason: m[2] } : { item: entry, reason: "Couldn't disable it." });
  }
  const message = res.launcher_message?.trim() ?? "";
  if (!res.launcher_set && message) {
    rows.push({ item: "Home app", reason: message });
  } else if (!res.launcher_set && plan?.launcher_to_set) {
    rows.push({ item: "Home app", reason: `${plan.launcher_to_set} wasn't set as Home.` });
  }
  for (const entry of res.settings_failed) {
    const cut = entry.indexOf(": ");
    rows.push(
      cut > 0
        ? { item: entry.slice(0, cut), reason: entry.slice(cut + 2) }
        : { item: entry, reason: "Couldn't write it." },
    );
  }
  return rows;
}
