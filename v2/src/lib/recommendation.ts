// What we suggest doing with one catalog app, in one place.
//
// The App List, the Optimize wizard's defaults and Health's Suggestion column
// all read this, so one app gets the same label and the same method on every
// screen.
//
// Pure: catalog entry, on-device state and safety verdict in, suggestion out.

import type { AppEntry } from "$lib/types";
import { isBlocked, type SafetyStatus } from "$lib/safety";
import sideloadCatalog from "$lib/sideload-catalog.json";

export type PackageState = "enabled" | "disabled" | "missing";
export type RemovalMethod = "disable" | "uninstall";

export type Recommendation =
  | { kind: "done"; label: string }
  | { kind: "act"; label: string; action: RemovalMethod }
  | { kind: "review"; label: string; action: RemovalMethod }
  | { kind: "restore"; label: string }
  | { kind: "unavailable"; label: string }
  | { kind: "keep"; label: string };

export const NO_CHANGE_LABEL = "No change needed";

export interface SideloadSource {
  name: string;
  url: string;
}

const SIDELOAD_SOURCES: Record<string, SideloadSource> = Object.fromEntries(
  (sideloadCatalog as { name: string; package: string; url: string }[]).map((s) => [
    s.package,
    { name: s.name, url: s.url },
  ]),
);

/// Where a sideloaded app comes from, when we know. Null means we don't — which
/// is not the same as "it's on the Play Store".
export function sideloadSource(pkg: string): SideloadSource | null {
  return SIDELOAD_SOURCES[pkg] ?? null;
}

/// Can the user get this back from the store after uninstalling it? Mirrors
/// the engine's `AppEntry::reinstallable` (crates/core/src/engine/types.rs).
export function isReinstallable(a: Pick<AppEntry, "play_store" | "defunct">): boolean {
  return a.play_store || !!a.defunct;
}

/// The method it is safe to *recommend* — mirrors the engine's `safe_method`:
/// an uninstall you cannot undo from the store is downgraded to the
/// reversible disable.
export function effectiveMethod(a: AppEntry): RemovalMethod {
  return a.method === "uninstall" && !isReinstallable(a) ? "disable" : a.method;
}

/// Whether Uninstall may be *offered* at all. Store apps, defunct services,
/// and sideloads whose source we can name — the confirm prompt names it. A
/// preinstalled app with no store listing gets Disable only.
export function canOfferUninstall(a: AppEntry): boolean {
  return isReinstallable(a) || sideloadSource(a.package) !== null;
}

/// Whether App List offers Uninstall beside Disable for a catalog store app
/// (#325). Only an ordinary store app: a high-risk or advanced system piece
/// (Google TV Home, Setup Wraith, the voice service) or one the TV ships with
/// and Reinstall restores keeps Disable as its only removal on that row.
export function canOfferStoreUninstall(a: AppEntry): boolean {
  return a.play_store && !a.defunct && !a.default_restore && (a.risk === "safe" || a.risk === "medium");
}

export function reviewLabel(method: RemovalMethod): string {
  return method === "uninstall" ? "Review: uninstall if unused" : "Review: disable if unused";
}

/// What we'd suggest for this row given its current on-device state.
/// `act` = recommended by default. `review` = worth removing if you don't use
/// it, never a default. `restore` = gone, and worth bringing back.
/// `done` / `keep` = nothing to do. `unavailable` = we can't say yet.
export function recommendation(
  a: AppEntry,
  state: PackageState | null,
  safety: SafetyStatus | undefined,
): Recommendation {
  if (state === null) return { kind: "unavailable", label: "State unavailable" };
  if (state === "missing") {
    if (a.default_restore) return { kind: "restore", label: "Reinstall" };
    if (a.default_optimize && a.method === "uninstall") {
      return { kind: "done", label: "Already uninstalled" };
    }
    return { kind: "keep", label: NO_CHANGE_LABEL };
  }
  const method = effectiveMethod(a);
  if (safety?.status !== "ready") return { kind: "unavailable", label: "Safety unavailable" };
  if (isBlocked(safety.verdict)) return { kind: "done", label: "Protected" };
  if (safety.verdict.kind === "unknown") {
    return state === "enabled"
      ? { kind: "review", label: reviewLabel(method), action: method }
      : { kind: "keep", label: NO_CHANGE_LABEL };
  }
  if (a.default_optimize) {
    if (method === "disable") {
      return state === "disabled"
        ? { kind: "done", label: "Already disabled" }
        : { kind: "act", label: "Disable", action: "disable" };
    }
    return { kind: "act", label: "Uninstall", action: "uninstall" };
  }
  if (a.review && state === "enabled") {
    return { kind: "review", label: reviewLabel(method), action: method };
  }
  return { kind: "keep", label: NO_CHANGE_LABEL };
}

export interface UninstallOutcome {
  /// One line for the row, shown while Uninstall is the chosen action.
  short: string;
  /// The paragraph the confirm prompt carries.
  detail: string;
}

/// What getting an app back takes after Uninstall. Every uninstall is
/// `pm uninstall --user 0`: it removes the app for this TV's user. A copy that
/// came with the TV keeps its APK, but the App List offers Reinstall only for
/// apps the catalog recommends restoring, so only those say so.
export function uninstallOutcome(pkg: string, entry: AppEntry | null): UninstallOutcome {
  const reinstall = entry?.default_restore
    ? " If it came with the TV, Reinstall in the App List can bring it back while its APK is still on the TV."
    : "";
  if (entry?.play_store) {
    return {
      short: "Get it back from the Play Store",
      detail: `Uninstall removes it for this TV's user. Get it back from the Play Store.${reinstall}`,
    };
  }
  const source = sideloadSource(pkg);
  if (source) {
    return {
      short: `Not on the Play Store: reinstall from ${source.name}`,
      detail: `Not on the Play Store. To get it back you would reinstall ${source.name} from ${source.url}. Back up its APK first if you might want this exact version.${reinstall}`,
    };
  }
  if (entry?.defunct) {
    return {
      short: "Discontinued: no way to download it again",
      detail: `This service is discontinued and has no store listing, so once it is uninstalled there is nowhere to download it again.${reinstall}`,
    };
  }
  return {
    short: "May not be on the Play Store",
    detail: `We can't confirm this is on the Play Store. If it isn't, getting it back needs its APK, so back it up first.${reinstall}`,
  };
}

/// The paragraph an uninstall confirm carries: how, or whether, the app can
/// come back.
export function uninstallNote(pkg: string, entry: AppEntry | null): string {
  return uninstallOutcome(pkg, entry).detail;
}
