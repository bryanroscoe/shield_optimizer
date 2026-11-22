import type { AppEntry } from "$lib/types";
import { SAFETY_TIERS, type Safety, type SafetyKind } from "$lib/safety";
import { uninstallOutcome } from "$lib/recommendation";

/// One app the run will change, as the confirmation names it.
export interface RunConfirmItem {
  package: string;
  name: string;
  action: "disable" | "uninstall" | "enable";
  /// The safety re-checked after confirmation; null for Enable.
  verdict: Safety | null;
  entry: AppEntry | null;
}

export interface RunConfirmApp {
  package: string;
  name: string;
  verb: "Disable" | "Uninstall" | "Re-enable";
  /// The safety reason. Shown for Caution, Unknown and Protected, where it is
  /// the consequence being agreed to; folded for Safe.
  reason: string;
  /// How, or whether, an uninstalled app comes back, in one short line.
  outcome: string | null;
  /// The full uninstall note, folded with the reason.
  outcomeDetail: string | null;
  /// No store listing to get it back from (a discontinued service or a
  /// sideloaded app): always shown.
  noWayBack: boolean;
}

export interface RunConfirmSection {
  kind: SafetyKind | "enable";
  label: string;
  cls: string;
  /// Whether each app's reason is shown rather than folded.
  showReason: boolean;
  apps: RunConfirmApp[];
}

const TIER_ORDER: SafetyKind[] = ["never_disable", "unknown", "caution", "safe"];

/// The Optimize/Restore confirmation, in sections: one per safety tier, most
/// serious first, then re-enables. Every app is named once.
export function runConfirmation(items: RunConfirmItem[]): {
  sections: RunConfirmSection[];
  uninstalls: number;
} {
  const byTier = new Map<SafetyKind, RunConfirmApp[]>();
  const enables: RunConfirmApp[] = [];
  let uninstalls = 0;
  for (const item of items) {
    if (item.action === "enable") {
      enables.push({
        package: item.package,
        name: item.name,
        verb: "Re-enable",
        reason: "",
        outcome: null,
        outcomeDetail: null,
        noWayBack: false,
      });
      continue;
    }
    const uninstall = item.action === "uninstall";
    if (uninstall) uninstalls++;
    const outcome = uninstall ? uninstallOutcome(item.package, item.entry) : null;
    const noWayBack = uninstall && !item.entry?.play_store;
    // A removal is never listed as a re-enable. Without a verdict it can claim
    // nothing about safety, so it reads as Unknown.
    const kind = item.verdict?.kind ?? "unknown";
    const apps = byTier.get(kind) ?? [];
    byTier.set(kind, apps);
    apps.push({
      package: item.package,
      name: item.name,
      verb: uninstall ? "Uninstall" : "Disable",
      reason: item.verdict?.reason.trim() ?? "",
      outcome: outcome?.short ?? null,
      outcomeDetail: outcome?.detail ?? null,
      noWayBack,
    });
  }
  const sections: RunConfirmSection[] = TIER_ORDER.filter((kind) => byTier.has(kind)).map((kind) => ({
    kind,
    label: SAFETY_TIERS[kind].label,
    cls: SAFETY_TIERS[kind].cls,
    showReason: kind !== "safe",
    apps: byTier.get(kind)!,
  }));
  if (enables.length > 0) {
    sections.push({ kind: "enable", label: "Turn back on", cls: "", showReason: false, apps: enables });
  }
  return { sections, uninstalls };
}

/// The native confirm for one app's Disable or Uninstall: the tier (with its
/// reason when that is the consequence), then what happens and how to undo it.
export function removalConfirmation(
  action: "disable" | "uninstall",
  pkg: string,
  name: string,
  safety: Safety,
  entry: AppEntry | null,
  supportWarning: string | null,
): string {
  const tier = SAFETY_TIERS[safety.kind].label;
  const reason = safety.reason.trim();
  const verdict = safety.kind === "safe" || !reason ? tier : `${tier}: ${reason}`;
  const consequences =
    action === "uninstall"
      ? [
          "Removed for this TV's user.",
          sentence(uninstallOutcome(pkg, entry).short),
          ...(entry?.default_restore ? ["Reinstall in the App List can bring it back while its APK is still on the TV."] : []),
        ]
      : ["Stopped for this TV's user. It stays installed.", "Enable in the App List turns it back on."];
  // The package id tells apart two apps that share a display name.
  const subject = name === pkg ? name : `${name} (${pkg})`;
  return [
    `${action === "uninstall" ? "Uninstall" : "Disable"} ${subject}?`,
    sentence(verdict),
    consequences.map((line) => `• ${line}`).join("\n"),
    supportWarning ?? "",
  ]
    .filter(Boolean)
    .join("\n\n");
}

function sentence(text: string): string {
  return /[.!?…]$/.test(text) ? text : `${text}.`;
}
