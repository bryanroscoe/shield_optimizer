// Shared by the global Snapshots page and a device's Snapshot tab, so both
// describe a restore's skipped Home app and kept packages the same way. The
// mobile counterpart is `mobile/src/lib/snapshotPreview.ts`.

import type { ApplyResult, KeptPackage, SnapshotApplyPlan } from "$lib/types";

/// Why the restore leaves the snapshot's Home app alone, or null when it sets
/// Home or Home already matches.
export function launcherSkipReason(plan: SnapshotApplyPlan): string | null {
  if (plan.launcher_to_set) return null;
  if (plan.launcher_not_installed) return `${plan.launcher_not_installed} isn't installed; skipped`;
  if (plan.launcher_disabled) return `${plan.launcher_disabled} is disabled; enable it first, then restore`;
  return null;
}

/// True only when the snapshot names a Home app and it already holds Home
/// here. A skipped launcher is never "no change".
export function homeAlreadyMatches(plan: SnapshotApplyPlan, snapshotLauncher: string | null): boolean {
  return snapshotLauncher !== null && !plan.launcher_to_set && launcherSkipReason(plan) === null;
}

/// A restore reports packages it kept on for Home inside `packages_failed` as
/// `pkg (kept: reason)`; those are not failures, so they are listed apart.
export function splitRestoreFailures(res: ApplyResult): { kept: KeptPackage[]; failed: string[] } {
  const kept: KeptPackage[] = [];
  const failed: string[] = [];
  for (const entry of res.packages_failed) {
    const m = /^(\S+) \(kept: ([^)]+)\)$/.exec(entry.trim());
    if (m) kept.push({ package: m[1], reason: m[2] });
    else failed.push(entry);
  }
  return { kept, failed };
}
