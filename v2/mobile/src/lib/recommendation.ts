// What we suggest doing with one catalog app, in one place. Ported from
// desktop's v2/src/lib/recommendation.ts; keep the two in step so the same app
// reads the same suggestion on phone and desktop.
//
// Pure: catalog entry, on-device state and safety verdict in, suggestion out.
// Uninstall is only ever recommended when the store can give the app back
// (`play_store || defunct`); otherwise it is downgraded to the reversible
// Disable. Nothing here is ever labelled "Remove".

import type { AppEntry } from "./types";
import { isBlocked, type SafetyStatus } from "./safety";
// Single source with desktop: the sideload catalog lives beside desktop's
// recommendation.ts and is read here rather than copied.
import sideloadCatalog from "../../../src/lib/sideload-catalog.json";

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

/// The extra paragraph an uninstall confirm carries when the store can't give
/// the app back. Null when it can, so store apps keep the short prompt.
export function uninstallNote(pkg: string, entry: AppEntry | null): string | null {
  if (entry && isReinstallable(entry)) return null;
  const source = sideloadSource(pkg);
  if (source) {
    return `Not on the Play Store. To get it back you would reinstall ${source.name} from ${source.url} — back up its APK first if you might want this exact version.`;
  }
  return "We can't confirm this is on the Play Store. If it isn't, getting it back needs its APK — back it up first.";
}
