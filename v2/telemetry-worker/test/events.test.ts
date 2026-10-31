import { env } from "cloudflare:workers";
import { describe, expect, it } from "vitest";
import { utcDay } from "../src/ingest";
import { call, event, newInstallId, post } from "./helpers";

async function rowsFor(installId: string): Promise<Array<Record<string, unknown>>> {
  const r = await env.DB.prepare("SELECT * FROM events WHERE install_id = ?1 ORDER BY id").bind(installId).all();
  return r.results as Array<Record<string, unknown>>;
}

describe("POST /v1/events", () => {
  it("stores a valid batch and returns 204", async () => {
    const id = newInstallId();
    const res = await post("/v1/events", {
      events: [
        event({ install_id: id }),
        event({ install_id: id, kind: "action_result", action: "disable", ok: true }),
        event({ install_id: id, kind: "action_result", action: "disable", ok: false, error_code: "pm_denied" }),
        event({ install_id: id, kind: "error", device: undefined, error_code: "adb_missing", message: "adb not found in /usr/local/bin" }),
      ],
    }, { cf: { country: "US", city: "Austin", latitude: "30.2", asn: 7922 } });
    expect(res.status).toBe(204);
    expect(res.headers.get("x-events-accepted")).toBe("4");
    expect(res.headers.get("access-control-allow-origin")).toBeNull();

    const rows = await rowsFor(id);
    expect(rows).toHaveLength(4);
    expect(rows[0]).toMatchObject({ manufacturer: "NVIDIA", model: "SHIELD Android TV", sdk: 30, country: "US", kind: "device_seen" });
    expect(rows[3]).toMatchObject({ kind: "error", manufacturer: null, message: "adb not found in [path]" });
    // Only the country is kept from request.cf.
    expect(JSON.stringify(rows)).not.toMatch(/Austin|30\.2|7922/);
  });

  it("never stores forbidden fields that a client sends", async () => {
    const id = newInstallId();
    const res = await post("/v1/events", {
      events: [
        event({
          install_id: id,
          serial: "0323220012345",
          ip: "192.168.1.5",
          tv_name: "Living Room",
          packages: ["com.a.b", "com.c.d", "com.e.f"],
          license: "ABCD-EFGH",
          kind: "error",
          error_code: "connect_failed",
          message: "connect 192.168.1.5:5555 for 0323220012345 (Living Room) a4:5e:60:c1:22:9b",
        }),
      ],
    }, { headers: { "cf-connecting-ip": "203.0.113.9", "x-forwarded-for": "203.0.113.9" } });
    expect(res.status).toBe(204);
    const dump = JSON.stringify(await rowsFor(id));
    for (const s of ["0323220012345", "192.168.1.5", "203.0.113.9", "a4:5e:60", "ABCD-EFGH", "com.c.d"]) {
      expect(dump).not.toContain(s);
    }
    const cols = Object.keys((await rowsFor(id))[0] ?? {});
    expect(cols).not.toContain("ip");
    expect(cols).not.toContain("serial");
  });

  it("accepts the valid part of a mixed batch and reports rejects", async () => {
    const id = newInstallId();
    const res = await post("/v1/events", { events: [event({ install_id: id }), event({ kind: "bogus" }), event({ support_tier: "x" })] });
    expect(res.status).toBe(204);
    expect(res.headers.get("x-events-accepted")).toBe("2");
    expect(res.headers.get("x-events-rejected")).toBe("1");
  });

  it("returns 400 when nothing in the batch is valid", async () => {
    const res = await post("/v1/events", { events: [event({ kind: "bogus" })] });
    expect(res.status).toBe(400);
    expect(await res.json()).toEqual({ error: "no_valid_events", rejected: { unknown_kind: 1 } });
  });

  it("rejects malformed envelopes", async () => {
    expect((await post("/v1/events", "{not json")).status).toBe(400);
    expect((await post("/v1/events", { events: Array.from({ length: 51 }, () => event()) })).status).toBe(400);
    expect((await post("/v1/events", { event: event() })).status).toBe(400);
    expect((await post("/v1/events", { events: [event()] }, { headers: { "content-type": "text/plain" } })).status).toBe(415);
  });

  it("rejects bodies over 64 KB", async () => {
    const big = JSON.stringify({ events: [event({ message: "x".repeat(70 * 1024) })] });
    const res = await post("/v1/events", big);
    expect(res.status).toBe(413);
  });

  it("caps events per install per day", async () => {
    // vitest.config.ts sets PER_INSTALL_DAILY_EVENTS=60.
    const id = newInstallId();
    const batch = { events: Array.from({ length: 50 }, () => event({ install_id: id })) };
    expect((await post("/v1/events", batch)).status).toBe(204);
    const capped = await post("/v1/events", { events: Array.from({ length: 20 }, () => event({ install_id: id })) });
    expect(capped.status).toBe(429);
    expect(await capped.json()).toEqual({ error: "install_daily_cap" });
    expect(Number(capped.headers.get("retry-after"))).toBeGreaterThan(0);
    // The capped batch stored nothing.
    expect(await rowsFor(id)).toHaveLength(50);
    // Other installs are unaffected.
    expect((await post("/v1/events", { events: [event()] })).status).toBe(204);
  });

  it("writes nothing for requests refused by a cap that is already reached", async () => {
    const day = utcDay(new Date());
    const id = newInstallId();
    await env.DB.prepare("INSERT INTO rate_install_daily (day, install_id, events) VALUES (?1, ?2, 60)").bind(day, id).run();
    const globalBefore = await env.DB.prepare("SELECT events FROM rate_global_daily WHERE day = ?1").bind(day).first();
    for (let i = 0; i < 3; i++) {
      expect((await post("/v1/events", { events: [event({ install_id: id })] })).status).toBe(429);
    }
    const install = await env.DB.prepare("SELECT events FROM rate_install_daily WHERE day = ?1 AND install_id = ?2").bind(day, id).first();
    expect(install).toEqual({ events: 60 });
    const globalAfter = await env.DB.prepare("SELECT events FROM rate_global_daily WHERE day = ?1").bind(day).first();
    expect(globalAfter).toEqual(globalBefore);
  });

  it("caps events globally per day", async () => {
    const day = utcDay(new Date());
    await env.DB.prepare(
      "INSERT INTO rate_global_daily (day, events) VALUES (?1, 1000) ON CONFLICT (day) DO UPDATE SET events = 1000",
    ).bind(day).run();
    try {
      const fresh = newInstallId();
      const res = await post("/v1/events", { events: [event({ install_id: fresh })] });
      expect(res.status).toBe(429);
      expect(await res.json()).toEqual({ error: "global_daily_cap" });
      // A new install id hitting a full global cap costs no writes.
      expect(await env.DB.prepare("SELECT events FROM rate_global_daily WHERE day = ?1").bind(day).first()).toEqual({ events: 1000 });
      expect(await env.DB.prepare("SELECT 1 FROM rate_install_daily WHERE install_id = ?1").bind(fresh).first()).toBeNull();
    } finally {
      await env.DB.prepare("DELETE FROM rate_global_daily WHERE day = ?1").bind(day).run();
    }
  });

  it("answers OPTIONS without granting any origin, and rejects other methods", async () => {
    const pre = await call("/v1/events", { method: "OPTIONS", headers: { origin: "https://evil.example" } });
    expect(pre.status).toBe(204);
    expect(pre.headers.get("access-control-allow-origin")).toBeNull();
    expect((await call("/v1/events")).status).toBe(405);
    expect((await call("/nope")).status).toBe(404);
  });

  it("rolls events into the aggregate tables", async () => {
    const id = newInstallId();
    const device = { manufacturer: "AggTest", model: "Model A", device: "agga", profile: "googletv", support_tier: "beta" };
    await post("/v1/events", {
      events: [
        event({ install_id: id, device }),
        event({ install_id: id, device, kind: "action_result", action: "optimize", ok: true }),
        event({ install_id: id, device, kind: "action_result", action: "optimize", ok: false, error_code: "pm_denied" }),
        event({ install_id: id, device, kind: "error", error_code: "pm_denied" }),
      ],
    });
    const daily = await env.DB.prepare("SELECT * FROM agg_device_daily WHERE manufacturer = 'AggTest'").first();
    expect(daily).toMatchObject({ events: 4, seen: 1, action_ok: 1, action_fail: 1, errors: 1 });
    const err = await env.DB.prepare("SELECT count FROM agg_error_daily WHERE manufacturer = 'AggTest' AND error_code = 'pm_denied'").first();
    expect(err).toEqual({ count: 2 });
    const installs = await env.DB.prepare("SELECT COUNT(*) AS n FROM agg_device_installs WHERE manufacturer = 'AggTest'").first();
    expect(installs).toEqual({ n: 1 });
  });
});
