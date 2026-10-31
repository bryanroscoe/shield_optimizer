import { checkAdmin, deviceReport, parseGroup } from "./admin";
import type { Env } from "./env";
import { error, json, readBodyLimited } from "./http";
import { consumeQuota, purgeExpired, releaseQuota, storeEvents, utcDay } from "./ingest";
import { readTiers, replaceTiers, validateTierRules } from "./tiers";
import { MAX_BODY_BYTES, cleanCountry, validateBatch } from "./validate";

export type { Env } from "./env";

// Nothing in this file logs. Request IPs, headers and request.cf (apart from
// the two-letter country) are never read into storage or console output.

const TIER_CACHE = "public, max-age=3600";
const ADMIN_MAX_BODY = 256 * 1024;

function secondsUntilUtcMidnight(now: Date): number {
  const next = Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate() + 1);
  return Math.max(1, Math.ceil((next - now.getTime()) / 1000));
}

async function readJson(request: Request, limit: number): Promise<{ ok: true; value: unknown } | { ok: false; response: Response }> {
  const type = request.headers.get("content-type") ?? "";
  if (!/^application\/json\b/i.test(type)) {
    return { ok: false, response: error(415, "content_type_must_be_application_json") };
  }
  const bytes = await readBodyLimited(request, limit);
  if (bytes === null) return { ok: false, response: error(413, "body_too_large", { max_bytes: limit }) };
  try {
    return { ok: true, value: JSON.parse(new TextDecoder("utf-8", { fatal: true, ignoreBOM: false }).decode(bytes)) };
  } catch {
    return { ok: false, response: error(400, "invalid_json") };
  }
}

async function postEvents(request: Request, env: Env): Promise<Response> {
  const body = await readJson(request, MAX_BODY_BYTES);
  if (!body.ok) return body.response;
  const batch = validateBatch(body.value);
  if (!batch.ok) return error(400, batch.reason);
  const { events, rejected } = batch.value;
  const rejectedCount = Object.values(rejected).reduce((a, b) => a + b, 0);
  if (events.length === 0) return error(400, "no_valid_events", { rejected });

  const now = new Date();
  let quota: Awaited<ReturnType<typeof consumeQuota>>;
  try {
    quota = await consumeQuota(env, events, now);
  } catch {
    return error(503, "storage_unavailable");
  }
  if (!quota.ok) {
    return error(429, quota.scope === "install" ? "install_daily_cap" : "global_daily_cap", {}, {
      "retry-after": String(secondsUntilUtcMidnight(now)),
    });
  }

  const cf = (request as { cf?: { country?: unknown } }).cf;
  try {
    await storeEvents(env, events, cleanCountry(cf?.country), now);
  } catch {
    await releaseQuota(env, quota.rows, now).catch(() => undefined);
    return error(503, "storage_unavailable");
  }
  return new Response(null, {
    status: 204,
    headers: {
      "x-events-accepted": String(events.length),
      "x-events-rejected": String(rejectedCount),
    },
  });
}

async function admin(request: Request, env: Env, path: string): Promise<Response> {
  const auth = await checkAdmin(request, env);
  if (auth === "disabled") return error(503, "admin_disabled");
  if (auth === "unauthorized") {
    return error(401, "unauthorized", {}, { "www-authenticate": 'Bearer realm="atv-optimizer-telemetry"' });
  }

  if (path === "/v1/admin/devices") {
    if (request.method !== "GET") return error(405, "method_not_allowed", {}, { allow: "GET" });
    const url = new URL(request.url);
    const group = parseGroup(url.searchParams.get("group"));
    if (!group) return error(400, "bad_group");
    // Bounded by default: the last 90 days and the 500 busiest groups, so junk
    // dimensions sent by a hostile client cannot make the report unusable.
    const days = Number(url.searchParams.get("days") ?? "90");
    if (!Number.isInteger(days) || days < 1 || days > 3650) return error(400, "bad_days");
    const limit = Number(url.searchParams.get("limit") ?? "500");
    if (!Number.isInteger(limit) || limit < 1 || limit > 5000) return error(400, "bad_limit");
    const since = utcDay(new Date(Date.now() - (days - 1) * 86_400_000));
    const rows = await deviceReport(env, group, since, limit);
    return json({ group, since, limit, truncated: rows.length === limit, rows }, 200, { "cache-control": "no-store" });
  }

  if (path === "/v1/admin/support-tiers") {
    if (request.method !== "PUT") return error(405, "method_not_allowed", {}, { allow: "PUT" });
    const body = await readJson(request, ADMIN_MAX_BODY);
    if (!body.ok) return body.response;
    const rules = validateTierRules(body.value);
    if (!rules.ok) return error(400, rules.reason);
    await replaceTiers(env, rules.value, new Date());
    return json(await readTiers(env), 200, { "cache-control": "no-store" });
  }

  return error(404, "not_found");
}

async function route(request: Request, env: Env): Promise<Response> {
  const url = new URL(request.url);
  const path = url.pathname.replace(/\/+$/, "") || "/";

  // Answer preflights without granting any origin. Native clients never send
  // them; browsers get a response that does not allow the cross-origin call.
  if (request.method === "OPTIONS") {
    return new Response(null, { status: 204, headers: { allow: "GET, POST, PUT, OPTIONS" } });
  }

  if (path === "/v1/events") {
    if (request.method !== "POST") return error(405, "method_not_allowed", {}, { allow: "POST, OPTIONS" });
    return postEvents(request, env);
  }

  if (path === "/v1/support-tiers") {
    if (request.method !== "GET" && request.method !== "HEAD") {
      return error(405, "method_not_allowed", {}, { allow: "GET, HEAD, OPTIONS" });
    }
    return json(await readTiers(env), 200, { "cache-control": TIER_CACHE });
  }

  if (path.startsWith("/v1/admin/")) return admin(request, env, path);

  return error(404, "not_found");
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    try {
      return await route(request, env);
    } catch {
      // No logging: the error could carry request data.
      return error(500, "internal_error");
    }
  },

  async scheduled(controller: ScheduledController, env: Env): Promise<void> {
    await purgeExpired(env, new Date(controller.scheduledTime));
  },
} satisfies ExportedHandler<Env>;
