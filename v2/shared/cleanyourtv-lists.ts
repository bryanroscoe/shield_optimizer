// CleanYourTV's per-model lists, ranked after the app's own catalog for apps it
// has no record of. Pure functions shared by desktop and Android: matching a TV
// to the lists that name it, choosing which lists to use, reading which apps a
// list turns off and which list decides, and pulling the author's note for a
// line. Nothing here decides safety or picks an action: the screens offer what
// this returns as "remove if unused" rows that start on Keep and can only be
// disabled, under the name of the list they came from.

import type { SourceProfile } from "./profile-catalog";

/// What the TV says about itself. Every field may be missing or empty.
export interface TvIdentity {
  manufacturer?: string | null;
  brand?: string | null;
  model?: string | null;
  device_codename?: string | null;
}

export type MatchReason = "codename" | "model" | "brand";

export interface ProfileCandidate {
  profile: SourceProfile;
  reasons: MatchReason[];
}

/// One app a list turns off.
export interface ListedApp {
  package: string;
  /// 1-based line in the list's debloat.txt.
  line: number;
}

/// Words of a label, split at every separator and also kept whole across
/// underscores and hyphens, so `BRAVIA_VH2` and its parts are both words.
const tokens = (text: string) => {
  const lower = text.toLowerCase();
  return new Set([...lower.split(/[^a-z0-9]+/), ...lower.split(/[^a-z0-9_-]+/)].filter(Boolean));
};
/// A label as space-separated words, for whole-word phrase matching.
const words = (text: string) => ` ${text.toLowerCase().split(/[^a-z0-9]+/).filter(Boolean).join(" ")} `;
const squash = (text: string) => text.toLowerCase().replace(/[^a-z0-9]/g, "");

/// Lists that may be this TV's, best first. A list that names the TV's device
/// codename as a word is a match; when any list does, those are the only ones,
/// because a shared model name ("Chromecast") also fits sibling devices with
/// another codename. With no codename match, lists containing the full model
/// name from the same maker are offered as suggestions to pick from. A model
/// name across makers ("Box R 4K Plus") and a brand alone match nothing.
export function candidateProfiles(
  profiles: SourceProfile[],
  tv: TvIdentity,
  limit = 4,
): ProfileCandidate[] {
  const codename = (tv.device_codename ?? "").trim().toLowerCase();
  const codenameKey = codename.length >= 3 ? codename : "";
  const codenameSquashed = squash(codenameKey);
  const modelKey = squash(tv.model ?? "").length >= 5 ? words(tv.model ?? "") : "";
  const makers = [tv.manufacturer, tv.brand]
    .map((value) => (value ?? "").trim().toLowerCase())
    .filter((value) => value.length >= 2);
  const scored: { candidate: ProfileCandidate; score: number }[] = [];
  for (const profile of profiles) {
    const context = profile.upstream_context;
    const labelWords = new Set([...tokens(context.tv), ...tokens(profile.profile)]);
    const codenameHit = !!codenameKey && (labelWords.has(codenameKey) || labelWords.has(codenameSquashed));
    const modelHit = !!modelKey && words(context.tv).includes(modelKey);
    const brandHit = makers.some(
      (maker) => context.brand.trim().toLowerCase() === maker || labelWords.has(maker),
    );
    if (!codenameHit && !(modelHit && brandHit)) continue;
    const reasons: MatchReason[] = [];
    if (codenameHit) reasons.push("codename");
    if (modelHit) reasons.push("model");
    if (brandHit) reasons.push("brand");
    scored.push({
      candidate: { profile, reasons },
      score: (codenameHit ? 2 : 0) + (modelHit ? 1 : 0) + (brandHit ? 1 : 0),
    });
  }
  const byCodename = scored.filter((entry) => entry.candidate.reasons.includes("codename"));
  return (byCodename.length > 0 ? byCodename : scored)
    .sort((a, b) => b.score - a.score || a.candidate.profile.profile.localeCompare(b.candidate.profile.profile))
    .slice(0, limit)
    .map((entry) => entry.candidate);
}

/// The apps a list turns off: its active disable lines. A package the same list
/// also keeps commented out is left out, because the author said both and we
/// can't tell which applies.
export function listedForDisable(profile: SourceProfile): ListedApp[] {
  const kept = new Set(
    profile.entries.filter((entry) => entry.upstream_form === "commented_out").map((entry) => entry.package),
  );
  const seen = new Set<string>();
  const listed: ListedApp[] = [];
  for (const entry of [...profile.entries].sort((a, b) => a.line - b.line)) {
    if (entry.upstream_form !== "active_disable" || kept.has(entry.package) || seen.has(entry.package)) continue;
    seen.add(entry.package);
    listed.push({ package: entry.package, line: entry.line });
  }
  return listed;
}

const PACKAGE_COMMENT = /^#\s*[A-Za-z][\w]*(\.[\w-]+)+\s*(#.*)?$/;
const NOTE_LIMIT = 220;

function clip(text: string): string {
  const flat = text.replace(/\s+/g, " ").trim();
  return flat.length > NOTE_LIMIT ? `${flat.slice(0, NOTE_LIMIT - 1).trimEnd()}…` : flat;
}

/// What the list's author wrote about a line: the comment on the line itself,
/// else the comment directly above the run of packages it sits in (a group
/// heading such as "Preloaded apps", or a caveat such as "re-enable if the Play
/// Store misbehaves"). Commented-out package lines are not notes. Empty when
/// the author wrote nothing.
export function sourceNote(sourceText: string, line: number): string {
  const lines = sourceText.split(/\r?\n/);
  const own = lines[line - 1] ?? "";
  const hash = own.indexOf("#");
  if (hash > 0) {
    const trailing = clip(own.slice(hash + 1));
    if (trailing) return trailing;
  }
  let index = Math.min(line - 2, lines.length - 1);
  while (index >= 0 && lines[index].trim() && !lines[index].trim().startsWith("#")) index--;
  const comment: string[] = [];
  while (index >= 0) {
    const text = lines[index].trim();
    if (!text.startsWith("#")) break;
    if (!PACKAGE_COMMENT.test(text)) comment.unshift(text.replace(/^#+\s?/, ""));
    index--;
  }
  return clip(comment.join(" "));
}

const STORAGE_KEY = "atvopt.cleanyourtvList";
/// The person turned the fallback off for this TV.
export const LIST_OFF = "off";

function readChoices(storage: Storage): Record<string, string> {
  try {
    const parsed = JSON.parse(storage.getItem(STORAGE_KEY) ?? "{}");
    return parsed && typeof parsed === "object" && !Array.isArray(parsed) ? parsed : {};
  } catch {
    return {};
  }
}

/// The list the person chose for this TV, `LIST_OFF`, or null when they haven't
/// said. Filed under the TV's verified hardware id only: the choice is about a
/// TV, and whatever later answers at its address may be another one. A TV with
/// no readable id is never remembered.
export function savedListChoice(storage: Storage, hardwareId: string | null | undefined): string | null {
  const id = hardwareId?.trim();
  if (!id || id.toLowerCase() === "unknown") return null;
  const value = readChoices(storage)[id];
  return typeof value === "string" ? value : null;
}

/// Remembers `choice` (a profile id, `LIST_OFF`, or null to forget) for this TV.
/// Returns false when the TV has no id or storage refused the write, so the
/// caller can keep the choice for the open screen only.
export function saveListChoice(
  storage: Storage,
  hardwareId: string | null | undefined,
  choice: string | null,
): boolean {
  const id = hardwareId?.trim();
  if (!id || id.toLowerCase() === "unknown") return false;
  const choices = readChoices(storage);
  if (choice === null) delete choices[id];
  else choices[id] = choice;
  try {
    storage.setItem(STORAGE_KEY, JSON.stringify(choices));
    return true;
  } catch {
    return false;
  }
}

/// Why a candidate is offered, in words.
export function matchReasonText(candidate: ProfileCandidate, tv: TvIdentity): string {
  const parts: string[] = [];
  if (candidate.reasons.includes("codename")) parts.push(`device ${tv.device_codename?.trim()}`);
  if (candidate.reasons.includes("model")) parts.push(`model ${tv.model?.trim()}`);
  return parts.length ? `Names your TV's ${parts.join(" and ")}` : "";
}

/// What the app reads back from the TV about one installed package.
export interface InstalledApp {
  package: string;
  enabled: boolean;
  name?: string | null;
}

/// Which CleanYourTV list a row came from, carried on its entry so the row
/// and its group can say so.
export interface ListedBy {
  profile: string;
  tv: string;
  /// The list file at the pinned commit.
  url: string;
}

/// The entry a list row is built from. It has the shape of a catalog entry so
/// Optimize treats it like any other "remove if unused" row: never pre-selected
/// (`review`, no `default_optimize`), Disable only (`method`, no store listing
/// so Uninstall is never offered), and the verdict still comes from the app's
/// own safety check.
export interface ListedEntry {
  package: string;
  name: string;
  method: "disable";
  risk: "medium";
  optimize_description: string;
  restore_description: string;
  default_optimize: false;
  default_restore: false;
  play_store: false;
  review: true;
  listed_by: ListedBy;
}

export interface ListedPlanItem {
  entry: ListedEntry;
  action: { kind: "disable" } | { kind: "enable" };
}

/// The CleanYourTV lists Optimize uses for this TV, in rank order, after the
/// app's own catalog: none when the person turned them off, the one they picked
/// when they picked one, otherwise every list that names the TV's device
/// codename, best match first. A model-name match is only ever a suggestion.
export function activeLists(
  profiles: SourceProfile[],
  tv: TvIdentity,
  choice: string | null,
): SourceProfile[] {
  if (choice === LIST_OFF) return [];
  if (choice) {
    const picked = profiles.find((profile) => profile.profile === choice);
    if (picked) return [picked];
  }
  return candidateProfiles(profiles, tv)
    .filter((candidate) => candidate.reasons.includes("codename"))
    .map((candidate) => candidate.profile);
}

/// Rows from a stack of lists, ranked: the app's own catalog first (`covered`:
/// everything it already has a row or a verdict for), then each list in order.
/// The first list that mentions an app decides it: a disable line there makes
/// a row, and a line it keeps commented out means no row, whatever a lower list
/// says. Optimize gets the installed apps that are on; Restore gets the ones
/// that are off, so a run can be undone the way it was made.
export function rankedPlanItems(
  lists: { profile: SourceProfile; text: string; url: string }[],
  installed: InstalledApp[],
  covered: ReadonlySet<string>,
  mode: "optimize" | "restore",
): ListedPlanItem[] {
  const byPackage = new Map(installed.map((app) => [app.package, app]));
  const decided = new Set(covered);
  const items: ListedPlanItem[] = [];
  for (const { profile, text, url } of lists) {
    const listed = new Map(listedForDisable(profile).map((app) => [app.package, app.line]));
    const mentioned = new Set(
      [...profile.entries]
        .sort((x, y) => x.line - y.line)
        .filter((entry) => entry.upstream_form !== "comment_reference")
        .map((entry) => entry.package),
    );
    for (const packageId of mentioned) {
      if (decided.has(packageId)) continue;
      decided.add(packageId);
      const line = listed.get(packageId);
      const app = byPackage.get(packageId);
      if (line === undefined || !app) continue;
      if (mode === "optimize" ? !app.enabled : app.enabled) continue;
      const note = sourceNote(text, line);
      const tv = profile.upstream_context.tv;
      items.push({
        entry: {
          package: packageId,
          name: app.name?.trim() || packageId,
          method: "disable",
          risk: "medium",
          optimize_description: note ? `List author's note: ${note}` : `On the CleanYourTV list for ${tv}.`,
          restore_description: note ? `List author's note: ${note}` : `On the CleanYourTV list for ${tv}.`,
          default_optimize: false,
          default_restore: false,
          play_store: false,
          review: true,
          listed_by: { profile: profile.profile, tv, url },
        },
        action: { kind: mode === "optimize" ? "disable" : "enable" },
      });
    }
  }
  return items;
}

/// The plan-sources help, in each app's own help control.
export const PLAN_SOURCES_HELP: readonly string[] = [
  "The plan comes from ATV Optimizer's own catalog first. For apps the catalog has no record of, it uses CleanYourTV, a community project where TV owners share the apps they turned off on their exact model, starting with the list that best matches this TV. The first list that mentions an app decides it.",
  "Apps from a CleanYourTV list sit under that list's name. None is selected for you, they can only be disabled, never uninstalled, and the do-not-disable list still refuses anything on it. Each note is the list author's, not reviewed by us. The lists are a pinned copy, used under the MIT license.",
];
