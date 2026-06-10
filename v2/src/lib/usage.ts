// Shared "last used" interpretation — UsageBadge renders it per-row and the
// Optimize wizard's review callout aggregates it; both must agree on what
// counts as stale.

import type { AppUsage } from "./types";

const DAY_MS = 86_400_000;

/// Days since last foreground use. `null` = no usable record, which is NOT
/// literally "never": usagestats keeps ~1 year of rolling buckets and resets
/// on a factory wipe, so old usage ages out (an absent map entry means the
/// same thing).
export function daysSinceUsed(u: AppUsage | undefined): number | null {
  if (!u || !u.last_used || u.launch_count === 0) return null;
  // "YYYY-MM-DD HH:MM:SS" → parse as local time (device-local, close enough).
  const then = new Date(u.last_used.replace(" ", "T"));
  if (Number.isNaN(then.getTime())) return null;
  return Math.floor((Date.now() - then.getTime()) / DAY_MS);
}

export function usageLabel(u: AppUsage | undefined): string {
  const days = daysSinceUsed(u);
  if (days === null) return "no recent use";
  if (days <= 0) return "used today";
  if (days === 1) return "used yesterday";
  if (days < 30) return `last used ${days}d ago`;
  if (days < 365) return `last used ${Math.floor(days / 30)}mo ago`;
  return `last used ${Math.floor(days / 365)}y ago`;
}

/// Stale = a removal candidate: no record at all, or untouched for 30+ days.
export function isStaleUsage(u: AppUsage | undefined): boolean {
  const days = daysSinceUsed(u);
  return days === null || days >= 30;
}
