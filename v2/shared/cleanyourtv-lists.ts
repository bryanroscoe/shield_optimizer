// CleanYourTV's per-model lists as a fallback for apps our own catalog has no
// record of. Pure functions shared by desktop and Android: matching a TV to the
// lists that name it, reading which apps a list turns off, and pulling the
// author's note for a line. Nothing here decides safety or picks an action; the
// screens offer what this returns as "remove if unused" rows, off by default,
// and only after the person has said which list is theirs.

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

const tokens = (text: string) => new Set(text.toLowerCase().split(/[^a-z0-9]+/).filter(Boolean));
const squash = (text: string) => text.toLowerCase().replace(/[^a-z0-9]/g, "");

/// Lists that name this TV, best first. A list qualifies when it names the
/// TV's device codename as a word, or contains its full model name and is from
/// the same maker. A model name alone across makers ("Box R 4K Plus") is too
/// common to mean anything, and nothing matches on brand alone. Several lists
/// can name one platform (a TV family shares a codename), so the person picks;
/// this only narrows the choice.
export function candidateProfiles(
  profiles: SourceProfile[],
  tv: TvIdentity,
  limit = 4,
): ProfileCandidate[] {
  const codename = (tv.device_codename ?? "").trim().toLowerCase();
  const codenameKey = codename.length >= 3 ? codename : "";
  const codenameSquashed = squash(codenameKey);
  const model = squash(tv.model ?? "");
  const modelKey = model.length >= 5 ? model : "";
  const makers = [tv.manufacturer, tv.brand]
    .map((value) => (value ?? "").trim().toLowerCase())
    .filter((value) => value.length >= 2);
  const scored: { candidate: ProfileCandidate; score: number }[] = [];
  for (const profile of profiles) {
    const context = profile.upstream_context;
    const words = new Set([...tokens(context.tv), ...tokens(profile.profile)]);
    const codenameHit = !!codenameKey && (words.has(codenameKey) || words.has(codenameSquashed));
    const modelHit = !!modelKey && squash(context.tv).includes(modelKey);
    const brandHit = makers.some(
      (maker) => context.brand.trim().toLowerCase() === maker || words.has(maker),
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
  return scored
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
  let index = line - 2;
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

/// Where a suggested row came from, carried on its catalog entry so the row
/// can say so and link the exact line.
export interface ListedBy {
  profile: string;
  tv: string;
  line: number;
  url: string;
}

/// The entry a suggested row is built from. It has the shape of a catalog
/// entry so Optimize treats it like any other "remove if unused" row: never
/// pre-selected (`review`, no `default_optimize`), Disable only (`method`, no
/// store listing so Uninstall is never offered), and the verdict still comes
/// from the app's own safety check.
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

/// Rows for the apps on the chosen list that are installed on this TV and that
/// `covered` (everything the app's own catalog already has a row or a verdict
/// for) doesn't include. Optimize gets the enabled ones to disable; Restore
/// gets the disabled ones to turn back on, so a run can always be undone the
/// same way it was made.
export function listedPlanItems(
  profile: SourceProfile,
  sourceText: string,
  sourceUrlFor: (line: number) => string,
  installed: InstalledApp[],
  covered: ReadonlySet<string>,
  mode: "optimize" | "restore",
): ListedPlanItem[] {
  const byPackage = new Map(installed.map((app) => [app.package, app]));
  const items: ListedPlanItem[] = [];
  for (const { package: packageId, line } of listedForDisable(profile)) {
    const app = byPackage.get(packageId);
    if (!app || covered.has(packageId)) continue;
    if (mode === "optimize" ? !app.enabled : app.enabled) continue;
    const note = sourceNote(sourceText, line);
    const tv = profile.upstream_context.tv;
    items.push({
      entry: {
        package: packageId,
        name: app.name?.trim() || packageId,
        method: "disable",
        risk: "medium",
        optimize_description: note || `On the CleanYourTV list for ${tv}.`,
        restore_description: note || `On the CleanYourTV list for ${tv}.`,
        default_optimize: false,
        default_restore: false,
        play_store: false,
        review: true,
        listed_by: { profile: profile.profile, tv, line, url: sourceUrlFor(line) },
      },
      action: { kind: mode === "optimize" ? "disable" : "enable" },
    });
  }
  return items;
}
