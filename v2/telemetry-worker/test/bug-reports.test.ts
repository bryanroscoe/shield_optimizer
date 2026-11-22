import { createExecutionContext, createScheduledController, waitOnExecutionContext } from "cloudflare:test";
import { env } from "cloudflare:workers";
import { describe, expect, it } from "vitest";
import worker from "../src/index";
import { buildReportEmail, headerValue, MAIL_COUNTER_KEY, type MailUpload } from "../src/notify";
import { redactNote } from "../src/redact";
import { shortId } from "../src/support";
import { MOBILE_MAX_TEXT, NOTE_MAX_CHARS, validateUpload } from "../src/uploads";
import { ADMIN, call, post } from "./helpers";

const auth = { authorization: `Bearer ${ADMIN}` };

function bug(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    schema: 2,
    kind: "bug",
    install_id: crypto.randomUUID(),
    app: "mobile",
    app_version: "0.2.0",
    os: "android 15",
    ts: "2026-11-22T12:00:00.000Z",
    reports_enabled: true,
    note: "The TV list is empty after I pair. My TV is at 192.168.1.40 and my email is jane@example.com.",
    text: "## ATV Optimizer bug report\n- Phone: Pixel 10 Pro, Android 15\n- TV: NVIDIA SHIELD Android TV, Android 11\n",
    ...overrides,
  };
}

function reason(raw: unknown): string | null {
  const r = validateUpload(raw);
  return r.ok ? null : r.reason;
}

const HOSTILE_NOTE = [
  "Pairing fails. TV at 192.168.1.40:5555 and fe80::1c2b:3d4e:5f60:7a8b, MAC a4:5e:60:c1:22:9b,",
  "serial 0323220012345 (also R5CRABCDEFZ), mail jane@example.com, see https://example.com/u/jane?x=1",
  "logs in C:\\Users\\Jane\\AppData and /home/jane/.config, also living-room.local and 1a2b3c.dns.nextdns.io",
  "adb-0323220012345-AbCdEf._adb-tls-connect._tcp my key ATVOPT-abc123-def456 and device_name 'Kids Room TV'",
  "It works on and/or off. Café ☕ ok.",
].join("\n");
const NOTE_LEAKS = [
  "192.168", "fe80", "a4:5e", "0323220012345", "R5CRABCDEFZ", "jane", "Jane", "example.com", "living-room",
  "1a2b3c", "nextdns", "AbCdEf", "ATVOPT", "abc123", "Kids Room",
];

describe("note redaction", () => {
  it("removes addresses, serials, emails, hosts, paths, URLs and licence keys", () => {
    const out = redactNote(HOSTILE_NOTE);
    for (const leak of NOTE_LEAKS) expect(out, out).not.toContain(leak);
    expect(out).toContain("Pairing fails.");
    expect(out).toContain("It works on and/or off. Café ☕ ok.");
    expect(out.split("\n")).toHaveLength(5);
  });

  it("stays linear on a hostile 4000-character note", () => {
    for (const text of ["a/".repeat(2000), "1:".repeat(2000), `${"x".repeat(3999)}@`, "a1".repeat(2000)]) {
      const started = Date.now();
      redactNote(text);
      expect(Date.now() - started).toBeLessThan(2000);
    }
  });
});

describe("POST /v1/diagnostics schema 2", () => {
  it("validates kind, app, note and text", () => {
    expect(reason(bug())).toBeNull();
    expect(reason(bug({ kind: undefined }))).toBe("bad_kind");
    expect(reason(bug({ kind: "crash" }))).toBe("bad_kind");
    expect(reason(bug({ app: "web" }))).toBe("unsupported_app");
    expect(reason(bug({ schema: 1 }))).toBe("unsupported_app");
    expect(reason(bug({ schema: 3 }))).toBe("unsupported_schema");
    expect(reason(bug({ note: 5 }))).toBe("bad_note");
    expect(reason(bug({ note: "x".repeat(NOTE_MAX_CHARS + 1) }))).toBe("note_too_large");
    // Trimmed first, and counted in characters, not UTF-16 units.
    expect(reason(bug({ note: `  ${"☕".repeat(NOTE_MAX_CHARS)}  ` }))).toBeNull();
    expect(reason(bug({ note: "😀".repeat(NOTE_MAX_CHARS) }))).toBeNull();
    expect(reason(bug({ note: "   ", text: "" }))).toBe("empty_report");
    expect(reason(bug({ note: undefined, text: undefined }))).toBe("empty_report");
    expect(reason(bug({ text: undefined }))).toBeNull();
    expect(reason(bug({ note: undefined }))).toBeNull();
    expect(reason(bug({ text: 7 }))).toBe("bad_text");
    expect(reason(bug({ kind: "logs", text: "  " }))).toBe("bad_text");
    expect(reason(bug({ text: "x".repeat(MOBILE_MAX_TEXT + 1) }))).toBe("text_too_large");
    expect(reason(bug({ app: "desktop", text: "x".repeat(MOBILE_MAX_TEXT + 1) }))).toBeNull();
  });

  it("stores a phone bug report with its redacted note and counts the note's bytes", async () => {
    const body = bug({ note: HOSTILE_NOTE });
    const day = new Date().toISOString().slice(0, 10);
    const counter = async () =>
      (await env.DB.prepare("SELECT events FROM rate_daily WHERE day = ?1 AND key = 'upload-bytes:*'").bind(day).first<{ events: number }>())
        ?.events ?? 0;
    const before = await counter();
    const res = await post("/v1/diagnostics", body);
    expect(res.status).toBe(201);
    const out = (await res.json()) as { ok: boolean; id: number; bytes: number };
    expect(out.ok).toBe(true);
    const row = await env.DB.prepare("SELECT * FROM diag_uploads WHERE id = ?1").bind(out.id).first<Record<string, unknown>>();
    expect(row).toMatchObject({ install_id: body.install_id, app: "mobile", kind: "bug", reports_on: 1 });
    const note = String(row?.note);
    for (const leak of NOTE_LEAKS) expect(note).not.toContain(leak);
    expect(note).toContain("Pairing fails.");
    const enc = new TextEncoder();
    const expected = enc.encode(String(row?.body)).length + enc.encode(note).length;
    expect(row?.bytes).toBe(expected);
    expect((await counter()) - before).toBe(expected);
  });

  it("takes a bug report with only a note, and keeps schema 1 as it was", async () => {
    const noteOnly = await post("/v1/diagnostics", bug({ text: undefined, app: "desktop", app_version: "2.6.0", os: "macos 15.1" }));
    expect(noteOnly.status).toBe(201);
    const { id, bytes } = (await noteOnly.json()) as { id: number; bytes: number };
    expect(bytes).toBe(0);
    expect(await env.DB.prepare("SELECT kind, body, app FROM diag_uploads WHERE id = ?1").bind(id).first()).toEqual({
      kind: "bug",
      body: "",
      app: "desktop",
    });

    const legacy = await post("/v1/diagnostics", { ...bug({ schema: 1, app: "desktop", kind: undefined }), note: "ignored on schema 1" });
    expect(legacy.status).toBe(201);
    const legacyId = ((await legacy.json()) as { id: number }).id;
    expect(await env.DB.prepare("SELECT kind, note FROM diag_uploads WHERE id = ?1").bind(legacyId).first()).toEqual({
      kind: "logs",
      note: null,
    });

    expect((await post("/v1/diagnostics", bug({ note: "x".repeat(NOTE_MAX_CHARS + 1) }))).status).toBe(400);
    expect((await post("/v1/diagnostics", bug({ schema: 1 }))).status).toBe(400);
  });

  it("accepts a full-size desktop log plus a full note when JSON escaping doubles them", async () => {
    const text = `args=["-s", "x"] C:\\Users\\path\n`.repeat(9000).slice(0, 280 * 1024);
    // A control character is the longest escape JSON has: six bytes.
    const note = `x${"\u0001".repeat(NOTE_MAX_CHARS - 2)}x`;
    expect(JSON.stringify(note).length).toBeGreaterThan(NOTE_MAX_CHARS * 5);
    const res = await post("/v1/diagnostics", bug({ app: "desktop", app_version: "2.6.0", os: "windows", kind: "logs", text, note }));
    expect(res.status).toBe(201);
  });
});

describe("admin views of bug reports", () => {
  it("puts the note at the top of the upload text and in the support lookup", async () => {
    const install_id = crypto.randomUUID();
    const res = await post("/v1/diagnostics", bug({ install_id, note: "Scan finds nothing.\nSecond line." }));
    const { id } = (await res.json()) as { id: number };

    const text = await call(`/v1/admin/uploads/${id}`, { headers: auth });
    expect(text.status).toBe(200);
    expect(text.headers.get("x-upload-kind")).toBe("bug");
    expect(text.headers.get("x-app")).toBe("mobile 0.2.0");
    const body = await text.text();
    expect(body.startsWith("## Description (user's own words, redacted)\n\nScan finds nothing.\nSecond line.\n\n## ATV Optimizer bug report")).toBe(true);

    const lookup = (await (await call(`/v1/admin/support/${install_id}`, { headers: auth })).json()) as {
      installs: Array<{ uploads: Array<{ id: number; kind: string; app: string; note: string | null }> }>;
    };
    expect(lookup.installs[0]?.uploads).toEqual([
      expect.objectContaining({ id, kind: "bug", app: "mobile", note: "Scan finds nothing.\nSecond line." }),
    ]);
  });

  it("lists recent uploads newest first, filtered by kind and time, with a bounded note preview", async () => {
    const before = new Date(Date.now() - 1000).toISOString();
    const first = (await (await post("/v1/diagnostics", bug({ note: `first ${"n".repeat(400)}` }))).json()) as { id: number };
    const logs = (await (await post("/v1/diagnostics", bug({ kind: "logs", note: undefined }))).json()) as { id: number };
    const second = (await (await post("/v1/diagnostics", bug({ note: "second" }))).json()) as { id: number };

    type List = {
      since: string | null;
      kind: string | null;
      limit: number;
      truncated: boolean;
      uploads: Array<{ id: number; kind: string; support_id: string; install_id: string; note_preview: string | null; note_chars: number }>;
    };
    const res = await call(`/v1/admin/uploads?kind=bug&since=${encodeURIComponent(before)}&limit=200`, { headers: auth });
    expect(res.status).toBe(200);
    const list = (await res.json()) as List;
    expect(list.kind).toBe("bug");
    const ids = list.uploads.map((u) => u.id);
    expect(ids).toContain(first.id);
    expect(ids).not.toContain(logs.id);
    expect(ids.indexOf(second.id)).toBeLessThan(ids.indexOf(first.id));
    const firstRow = list.uploads.find((u) => u.id === first.id)!;
    expect(firstRow.note_preview).toHaveLength(200);
    expect(firstRow.note_chars).toBe(406);
    expect(firstRow.support_id).toBe(shortId(firstRow.install_id));
    expect(JSON.stringify(list)).not.toContain("ATV Optimizer bug report");

    const one = (await (await call("/v1/admin/uploads?limit=1", { headers: auth })).json()) as List;
    expect(one.uploads).toHaveLength(1);
    expect(one.truncated).toBe(true);
    const future = (await (await call("/v1/admin/uploads?since=2999-01-01", { headers: auth })).json()) as List;
    expect(future.uploads).toEqual([]);

    expect((await call("/v1/admin/uploads")).status).toBe(401);
    for (const bad of ["kind=crash", "since=yesterday", "since=2026-13-45", "limit=0", "limit=201", "limit=x"]) {
      expect((await call(`/v1/admin/uploads?${bad}`, { headers: auth })).status, bad).toBe(400);
    }
    expect((await call("/v1/admin/uploads", { method: "POST", headers: auth })).status).toBe(405);
  });

  it("purges bug reports with the other uploads after 30 days", async () => {
    const install_id = crypto.randomUUID();
    await env.DB.prepare(
      `INSERT INTO diag_uploads (received_at, client_ts, install_id, app, app_version, os, bytes, body, kind, note)
       VALUES ('2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z', ?1, 'mobile', '0.2.0', 'android 15', 4, '', 'bug', 'old!')`,
    )
      .bind(install_id)
      .run();
    await worker.scheduled(createScheduledController({ scheduledTime: new Date(), cron: "17 3 * * *" }), env);
    expect(await env.DB.prepare("SELECT COUNT(*) AS n FROM diag_uploads WHERE install_id = ?1").bind(install_id).first()).toEqual({
      n: 0,
    });
  });
});

interface Sent {
  from: string;
  to: string;
  raw: string;
}

function fakeMail(opts: { fail?: boolean } = {}): { binding: SendEmail; sent: Sent[] } {
  const sent: Sent[] = [];
  const binding = {
    async send(message: EmailMessage) {
      if (opts.fail) throw new Error("destination not verified");
      // Miniflare keeps the raw message under this key; the test only reads it.
      const raw = (message as unknown as Record<string, unknown>)["EmailMessage::raw"];
      sent.push({ from: message.from, to: message.to, raw: typeof raw === "string" ? raw : "" });
      return { messageId: "fake" };
    },
  } as unknown as SendEmail;
  return { binding, sent };
}

function mailEnv(binding: SendEmail | undefined, extra: Record<string, string> = {}): typeof env {
  return {
    ...env,
    REPORT_MAIL: binding,
    REPORT_MAIL_FROM: "reports@example.org",
    REPORT_MAIL_TO: "owner@example.net",
    ...extra,
  };
}

async function postWith(e: typeof env, body: unknown): Promise<Response> {
  const ctx = createExecutionContext();
  const res = await worker.fetch(
    new Request("https://telemetry.test/v1/diagnostics", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    }),
    e,
    ctx,
  );
  await waitOnExecutionContext(ctx);
  return res;
}

function decode(raw: string): { headers: string[]; body: string } {
  const split = raw.indexOf("\r\n\r\n");
  const headers = raw.slice(0, split).split("\r\n");
  const b64 = raw.slice(split + 4).replace(/\r\n/g, "");
  const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
  return { headers, body: new TextDecoder().decode(bytes) };
}

async function mailCount(): Promise<number> {
  const day = new Date().toISOString().slice(0, 10);
  return (
    (await env.DB.prepare("SELECT events FROM rate_daily WHERE day = ?1 AND key = ?2").bind(day, MAIL_COUNTER_KEY).first<{ events: number }>())
      ?.events ?? 0
  );
}

describe("upload email", () => {
  it("emails the owner once per bug report, with the redacted note and no log text", async () => {
    const { binding, sent } = fakeMail();
    const body = bug({ note: HOSTILE_NOTE });
    const res = await postWith(mailEnv(binding), body);
    expect(res.status).toBe(201);
    const { id } = (await res.json()) as { id: number };
    expect(sent).toHaveLength(1);
    const mail = sent[0]!;
    expect(mail.from).toBe("reports@example.org");
    expect(mail.to).toBe("owner@example.net");
    const { headers, body: text } = decode(mail.raw);
    const support = shortId(String(body.install_id));
    expect(headers).toContain(`Subject: [ATV Optimizer] Bug report #${id} from mobile 0.2.0 (${support})`);
    expect(headers).toContain("From: ATV Optimizer reports <reports@example.org>");
    expect(headers).toContain("To: <owner@example.net>");
    expect(headers).toContain("Content-Type: text/plain; charset=utf-8");
    expect(headers).toContain("Content-Transfer-Encoding: base64");
    expect(headers.some((h) => /^Message-ID: <upload-\d+\.[0-9a-f-]+@example\.org>$/.test(h))).toBe(true);
    expect(headers.some((h) => /^Date: \w{3}, \d{2} \w{3} \d{4} \d{2}:\d{2}:\d{2} \+0000$/.test(h))).toBe(true);
    expect(mail.raw.split("\r\n").every((l) => l.length <= 998)).toBe(true);
    expect(text).toContain(`Support ID:  ${support}`);
    expect(text).toContain("Kind:        bug");
    expect(text).toContain("OS:          android 15");
    expect(text).toContain("Pairing fails.");
    expect(text).toContain("Café ☕ ok.");
    expect(text).toContain(`GET /v1/admin/uploads/${id}`);
    for (const leak of NOTE_LEAKS) expect(text).not.toContain(leak);
    expect(text).not.toContain("ATV Optimizer bug report");
    expect(text).not.toContain("Pixel 10 Pro");
  });

  it("sends a logs email with its own subject, and truncates a long note", async () => {
    const { binding, sent } = fakeMail();
    const res = await postWith(mailEnv(binding), bug({ kind: "logs", app: "desktop", app_version: "2.6.0", os: "linux", note: "word ".repeat(800) }));
    expect(res.status).toBe(201);
    expect(sent).toHaveLength(1);
    const { headers, body } = decode(sent[0]!.raw);
    expect(headers.find((h) => h.startsWith("Subject:"))).toMatch(/^Subject: \[ATV Optimizer\] Logs #\d+ from desktop 2\.6\.0/);
    expect(body).toContain("more characters in the full upload]");
    expect(body.length).toBeLessThan(3000);
  });

  it("sends nothing without the binding or the addresses", async () => {
    const before = await mailCount();
    expect((await postWith(mailEnv(undefined), bug())).status).toBe(201);
    const { binding, sent } = fakeMail();
    expect((await postWith(mailEnv(binding, { REPORT_MAIL_TO: "" }), bug())).status).toBe(201);
    expect((await postWith({ ...env, REPORT_MAIL: binding }, bug())).status).toBe(201);
    expect(sent).toHaveLength(0);
    expect(await mailCount()).toBe(before);
  });

  it("stops at the daily cap and still stores the uploads", async () => {
    const { binding, sent } = fakeMail();
    const cap = String((await mailCount()) + 1);
    const ids: number[] = [];
    for (let i = 0; i < 3; i++) {
      const res = await postWith(mailEnv(binding, { REPORT_MAIL_DAILY: cap }), bug());
      expect(res.status).toBe(201);
      ids.push(((await res.json()) as { id: number }).id);
    }
    expect(sent).toHaveLength(1);
    expect(await mailCount()).toBe(Number(cap));
    for (const id of ids) {
      expect(await env.DB.prepare("SELECT kind FROM diag_uploads WHERE id = ?1").bind(id).first()).toEqual({ kind: "bug" });
    }
  });

  it("neutralises header injection", async () => {
    const { binding, sent } = fakeMail();
    // A var with a line break in it is not an address, so nothing is sent.
    const injected = mailEnv(binding, { REPORT_MAIL_TO: "owner@example.net\r\nBcc: evil@example.com" });
    expect((await postWith(injected, bug())).status).toBe(201);
    expect(sent).toHaveLength(0);

    expect(headerValue("a\r\nBcc: evil@example.com")).toBe("a Bcc: evil@example.com");
    expect(headerValue("Café")).toBe("=?UTF-8?B?Q2Fmw6k=?=");

    const m: MailUpload = {
      id: 7,
      received_at: "2026-11-22T12:00:00.000Z",
      upload: {
        install_id: "7d0b0b9e-1f43-4e43-9b47-3a5d6f0f9c11",
        kind: "bug",
        app: "mobile",
        app_version: "0.2.0\r\nBcc: evil@example.com",
        os: "android\r\nX-Evil: 1",
        client_ts: "2026-11-22T12:00:00.000Z",
        reports_on: null,
        body: "",
        note: "line\r\nBcc: evil@example.com\r\n\r\nInjected body",
      },
    };
    const raw = buildReportEmail(m, "reports@example.org", "owner@example.net", new Date("2026-11-22T12:00:00Z"));
    const { headers, body } = decode(raw);
    expect(headers).toHaveLength(8);
    expect(headers.filter((h) => /^(Bcc|X-Evil):/i.test(h))).toEqual([]);
    expect(headers.find((h) => h.startsWith("Subject:"))).toBe(
      "Subject: [ATV Optimizer] Bug report #7 from mobile 0.2.0 Bcc: evil@example.com (7D0B-0B9E)",
    );
    // The note sits inside the base64 body, where a line break is just text.
    expect(body).toContain("Injected body");
    expect(raw.slice(raw.indexOf("\r\n\r\n") + 4)).toMatch(/^[A-Za-z0-9+/=\r\n]+$/);
  });

  it("still answers 201 when sending throws", async () => {
    const { binding } = fakeMail({ fail: true });
    const res = await postWith(mailEnv(binding), bug());
    expect(res.status).toBe(201);
    expect(((await res.json()) as { ok: boolean }).ok).toBe(true);
  });
});
