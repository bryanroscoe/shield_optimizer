// Shared by the global Snapshots page and a device's Snapshot tab, so both
// describe a restore's skipped Home app and kept packages the same way. The
// mobile counterpart is `mobile/src/lib/snapshotPreview.ts`.

import type { ApplyResult, KeptPackage, SnapshotApplyPlan } from "$lib/types";

/// Why the restore leaves the snapshot's Home app alone, or null when it sets
/// Home or Home already matches.
export function launcherSkipReason(plan: SnapshotApplyPlan): string | null {
  if (plan.launcher_to_set) return null;
  if (plan.launcher_not_installed) return `${plan.launcher_not_installed} isn't installed; skipped`;
  if (plan.launcher_disabled) return `${plan.launcher_disabled} is disabled; re-enable it first, then restore`;
  return null;
}

/// True only when the snapshot names a Home app and it already holds Home
/// here. A skipped launcher is never "no change".
export function homeAlreadyMatches(plan: SnapshotApplyPlan, snapshotLauncher: string | null): boolean {
  return snapshotLauncher !== null && !plan.launcher_to_set && launcherSkipReason(plan) === null;
}

/// A restore reports packages it kept on for Home inside `packages_failed` as
/// `pkg (kept: reason)`, and protected ones as `pkg (protected: reason)`;
/// neither is a failure, so they are listed apart.
export function splitRestoreFailures(res: ApplyResult): {
  kept: KeptPackage[];
  protected: KeptPackage[];
  failed: string[];
} {
  const kept: KeptPackage[] = [];
  const held: KeptPackage[] = [];
  const failed: string[] = [];
  for (const entry of res.packages_failed) {
    const k = /^(\S+) \(kept: ([^)]+)\)$/.exec(entry.trim());
    const p = /^(\S+) \(protected: ([\s\S]+)\)$/.exec(entry.trim());
    if (k) kept.push({ package: k[1], reason: k[2] });
    else if (p) held.push({ package: p[1], reason: p[2] });
    else failed.push(entry);
  }
  return { kept, protected: held, failed };
}

/// A failure entry is `pkg (why)`; split it so the list can show the name
/// first. Anything else is shown as it came.
export function failureOutcome(entry: string): { package: string; reason: string | null } {
  const m = /^(\S+) \((.+)\)$/s.exec(entry.trim());
  return m ? { package: m[1], reason: m[2] } : { package: entry.trim(), reason: null };
}
