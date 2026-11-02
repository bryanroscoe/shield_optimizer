import { env } from "cloudflare:workers";
import { describe, expect, it } from "vitest";
import worker from "../src/index";
import { ADMIN, call, event, newInstallId, post } from "./helpers";

// Owner decision 2026-11-01: the backend stores no location of any kind, so
// request.cf (country, city, colo, ASN, coordinates...) must never be read,
// stored or reported.

const LOCATION_WORDS = /country|city|region|colo|asn|latitude|longitude|timezone|postal|continent|metro/i;

const CF = {
  country: "NZ",
  city: "Auckland",
  region: "Auckland Region",
  colo: "AKL",
  asn: 9500,
  latitude: "-36.84850",
  longitude: "174.76333",
  timezone: "Pacific/Auckland",
  postalCode: "1010",
  continent: "OC",
};

describe("no location", () => {
  it("never reads request.cf while ingesting", async () => {
    const id = newInstallId();
    const request = new Request("https://telemetry.test/v1/events", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ events: [event({ install_id: id })] }),
    });
    let reads = 0;
    Object.defineProperty(request, "cf", {
      get() {
        reads += 1;
        return CF;
      },
    });
    const res = await worker.fetch(request as Parameters<typeof worker.fetch>[0], env);
    expect(res.status).toBe(204);
    expect(reads).toBe(0);
    const stored = await env.DB.prepare("SELECT COUNT(*) AS n FROM events WHERE install_id = ?1").bind(id).first<{ n: number }>();
    expect(stored?.n).toBe(1);
  });

  it("has no location column in any table", async () => {
    const tables = await env.DB.prepare(
      "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '_cf_%' AND name <> 'd1_migrations'",
    ).all<{ name: string }>();
    const names = tables.results.map((t) => t.name);
    expect(names).toContain("events");
    for (const name of names) {
      const cols = await env.DB.prepare("SELECT name FROM pragma_table_info(?1)").bind(name).all<{ name: string }>();
      for (const c of cols.results) expect(`${name}.${c.name}`).not.toMatch(LOCATION_WORDS);
    }
  });

  it("keeps nothing from request.cf in storage or the admin report", async () => {
    const id = newInstallId();
    const dev = { manufacturer: "GeoCo", brand: "GeoCo", model: "Geo 1", device: "geo1", profile: "googletv", support_tier: "beta" };
    const res = await post("/v1/events", {
      events: [event({ install_id: id, device: dev }), event({ install_id: id, device: dev, kind: "error", error_code: "adb_timeout" })],
    }, { cf: CF });
    expect(res.status).toBe(204);

    const rows = await env.DB.prepare("SELECT * FROM events WHERE install_id = ?1").bind(id).all();
    expect(rows.results).toHaveLength(2);
    const values = JSON.stringify(rows.results);
    for (const v of Object.values(CF)) expect(values).not.toContain(String(v));

    const report = await call("/v1/admin/devices", { headers: { authorization: `Bearer ${ADMIN}` } });
    expect(report.status).toBe(200);
    const body = (await report.json()) as { rows: Array<Record<string, unknown>> };
    const row = body.rows.find((r) => r.manufacturer === "GeoCo");
    expect(row).toBeDefined();
    for (const r of body.rows) for (const k of Object.keys(r)) expect(k).not.toMatch(LOCATION_WORDS);
    const text = JSON.stringify(body);
    for (const v of Object.values(CF)) expect(text).not.toContain(String(v));
  });
});
