// The Apps screen's two sections, merged and filtered in one place: the
// catalogued ("Recognised") apps from `app_list_for_device` + `package_states`,
// and everything else from `list_other_packages`. Pure, so the rules that
// decide whether a user can find an app are tested without a webview.
//
// Mirrors desktop's App List (v2/src/routes/devices/[serial]/+page.svelte):
// one search over name and package id across both sections, "Hide not
// installed" for catalogue rows, "Show system apps" for everything else.

import type { AppEntry, OtherPackage } from "./types";
import type { PackageState } from "./recommendation";

export type StatusFilter = "all" | "enabled" | "disabled";

/// One row as the screen, the detail sheet and the action menu see it.
export interface AppItem {
  package: string;
  /// Friendly name when one is known; null shows the package id instead.
  name: string | null;
  /// Null for catalogue rows: the catalogue does not say, and guessing would
  /// put a "system" label on something we never read.
  system: boolean | null;
  /// Null when the state read failed or did not cover this package. Null is
  /// unknown, never "enabled".
  state: PackageState | null;
  /// The catalogue entry, for recognised apps only.
  entry: AppEntry | null;
  description: string | null;
}

/// One entry in the long-press menu. `run` is the screen's own handler.
export interface AppMenuItem {
  id: string;
  label: string;
  icon: string;
  disabled?: boolean;
  danger?: boolean;
  run: () => void;
}

export interface ListFilters {
  query: string;
  status: StatusFilter;
  showSystem: boolean;
  hideNotInstalled: boolean;
}

export function normalizeQuery(query: string): string {
  return query.trim().toLowerCase();
}

/// Name or package id, case-insensitive. An empty query matches everything.
export function matchesQuery(item: Pick<AppItem, "name" | "package">, query: string): boolean {
  const q = normalizeQuery(query);
  if (!q) return true;
  return (item.name ?? "").toLowerCase().includes(q) || item.package.toLowerCase().includes(q);
}

/// The state map with anything that is not one of the three known states
/// dropped, so a malformed reply reads as "unknown" rather than as a state.
export function validatedStates(
  packages: string[],
  raw: unknown,
): Record<string, PackageState> {
  const out: Record<string, PackageState> = {};
  if (typeof raw !== "object" || raw === null || Array.isArray(raw)) return out;
  const record = raw as Record<string, unknown>;
  for (const pkg of packages) {
    if (!Object.prototype.hasOwnProperty.call(record, pkg)) continue;
    const s = record[pkg];
    if (s === "enabled" || s === "disabled" || s === "missing") out[pkg] = s;
  }
  return out;
}

export function catalogItem(entry: AppEntry, state: PackageState | null): AppItem {
  return {
    package: entry.package,
    name: entry.name || null,
    system: null,
    state,
    entry,
    description: entry.optimize_description || null,
  };
}

export function otherItem(app: OtherPackage): AppItem {
  return {
    package: app.package,
    name: app.name || null,
    system: app.system,
    state: app.enabled ? "enabled" : "disabled",
    entry: null,
    description: app.description || null,
  };
}

/// Catalogue rows in catalogue order. A package that also comes back from the
/// other-packages list is kept only here, so no app is listed twice.
export function catalogItems(
  entries: AppEntry[],
  states: Record<string, PackageState>,
): AppItem[] {
  const seen = new Set<string>();
  const out: AppItem[] = [];
  for (const entry of entries) {
    if (seen.has(entry.package)) continue;
    seen.add(entry.package);
    out.push(catalogItem(entry, states[entry.package] ?? null));
  }
  return out;
}

export function otherItems(others: OtherPackage[], catalogPackages: Set<string>): AppItem[] {
  return others.filter((o) => !catalogPackages.has(o.package)).map(otherItem);
}

function matchesStatus(item: AppItem, status: StatusFilter): boolean {
  if (status === "enabled") return item.state === "enabled";
  if (status === "disabled") return item.state === "disabled";
  return true;
}

/// Catalogue rows passing every filter. "Show system apps" does not apply:
/// the catalogue does not record which entries are system packages, and most
/// of what it covers is preinstalled, so hiding them would hide the section.
export function filterCatalog(items: AppItem[], f: ListFilters): AppItem[] {
  return items.filter(
    (item) =>
      !(f.hideNotInstalled && item.state === "missing") &&
      matchesStatus(item, f.status) &&
      matchesQuery(item, f.query),
  );
}

export function filterOthers(items: AppItem[], f: ListFilters): AppItem[] {
  return items.filter(
    (item) =>
      (f.showSystem || item.system !== true) &&
      matchesStatus(item, f.status) &&
      matchesQuery(item, f.query),
  );
}

/// Rows a filter toggle is hiding from the current search, so the empty state
/// can offer to reveal them rather than claim nothing matched.
export function hiddenMatches(
  catalog: AppItem[],
  others: AppItem[],
  f: ListFilters,
): { notInstalled: number; system: number } {
  return {
    notInstalled: f.hideNotInstalled
      ? filterCatalog(catalog, { ...f, hideNotInstalled: false }).length -
        filterCatalog(catalog, f).length
      : 0,
    system: f.showSystem
      ? 0
      : filterOthers(others, { ...f, showSystem: true }).length - filterOthers(others, f).length,
  };
}

/// Rows that are (or may be) on the TV: everything except catalogue rows
/// known to be missing. Used for both sides of "N of M visible".
export function onDeviceCount(items: AppItem[]): number {
  return items.filter((i) => i.state !== "missing").length;
}
