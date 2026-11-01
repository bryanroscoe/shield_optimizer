// The device report: rows from GET /v1/admin/devices, the tier the apps
// would show (from the repo's rules) and a suggested tier. Pure, so the
// criteria are unit-tested. docs/DEVICE-SUPPORT.md explains them for people;
// keep the two in step.
//
// Suggestions only. Nothing here writes a tier: a tier changes by a PR to
// v2/crates/core/data/device-support.json.

import { classify, type Classification, type TierRule } from "./tiers-lib.ts";

export interface ReportRow {
  manufacturer?: string | null;
  model?: string | null;
  device?: string | null;
  installs: number;
  events?: number;
  seen?: number;
  action_ok: number;
  action_fail: number;
  errors?: number;
  top_errors: Array<{ code: string; count: number }>;
  last_seen: string | null;
  [k: string]: unknown;
}

export interface ReportBody {
  group: string[];
  since: string;
  truncated: boolean;
  rows: ReportRow[];
}

export const CRITERIA = {
  beta: { minInstalls: 3, minActions: 5, minSuccess: 0.8 },
  supported: { minInstalls: 10, minActions30d: 20, minSuccess30d: 0.95, maxErrorShare30d: 0.05 },
  fullyCandidate: { minInstalls: 25 },
  notSupported: { minInstalls: 5, minActions: 5, maxSuccess: 0.3, systematicErrorShare: 0.5 },
  review: { minSuccess30d: 0.8, minActions30d: 10 },
} as const;

/// Codes that describe the user's setup or an expected refusal rather than
/// the TV misbehaving (network, pairing, adb not installed, our own
/// do-not-disable gate, Pro lock). They never count against a TV.
/// Everything else (`permission_denied`, `pm_failure`, `unsupported`,
/// `partial_failure`, `unknown`, `panic`, ...) is unexplained.
export const EXPLAINED_CODES: ReadonlySet<string> = new Set([
  "adb_missing",
  "app_quitting",
  "connection_lost",
  "not_connected",
  "device_unauthorized",
  "device_offline",
  "wrong_pin",
  "pairing_expired",
  "connection_refused",
  "timeout",
  "unreachable",
  "protected_package",
  "package_not_found",
  "storage_full",
  "locked",
]);

export function isExplained(code: string): boolean {
  return EXPLAINED_CODES.has(code) || code.startsWith("billing_");
}

/// The Worker returns at most this many codes per row.
const TOP_ERRORS_MAX = 5;

function reported(v: string | null | undefined): boolean {
  const t = (v ?? "").trim();
  return t !== "" && t.toLowerCase() !== "unknown" && t !== "[redacted]";
}

/// A row can only be judged when the TV said what it is. Rows without a
/// manufacturer and model (an unauthorized TV, an error not tied to a
/// device, a value the server redacted) never get a suggested tier.
export function identified(row: Pick<ReportRow, "manufacturer" | "model">): boolean {
  return reported(row.manufacturer) && reported(row.model);
}

function totalActions(row: ReportRow | undefined): number {
  return row ? row.action_ok + row.action_fail : 0;
}

/// Failed actions that carried a setup code. The Worker counts a failed
/// action's code together with error events' codes and doesn't say which is
/// which, so this is an upper bound: every listed setup code is assumed to
/// be a failed action, capped at the failures there were. That can only err
/// in the TV's favour, which is the rule: setup never counts against a TV.
function setupFailures(row: ReportRow): number {
  const listed = row.top_errors.filter((e) => isExplained(e.code)).reduce((n, e) => n + e.count, 0);
  return Math.min(row.action_fail, listed);
}

/// Actions that say something about the TV: everything except failures
/// caused by the user's setup. Success rates and minimum counts use these.
function actions(row: ReportRow | undefined): number {
  return row ? totalActions(row) - setupFailures(row) : 0;
}

function success(row: ReportRow | undefined): number | null {
  const n = actions(row);
  return row && n > 0 ? row.action_ok / n : null;
}

/// With a full top-5 list, setup codes outside it may hide more setup
/// failures, so `success` is only a lower bound. A lower bound may hold back
/// a promotion; it must never be the reason to call a TV broken.
function successIsExact(row: ReportRow | undefined): boolean {
  return !row || row.action_fail === 0 || row.top_errors.length < TOP_ERRORS_MAX;
}

function pct(x: number | null): string {
  return x === null ? "-" : `${(100 * x).toFixed(1).replace(/\.0$/, "")}%`;
}

/// The worst unexplained code's share of all actions. `null` when there
/// were no actions.
function worstUnexplained(row: ReportRow | undefined): { code: string; share: number } | null {
  const n = totalActions(row);
  if (!row || n === 0) return null;
  let worst: { code: string; share: number } | null = null;
  for (const e of row.top_errors) {
    if (isExplained(e.code)) continue;
    const share = e.count / n;
    if (!worst || share > worst.share) worst = { code: e.code, share };
  }
  return worst;
}

function listMayHideCodeOver(row: ReportRow | undefined, threshold: number): boolean {
  if (!row || row.top_errors.length < TOP_ERRORS_MAX) return false;
  const smallest = Math.min(...row.top_errors.map((e) => e.count));
  return smallest / Math.max(1, totalActions(row)) > threshold;
}

export type Suggestion =
  | { kind: "none"; why: string }
  | { kind: "keep"; why: string }
  | { kind: "move"; to: "beta" | "supported"; why: string }
  | { kind: "not_supported"; why: string }
  | { kind: "fully_supported_candidate"; why: string }
  | { kind: "review"; why: string };

function notSupportedSignal(all: ReportRow): string | null {
  const c = CRITERIA.notSupported;
  if (all.installs < c.minInstalls || actions(all) < c.minActions) return null;
  const s = success(all);
  if (s !== null && s <= c.maxSuccess && successIsExact(all)) {
    return `${pct(s)} of ${actions(all)} actions succeeded across ${all.installs} installs`;
  }
  const worst = worstUnexplained(all);
  if (worst && worst.share >= c.systematicErrorShare) {
    return `\`${worst.code}\` on ${pct(worst.share)} of actions across ${all.installs} installs`;
  }
  return null;
}

function betaGaps(all: ReportRow): string[] {
  const c = CRITERIA.beta;
  const gaps: string[] = [];
  if (all.installs < c.minInstalls) gaps.push(`${c.minInstalls} installs (has ${all.installs})`);
  if (actions(all) < c.minActions) gaps.push(`${c.minActions} actions (has ${actions(all)})`);
  else if ((success(all) ?? 0) < c.minSuccess) gaps.push(`${pct(c.minSuccess)} success (has ${pct(success(all))})`);
  return gaps;
}

function supportedGaps(all: ReportRow, recent: ReportRow | undefined): string[] {
  const c = CRITERIA.supported;
  const gaps: string[] = [];
  if (all.installs < c.minInstalls) gaps.push(`${c.minInstalls} installs (has ${all.installs})`);
  if (actions(recent) < c.minActions30d) gaps.push(`${c.minActions30d} actions in 30 days (has ${actions(recent)})`);
  else {
    const s = success(recent) ?? 0;
    if (s < c.minSuccess30d) gaps.push(`${pct(c.minSuccess30d)} success in 30 days (has ${pct(s)})`);
    const worst = worstUnexplained(recent);
    if (worst && worst.share > c.maxErrorShare30d) {
      gaps.push(`no unexplained code over ${pct(c.maxErrorShare30d)} (\`${worst.code}\` is ${pct(worst.share)})`);
    } else if (listMayHideCodeOver(recent, c.maxErrorShare30d)) {
      gaps.push(`no unexplained code over ${pct(c.maxErrorShare30d)} (the error list is cut at ${TOP_ERRORS_MAX} codes)`);
    }
  }
  return gaps;
}

/// `all` is the long window (all time), `recent` the last 30 days.
export function suggest(current: Classification, all: ReportRow, recent: ReportRow | undefined): Suggestion {
  if (!identified(all)) {
    return { kind: "none", why: "the TV did not report a manufacturer and model, so no tier can be claimed for it" };
  }
  const tier = current.tier;
  if (tier === "undetermined") {
    return { kind: "none", why: "a rule that matches on brand may apply, and the report can't see brand" };
  }
  const ns = notSupportedSignal(all);

  if (tier === "not_supported") {
    const betaOk = betaGaps(all).length === 0;
    return betaOk && !ns
      ? { kind: "review", why: `marked not supported, but ${pct(success(all))} of ${actions(all)} actions succeeded across ${all.installs} installs` }
      : { kind: "keep", why: "still looks unsupported" };
  }
  if (ns) return { kind: "not_supported", why: `${ns}; confirm the cause before moving it` };

  if (tier === "unknown") {
    const gaps = betaGaps(all);
    return gaps.length === 0
      ? { kind: "move", to: "beta", why: `${all.installs} installs, ${pct(success(all))} of ${actions(all)} actions succeeded` }
      : { kind: "keep", why: `beta needs ${gaps.join(", ")}` };
  }
  if (tier === "beta") {
    const gaps = supportedGaps(all, recent);
    return gaps.length === 0
      ? {
          kind: "move",
          to: "supported",
          why: `${all.installs} installs, ${pct(success(recent))} of ${actions(recent)} actions in 30 days, no unexplained code over ${pct(CRITERIA.supported.maxErrorShare30d)}`,
        }
      : { kind: "keep", why: `supported needs ${gaps.join(", ")}` };
  }

  const r = CRITERIA.review;
  const s30 = success(recent);
  if (actions(recent) >= r.minActions30d && s30 !== null && s30 < r.minSuccess30d && successIsExact(recent)) {
    return { kind: "review", why: `only ${pct(s30)} of ${actions(recent)} actions succeeded in the last 30 days` };
  }
  if (tier === "supported") {
    const gaps = supportedGaps(all, recent);
    if (gaps.length === 0 && all.installs >= CRITERIA.fullyCandidate.minInstalls) {
      return { kind: "fully_supported_candidate", why: `${all.installs} installs and clean for 30 days; needs the owner's physical test` };
    }
    return { kind: "keep", why: gaps.length === 0 ? "meets supported" : `slipping: ${gaps.join(", ")}` };
  }
  return { kind: "keep", why: "physically tested" };
}

export function suggestionLabel(s: Suggestion): string {
  switch (s.kind) {
    case "none":
      return "none (unidentified)";
    case "keep":
      return "keep";
    case "move":
      return `**${s.to}**`;
    case "not_supported":
      return "**not_supported?** (owner confirms)";
    case "fully_supported_candidate":
      return "fully_supported candidate (physical test)";
    case "review":
      return "**review**";
  }
}

function keyOf(r: Pick<ReportRow, "manufacturer" | "model" | "device">): string {
  return JSON.stringify([r.manufacturer ?? null, r.model ?? null, r.device ?? null]);
}

export interface AnalyzedRow {
  all: ReportRow;
  recent: ReportRow | undefined;
  current: Classification;
  suggestion: Suggestion;
}

export function analyze(rules: TierRule[], all: ReportBody, recent: ReportBody): AnalyzedRow[] {
  const recentByKey = new Map(recent.rows.map((r) => [keyOf(r), r]));
  return all.rows.map((row) => {
    // The report has no brand column, so a brand rule can't be evaluated.
    const current = classify(rules, row, { brandKnown: false });
    const r = recentByKey.get(keyOf(row));
    return { all: row, recent: r, current, suggestion: suggest(current, row, r) };
  });
}

/// Device strings are free text the TV chose. Shown as code so they can't
/// render as links, mentions or HTML; pipes are escaped for the table.
function cell(v: string | null | undefined): string {
  if (!reported(v)) return "_(not reported)_";
  const text = String(v).replace(/[`\r\n]/g, " ").replace(/\|/g, "\\|");
  return `\`${text}\``;
}

function errorsCell(row: ReportRow | undefined): string {
  if (!row || row.top_errors.length === 0) return "";
  return row.top_errors
    .slice(0, 3)
    .map((e) => `\`${e.code}\` ${e.count}${isExplained(e.code) ? "" : "*"}`)
    .join(", ");
}

function tableRow(a: AnalyzedRow): string {
  const tier = a.current.tier === "undetermined" ? "? (brand rule)" : a.current.tier;
  const cols = [
    cell(a.all.manufacturer),
    cell(a.all.model),
    cell(a.all.device),
    tier,
    String(a.all.installs),
    `${a.all.action_ok} / ${a.all.action_fail}`,
    `${pct(success(a.all))} / ${pct(success(a.recent))}`,
    errorsCell(a.all),
    (a.all.last_seen ?? "").slice(0, 10),
    suggestionLabel(a.suggestion),
    a.suggestion.why.replace(/\|/g, "\\|"),
  ];
  return `| ${cols.join(" | ")} |`;
}

const HEADER =
  "| Manufacturer | Model | Device | Current tier | Installs | Actions ok / fail | Success all / 30d | Top error codes | Last seen | Suggested | Why |\n" +
  "| --- | --- | --- | --- | ---: | ---: | ---: | --- | --- | --- | --- |";

export const ISSUE_TITLE = "Device support report";
export const ISSUE_MARKER = "<!-- device-support-report -->";
/// GitHub's issue body limit is 65,536 characters.
const BODY_BUDGET = 60_000;

export interface MarkdownOptions {
  generatedAt: Date;
  rulesUpdated: string;
  docsUrl: string;
  runUrl?: string;
}

export function issueMarkdown(rows: AnalyzedRow[], all: ReportBody, recent: ReportBody, opts: MarkdownOptions): string {
  const changes = rows.filter((r) => r.suggestion.kind !== "keep" && r.suggestion.kind !== "none");
  const head = [
    ISSUE_MARKER,
    `Updated ${opts.generatedAt.toISOString().slice(0, 16).replace("T", " ")} UTC by the weekly device report` +
      (opts.runUrl ? ` ([run](${opts.runUrl}))` : "") +
      ". This issue is rewritten every week; comments stay.",
    "",
    `**Suggestions only.** Nothing here changes a tier. A tier changes by a PR to \`v2/crates/core/data/device-support.json\` ` +
      `(rules dated ${opts.rulesUpdated}). How to read this and how to change a tier: [docs/DEVICE-SUPPORT.md](${opts.docsUrl}).`,
    "",
    `Data: every TV that reported since ${all.since} (all time) and since ${recent.since} (30 days), grouped by manufacturer, model and device codename. ` +
      "Only what TVs actually reported. `*` marks an error code that counts against the TV; the others are network, pairing or setup.",
  ];
  if (all.truncated || recent.truncated) {
    head.push("", "> The Worker cut the list at its row limit; the quietest TVs are missing.");
  }

  let body = `${head.join("\n")}\n\n`;
  // Both tables share GitHub's size limit; the suggested changes come first
  // so they are the last thing cut.
  const table = (title: string, list: AnalyzedRow[], empty: string): void => {
    body += `## ${title} (${list.length})\n\n`;
    if (list.length === 0) {
      body += `${empty}\n\n`;
      return;
    }
    const lines = [HEADER];
    let size = body.length + HEADER.length;
    for (const r of list) {
      const line = tableRow(r);
      if (size + line.length + 400 > BODY_BUDGET) break;
      lines.push(line);
      size += line.length + 1;
    }
    body += `${lines.join("\n")}\n\n`;
    const hidden = list.length - (lines.length - 1);
    if (hidden > 0) {
      body += `${hidden} more not shown (issue size limit). \`npm run report\` in \`v2/telemetry-worker\` prints every row.\n\n`;
    }
  };
  table("Suggested changes", changes, "None this week.");
  table("All TVs", rows, "No TVs have reported yet.");
  body = body.trimEnd();
  return `${body}\n`;
}

/// One plain object per row for `console.table`.
export function consoleRows(body: ReportBody): Array<Record<string, unknown>> {
  return body.rows.map((r) => {
    const out: Record<string, unknown> = {};
    for (const g of body.group) out[g] = r[g] ?? "-";
    out.installs = r.installs;
    out.seen = r.seen;
    out.ok = r.action_ok;
    out.fail = r.action_fail;
    out["ok %"] = totalActions(r) > 0 ? pct(r.action_ok / totalActions(r)) : "-";
    out.errors = r.errors;
    out["top errors"] = r.top_errors
      .slice(0, 3)
      .map((e) => `${e.code} (${e.count})`)
      .join(", ");
    out["last seen"] = String(r.last_seen ?? "").slice(0, 10);
    return out;
  });
}

export async function fetchReport(
  base: string,
  token: string,
  params: { group?: string; days?: string | number; limit?: string | number },
  fetchImpl: typeof fetch = fetch,
): Promise<ReportBody> {
  const url = new URL("/v1/admin/devices", base);
  if (params.group) url.searchParams.set("group", params.group);
  if (params.days !== undefined) url.searchParams.set("days", String(params.days));
  if (params.limit !== undefined) url.searchParams.set("limit", String(params.limit));
  const res = await fetchImpl(url, { headers: { authorization: `Bearer ${token}` } });
  if (!res.ok) throw new Error(`GET ${url.pathname}: ${res.status} ${await res.text()}`);
  return (await res.json()) as ReportBody;
}
