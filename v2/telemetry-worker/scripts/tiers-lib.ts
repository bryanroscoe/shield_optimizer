// The support-tier file (`v2/crates/core/data/device-support.json`) is the one
// source of truth. This module checks it before it ships and compares it with
// what the Worker serves. Pure: no I/O, so it runs in the CLI and in tests.
//
// `classify` mirrors `classify_support` in `v2/crates/core/src/engine/support.rs`
// so the weekly report shows the tier the apps would show.

export const TIERS = ["unknown", "not_supported", "beta", "supported", "fully_supported"] as const;
export type Tier = (typeof TIERS)[number];

export const MATCH_KEYS = ["manufacturer", "brand", "model_prefix", "device"] as const;
export type MatchKey = (typeof MATCH_KEYS)[number];
export type TierMatch = Partial<Record<MatchKey, string>>;

export interface TierRule {
  match: TierMatch;
  tier: Tier;
  note: string;
}

export interface TierDoc {
  schema: number;
  updated: string;
  rules: TierRule[];
}

// The tighter of the two sides: the Worker takes 200 rules and 64-character
// match values; the apps take 500 rules and 200-character notes.
export const MAX_RULES = 200;
export const MATCH_MAX = 64;
export const NOTE_MAX = 200;

const DATE = /^\d{4}-\d{2}-\d{2}$/;

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

function chars(s: string): number {
  return Array.from(s).length;
}

function asciiLower(s: string): string {
  return s.replace(/[A-Z]/g, (c) => c.toLowerCase());
}

export function utcDay(d: Date): string {
  return d.toISOString().slice(0, 10);
}

function validDate(s: string): boolean {
  return DATE.test(s) && utcDay(new Date(`${s}T00:00:00Z`)) === s;
}

function canonicalRules(rules: TierRule[]): string {
  return JSON.stringify(
    rules.map((r) => ({
      match: Object.fromEntries(MATCH_KEYS.filter((k) => r.match[k] !== undefined).map((k) => [k, r.match[k]])),
      tier: r.tier,
      note: r.note,
    })),
  );
}

export function rulesEqual(a: TierRule[], b: TierRule[]): boolean {
  return canonicalRules(a) === canonicalRules(b);
}

// Rule `later` can never match if every device `earlier` matches includes all
// of `later`'s devices: first match wins, so `later` is dead.
function shadows(earlier: TierMatch, later: TierMatch): boolean {
  for (const k of MATCH_KEYS) {
    const e = earlier[k];
    if (e === undefined) continue;
    const l = later[k];
    if (l === undefined) return false;
    if (k === "model_prefix" ? !asciiLower(l).startsWith(asciiLower(e)) : asciiLower(l) !== asciiLower(e)) return false;
  }
  return true;
}

export interface CheckOptions {
  /// The same file on the base branch, when checking a change.
  base?: unknown;
  /// UTC `YYYY-MM-DD`; defaults to now.
  today?: string;
}

export interface CheckResult {
  errors: string[];
  warnings: string[];
  doc: TierDoc | null;
}

/// Everything that must hold before the file is merged and pushed live.
/// Stricter than either parser on purpose: what the Worker would silently
/// trim or clamp is rejected here, so the served list is byte-for-byte the
/// file's and the sync can compare them exactly.
export function checkTierFile(input: unknown, opts: CheckOptions = {}): CheckResult {
  const errors: string[] = [];
  const warnings: string[] = [];
  const today = opts.today ?? utcDay(new Date());

  if (!isRecord(input)) return { errors: ["the file must be a JSON object"], warnings, doc: null };
  for (const k of Object.keys(input)) {
    if (!["schema", "updated", "rules"].includes(k)) errors.push(`unknown top-level key "${k}"`);
  }
  if (input.schema !== 1) errors.push("schema must be 1 (the only one the apps understand)");
  const updated = input.updated;
  if (typeof updated !== "string" || !validDate(updated)) {
    errors.push(`updated must be a YYYY-MM-DD date, got ${JSON.stringify(updated)}`);
  } else if (updated > today) {
    errors.push(
      `updated ${updated} is in the future (today is ${today} UTC). Apps ignore a served list older than ` +
        "their embedded one, so a future date would hide the live list from every release that ships this file.",
    );
  }
  if (!Array.isArray(input.rules)) {
    errors.push("rules must be an array");
    return { errors, warnings, doc: null };
  }
  if (input.rules.length > MAX_RULES) errors.push(`more than ${MAX_RULES} rules (the Worker's limit)`);

  const rules: TierRule[] = [];
  input.rules.forEach((raw, i) => {
    const at = `rule ${i}`;
    if (!isRecord(raw)) {
      errors.push(`${at} is not an object`);
      return;
    }
    for (const k of Object.keys(raw)) {
      if (!["match", "tier", "note"].includes(k)) errors.push(`${at}: unknown key "${k}" (the Worker would drop it)`);
    }
    const tier = (TIERS as readonly unknown[]).includes(raw.tier) ? (raw.tier as Tier) : null;
    if (!tier) errors.push(`${at}: tier must be one of ${TIERS.join(", ")}`);
    const note = raw.note;
    if (typeof note !== "string" || note.trim() === "") errors.push(`${at}: note is required`);
    else {
      if (note !== note.trim()) errors.push(`${at}: note has leading or trailing whitespace`);
      if (chars(note) > NOTE_MAX) errors.push(`${at}: note is over ${NOTE_MAX} characters`);
    }
    const match: TierMatch = {};
    if (!isRecord(raw.match)) errors.push(`${at}: match must be an object`);
    else {
      for (const [k, v] of Object.entries(raw.match)) {
        if (!(MATCH_KEYS as readonly string[]).includes(k)) {
          errors.push(`${at}: unknown match key "${k}" (apps reject the whole file)`);
          continue;
        }
        if (typeof v !== "string" || v.trim() === "") {
          errors.push(`${at}: match.${k} must be a non-blank string`);
          continue;
        }
        if (v !== v.trim()) errors.push(`${at}: match.${k} has leading or trailing whitespace`);
        if (chars(v) > MATCH_MAX) errors.push(`${at}: match.${k} is over ${MATCH_MAX} characters`);
        if (v.trim().toLowerCase() === "unknown") {
          errors.push(`${at}: match.${k} is "unknown", which the apps treat as not reported, so it can never match`);
        }
        match[k as MatchKey] = v;
      }
      if (Object.keys(match).length === 0) errors.push(`${at}: match needs at least one key (an empty match is every TV)`);
      if (match.brand !== undefined) {
        warnings.push(`${at}: matches on brand, which the weekly device report cannot see; it will show these TVs as undetermined`);
      }
    }
    if (tier && typeof note === "string") rules.push({ match, tier, note });
  });

  rules.forEach((rule, j) => {
    if (Object.keys(rule.match).length === 0) return;
    const i = rules.slice(0, j).findIndex((e) => Object.keys(e.match).length > 0 && shadows(e.match, rule.match));
    if (i >= 0) {
      errors.push(`rule ${j} can never match: rule ${i} comes first and covers every TV it does. Put the more specific rule first.`);
    }
  });

  const doc: TierDoc | null = errors.length === 0 ? { schema: 1, updated: updated as string, rules } : null;

  if (opts.base !== undefined && isRecord(opts.base) && typeof updated === "string" && validDate(updated)) {
    const baseUpdated = typeof opts.base.updated === "string" ? opts.base.updated : "";
    const baseRules = Array.isArray(opts.base.rules) ? (opts.base.rules as TierRule[]) : [];
    if (updated < baseUpdated) errors.push(`updated went backwards: ${baseUpdated} -> ${updated}`);
    else if (!rulesEqual(rules, baseRules) && updated === baseUpdated && baseUpdated !== today) {
      errors.push(
        `the rules changed but updated is still ${baseUpdated}. Set it to today's date (${today}): ` +
          "an app caches the served list, and only a newer embedded date makes a release override that cache.",
      );
    }
  }

  return { errors, warnings, doc };
}

export interface DeviceProps {
  manufacturer?: string | null;
  brand?: string | null;
  model?: string | null;
  device?: string | null;
}

/// `undetermined` means a rule that needs `brand` could decide this TV and
/// the caller has no brand to test it with.
export type Classification = { tier: Tier; rule: number | null } | { tier: "undetermined"; rule: number };

function reported(v: string | null | undefined): string | null {
  const t = (v ?? "").trim();
  return t !== "" && asciiLower(t) !== "unknown" ? t : null;
}

/// Same semantics as the apps: each present key must match a value the TV
/// actually reported (empty and the literal "unknown" are not reported),
/// case-insensitive (ASCII), `model_prefix` as a prefix, first match wins.
export function classify(rules: TierRule[], props: DeviceProps | null, opts: { brandKnown?: boolean } = {}): Classification {
  if (!props) return { tier: "unknown", rule: null };
  const brandKnown = opts.brandKnown ?? true;
  const exact = (want: string | undefined, got: string | null | undefined) => {
    const w = want?.trim();
    if (!w) return true;
    const g = reported(got);
    return g !== null && asciiLower(g) === asciiLower(w);
  };
  for (const [i, rule] of rules.entries()) {
    const m = rule.match;
    if (!MATCH_KEYS.some((k) => m[k]?.trim())) continue;
    if (!exact(m.manufacturer, props.manufacturer)) continue;
    if (!exact(m.device, props.device)) continue;
    const prefix = m.model_prefix?.trim();
    if (prefix) {
      const model = reported(props.model);
      if (model === null || !asciiLower(model).startsWith(asciiLower(prefix))) continue;
    }
    if (m.brand?.trim()) {
      if (!brandKnown) return { tier: "undetermined", rule: i };
      if (!exact(m.brand, props.brand)) continue;
    }
    return { tier: rule.tier, rule: i };
  }
  return { tier: "unknown", rule: null };
}
