import { createScheduledController } from "cloudflare:test";
import { env } from "cloudflare:workers";
import { describe, expect, it } from "vitest";
import worker from "../src/index";
import { matchTier, type TierDoc } from "../src/tiers";
import { ADMIN, call, event, newInstallId, post } from "./helpers";

const auth = { authorization: `Bearer ${ADMIN}` };

describe("GET /v1/support-tiers", () => {
  it("serves the seeded rules with a one-hour public cache", async () => {
    const res = await call("/v1/support-tiers");
    expect(res.status).toBe(200);
    expect(res.headers.get("cache-control")).toBe("public, max-age=3600");
    const doc = (await res.json()) as TierDoc;
    expect(doc.schema).toBe(1);
    expect(doc.updated).toBe("2026-10-31T00:00:00Z");
    expect(doc.rules.map((r) => r.tier)).toEqual(["fully_supported", "beta", "not_supported"]);
    expect(doc.rules[2]).toMatchObject({ match: { manufacturer: "Amazon", model_prefix: "AFT" } });
  });

  it("seed rules classify real device strings (first match wins, case-insensitive)", async () => {
    const doc = (await (await call("/v1/support-tiers")).json()) as TierDoc;
    expect(matchTier(doc.rules, { manufacturer: "NVIDIA", model: "SHIELD Android TV" })?.tier).toBe("fully_supported");
    expect(matchTier(doc.rules, { manufacturer: "TCL", model: "QM7L Pro" })?.tier).toBe("beta");
    expect(matchTier(doc.rules, { manufacturer: "Amazon", model: "AFTMM" })?.tier).toBe("not_supported");
    expect(matchTier(doc.rules, { manufacturer: "amazon", model: "aftkrt" })?.tier).toBe("not_supported");
    expect(matchTier(doc.rules, { manufacturer: "Google", model: "Chromecast" })).toBeNull();
  });
});

describe("admin auth", () => {
  it("rejects missing and wrong tokens", async () => {
    expect((await call("/v1/admin/devices")).status).toBe(401);
    expect((await call("/v1/admin/devices", { headers: { authorization: "Bearer wrong-token-0000000000" } })).status).toBe(401);
    expect((await call("/v1/admin/devices", { headers: { authorization: ADMIN } })).status).toBe(401);
    const put = await post("/v1/admin/support-tiers", { rules: [] }, { method: "PUT" });
    expect(put.status).toBe(401);
    expect(put.headers.get("www-authenticate")).toContain("Bearer");
  });

  it("is disabled when ADMIN_TOKEN is unset or too short", async () => {
    const req = new Request("https://telemetry.test/v1/admin/devices", { headers: { authorization: "Bearer short" } });
    const res = await worker.fetch(req, { ...env, ADMIN_TOKEN: "short" });
    expect(res.status).toBe(503);
    const res2 = await worker.fetch(req, { ...env, ADMIN_TOKEN: undefined });
    expect(res2.status).toBe(503);
  });
});

describe("PUT /v1/admin/support-tiers", () => {
  it("validates and replaces the rule list", async () => {
    const bad = await post("/v1/admin/support-tiers", { rules: [{ match: {}, tier: "beta", note: "x" }] }, { method: "PUT", headers: auth });
    expect(bad.status).toBe(400);
    expect(await bad.json()).toEqual({ error: "rule_0_empty_match" });
    const badTier = await post("/v1/admin/support-tiers", { rules: [{ match: { brand: "X" }, tier: "great", note: "x" }] }, { method: "PUT", headers: auth });
    expect(badTier.status).toBe(400);

    const rules = [
      { match: { manufacturer: "Sony", model_prefix: "BRAVIA" }, tier: "supported", note: "Sony BRAVIA works." },
      { match: { brand: "onn", device: "dopinder" }, tier: "beta", note: "onn 4K box in beta.", extra: "dropped" },
    ];
    const res = await post("/v1/admin/support-tiers", { rules }, { method: "PUT", headers: auth });
    expect(res.status).toBe(200);
    const doc = (await (await call("/v1/support-tiers")).json()) as TierDoc;
    expect(doc.rules).toEqual([
      { match: { manufacturer: "Sony", model_prefix: "BRAVIA" }, tier: "supported", note: "Sony BRAVIA works." },
      { match: { brand: "onn", device: "dopinder" }, tier: "beta", note: "onn 4K box in beta." },
    ]);
    expect(doc.updated).not.toBe("2026-10-31T00:00:00Z");
  });
});

describe("GET /v1/admin/devices", () => {
  it("aggregates installs, action results and top error codes per device", async () => {
    const dev = { manufacturer: "ReportCo", brand: "ReportCo", model: "RC 4K", device: "rc4k", profile: "googletv", support_tier: "beta" };
    const a = newInstallId();
    const b = newInstallId();
    await post("/v1/events", {
      events: [
        event({ install_id: a, device: dev }),
        event({ install_id: a, device: dev, kind: "action_result", action: "disable", ok: true }),
        event({ install_id: a, device: dev, kind: "action_result", action: "disable", ok: false, error_code: "pm_denied" }),
        event({ install_id: b, device: dev }),
        event({ install_id: b, device: dev, kind: "error", error_code: "pm_denied" }),
        event({ install_id: b, device: dev, kind: "error", error_code: "adb_timeout" }),
        event({ install_id: b, device: dev, app_version: "2.3.2" }),
      ],
    });

    const res = await call("/v1/admin/devices?group=manufacturer,model,device,profile", { headers: auth });
    expect(res.status).toBe(200);
    expect(res.headers.get("cache-control")).toBe("no-store");
    const body = (await res.json()) as { rows: Array<Record<string, unknown>> };
    const row = body.rows.find((r) => r.manufacturer === "ReportCo");
    expect(row).toMatchObject({
      model: "RC 4K",
      device: "rc4k",
      profile: "googletv",
      installs: 2,
      seen: 3,
      action_ok: 1,
      action_fail: 1,
      errors: 2,
      top_errors: [
        { code: "pm_denied", count: 2 },
        { code: "adb_timeout", count: 1 },
      ],
    });
    expect(typeof row?.last_seen).toBe("string");

    const full = (await (await call("/v1/admin/devices", { headers: auth })).json()) as { rows: Array<Record<string, unknown>> };
    const versions = full.rows.filter((r) => r.manufacturer === "ReportCo").map((r) => r.app_version).sort();
    expect(versions).toEqual(["2.3.1", "2.3.2"]);

    expect((await call("/v1/admin/devices?group=serial", { headers: auth })).status).toBe(400);
    expect((await call("/v1/admin/devices?days=0", { headers: auth })).status).toBe(400);
    expect((await call("/v1/admin/devices?days=30", { headers: auth })).status).toBe(200);
  });
});

describe("retention cron", () => {
  it("purges raw events past 90 days and keeps aggregates", async () => {
    const id = newInstallId();
    const dev = { manufacturer: "OldCo", model: "Old", device: "old", profile: "androidtv" };
    expect((await post("/v1/events", { events: [event({ install_id: id, device: dev })] })).status).toBe(204);
    await env.DB.prepare(
      `INSERT INTO events (received_at, client_ts, install_id, app, app_version, os, kind)
       VALUES ('2000-01-01T00:00:00.000Z', '2000-01-01T00:00:00.000Z', ?1, 'desktop', '1.0.0', 'linux', 'device_seen')`,
    ).bind(id).run();
    await env.DB.prepare("INSERT INTO rate_install_daily (day, install_id, events) VALUES ('2000-01-01', ?1, 5)").bind(id).run();

    await worker.scheduled(createScheduledController({ scheduledTime: new Date(), cron: "17 3 * * *" }), env);

    const left = await env.DB.prepare("SELECT received_at FROM events WHERE install_id = ?1").bind(id).all();
    expect(left.results).toHaveLength(1);
    expect((left.results[0] as { received_at: string }).received_at.startsWith("2000")).toBe(false);
    const rate = await env.DB.prepare("SELECT COUNT(*) AS n FROM rate_install_daily WHERE day = '2000-01-01'").first();
    expect(rate).toEqual({ n: 0 });
    const agg = await env.DB.prepare("SELECT COUNT(*) AS n FROM agg_device_daily WHERE manufacturer = 'OldCo'").first();
    expect(agg).toEqual({ n: 1 });
  });
});
