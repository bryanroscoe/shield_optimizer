// Pure view logic for the Launcher screen. Mirrors the desktop Launcher tab
// (src/routes/devices/[serial]/+page.svelte) so the two apps reach the same
// conclusions from the same launcher rows.

import type { LauncherStatus, OtherPackage } from "./types";

/// "off": disabled. "on": enabled while an enabled stock launcher can still
/// hold Home. "risk": enabled while stock is off, so it will likely take Home.
/// The role can name a custom launcher while the helper's higher-priority
/// filter wins the Home button, so the current launcher never clears "risk".
export type SetupHelperState = "off" | "on" | "risk";

export interface SetupHelperView {
  row: LauncherStatus;
  state: SetupHelperState;
}

/// Google TV's setup helper (Setup Wraith) whenever it is installed. A
/// disabled one no longer answers the Home query, so the backend reads it from
/// the package lists and it stays a row either way.
export function setupHelperView(launchers: LauncherStatus[]): SetupHelperView | null {
  const row = launchers.find((l) => l.setup_helper && l.installed);
  if (!row) return null;
  if (!row.enabled) return { row, state: "off" };
  return { row, state: launchers.some((l) => l.stock && l.enabled) ? "on" : "risk" };
}

/// A stock takeover also turns the setup helper off: it is enabled and an
/// enabled stock launcher names it in the catalog's `disable_with`.
export function takeoverTurnsOffSetupHelper(launchers: LauncherStatus[]): boolean {
  const helper = launchers.find((l) => l.setup_helper && l.installed);
  if (!helper?.enabled) return false;
  return launchers.some(
    (l) => l.stock && l.enabled && (l.entry.disable_with ?? []).includes(helper.entry.package),
  );
}

export interface StockDisableGate {
  allowed: boolean;
  /// Why the button is off. Empty when allowed.
  reason: string;
}

/// Whether "Disable stock launcher" may be offered for the picked app. It stays
/// off until the picked app holds Home, or until `set_home_any` reported that
/// the stock launcher is what's in the way for exactly this app. The backend
/// re-checks all of it (and the last-Home-handler guard); this only keeps the
/// button from inviting a request it will refuse.
export function stockDisableGate(input: {
  launchers: LauncherStatus[];
  choice: string;
  currentPkg: string | null;
  stockHoldsHomeFor: string | null;
}): StockDisableGate {
  const { launchers, choice, currentPkg, stockHoldsHomeFor } = input;
  if (!choice) return { allowed: false, reason: "Choose the app that should take over Home first." };
  if (!launchers.some((l) => l.stock && l.enabled)) {
    return { allowed: false, reason: "No enabled stock launcher on this TV." };
  }
  if (launchers.some((l) => l.stock && l.entry.package === choice)) {
    return { allowed: false, reason: "Pick the app that should take over, not the stock launcher." };
  }
  if (launchers.some((l) => l.setup_helper && l.entry.package === choice)) {
    return {
      allowed: false,
      reason: "That is Google TV's setup helper, not a launcher. Pick a real launcher.",
    };
  }
  if (currentPkg === choice || stockHoldsHomeFor === choice) return { allowed: true, reason: "" };
  return {
    allowed: false,
    reason:
      "Set the app as Home first. This stays off until it's Home, or until Android says the stock launcher is what's in the way.",
  };
}

/// The picker's rows: user apps unless system apps are revealed, filtered by a
/// case-insensitive name/package search, one row per package, sorted by name.
export function filterHomeCandidates(
  packages: OtherPackage[],
  query: string,
  showSystem: boolean,
): OtherPackage[] {
  const q = query.trim().toLowerCase();
  const unique = new Map<string, OtherPackage>();
  for (const p of packages) {
    if (!showSystem && p.system) continue;
    if (
      q &&
      !p.package.toLowerCase().includes(q) &&
      !(p.name ?? "").toLowerCase().includes(q)
    ) {
      continue;
    }
    if (!unique.has(p.package)) unique.set(p.package, p);
  }
  return [...unique.values()].sort((a, b) =>
    (a.name ?? a.package).localeCompare(b.name ?? b.package),
  );
}

/// The host to show for a launcher's source site, or null when the catalog
/// value is not an http(s) URL (nothing is offered for anything else).
export function sourceSiteHost(url: string | null | undefined): string | null {
  if (!url) return null;
  try {
    const parsed = new URL(url);
    if (parsed.protocol !== "https:" && parsed.protocol !== "http:") return null;
    return parsed.host.replace(/^www\./, "") || null;
  } catch {
    return null;
  }
}

/// True once the launcher list reports `pkg` installed.
export function isInstalled(launchers: LauncherStatus[], pkg: string): boolean {
  return launchers.some((l) => l.entry.package === pkg && l.installed);
}

/// The message for a finished `disable_stock_launcher`. Success is only
/// claimed when the backend verified it; otherwise its own sentence is shown.
/// The Setup Wraith note comes from the re-read after the call, not from the
/// prediction before it: `helperAfter` is null when there is no helper row or
/// the re-read failed, and then nothing is said about it.
export function stockDisableMessage(
  result: { ok: boolean; last_error: string | null },
  target: string,
  helperExpectedOff: boolean,
  helperAfter: SetupHelperState | null,
): string {
  if (!result.ok) return result.last_error ?? "The stock launcher was left alone.";
  let wraith = "";
  if (helperExpectedOff && helperAfter === "off") {
    wraith = " Google TV's setup helper (Setup Wraith) was turned off too.";
  } else if (helperAfter === "risk") {
    wraith = " Setup Wraith still reads as on, so it may take the Home button back; see below.";
  }
  return `The stock launcher is disabled and ${target} is Home.${wraith} Re-enable stock from the list above any time.`;
}
