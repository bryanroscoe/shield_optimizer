// App-files finder and TV-side delete rules for the Files screen. Pure, so the
// #86 distinction (a search that could not run is not "no matches") and the
// delete confinement are testable without a device.

import type { FindResult } from "./types";
// Single source with desktop's Files tab: read the catalog, don't copy it.
import catalog from "../../../src/lib/app-files-catalog.json";

export interface AppFilesEntry {
  id: string;
  package: string;
  name: string;
  hint: string;
  search_dirs: string[];
  pattern: string;
}

export const appFilesCatalog: AppFilesEntry[] = catalog;

export type FindSummary =
  | { kind: "found"; hits: string[]; unsearched: string[] }
  | { kind: "none" }
  | { kind: "unsearched"; unsearched: string[] };

/// "none" only when every directory was actually searched and nothing matched.
export function summarizeFind(result: FindResult): FindSummary {
  if (result.hits.length > 0) {
    return { kind: "found", hits: result.hits, unsearched: result.unsearched };
  }
  if (result.unsearched.length > 0) return { kind: "unsearched", unsearched: result.unsearched };
  return { kind: "none" };
}

export function unsearchedMessage(dirs: string[]): string {
  return `Couldn't search ${dirs.join(", ")} — the TV didn't answer. Check the connection and try again.`;
}

export const NO_MATCHES_MESSAGE = "No matches — export from the app first, then search again.";

export function parentDir(path: string): string {
  const cut = path.lastIndexOf("/");
  return cut <= 0 ? "/" : path.slice(0, cut);
}

export function baseName(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1);
}

/// Mirrors the backend guard (crates/core commands::files): a delete target
/// must be strictly inside /sdcard. The backend refuses anything else anyway;
/// this keeps the button off rows it would refuse.
export function canDeleteOnTv(path: string): boolean {
  if (!path.startsWith("/sdcard/") || path.length <= "/sdcard/".length) return false;
  if (/[\u0000-\u001f\u007f]/.test(path)) return false;
  return !path.split("/").some((segment) => segment === "..");
}
