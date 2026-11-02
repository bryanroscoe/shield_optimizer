import { createScheduledController } from "cloudflare:test";
import { env } from "cloudflare:workers";
import { describe, expect, it } from "vitest";
import { cleanDiag } from "../src/diag";
import worker from "../src/index";
import {
  EXCERPT_MAX_BYTES,
  EXCERPT_MAX_LINES,
  LINE_REMOVED_PATH,
  LONG_TOKEN_REMOVED,
  redactLogExcerpt,
  redactUploadText,
} from "../src/redact";
import { UPLOAD_GLOBAL_KEY } from "../src/uploads";
import { parseSupportQuery, shortId } from "../src/support";
import { validateEvent } from "../src/validate";
import { ADMIN, call, event, post } from "./helpers";

const auth = { authorization: `Bearer ${ADMIN}` };

// Distinct first eight hex digits per test, so a short-id lookup finds one install.
function uniqueId(): string {
  return crypto.randomUUID();
}

// What the desktop app sends when Scan LAN finds nothing (engine/telemetry_diag.rs).
function scanDiag(): Record<string, unknown> {
  return {
    adb: { source: "bundled", version: "1.0.41", tools: "35.0.2", server: "reachable" },
    transports: { device: 0, unauthorized: 1, offline: 0, other: 0 },
    mdns: { check: "ok", services: 0, connect: 0, pairing: 0, legacy: 0 },
    net: { ipv4: 1, ipv6: 2, link_local: 0, vpn: 1, prefixes: [24], gateway: "detected", subnet_local: true },
    sweep: { probed: 254, open: 0, timeouts: 250, refused: 4, ms: 4100 },
    connect: { attempted: 0, connected: 0, unauthorized: 0, failed: 0, pairing: 0 },
    errors: ["adb_server"],
    ms: 5200,
  };
}

function diagnostic(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return event({
    schema: 2,
    kind: "diagnostic",
    device: undefined,
    action: "scan",
    error_code: "scan_empty",
    diag: scanDiag(),
    ...overrides,
  });
}

const EXCERPT = [
  "2026-11-02T14:00:01.500000Z DEBUG shield_optimizer_v2_lib::adb::driver: adb done args=[\"devices\"] ms=13",
  "<ip> unauthorized",
  "2026-11-02T14:00:02.000000Z INFO shield_optimizer_core::commands::devices: connect finished address=<redacted> error=failed: Connection refused",
].join("\n");

describe("cleanDiag", () => {
  it("keeps the contract fields", () => {
    expect(JSON.parse(cleanDiag(scanDiag()) ?? "null")).toEqual(scanDiag());
  });

  it("drops anything that could identify someone", () => {
    const hostile = {
      ...scanDiag(),
      serial: "0323220012345",
      adb: { source: "bundled", version: "/Users/jane/adb", tools: "35.0.2 (Jane's Mac)", server: "reachable", path: "/Users/jane" },
      transports: { device: 1, unauthorized: "192.168.1.5", offline: -1, other: 1.5, serials: ["0323220012345"] },
      mdns: { check: "Living Room TV", services: 99999, instances: ["adb-0323220012345-AbCdEf"] },
      net: { ipv4: 1, prefixes: [24, 24, 999, "16", 16], gateway: "192.168.1.1", names: ["en0"] },
      device: { state: "device", transport: "network", empty_props: ["ro.serialno", "persist.sys.device_name", "0323220012345"], error: "Has Spaces" },
      connect: { errors: ["connection_refused", "ip_192_168_1_5", "Bad Code", 7] },
      errors: "timeout",
    };
    const cleaned = JSON.parse(cleanDiag(hostile) ?? "null");
    const text = JSON.stringify(cleaned);
    for (const leak of ["0323220012345", "192.168", "jane", "Jane", "Living Room", "AbCdEf", "en0", "device_name", "Has Spaces", "path"]) {
      expect(text).not.toContain(leak);
    }
    expect(cleaned.adb).toEqual({ source: "bundled", server: "reachable" });
    expect(cleaned.transports).toEqual({ device: 1 });
    expect(cleaned.mdns).toEqual({ services: 9999 });
    expect(cleaned.net.prefixes).toEqual([16, 24]);
    expect(cleaned.net.gateway).toBeUndefined();
    expect(cleaned.device).toEqual({ state: "device", transport: "network", empty_props: ["ro.serialno"] });
    expect(cleaned.connect.errors).toEqual(["connection_refused", "redacted_code"]);
    expect(cleaned.errors).toBeUndefined();
    expect(cleanDiag("nope")).toBeNull();
    expect(cleanDiag({ unrelated: 1 })).toBeNull();
  });
});

describe("log text redaction", () => {
  it("re-redacts an excerpt and keeps the timestamp, level and module", () => {
    const hostile = [
      "2026-11-02T14:00:00.000000Z  INFO shield_optimizer_core::commands::devices: connect 192.168.42.211:5555 for 0323220012345 at living-room.local",
      "2026-11-02T14:00:01.000000Z DEBUG shield_optimizer_v2_lib::adb::driver: copy data/Jane Smith/x failed",
      "adb-0323220012345-AbCdEf\t_adb-tls-connect._tcp\t10.0.0.5:37000 a4:5e:60:c1:22:9b jane@example.com",
      "key ATVOPT-abc-def",
    ].join("\n");
    const out = redactLogExcerpt(hostile);
    for (const leak of ["192.168", "0323220012345", "living-room", "jane", "Jane", "AbCdEf", "10.0.0.5", "a4:5e", "ATVOPT"]) {
      expect(out).not.toContain(leak);
    }
    expect(out).toContain("2026-11-02T14:00:00.000000Z INFO shield_optimizer_core::commands::devices: connect [ip] for [serial] at [host]");
    expect(out).toContain(LINE_REMOVED_PATH);
    expect(redactLogExcerpt(EXCERPT)).toBe(EXCERPT);
  });

  it("bounds an excerpt by lines and bytes, keeping the newest", () => {
    const many = Array.from({ length: 500 }, (_, i) => `line ${i} ${"x".repeat(100)}`).join("\n");
    const out = redactLogExcerpt(many);
    expect(out.split("\n").length).toBeLessThanOrEqual(EXCERPT_MAX_LINES);
    expect(new TextEncoder().encode(out).length).toBeLessThanOrEqual(EXCERPT_MAX_BYTES);
    expect(out).toContain("line 499");
  });

  // The exact lines an independent review got past both redactors, as they
  // would arrive from a client that missed them.
  const NAME_AND_DNS = [
    "2026-11-02T14:00:00.000000Z DEBUG shield_optimizer_v2_lib::adb::driver: adb done args=[\"shell\", \"settings get global device_name\"] ms=40 exit_code=Some(0) stderr= stdout=Bedroom Shield",
    "2026-11-02T14:00:01.000000Z WARN shield_optimizer_v2_lib::adb::driver: adb timeout args=[\"shell\", \"settings put global device_name 'Kids Room TV'\"] ms=30001",
    "2026-11-02T14:00:02.000000Z WARN shield_optimizer_v2_lib::adb::driver: adb exited nonzero args=[\"shell\", \"settings put global private_dns_specifier '1a2b3c.dns.nextdns.io'; settings put global private_dns_mode hostname\"] exit_code=Some(1) stderr=Error: timed out",
    "while talking to 1a2b3c.dns.nextdns.io",
    "2026-11-02T14:00:03.000000Z DEBUG shield_optimizer_v2_lib::adb::driver: adb done stderr= stdout=[persist.sys.device_name]: [Living Room]",
    "[ro.product.model]: [SHIELD Android TV]",
    "2026-11-02T14:00:04.000000Z INFO shield_optimizer_core::commands::devices: rename failed: device_name = \"Den TV\"; dns private_dns_specifier=4a5b6c.dns.nextdns.io",
    "2026-11-02T14:00:05.000000Z INFO shield_optimizer_core::commands::devices: probe 7x7.example-dns.net and 0123abcd.dns.nextdns.io failed",
    "2026-11-02T14:00:06.000000Z INFO shield_optimizer_core::commands::devices: rename failed stderr=<timeout>",
  ].join("\n");
  const NAME_LEAKS = ["Bedroom", "Kids Room", "1a2b3c", "4a5b6c", "0123abcd", "nextdns", "Living Room", "Den TV", "7x7", "example-dns", "SHIELD Android"];

  it("drops command output, TV names and DNS hosts from excerpts and uploads", () => {
    for (const out of [redactLogExcerpt(NAME_AND_DNS), redactUploadText(NAME_AND_DNS)]) {
      for (const leak of NAME_LEAKS) expect(out, out).not.toContain(leak);
      expect(out).toContain("stdout=<omitted>");
      expect(out).toContain("private_dns_mode hostname");
      expect(out).toContain("rename failed stderr=<timeout>");
    }
    expect(redactLogExcerpt(NAME_AND_DNS)).toContain("device_name [redacted]");
  });

  it("redacts a pathological 256 KiB upload in linear time", () => {
    const shapes = [
      "a1".repeat(128 * 1024),
      "a.".repeat(128 * 1024),
      "a-".repeat(128 * 1024),
      `${"x".repeat(256 * 1024 - 1)}@`,
      "adb-".repeat(64 * 1024),
      "1.".repeat(128 * 1024),
      "device_name ".repeat(20 * 1024),
    ];
    for (const text of shapes) {
      const started = Date.now();
      const out = redactUploadText(text);
      expect(out.length).toBeGreaterThan(0);
      // Locally each shape takes about a millisecond; the old unbounded
      // pattern took from hundreds of milliseconds to over a minute.
      expect(Date.now() - started).toBeLessThan(100);
    }
    expect(redactUploadText(`ok ${"a1".repeat(1024)} ok`)).toBe(`ok ${LONG_TOKEN_REMOVED} ok`);
  });

  it("gives uploads a second pass for addresses and names", () => {
    const out = redactUploadText("connect 192.168.1.5:5555 a4:5e:60:c1:22:9b jane@example.com adb-0323220012345-x tv.local\nok");
    expect(out).toBe("connect [redacted] [redacted] [redacted] [redacted] [redacted]\nok");
  });
});

describe("schema 2 events", () => {
  it("validates diagnostics and keeps old clients working", () => {
    const d = validateEvent(diagnostic());
    expect(d.ok).toBe(true);
    if (d.ok) {
      expect(d.value.kind).toBe("diagnostic");
      expect(d.value.ok).toBeNull();
      expect(JSON.parse(d.value.diag ?? "null")).toEqual(scanDiag());
    }
    const reason = (raw: unknown) => {
      const r = validateEvent(raw);
      return r.ok ? null : r.reason;
    };
    expect(reason(diagnostic({ error_code: undefined }))).toBe("diagnostic_without_error_code");
    expect(reason(diagnostic({ diag: { junk: 1 } }))).toBe("diagnostic_without_diag");
    // A schema 1 client can't send the new kind, and its extra fields are ignored.
    expect(reason(diagnostic({ schema: 1 }))).toBe("unknown_kind");
    const old = validateEvent(event({ schema: 1, kind: "error", error_code: "x", diag: scanDiag(), log_excerpt: EXCERPT }));
    expect(old.ok && old.value.diag === null && old.value.log_excerpt === null).toBe(true);
  });

  it("stores diag and a desktop excerpt, but never one from the phone app", async () => {
    const id = uniqueId();
    const res = await post("/v1/events", {
      events: [
        diagnostic({ install_id: id, log_excerpt: EXCERPT }),
        diagnostic({ install_id: id, app: "mobile", app_version: "0.1.0", os: "android 15", error_code: "discover_empty", log_excerpt: EXCERPT }),
        event({ schema: 2, install_id: id, kind: "action_result", action: "connect", ok: true, log_excerpt: EXCERPT }),
        event({ schema: 1, install_id: id }),
      ],
    });
    expect(res.status).toBe(204);
    expect(res.headers.get("x-events-accepted")).toBe("4");
    const rows = (await env.DB.prepare("SELECT app, kind, diag, log_excerpt FROM events WHERE install_id = ?1 ORDER BY id")
      .bind(id)
      .all()).results as Array<Record<string, string | null>>;
    expect(JSON.parse(rows[0]?.diag ?? "null")).toEqual(scanDiag());
    expect(rows[0]?.log_excerpt).toBe(EXCERPT);
    expect(rows[1]).toMatchObject({ app: "mobile", log_excerpt: null });
    expect(rows[2]?.log_excerpt).toBeNull();
    expect(rows[3]).toMatchObject({ kind: "device_seen", diag: null, log_excerpt: null });
  });
});

function upload(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    schema: 1,
    install_id: uniqueId(),
    app: "desktop",
    app_version: "2.4.2",
    os: "macos 15.1",
    ts: "2026-11-02T12:00:00.000Z",
    reports_enabled: false,
    text: "## ATV Optimizer logs\n- Support ID: X\n2026-11-02T12:00:00Z INFO x::adb: connect 192.168.1.5:5555 refused\n",
    ...overrides,
  };
}

describe("POST /v1/diagnostics", () => {
  it("stores a redacted upload under the install id", async () => {
    const body = upload();
    const res = await post("/v1/diagnostics", body);
    expect(res.status).toBe(201);
    const out = (await res.json()) as { ok: boolean; id: number };
    expect(out.ok).toBe(true);
    const row = await env.DB.prepare("SELECT * FROM diag_uploads WHERE id = ?1").bind(out.id).first<Record<string, unknown>>();
    expect(row).toMatchObject({ install_id: body.install_id, app: "desktop", reports_on: 0 });
    expect(String(row?.body)).toContain("connect [redacted] refused");
    expect(String(row?.body)).not.toContain("192.168");
  });

  it("refuses the phone app, bad shapes and oversized text", async () => {
    expect((await post("/v1/diagnostics", upload({ app: "mobile" }))).status).toBe(400);
    expect((await post("/v1/diagnostics", upload({ install_id: "0323220012345" }))).status).toBe(400);
    expect((await post("/v1/diagnostics", upload({ text: "" }))).status).toBe(400);
    expect((await post("/v1/diagnostics", upload({ schema: 2 }))).status).toBe(400);
    expect((await post("/v1/diagnostics", upload({ text: "x".repeat(400 * 1024) }))).status).toBe(400);
    expect((await post("/v1/diagnostics", upload({ text: "x".repeat(700 * 1024) }))).status).toBe(413);
    expect((await post("/v1/diagnostics", "{}", { headers: { "content-type": "text/plain" } })).status).toBe(415);
    expect((await call("/v1/diagnostics")).status).toBe(405);
  });

  it("accepts a full-size log even when JSON escaping doubles it", async () => {
    const text = `args=["-s", "x"] C:\\Users\\path\n`.repeat(9000).slice(0, 280 * 1024);
    const body = JSON.stringify(upload({ text }));
    expect(body.length).toBeGreaterThan(text.length * 1.2);
    expect((await post("/v1/diagnostics", body)).status).toBe(201);
  });

  it("caps uploads per install per day", async () => {
    const install_id = uniqueId();
    for (let i = 0; i < 5; i++) expect((await post("/v1/diagnostics", upload({ install_id }))).status).toBe(201);
    const capped = await post("/v1/diagnostics", upload({ install_id }));
    expect(capped.status).toBe(429);
    expect(((await capped.json()) as { error: string }).error).toBe("upload_daily_cap");
    expect(Number(capped.headers.get("retry-after"))).toBeGreaterThan(0);
    // Refused attempts don't spend the shared budget.
    const day = new Date().toISOString().slice(0, 10);
    const global = async () =>
      ((await env.DB.prepare("SELECT events FROM rate_daily WHERE day = ?1 AND key = ?2").bind(day, UPLOAD_GLOBAL_KEY).first()) as {
        events: number;
      } | null)?.events ?? 0;
    const before = await global();
    for (let i = 0; i < 3; i++) expect((await post("/v1/diagnostics", upload({ install_id }))).status).toBe(429);
    expect(await global()).toBe(before);
    // Another install is unaffected.
    expect((await post("/v1/diagnostics", upload())).status).toBe(201);
  });
});

describe("POST /v1/diagnostics storage failures", () => {
  it("gives the per-install upload quota back when storing fails", async () => {
    const install_id = uniqueId();
    const day = new Date().toISOString().slice(0, 10);
    await env.DB.prepare(
      "CREATE TRIGGER fail_upload BEFORE INSERT ON diag_uploads WHEN NEW.os = 'fail-store' BEGIN SELECT RAISE(ABORT, 'simulated'); END",
    ).run();
    try {
      for (let i = 0; i < 6; i++) {
        const res = await post("/v1/diagnostics", upload({ install_id, os: "fail-store" }));
        expect(res.status).toBe(503);
      }
    } finally {
      await env.DB.prepare("DROP TRIGGER fail_upload").run();
    }
    const counter = await env.DB.prepare("SELECT events FROM rate_daily WHERE day = ?1 AND key = ?2")
      .bind(day, `upload:${install_id}`)
      .first();
    expect(counter).toEqual({ events: 0 });
    // Storage is back: the user can still send today.
    expect((await post("/v1/diagnostics", upload({ install_id }))).status).toBe(201);
  });
});

describe("support lookup", () => {
  it("parses full and short support ids", () => {
    const id = "7d0b0b9e-1f43-4e43-9b47-3a5d6f0f9c11";
    expect(parseSupportQuery(id.toUpperCase())).toEqual({ kind: "full", id });
    expect(parseSupportQuery("7D0B-0B9E")).toEqual({ kind: "prefix", prefix: "7d0b0b9e" });
    expect(parseSupportQuery(" 7d0b0b9e ")).toEqual({ kind: "prefix", prefix: "7d0b0b9e" });
    for (const bad of ["", "7d0b", "7d0b0b9e1", "zzzz-zzzz", "7d0b0b9e-1f43", "' OR 1=1 --"]) {
      expect(parseSupportQuery(bad)).toBeNull();
    }
    expect(shortId(id)).toBe("7D0B-0B9E");
  });

  it("finds an install's events, diagnostics, excerpts and uploads by short or full id", async () => {
    const id = uniqueId();
    expect((await post("/v1/events", {
      events: [
        event({ install_id: id }),
        diagnostic({ install_id: id, log_excerpt: EXCERPT }),
        event({ install_id: id, kind: "error", device: undefined, error_code: "timeout", message: "adb command timed out" }),
      ],
    })).status).toBe(204);
    const sent = await post("/v1/diagnostics", upload({ install_id: id }));
    const { id: uploadId } = (await sent.json()) as { id: number };

    const short = shortId(id);
    for (const q of [short, short.replace("-", "").toLowerCase(), id, id.toUpperCase()]) {
      const res = await call(`/v1/admin/support/${encodeURIComponent(q)}`, { headers: auth });
      expect(res.status).toBe(200);
      const body = (await res.json()) as {
        matches: number;
        installs: Array<{
          install_id: string;
          events: number;
          kinds: Record<string, number>;
          devices: Array<{ model: string }>;
          error_codes: Array<{ code: string }>;
          recent: Array<{ kind: string; diag: unknown; log_excerpt: string | null }>;
          uploads: Array<{ id: number; bytes: number }>;
        }>;
      };
      expect(body.matches).toBe(1);
      const install = body.installs[0]!;
      expect(install.install_id).toBe(id);
      expect(install.events).toBe(3);
      expect(install.kinds).toEqual({ device_seen: 1, diagnostic: 1, error: 1 });
      expect(install.devices[0]?.model).toBe("SHIELD Android TV");
      expect(install.error_codes.map((c) => c.code).sort()).toEqual(["scan_empty", "timeout"]);
      const diag = install.recent.find((e) => e.kind === "diagnostic");
      expect(diag?.diag).toEqual(scanDiag());
      expect(diag?.log_excerpt).toBe(EXCERPT);
      expect(install.uploads.map((u) => u.id)).toEqual([uploadId]);
    }

    const text = await call(`/v1/admin/uploads/${uploadId}`, { headers: auth });
    expect(text.status).toBe(200);
    expect(text.headers.get("content-type")).toMatch(/^text\/plain/);
    expect(text.headers.get("x-install-id")).toBe(id);
    expect(await text.text()).toContain("## ATV Optimizer logs");
  });

  it("finds an install that only sent logs, and answers empty for an unknown one", async () => {
    const id = uniqueId();
    expect((await post("/v1/diagnostics", upload({ install_id: id }))).status).toBe(201);
    const found = (await (await call(`/v1/admin/support/${id}`, { headers: auth })).json()) as {
      installs: Array<{ events: number; uploads: unknown[] }>;
    };
    expect(found.installs[0]?.events).toBe(0);
    expect(found.installs[0]?.uploads).toHaveLength(1);
    const none = (await (await call("/v1/admin/support/00000000-0000-4000-8000-000000000000", { headers: auth })).json()) as {
      matches: number;
    };
    expect(none.matches).toBe(0);
  });

  it("needs the admin token and a well-formed id", async () => {
    expect((await call("/v1/admin/support/7D0B-0B9E")).status).toBe(401);
    expect((await call("/v1/admin/support/7D0B-0B9E", { headers: { authorization: "Bearer wrong-token-0123456789" } })).status).toBe(401);
    expect((await call("/v1/admin/uploads/1")).status).toBe(401);
    expect((await call("/v1/admin/support/not-an-id", { headers: auth })).status).toBe(400);
    expect((await call("/v1/admin/support/7D0B-0B9E?limit=0", { headers: auth })).status).toBe(400);
    expect((await call("/v1/admin/uploads/999999999", { headers: auth })).status).toBe(404);
  });
});

describe("log retention", () => {
  it("clears excerpts and deletes uploads after 30 days, keeping the events and diag", async () => {
    const id = uniqueId();
    const old = "2026-01-01T00:00:00.000Z";
    const recent = new Date(Date.now() - 5 * 86_400_000).toISOString();
    await env.DB.batch([
      env.DB.prepare(
        `INSERT INTO events (received_at, client_ts, install_id, app, app_version, os, kind, error_code, diag, log_excerpt)
         VALUES (?1, ?1, ?2, 'desktop', '2.4.2', 'linux', 'diagnostic', 'scan_empty', '{"ms":1}', 'old excerpt')`,
      ).bind(new Date(Date.now() - 40 * 86_400_000).toISOString(), id),
      env.DB.prepare(
        `INSERT INTO events (received_at, client_ts, install_id, app, app_version, os, kind, error_code, diag, log_excerpt)
         VALUES (?1, ?1, ?2, 'desktop', '2.4.2', 'linux', 'diagnostic', 'scan_empty', '{"ms":2}', 'new excerpt')`,
      ).bind(recent, id),
      env.DB.prepare(
        `INSERT INTO diag_uploads (received_at, client_ts, install_id, app, app_version, os, bytes, body)
         VALUES (?1, ?1, ?2, 'desktop', '2.4.2', 'linux', 3, 'old')`,
      ).bind(old, id),
      env.DB.prepare(
        `INSERT INTO diag_uploads (received_at, client_ts, install_id, app, app_version, os, bytes, body)
         VALUES (?1, ?1, ?2, 'desktop', '2.4.2', 'linux', 3, 'new')`,
      ).bind(recent, id),
    ]);

    await worker.scheduled(createScheduledController({ scheduledTime: new Date(), cron: "17 3 * * *" }), env);

    const events = (await env.DB.prepare("SELECT diag, log_excerpt FROM events WHERE install_id = ?1 ORDER BY received_at")
      .bind(id)
      .all()).results;
    expect(events).toEqual([
      { diag: '{"ms":1}', log_excerpt: null },
      { diag: '{"ms":2}', log_excerpt: "new excerpt" },
    ]);
    const uploads = (await env.DB.prepare("SELECT body FROM diag_uploads WHERE install_id = ?1").bind(id).all()).results;
    expect(uploads).toEqual([{ body: "new" }]);
  });
});
