// Health's Suggestion for one memory row — the mobile port of desktop's
// `memorySuggestion` (routes/devices/[serial]/+page.svelte).
//
// A row is an app only once the TV confirms its package candidate is
// installed; then it gets the same `recommendation()` the App List and
// Optimize use. Any other name is a process: it is classified catalog-free
// (`process_safety_info`) and never offered as something to remove. When the
// installed list could not be read nothing is confirmed, and a name with
// package shape says so instead of being called "Not an app".

import type { AppEntry, MemoryEntry } from "./types";
import { recommendation, type PackageState, type Recommendation } from "./recommendation";
import { CHECKING_LABEL, safetyLabel, type SafetyStatus } from "./safety";

/// Installed packages and their state, or why we don't have them.
export type InstalledState =
  | { status: "loading" }
  | { status: "failed" }
  | { status: "ready"; packages: Map<string, PackageState> };

export type CatalogState =
  | { status: "loading" }
  | { status: "failed" }
  | { status: "ready"; entries: Map<string, AppEntry> };

export type MemorySuggestion =
  | { kind: "checking" }
  | { kind: "unconfirmed" }
  | { kind: "recommendation"; pkg: string; rec: Recommendation }
  | { kind: "verdict"; pkg: string; status: SafetyStatus | undefined }
  | { kind: "process"; status: SafetyStatus | undefined };

/// The row's package, only once the device has confirmed it is installed.
export function confirmedPackage(m: MemoryEntry, installed: InstalledState): string | null {
  if (installed.status !== "ready" || m.package === null) return null;
  return installed.packages.has(m.package) ? m.package : null;
}

/// What to ask about this row: the package verdict for a confirmed app, the
/// catalog-free process verdict for anything else. Null while it can't be
/// decided yet.
export function safetyQuery(
  m: MemoryEntry,
  installed: InstalledState,
): { kind: "package" | "process"; name: string } | null {
  if (installed.status === "loading") return null;
  const pkg = confirmedPackage(m, installed);
  return pkg ? { kind: "package", name: pkg } : { kind: "process", name: m.process };
}

export function memorySuggestion(
  m: MemoryEntry,
  installed: InstalledState,
  catalog: CatalogState,
  safety: SafetyStatus | undefined,
): MemorySuggestion {
  if (installed.status === "loading") return { kind: "checking" };
  const pkg = confirmedPackage(m, installed);
  if (pkg === null) {
    if (installed.status === "failed" && m.package !== null) return { kind: "unconfirmed" };
    return { kind: "process", status: safety };
  }
  // A verdict that flips to a recommendation a second later is a column that
  // cannot be trusted at a glance, so wait for the catalog.
  if (catalog.status === "loading") return { kind: "checking" };
  const entry = catalog.status === "ready" ? catalog.entries.get(pkg) : undefined;
  if (entry && installed.status === "ready") {
    const state = installed.packages.get(pkg) ?? null;
    return { kind: "recommendation", pkg, rec: recommendation(entry, state, safety) };
  }
  return { kind: "verdict", pkg, status: safety };
}

/// Label plus a style hint. `act` reads as the accent, `review` /
/// `unavailable` as amber ("look at this first"), and anything that needs
/// nothing recedes — the same scheme as desktop's Suggestion column.
export function suggestionDisplay(s: MemorySuggestion): {
  label: string;
  tone: "act" | "warn" | "muted" | "safe" | "caution" | "blocked" | "unknown";
} {
  switch (s.kind) {
    case "checking":
      return { label: CHECKING_LABEL, tone: "muted" };
    case "unconfirmed":
      return { label: "Not checked", tone: "warn" };
    case "process":
      return { label: "Not an app", tone: "muted" };
    case "recommendation":
      if (s.rec.kind === "act") return { label: s.rec.label, tone: "act" };
      if (s.rec.kind === "review" || s.rec.kind === "unavailable") {
        return { label: s.rec.label, tone: "warn" };
      }
      return { label: s.rec.label, tone: "muted" };
    case "verdict": {
      const label = safetyLabel(s.status);
      if (s.status?.status !== "ready") {
        return { label, tone: s.status?.status === "checking" ? "muted" : "warn" };
      }
      const kind = s.status.verdict.kind;
      return {
        label,
        tone: kind === "never_disable" ? "blocked" : kind,
      };
    }
  }
}
