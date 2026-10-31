import type { Env } from "./env";
import { clamp } from "./redact";
import { TIERS, type Result, type Tier } from "./validate";

// Rules are informational: the apps show the tier and note to the user and never
// use them to relax a safety gate.

export interface TierMatch {
  manufacturer?: string;
  model_prefix?: string;
  brand?: string;
  device?: string;
}

export interface TierRule {
  match: TierMatch;
  tier: Tier;
  note: string;
}

export interface TierDoc {
  schema: 1;
  updated: string;
  rules: TierRule[];
}

const MATCH_KEYS = ["manufacturer", "model_prefix", "brand", "device"] as const;
export const MAX_RULES = 200;
const NOTE_MAX = 200;
const MATCH_MAX = 64;

interface RuleRow {
  manufacturer: string | null;
  brand: string | null;
  model_prefix: string | null;
  device: string | null;
  tier: Tier;
  note: string;
}

export async function readTiers(env: Env): Promise<TierDoc> {
  const [rules, meta] = await env.DB.batch([
    env.DB.prepare(
      "SELECT manufacturer, brand, model_prefix, device, tier, note FROM support_tier_rules ORDER BY position",
    ),
    env.DB.prepare("SELECT updated FROM support_tier_meta WHERE id = 1"),
  ]);
  const rows = (rules?.results ?? []) as RuleRow[];
  const updated = ((meta?.results ?? [])[0] as { updated?: string } | undefined)?.updated ?? "1970-01-01T00:00:00Z";
  return {
    schema: 1,
    updated,
    rules: rows.map((r) => {
      const match: TierMatch = {};
      for (const k of MATCH_KEYS) {
        const v = r[k];
        if (v) match[k] = v;
      }
      return { match, tier: r.tier, note: r.note };
    }),
  };
}

export function validateTierRules(body: unknown): Result<TierRule[]> {
  if (typeof body !== "object" || body === null || !Array.isArray((body as { rules?: unknown }).rules)) {
    return { ok: false, reason: "body_must_be_object_with_rules_array" };
  }
  const input = (body as { rules: unknown[] }).rules;
  if (input.length > MAX_RULES) return { ok: false, reason: "too_many_rules" };
  const out: TierRule[] = [];
  for (const [i, raw] of input.entries()) {
    if (typeof raw !== "object" || raw === null) return { ok: false, reason: `rule_${i}_not_object` };
    const r = raw as Record<string, unknown>;
    const tier = typeof r.tier === "string" && (TIERS as readonly string[]).includes(r.tier) ? (r.tier as Tier) : null;
    if (!tier) return { ok: false, reason: `rule_${i}_unknown_tier` };
    if (typeof r.note !== "string" || r.note.trim() === "") return { ok: false, reason: `rule_${i}_missing_note` };
    if (typeof r.match !== "object" || r.match === null) return { ok: false, reason: `rule_${i}_missing_match` };
    const m = r.match as Record<string, unknown>;
    const match: TierMatch = {};
    for (const k of MATCH_KEYS) {
      const v = m[k];
      if (v === undefined || v === null) continue;
      // A blank value would be published as an absent key, turning the rule
      // into a match-everything rule.
      if (typeof v !== "string" || v.trim() === "") return { ok: false, reason: `rule_${i}_bad_${k}` };
      match[k] = clamp(v.trim(), MATCH_MAX);
    }
    if (Object.keys(match).length === 0) return { ok: false, reason: `rule_${i}_empty_match` };
    out.push({ match, tier, note: clamp(r.note.trim(), NOTE_MAX) });
  }
  return { ok: true, value: out };
}

export async function replaceTiers(env: Env, rules: TierRule[], now: Date): Promise<void> {
  // Clients ignore a document older than the rules they embed, so `updated`
  // must move forward on every replace, even two replaces in one millisecond
  // or after a clock step backwards.
  const prev = await env.DB.prepare("SELECT updated FROM support_tier_meta WHERE id = 1").first<{ updated: string }>();
  const prevMs = prev ? Date.parse(prev.updated) : Number.NaN;
  const updated = new Date(Number.isNaN(prevMs) ? now.getTime() : Math.max(now.getTime(), prevMs + 1)).toISOString();
  const rows = rules.map((r, i) => ({
    position: i + 1,
    manufacturer: r.match.manufacturer ?? null,
    brand: r.match.brand ?? null,
    model_prefix: r.match.model_prefix ?? null,
    device: r.match.device ?? null,
    tier: r.tier,
    note: r.note,
  }));
  // D1 runs a batch as one transaction, so readers never see a half-replaced list.
  await env.DB.batch([
    env.DB.prepare("DELETE FROM support_tier_rules"),
    env.DB.prepare(
      `INSERT INTO support_tier_rules (position, manufacturer, brand, model_prefix, device, tier, note)
       SELECT json_extract(value, '$.position'), json_extract(value, '$.manufacturer'), json_extract(value, '$.brand'),
              json_extract(value, '$.model_prefix'), json_extract(value, '$.device'), json_extract(value, '$.tier'),
              json_extract(value, '$.note')
       FROM json_each(?1)`,
    ).bind(JSON.stringify(rows)),
    env.DB.prepare(
      "INSERT INTO support_tier_meta (id, updated) VALUES (1, ?1) ON CONFLICT (id) DO UPDATE SET updated = excluded.updated",
    ).bind(updated),
  ]);
}

// Reference matcher for the documented semantics: every field present in
// `match` must match (case-insensitive; model_prefix is a prefix test), and the
// first matching rule wins. The apps implement the same thing client-side.
export function matchTier(rules: TierRule[], d: { manufacturer?: string; model?: string; brand?: string; device?: string }): TierRule | null {
  const eq = (a: string | undefined, b: string) => (a ?? "").toLowerCase() === b.toLowerCase();
  for (const rule of rules) {
    const m = rule.match;
    if (m.manufacturer !== undefined && !eq(d.manufacturer, m.manufacturer)) continue;
    if (m.brand !== undefined && !eq(d.brand, m.brand)) continue;
    if (m.device !== undefined && !eq(d.device, m.device)) continue;
    if (m.model_prefix !== undefined && !(d.model ?? "").toLowerCase().startsWith(m.model_prefix.toLowerCase())) continue;
    return rule;
  }
  return null;
}
