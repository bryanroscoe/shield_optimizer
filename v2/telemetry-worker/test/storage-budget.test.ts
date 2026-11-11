import { createScheduledController } from "cloudflare:test";
import { env } from "cloudflare:workers";
import { beforeEach, describe, expect, it } from "vitest";
import type { Env } from "../src/env";
import worker from "../src/index";
import { enforceStorageBudget, estimateStorageBytes, EVENT_ROW_OVERHEAD } from "../src/ingest";

// Each test file gets its own D1, but tests within a file share it, so every
// test here starts from empty data tables (the support-tier seed stays).
beforeEach(async () => {
  await env.DB.batch(
    ["events", "diag_uploads", "agg_device_daily", "agg_device_installs", "agg_error_daily", "rate_daily"].map((t) =>
      env.DB.prepare(`DELETE FROM ${t}`),
    ),
  );
});

const withBudget = (budget: number): Env => ({ DB: env.DB, STORAGE_BUDGET_BYTES: String(budget) }) as Env;
// A budget whose 80% target sits `excess` bytes under the current estimate.
const budgetFreeing = (estimate: number, excess: number) => Math.ceil((estimate - excess) / 0.8);

async function seed(): Promise<void> {
  const install = "5f0c2b1e-8d3a-4c6b-9e2f-0000000b0d9e";
  await env.DB.batch([
    // Oldest of all, but raw events go only after uploads and excerpts.
    env.DB.prepare(
      `INSERT INTO events (received_at, client_ts, install_id, app, app_version, os, kind, error_code, diag, log_excerpt)
       VALUES ('1999-01-01T00:00:00.000Z', '1999-01-01T00:00:00.000Z', ?1, 'desktop', '2.5.0', 'linux', 'diagnostic', 'scan_empty', '{"ms":1}', ?2)`,
    ).bind(install, "e".repeat(20_000)),
    env.DB.prepare(
      `INSERT INTO events (received_at, client_ts, install_id, app, app_version, os, kind, error_code, message)
       VALUES ('1999-01-02T00:00:00.000Z', '1999-01-02T00:00:00.000Z', ?1, 'desktop', '2.5.0', 'linux', 'error', 'panic', ?2)`,
    ).bind(install, "m".repeat(400)),
    env.DB.prepare(
      `INSERT INTO diag_uploads (received_at, client_ts, install_id, app, app_version, os, bytes, body)
       VALUES ('2000-01-01T00:00:00.000Z', '2000-01-01T00:00:00.000Z', ?1, 'desktop', '2.5.0', 'linux', 40000, ?2)`,
    ).bind(install, "u".repeat(40_000)),
    env.DB.prepare(
      `INSERT INTO diag_uploads (received_at, client_ts, install_id, app, app_version, os, bytes, body)
       VALUES ('2000-01-02T00:00:00.000Z', '2000-01-02T00:00:00.000Z', ?1, 'desktop', '2.5.0', 'linux', 40000, ?2)`,
    ).bind(install, "v".repeat(40_000)),
    env.DB.prepare(
      `INSERT INTO agg_device_daily (day, manufacturer, model, device, profile, app, app_version, events, last_seen)
       VALUES ('1999-01-01', 'BudgetTest', 'M', 'd', 'shield', 'desktop', '2.5.0', 1, '1999-01-01T00:00:00.000Z')`,
    ),
  ]);
}

const count = async (sql: string) => ((await env.DB.prepare(sql).first<{ n: number }>())?.n ?? -1);

describe("storage budget", () => {
  it("estimates stored bytes from the rows", async () => {
    expect(await estimateStorageBytes(env)).toBe(0);
    await seed();
    const estimate = await estimateStorageBytes(env);
    // Two uploads, the excerpt and the message dominate; every row adds its overhead.
    expect(estimate).toBeGreaterThan(40_000 * 2 + 20_000 + 400 + 4 * EVENT_ROW_OVERHEAD);
    expect(estimate).toBeLessThan(40_000 * 2 + 20_000 + 400 + 2_000);
  });

  it("does nothing under the budget", async () => {
    await seed();
    const estimate = await estimateStorageBytes(env);
    const r = await enforceStorageBudget(withBudget(estimate + 1));
    expect(r).toMatchObject({ before: estimate, after: estimate, trimmed: [] });
    expect(await count("SELECT COUNT(*) AS n FROM diag_uploads")).toBe(2);
  });

  it("frees the oldest upload first and stops once under 80% of the budget", async () => {
    await seed();
    const estimate = await estimateStorageBytes(env);
    const budget = budgetFreeing(estimate, 30_000);
    const r = await enforceStorageBudget(withBudget(budget));
    expect(r.trimmed).toEqual(["uploads"]);
    expect(r.after).toBeLessThanOrEqual(Math.floor(budget * 0.8));
    // Only the older upload went; the older events and their excerpt stay.
    expect((await env.DB.prepare("SELECT body FROM diag_uploads").all()).results).toEqual([{ body: "v".repeat(40_000) }]);
    expect(await count("SELECT COUNT(*) AS n FROM events WHERE log_excerpt IS NOT NULL")).toBe(1);
  });

  it("then clears excerpts, then deletes raw events, before touching aggregates", async () => {
    await seed();
    const estimate = await estimateStorageBytes(env);
    // Both uploads and the excerpt, plus a little: the oldest event goes too.
    const r = await enforceStorageBudget(withBudget(budgetFreeing(estimate, 80_000 + 2 * EVENT_ROW_OVERHEAD + 20_000 + 10)));
    expect(r.trimmed).toEqual(["uploads", "excerpts", "events"]);
    expect(await count("SELECT COUNT(*) AS n FROM diag_uploads")).toBe(0);
    expect(await count("SELECT COUNT(*) AS n FROM events")).toBe(1);
    expect(await count("SELECT COUNT(*) AS n FROM events WHERE message IS NOT NULL")).toBe(1);
    expect(await count("SELECT COUNT(*) AS n FROM agg_device_daily")).toBe(1);
  });

  it("frees aggregates last, and never the support-tier list", async () => {
    await seed();
    const tiers = await count("SELECT COUNT(*) AS n FROM support_tier_rules");
    const r = await enforceStorageBudget(withBudget(1));
    expect(r.trimmed).toEqual(["uploads", "excerpts", "events", "agg_device_daily"]);
    expect(r.after).toBe(0);
    expect(await count("SELECT COUNT(*) AS n FROM agg_device_daily")).toBe(0);
    expect(await count("SELECT COUNT(*) AS n FROM support_tier_rules")).toBe(tiers);
  });

  it("runs from the daily cron", async () => {
    await seed();
    await worker.scheduled(
      createScheduledController({ scheduledTime: new Date("2000-01-03T03:17:00Z"), cron: "17 3 * * *" }),
      { ...withBudget(1), RAW_RETENTION_DAYS: "3650", LOG_RETENTION_DAYS: "3650" } as Env,
    );
    expect(await count("SELECT COUNT(*) AS n FROM diag_uploads")).toBe(0);
    expect(await count("SELECT COUNT(*) AS n FROM events")).toBe(0);
  });
});
