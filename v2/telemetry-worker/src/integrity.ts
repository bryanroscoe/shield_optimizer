// Play Integrity verdicts, recorded and never acted on.
//
// The Android app (Play builds only) attaches a Standard Play Integrity token
// to its once-a-day automatic report batch and to a bug report it sends, in
// the `x-play-integrity` header. The token's requestHash is the lowercase hex
// SHA-256 of the exact request body, so a token can't be lifted onto another
// request. This module asks Google to decode it, checks the package, the hash
// and the token's age, and stores a coarse summary: Google's enum labels and
// the SDK level, never the token, the certificate digest or anything else that
// could identify a phone. Rows carry no install id; a bug report's row points
// at its upload.
//
// Nothing here can fail or change the request it rides on: no secret, a
// malformed token, a Google outage or a refused decode is stored as
// `unevaluated` (or not at all) and the upload or batch is answered exactly as
// without a token. No verdict ever gates anything, here or in the app.

import type { Env } from "./env";
import { intVar } from "./env";
import { admit, utcDay, type QuotaRow } from "./ingest";

export const INTEGRITY_HEADER = "x-play-integrity";
export const PACKAGE_NAME = "com.atvoptimizer.mobile";
const DECODE_URL = `https://playintegrity.googleapis.com/v1/${PACKAGE_NAME}:decodeIntegrityToken`;
const OAUTH_URL = "https://oauth2.googleapis.com/token";
const SCOPE = "https://www.googleapis.com/auth/playintegrity";
const GOOGLE_TIMEOUT_MS = 5000;
// Standard tokens are a few KB of base64url; anything far bigger is not one.
const TOKEN_MAX_CHARS = 16 * 1024;
const TOKEN_SHAPE = /^[A-Za-z0-9_\-.=]+$/;
// The app requests the token right before posting. Google stamps it when it is
// issued, so this only has to cover the upload itself and clock skew.
export const MAX_TOKEN_AGE_MS = 10 * 60 * 1000;
const MAX_FUTURE_SKEW_MS = 2 * 60 * 1000;
export const INTEGRITY_GLOBAL_KEY = "integrity:*";
// Google's default quota is 10,000 decodes a day per app; stay well under it.
export const DEFAULT_GLOBAL_DAILY_INTEGRITY = 1000;
export const DEFAULT_PER_INSTALL_DAILY_INTEGRITY = 3;

export type IntegritySource = "events" | "bug";
export type IntegrityStatus = "ok" | "mismatch" | "unevaluated";

export interface IntegritySummary {
  status: IntegrityStatus;
  // Why it is not `ok`: package, request_hash, stale, malformed,
  // no_credentials or google_error.
  reason: string | null;
  app_recognition: string | null;
  app_version_code: number | null;
  // Sorted, comma-joined MEETS_* labels; "" when Google returned none.
  device_labels: string | null;
  device_activity: string | null;
  device_sdk: number | null;
  licensing: string | null;
  play_protect: string | null;
  // Sorted, comma-joined appsDetected categories; "" when none were found.
  app_access_risk: string | null;
}

const APP_RECOGNITION = ["PLAY_RECOGNIZED", "UNRECOGNIZED_VERSION", "UNEVALUATED"];
const DEVICE_LABELS = ["MEETS_BASIC_INTEGRITY", "MEETS_DEVICE_INTEGRITY", "MEETS_STRONG_INTEGRITY", "MEETS_VIRTUAL_INTEGRITY"];
const DEVICE_ACTIVITY = ["LEVEL_1", "LEVEL_2", "LEVEL_3", "LEVEL_4", "UNEVALUATED"];
const LICENSING = ["LICENSED", "UNLICENSED", "UNEVALUATED"];
const PLAY_PROTECT = ["NO_ISSUES", "NO_DATA", "POSSIBLE_RISK", "MEDIUM_RISK", "HIGH_RISK", "UNEVALUATED"];
const APP_ACCESS_RISK = [
  "KNOWN_INSTALLED", "KNOWN_CAPTURING", "KNOWN_OVERLAYS", "KNOWN_CONTROLLING",
  "UNKNOWN_INSTALLED", "UNKNOWN_CAPTURING", "UNKNOWN_OVERLAYS", "UNKNOWN_CONTROLLING",
];

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

function field(obj: unknown, ...path: string[]): unknown {
  let cur = obj;
  for (const key of path) {
    if (!isRecord(cur)) return undefined;
    cur = cur[key];
  }
  return cur;
}

// One of Google's labels, or OTHER for one this Worker doesn't know yet. A
// missing field is null: unevaluated, it claims nothing.
function label(v: unknown, known: string[]): string | null {
  if (typeof v !== "string") return null;
  return known.includes(v) ? v : "OTHER";
}

function labels(v: unknown, known: string[]): string | null {
  if (!Array.isArray(v)) return null;
  const out = new Set<string>();
  for (const item of v.slice(0, 16)) {
    const l = label(item, known);
    if (l) out.add(l);
  }
  return [...out].sort().join(",");
}

function smallInt(v: unknown, max: number): number | null {
  const n = typeof v === "string" && /^\d{1,16}$/.test(v) ? Number(v) : v;
  return typeof n === "number" && Number.isInteger(n) && n >= 0 && n <= max ? n : null;
}

function empty(status: IntegrityStatus, reason: string | null): IntegritySummary {
  return {
    status,
    reason,
    app_recognition: null,
    app_version_code: null,
    device_labels: null,
    device_activity: null,
    device_sdk: null,
    licensing: null,
    play_protect: null,
    app_access_risk: null,
  };
}

// The decoded payload, reduced to what is stored. Pure: the request checks
// come first, and a token that fails one says nothing about this request, so
// none of its verdicts are kept.
export function summarizeVerdict(decoded: unknown, requestHash: string, now: number): IntegritySummary {
  const payload = field(decoded, "tokenPayloadExternal");
  if (!isRecord(payload)) return empty("unevaluated", "malformed");
  const request = field(payload, "requestDetails");
  if (field(request, "requestPackageName") !== PACKAGE_NAME) return empty("mismatch", "package");
  if (field(request, "requestHash") !== requestHash) return empty("mismatch", "request_hash");
  const issued = smallInt(field(request, "timestampMillis"), Number.MAX_SAFE_INTEGER);
  if (issued === null || now - issued > MAX_TOKEN_AGE_MS || issued - now > MAX_FUTURE_SKEW_MS) {
    return empty("mismatch", "stale");
  }
  const appPackage = field(payload, "appIntegrity", "packageName");
  if (appPackage !== undefined && appPackage !== PACKAGE_NAME) return empty("mismatch", "package");
  return {
    status: "ok",
    reason: null,
    app_recognition: label(field(payload, "appIntegrity", "appRecognitionVerdict"), APP_RECOGNITION),
    app_version_code: smallInt(field(payload, "appIntegrity", "versionCode"), 2_100_000_000),
    device_labels: labels(field(payload, "deviceIntegrity", "deviceRecognitionVerdict"), DEVICE_LABELS),
    device_activity: label(field(payload, "deviceIntegrity", "recentDeviceActivity", "deviceActivityLevel"), DEVICE_ACTIVITY),
    device_sdk: smallInt(field(payload, "deviceIntegrity", "deviceAttributes", "sdkVersion"), 1000),
    licensing: label(field(payload, "accountDetails", "appLicensingVerdict"), LICENSING),
    play_protect: label(field(payload, "environmentDetails", "playProtectVerdict"), PLAY_PROTECT),
    app_access_risk: labels(field(payload, "environmentDetails", "appAccessRiskVerdict", "appsDetected"), APP_ACCESS_RISK),
  };
}

export async function sha256Hex(bytes: Uint8Array): Promise<string> {
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
  return [...digest].map((b) => b.toString(16).padStart(2, "0")).join("");
}

interface ServiceAccount {
  client_email: string;
  private_key: string;
}

function serviceAccount(env: Env): ServiceAccount | null {
  const raw = env.PLAY_INTEGRITY_SA_JSON;
  if (!raw) return null;
  try {
    const sa: unknown = JSON.parse(raw);
    if (isRecord(sa) && typeof sa.client_email === "string" && typeof sa.private_key === "string") {
      return { client_email: sa.client_email, private_key: sa.private_key };
    }
  } catch {
    // An unparsable secret is the same as none.
  }
  return null;
}

function base64url(bytes: Uint8Array | string): string {
  const data = typeof bytes === "string" ? new TextEncoder().encode(bytes) : bytes;
  let bin = "";
  for (const b of data) bin += String.fromCharCode(b);
  return btoa(bin).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function pemToDer(pem: string): Uint8Array {
  const body = pem.replace(/-----(BEGIN|END) PRIVATE KEY-----/g, "").replace(/\s+/g, "");
  return Uint8Array.from(atob(body), (c) => c.charCodeAt(0));
}

// An access token lasts an hour. Kept per isolate and reused until five
// minutes before it expires, so most decodes cost one Google call, not two.
let cachedToken: { email: string; token: string; expires: number } | null = null;

export function resetIntegrityCache(): void {
  cachedToken = null;
}

async function accessToken(sa: ServiceAccount, now: number): Promise<string | null> {
  if (cachedToken && cachedToken.email === sa.client_email && cachedToken.expires - 300_000 > now) {
    return cachedToken.token;
  }
  const key = await crypto.subtle.importKey(
    "pkcs8",
    pemToDer(sa.private_key),
    { name: "RSASSA-PKCS1-v1_5", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const iat = Math.floor(now / 1000);
  const unsigned = `${base64url(JSON.stringify({ alg: "RS256", typ: "JWT" }))}.${base64url(
    JSON.stringify({ iss: sa.client_email, scope: SCOPE, aud: OAUTH_URL, iat, exp: iat + 3600 }),
  )}`;
  const signature = new Uint8Array(await crypto.subtle.sign("RSASSA-PKCS1-v1_5", key, new TextEncoder().encode(unsigned)));
  const res = await fetch(OAUTH_URL, {
    method: "POST",
    headers: { "content-type": "application/x-www-form-urlencoded" },
    body: new URLSearchParams({
      grant_type: "urn:ietf:params:oauth:grant-type:jwt-bearer",
      assertion: `${unsigned}.${base64url(signature)}`,
    }).toString(),
    signal: AbortSignal.timeout(GOOGLE_TIMEOUT_MS),
  });
  if (!res.ok) return null;
  const body: unknown = await res.json();
  const token = field(body, "access_token");
  const ttl = smallInt(field(body, "expires_in"), 86_400) ?? 3600;
  if (typeof token !== "string" || !token) return null;
  cachedToken = { email: sa.client_email, token, expires: now + ttl * 1000 };
  return token;
}

async function decode(sa: ServiceAccount, token: string, now: number): Promise<unknown | null> {
  const bearer = await accessToken(sa, now);
  if (!bearer) return null;
  const res = await fetch(DECODE_URL, {
    method: "POST",
    headers: { authorization: `Bearer ${bearer}`, "content-type": "application/json" },
    body: JSON.stringify({ integrity_token: token }),
    signal: AbortSignal.timeout(GOOGLE_TIMEOUT_MS),
  });
  if (res.status === 401) cachedToken = null;
  if (!res.ok) return null;
  return res.json();
}

export function integrityToken(request: Request): string | null {
  const token = request.headers.get(INTEGRITY_HEADER)?.trim();
  if (!token || token.length > TOKEN_MAX_CHARS || !TOKEN_SHAPE.test(token)) return null;
  return token;
}

export interface IntegrityContext {
  token: string;
  // Lowercase hex SHA-256 of the request body as received.
  bodyHash: string;
  source: IntegritySource;
  installId: string;
  appVersion: string;
  uploadId: number | null;
  now: Date;
}

// Admits, decodes and stores one verdict. Never throws: the request it came
// with has already been stored and answered the same either way. Over the
// daily caps nothing is decoded or stored, which bounds both Google's quota
// and the rows this table can gain in a day.
export async function recordIntegrity(env: Env, ctx: IntegrityContext): Promise<void> {
  try {
    const rows: QuotaRow[] = [
      {
        key: `integrity:${ctx.installId}`,
        n: 1,
        cap: intVar(env.PER_INSTALL_DAILY_INTEGRITY, DEFAULT_PER_INSTALL_DAILY_INTEGRITY),
      },
      { key: INTEGRITY_GLOBAL_KEY, n: 1, cap: intVar(env.GLOBAL_DAILY_INTEGRITY, DEFAULT_GLOBAL_DAILY_INTEGRITY) },
    ];
    if (!(await admit(env, utcDay(ctx.now), rows))) return;
    const sa = serviceAccount(env);
    let summary: IntegritySummary;
    if (!sa) {
      summary = empty("unevaluated", "no_credentials");
    } else {
      let decoded: unknown | null = null;
      try {
        decoded = await decode(sa, ctx.token, ctx.now.getTime());
      } catch {
        decoded = null;
      }
      summary =
        decoded === null ? empty("unevaluated", "google_error") : summarizeVerdict(decoded, ctx.bodyHash, ctx.now.getTime());
    }
    await env.DB.prepare(
      `INSERT INTO integrity_verdicts (received_at, source, upload_id, app_version, status, reason, app_recognition,
         app_version_code, device_labels, device_activity, device_sdk, licensing, play_protect, app_access_risk)
       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)`,
    )
      .bind(
        ctx.now.toISOString(),
        ctx.source,
        ctx.uploadId,
        ctx.appVersion,
        summary.status,
        summary.reason,
        summary.app_recognition,
        summary.app_version_code,
        summary.device_labels,
        summary.device_activity,
        summary.device_sdk,
        summary.licensing,
        summary.play_protect,
        summary.app_access_risk,
      )
      .run();
  } catch {
    // Record-only: losing a verdict costs nothing else.
  }
}
