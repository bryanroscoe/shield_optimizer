import { env } from "cloudflare:workers";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import worker from "../src/index";
import type { Env } from "../src/env";
import { purgeExpired } from "../src/ingest";
import {
  INTEGRITY_HEADER,
  MAX_TOKEN_AGE_MS,
  PACKAGE_NAME,
  resetIntegrityCache,
  sha256Hex,
  summarizeVerdict,
} from "../src/integrity";
import { ADMIN, event } from "./helpers";

const TOKEN = "eyJhbGciOiJBMjU2S1ciLCJlbmMiOiJBMjU2R0NNIn0.c2VjcmV0LXRva2VuLWJvZHk.aXY.dGFn";
const HASH = "a".repeat(64);
const NOW = Date.parse("2026-11-22T12:00:00Z");

function payload(overrides: Record<string, unknown> = {}, request: Record<string, unknown> = {}) {
  return {
    tokenPayloadExternal: {
      requestDetails: {
        requestPackageName: PACKAGE_NAME,
        requestHash: HASH,
        timestampMillis: String(NOW - 2000),
        ...request,
      },
      appIntegrity: {
        appRecognitionVerdict: "PLAY_RECOGNIZED",
        packageName: PACKAGE_NAME,
        certificateSha256Digest: ["c2VjcmV0LWNlcnQ"],
        versionCode: "1005",
      },
      deviceIntegrity: {
        deviceRecognitionVerdict: ["MEETS_STRONG_INTEGRITY", "MEETS_DEVICE_INTEGRITY", "MEETS_BASIC_INTEGRITY"],
        recentDeviceActivity: { deviceActivityLevel: "LEVEL_1" },
        deviceAttributes: { sdkVersion: 36 },
      },
      accountDetails: { appLicensingVerdict: "LICENSED" },
      environmentDetails: {
        playProtectVerdict: "NO_ISSUES",
        appAccessRiskVerdict: { appsDetected: ["KNOWN_INSTALLED", "UNKNOWN_INSTALLED", "KNOWN_INSTALLED"] },
      },
      ...overrides,
    },
  };
}

describe("summarizeVerdict", () => {
  it("keeps Google's labels, several device labels included, and nothing identifying", () => {
    const s = summarizeVerdict(payload(), HASH, NOW);
    expect(s).toEqual({
      status: "ok",
      reason: null,
      app_recognition: "PLAY_RECOGNIZED",
      app_version_code: 1005,
      device_labels: "MEETS_BASIC_INTEGRITY,MEETS_DEVICE_INTEGRITY,MEETS_STRONG_INTEGRITY",
      device_activity: "LEVEL_1",
      device_sdk: 36,
      licensing: "LICENSED",
      play_protect: "NO_ISSUES",
      app_access_risk: "KNOWN_INSTALLED,UNKNOWN_INSTALLED",
    });
    expect(JSON.stringify(s)).not.toContain("c2VjcmV0");
  });

  it("treats every optional verdict as unevaluated when it is missing", () => {
    const s = summarizeVerdict(
      payload({ appIntegrity: undefined, deviceIntegrity: {}, accountDetails: undefined, environmentDetails: undefined }),
      HASH,
      NOW,
    );
    expect(s).toMatchObject({
      status: "ok",
      app_recognition: null,
      app_version_code: null,
      device_labels: null,
      device_activity: null,
      device_sdk: null,
      licensing: null,
      play_protect: null,
      app_access_risk: null,
    });
  });

  it("stores an empty label list as none and an unknown label as OTHER", () => {
    const s = summarizeVerdict(
      payload({
        deviceIntegrity: { deviceRecognitionVerdict: [] },
        environmentDetails: { playProtectVerdict: "SOMETHING_NEW", appAccessRiskVerdict: { appsDetected: [] } },
      }),
      HASH,
      NOW,
    );
    expect(s).toMatchObject({ device_labels: "", play_protect: "OTHER", app_access_risk: "" });
  });

  it("keeps no verdict from a token minted for another request, package or time", () => {
    expect(summarizeVerdict(payload({}, { requestHash: "b".repeat(64) }), HASH, NOW)).toMatchObject({
      status: "mismatch",
      reason: "request_hash",
      device_labels: null,
    });
    expect(summarizeVerdict(payload({}, { requestPackageName: "com.example.other" }), HASH, NOW)).toMatchObject({
      status: "mismatch",
      reason: "package",
    });
    expect(
      summarizeVerdict(payload({ appIntegrity: { packageName: "com.example.other", appRecognitionVerdict: "PLAY_RECOGNIZED" } }), HASH, NOW),
    ).toMatchObject({ status: "mismatch", reason: "package", app_recognition: null });
    const old = String(NOW - MAX_TOKEN_AGE_MS - 1);
    expect(summarizeVerdict(payload({}, { timestampMillis: old }), HASH, NOW)).toMatchObject({ status: "mismatch", reason: "stale" });
    const future = String(NOW + 10 * 60_000);
    expect(summarizeVerdict(payload({}, { timestampMillis: future }), HASH, NOW)).toMatchObject({ reason: "stale" });
    expect(summarizeVerdict(payload({}, { timestampMillis: undefined }), HASH, NOW)).toMatchObject({ reason: "stale" });
    expect(summarizeVerdict({ nope: true }, HASH, NOW)).toMatchObject({ status: "unevaluated", reason: "malformed" });
  });
});

let saJson = "";
let publicKey: CryptoKey;

beforeAll(async () => {
  const pair = (await crypto.subtle.generateKey(
    { name: "RSASSA-PKCS1-v1_5", modulusLength: 2048, publicExponent: new Uint8Array([1, 0, 1]), hash: "SHA-256" },
    true,
    ["sign", "verify"],
  )) as CryptoKeyPair;
  publicKey = pair.publicKey;
  const der = new Uint8Array((await crypto.subtle.exportKey("pkcs8", pair.privateKey)) as ArrayBuffer);
  let bin = "";
  for (const b of der) bin += String.fromCharCode(b);
  const pem = `-----BEGIN PRIVATE KEY-----\n${btoa(bin).replace(/(.{64})/g, "$1\n")}\n-----END PRIVATE KEY-----\n`;
  saJson = JSON.stringify({ type: "service_account", client_email: "verdicts@atv-optimizer-play.iam.gserviceaccount.com", private_key: pem });
});

function fromBase64url(s: string): Uint8Array {
  const b64 = s.replace(/-/g, "+").replace(/_/g, "/").padEnd(Math.ceil(s.length / 4) * 4, "=");
  return Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
}

interface GoogleCalls {
  oauth: number;
  decode: string[];
}

// Google, mocked: checks the service-account JWT's signature and claims, then
// answers the decode with `verdict` built for the request it sees.
function mockGoogle(verdict: (hash: string) => unknown, hashOf: () => string, opts: { decodeStatus?: number } = {}): GoogleCalls {
  const calls: GoogleCalls = { oauth: 0, decode: [] };
  vi.spyOn(globalThis, "fetch").mockImplementation(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input instanceof Request ? input.url : input);
    if (url === "https://oauth2.googleapis.com/token") {
      calls.oauth += 1;
      const form = new URLSearchParams(String(init?.body));
      expect(form.get("grant_type")).toBe("urn:ietf:params:oauth:grant-type:jwt-bearer");
      const [h, c, sig] = form.get("assertion")!.split(".");
      const ok = await crypto.subtle.verify("RSASSA-PKCS1-v1_5", publicKey, fromBase64url(sig!), new TextEncoder().encode(`${h}.${c}`));
      expect(ok).toBe(true);
      const claims = JSON.parse(new TextDecoder().decode(fromBase64url(c!)));
      expect(claims).toMatchObject({
        iss: "verdicts@atv-optimizer-play.iam.gserviceaccount.com",
        scope: "https://www.googleapis.com/auth/playintegrity",
        aud: "https://oauth2.googleapis.com/token",
      });
      return Response.json({ access_token: "ya29.test", expires_in: 3599, token_type: "Bearer" });
    }
    if (url === `https://playintegrity.googleapis.com/v1/${PACKAGE_NAME}:decodeIntegrityToken`) {
      expect(new Headers(init?.headers).get("authorization")).toBe("Bearer ya29.test");
      calls.decode.push(JSON.parse(String(init?.body)).integrity_token);
      if (opts.decodeStatus) return new Response("{}", { status: opts.decodeStatus });
      return Response.json(verdict(hashOf()));
    }
    throw new Error(`unexpected fetch ${url}`);
  });
  return calls;
}

function livePayload(hash: string) {
  return payload({}, { requestHash: hash, timestampMillis: String(Date.now() - 1000) });
}

function bugBody(overrides: Record<string, unknown> = {}): string {
  return JSON.stringify({
    schema: 2,
    kind: "bug",
    install_id: crypto.randomUUID(),
    app: "mobile",
    app_version: "0.1.6",
    os: "android 17",
    ts: new Date().toISOString(),
    reports_enabled: true,
    note: "Scan finds nothing.",
    text: "## ATV Optimizer bug report (Android)\n- Play Integrity check: attached\n",
    ...overrides,
  });
}

function postWith(path: string, body: string, e: Partial<Env>, token: string | null = TOKEN): Promise<Response> {
  const headers: Record<string, string> = { "content-type": "application/json" };
  if (token !== null) headers[INTEGRITY_HEADER] = token;
  return worker.fetch(new Request(`https://telemetry.test${path}`, { method: "POST", headers, body }), {
    ...env,
    ...e,
  } as Env);
}

async function verdictFor(uploadId: number): Promise<Record<string, unknown> | null> {
  return env.DB.prepare("SELECT * FROM integrity_verdicts WHERE upload_id = ?1").bind(uploadId).first();
}

beforeEach(() => {
  resetIntegrityCache();
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("Play Integrity on a bug report", () => {
  it("decodes the token, checks it against the body and stores only the summary", async () => {
    const body = bugBody();
    const hash = await sha256Hex(new TextEncoder().encode(body));
    const calls = mockGoogle(livePayload, () => hash);
    const res = await postWith("/v1/diagnostics", body, { PLAY_INTEGRITY_SA_JSON: saJson });
    expect(res.status).toBe(201);
    const { id } = (await res.json()) as { id: number };
    expect(calls).toEqual({ oauth: 1, decode: [TOKEN] });
    const row = await verdictFor(id);
    expect(row).toMatchObject({
      source: "bug",
      app_version: "0.1.6",
      status: "ok",
      reason: null,
      app_recognition: "PLAY_RECOGNIZED",
      app_version_code: 1005,
      device_labels: "MEETS_BASIC_INTEGRITY,MEETS_DEVICE_INTEGRITY,MEETS_STRONG_INTEGRITY",
      device_activity: "LEVEL_1",
      device_sdk: 36,
      licensing: "LICENSED",
      play_protect: "NO_ISSUES",
      app_access_risk: "KNOWN_INSTALLED,UNKNOWN_INSTALLED",
    });
    // Never the token, the certificate digest or an install id.
    const all = JSON.stringify(await env.DB.prepare("SELECT * FROM integrity_verdicts").all());
    for (const leak of ["c2VjcmV0", "eyJhbGci", JSON.parse(body).install_id]) expect(all).not.toContain(leak);
    expect(Object.keys(row!)).not.toContain("install_id");

    // The support lookup shows it beside the upload.
    const lookup = await worker.fetch(
      new Request(`https://telemetry.test/v1/admin/support/${JSON.parse(body).install_id}`, {
        headers: { authorization: `Bearer ${ADMIN}` },
      }),
      env,
    );
    const report = (await lookup.json()) as { installs: Array<{ uploads: Array<{ id: number; integrity: unknown }> }> };
    expect(report.installs[0]!.uploads[0]).toMatchObject({
      id,
      integrity: { status: "ok", licensing: "LICENSED", device_labels: expect.stringContaining("MEETS_STRONG_INTEGRITY") },
    });
  });

  it("reuses the access token for the next decode", async () => {
    let hash = "";
    const calls = mockGoogle(livePayload, () => hash);
    for (let i = 0; i < 2; i++) {
      const body = bugBody();
      hash = await sha256Hex(new TextEncoder().encode(body));
      expect((await postWith("/v1/diagnostics", body, { PLAY_INTEGRITY_SA_JSON: saJson })).status).toBe(201);
    }
    expect(calls.oauth).toBe(1);
    expect(calls.decode).toHaveLength(2);
  });

  it("records a token for another body as a mismatch, and the upload is unaffected", async () => {
    const body = bugBody();
    mockGoogle(livePayload, () => "0".repeat(64));
    const res = await postWith("/v1/diagnostics", body, { PLAY_INTEGRITY_SA_JSON: saJson });
    expect(res.status).toBe(201);
    const { id } = (await res.json()) as { id: number };
    expect(await verdictFor(id)).toMatchObject({ status: "mismatch", reason: "request_hash", device_labels: null, licensing: null });
  });

  it("without the secret stores unevaluated and calls nobody", async () => {
    const spy = vi.spyOn(globalThis, "fetch");
    const res = await postWith("/v1/diagnostics", bugBody(), { PLAY_INTEGRITY_SA_JSON: undefined });
    expect(res.status).toBe(201);
    const { id } = (await res.json()) as { id: number };
    expect(spy).not.toHaveBeenCalled();
    expect(await verdictFor(id)).toMatchObject({ status: "unevaluated", reason: "no_credentials", app_recognition: null });
  });

  it("an unusable secret is the same as none", async () => {
    const res = await postWith("/v1/diagnostics", bugBody(), { PLAY_INTEGRITY_SA_JSON: "{not json" });
    const { id } = (await res.json()) as { id: number };
    expect(await verdictFor(id)).toMatchObject({ status: "unevaluated", reason: "no_credentials" });
  });

  it("a Google error or outage stores unevaluated and still answers 201", async () => {
    const body = bugBody();
    mockGoogle(livePayload, () => "", { decodeStatus: 503 });
    const refused = await postWith("/v1/diagnostics", body, { PLAY_INTEGRITY_SA_JSON: saJson });
    expect(refused.status).toBe(201);
    expect(await verdictFor(((await refused.json()) as { id: number }).id)).toMatchObject({
      status: "unevaluated",
      reason: "google_error",
    });

    vi.restoreAllMocks();
    resetIntegrityCache();
    vi.spyOn(globalThis, "fetch").mockRejectedValue(new TypeError("network down"));
    const down = await postWith("/v1/diagnostics", bugBody(), { PLAY_INTEGRITY_SA_JSON: saJson });
    expect(down.status).toBe(201);
    expect(await verdictFor(((await down.json()) as { id: number }).id)).toMatchObject({ reason: "google_error" });
  });

  it("ignores a malformed header, a desktop upload's header and a missing header", async () => {
    const spy = vi.spyOn(globalThis, "fetch");
    for (const [body, token] of [
      [bugBody(), "not a token!"],
      [bugBody({ app: "desktop", kind: "logs", text: "log line", note: undefined }), TOKEN],
      [bugBody(), null],
    ] as const) {
      const res = await postWith("/v1/diagnostics", body, { PLAY_INTEGRITY_SA_JSON: saJson }, token);
      expect(res.status).toBe(201);
      expect(await verdictFor(((await res.json()) as { id: number }).id)).toBeNull();
    }
    expect(spy).not.toHaveBeenCalled();
  });

  it("decodes at most PER_INSTALL_DAILY_INTEGRITY tokens per install a day", async () => {
    const installId = crypto.randomUUID();
    const ids: number[] = [];
    for (let i = 0; i < 3; i++) {
      const res = await postWith("/v1/diagnostics", bugBody({ install_id: installId }), {
        PLAY_INTEGRITY_SA_JSON: undefined,
        PER_INSTALL_DAILY_INTEGRITY: "2",
      });
      expect(res.status).toBe(201);
      ids.push(((await res.json()) as { id: number }).id);
    }
    expect(await verdictFor(ids[0]!)).not.toBeNull();
    expect(await verdictFor(ids[1]!)).not.toBeNull();
    expect(await verdictFor(ids[2]!)).toBeNull();
  });
});

describe("Play Integrity on the automatic report batch", () => {
  it("records a mobile batch's token without an upload id", async () => {
    const installId = crypto.randomUUID();
    const body = JSON.stringify({
      events: [event({ install_id: installId, app: "mobile", app_version: "0.1.7", os: "android 17" })],
    });
    const hash = await sha256Hex(new TextEncoder().encode(body));
    mockGoogle(livePayload, () => hash);
    const res = await postWith("/v1/events", body, { PLAY_INTEGRITY_SA_JSON: saJson });
    expect(res.status).toBe(204);
    const row = await env.DB.prepare("SELECT * FROM integrity_verdicts WHERE app_version = '0.1.7'").first();
    expect(row).toMatchObject({ source: "events", upload_id: null, status: "ok", licensing: "LICENSED" });
  });

  it("ignores a token on a desktop batch", async () => {
    const spy = vi.spyOn(globalThis, "fetch");
    const body = JSON.stringify({ events: [event({ app_version: "2.9.1" })] });
    expect((await postWith("/v1/events", body, { PLAY_INTEGRITY_SA_JSON: saJson })).status).toBe(204);
    expect(spy).not.toHaveBeenCalled();
    expect(await env.DB.prepare("SELECT * FROM integrity_verdicts WHERE app_version = '2.9.1'").first()).toBeNull();
  });

  it("a batch refused by validation never reaches Google", async () => {
    const spy = vi.spyOn(globalThis, "fetch");
    const res = await postWith("/v1/events", JSON.stringify({ events: [{ kind: "nope" }] }), { PLAY_INTEGRITY_SA_JSON: saJson });
    expect(res.status).toBe(400);
    expect(spy).not.toHaveBeenCalled();
  });
});

describe("retention", () => {
  it("purges verdicts with the raw events", async () => {
    await env.DB.prepare(
      `INSERT INTO integrity_verdicts (received_at, source, app_version, status) VALUES
         ('2000-01-01T00:00:00.000Z', 'events', '0.0.1', 'ok'), ('2999-01-01T00:00:00.000Z', 'events', '0.0.2', 'ok')`,
    ).run();
    const result = await purgeExpired({ ...env, RAW_RETENTION_DAYS: "30" } as Env, new Date("2026-11-22T03:17:00Z"));
    expect(result.integrity).toBeGreaterThanOrEqual(1);
    const left = await env.DB.prepare("SELECT app_version FROM integrity_verdicts WHERE app_version IN ('0.0.1', '0.0.2')").all();
    expect(left.results).toEqual([{ app_version: "0.0.2" }]);
  });
});
