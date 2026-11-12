import { createScheduledController } from "cloudflare:test";
import { env } from "cloudflare:workers";
import { beforeEach, describe, expect, it } from "vitest";
import type { Env } from "../src/env";
import worker from "../src/index";
import { enforceStorageBudget, estimateStorageBytes, EVENT_ROW_OVERHEAD, purgeExpired } from "../src/ingest";

// Each test file gets its own D1, but tests within a file share it, so every
// test here starts from empty data tables (the support-tier seed stays).
beforeEach(async () => {
  await env.DB.batch(
    ["events", "diag_uploads", "agg_device_daily", "agg_device_installs", "agg_error_daily", "rate_daily"].map((t) =>
      env.DB.prepare(`DELETE FROM ${t}`),
    ),
  );
});

const withBudget = (budget: number, maxRowWrites?: number): Env =>
  ({
    DB: env.DB,
    STORAGE_BUDGET_BYTES: String(budget),
    ...(maxRowWrites ? { STORAGE_TRIM_MAX_ROW_WRITES: String(maxRowWrites) } : {}),
  }) as Env;
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

  // Twenty 10 KB uploads, one per day, oldest first.
  async function seedUploads(): Promise<void> {
    const install = "5f0c2b1e-8d3a-4c6b-9e2f-0000000b0d9e";
    await env.DB.batch(
      Array.from({ length: 20 }, (_, i) => {
        const at = `2000-01-${String(i + 1).padStart(2, "0")}T00:00:00.000Z`;
        return env.DB.prepare(
          `INSERT INTO diag_uploads (received_at, client_ts, install_id, app, app_version, os, bytes, body)
           VALUES (?1, ?1, ?2, 'desktop', '2.5.0', 'linux', 10000, ?3)`,
        ).bind(at, install, String(i).padStart(2, "0").repeat(5_000));
      }),
    );
  }

  it("writes at most STORAGE_TRIM_MAX_ROW_WRITES rows per run, index entries included", async () => {
    await seedUploads();
    const estimate = await estimateStorageBytes(env);
    // Freeing everything would take 20 deletes x 3 writes; the cap allows 9 writes, so 3 uploads.
    const r = await enforceStorageBudget(withBudget(1, 9));
    expect(r).toMatchObject({ trimmed: ["uploads"], rowWrites: 9, capped: true });
    expect(r.after).toBe(estimate - 3 * (10_000 + EVENT_ROW_OVERHEAD));
    expect(r.after).toBe(await estimateStorageBytes(env));
    const left = (await env.DB.prepare("SELECT received_at FROM diag_uploads ORDER BY received_at").all<{ received_at: string }>())
      .results;
    expect(left).toHaveLength(17);
    expect(left[0]?.received_at).toBe("2000-01-04T00:00:00.000Z");
  });

  it("gives the trim only what the run's retention purge left of the allowance", async () => {
    await seedUploads();
    // 9 writes allowed, 3 already spent by the purge: 6 left, so 2 uploads.
    const r = await enforceStorageBudget(withBudget(1, 9), 3);
    expect(r).toMatchObject({ trimmed: ["uploads"], rowWrites: 6, capped: true });
    expect(await count("SELECT COUNT(*) AS n FROM diag_uploads")).toBe(18);
    // A purge that spent it all leaves the trim nothing, and says so.
    const spent = await enforceStorageBudget(withBudget(1, 9), 9);
    expect(spent).toMatchObject({ trimmed: [], rowWrites: 0, capped: true });
    expect(await count("SELECT COUNT(*) AS n FROM diag_uploads")).toBe(18);
  });

  it("counts the cron's retention deletes against the trim's allowance", async () => {
    await seedUploads();
    // Three uploads past LOG_RETENTION_DAYS cost the purge 9 writes of the 15:
    // the trim may then delete two more uploads, not five.
    const result = await purgeExpired(
      { ...withBudget(1, 15), LOG_RETENTION_DAYS: "1", RAW_RETENTION_DAYS: "1" } as Env,
      new Date("2000-01-04T03:17:00Z"),
    );
    expect(result.uploads).toBe(3);
    expect(result.storage).toMatchObject({ trimmed: ["uploads"], rowWrites: 6, capped: true });
    expect(await count("SELECT COUNT(*) AS n FROM diag_uploads")).toBe(15);
  });

  it("converges over several runs, oldest first, and stops once under the target", async () => {
    await seedUploads();
    // Budget just under 14 uploads' worth, so its 80% target means 11 uploads.
    // Each run may free 3; a capped run resumes the next day while still over budget.
    const perUpload = 10_000 + EVENT_ROW_OVERHEAD;
    const budget = 14 * perUpload - 1;
    const runs: Array<{ rowWrites: number; capped: boolean }> = [];
    for (let i = 0; i < 5; i++) {
      const r = await enforceStorageBudget(withBudget(budget, 9));
      runs.push({ rowWrites: r.rowWrites, capped: r.capped });
      expect(r.after).toBe(await estimateStorageBytes(env));
    }
    expect(runs).toEqual([
      { rowWrites: 9, capped: true },
      { rowWrites: 9, capped: true },
      { rowWrites: 9, capped: false },
      { rowWrites: 0, capped: false },
      { rowWrites: 0, capped: false },
    ]);
    expect(await estimateStorageBytes(env)).toBeLessThanOrEqual(Math.floor(budget * 0.8));
    const left = (await env.DB.prepare("SELECT MIN(received_at) AS first, COUNT(*) AS n FROM diag_uploads").first<{
      first: string;
      n: number;
    }>())!;
    expect(left).toEqual({ first: "2000-01-10T00:00:00.000Z", n: 11 });
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
