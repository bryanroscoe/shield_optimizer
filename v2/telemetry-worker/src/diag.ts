// The `diag` object on schema 2 events: what the app saw when a scan found
// nothing or a TV gave no details. The client builds it from counts, fixed
// enums, version numbers and error codes only (engine/telemetry_diag.rs).
//
// The Worker trusts none of that. The object is rebuilt key by key from the
// spec below: a key the spec does not name is dropped, a value of the wrong
// type or outside its list is dropped, counts are clamped, and the only
// strings that survive are enum members, `X.Y.Z` versions and short
// snake_case codes. There is no field a serial, address, name or path fits in.

import { REDACTED_CODE, plausibleErrorCode } from "./codes";

type Leaf =
  | { t: "count" }
  | { t: "ms" }
  | { t: "bool" }
  | { t: "enum"; values: readonly string[] }
  | { t: "version" }
  | { t: "code" }
  | { t: "codes" }
  | { t: "prefixes" }
  | { t: "props" };

type Spec = { [key: string]: Leaf | Spec };

const count: Leaf = { t: "count" };
const ms: Leaf = { t: "ms" };
const code: Leaf = { t: "code" };
const codes: Leaf = { t: "codes" };

export const PROP_KEYS = [
  "ro.product.manufacturer",
  "ro.product.brand",
  "ro.product.model",
  "ro.product.device",
  "ro.build.version.release",
  "ro.build.version.sdk",
  "ro.build.characteristics",
  "ro.serialno",
  "android.software.leanback",
] as const;

export const DIAG_SPEC: Spec = {
  adb: {
    source: { t: "enum", values: ["bundled", "sdk", "system", "custom", "builtin", "none", "unknown"] },
    version: { t: "version" },
    tools: { t: "version" },
    server: { t: "enum", values: ["reachable", "unreachable", "unknown"] },
    server_error: code,
  },
  transports: { device: count, unauthorized: count, offline: count, other: count },
  mdns: {
    check: { t: "enum", values: ["ok", "unavailable", "failed", "not_run"] },
    services: count,
    connect: count,
    pairing: count,
    legacy: count,
  },
  net: {
    ipv4: count,
    ipv6: count,
    link_local: count,
    vpn: count,
    prefixes: { t: "prefixes" },
    gateway: { t: "enum", values: ["detected", "not_detected", "override"] },
    subnet_local: { t: "bool" },
  },
  sweep: { probed: count, open: count, timeouts: count, refused: count, ms },
  connect: {
    attempted: count,
    connected: count,
    unauthorized: count,
    failed: count,
    pairing: count,
    errors: codes,
  },
  device: {
    state: { t: "enum", values: ["device", "unauthorized", "offline", "other"] },
    transport: { t: "enum", values: ["usb", "network"] },
    empty_props: { t: "props" },
    error: code,
  },
  discovery: { timeout_ms: ms, found: count },
  errors: codes,
  ms,
};

const MAX_COUNT = 9999;
const MAX_MS = 3_600_000;
const MAX_CODES = 8;
const MAX_PREFIXES = 8;
export const DIAG_MAX_BYTES = 4096;
const VERSION = /^\d{1,3}(?:\.\d{1,4}){1,3}$/;
const CODE = /^[a-z][a-z0-9_]{0,47}$/;

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

function isLeaf(s: Leaf | Spec): s is Leaf {
  return typeof (s as Leaf).t === "string";
}

function cleanCode(v: unknown): string | undefined {
  if (typeof v !== "string" || !CODE.test(v)) return undefined;
  return plausibleErrorCode(v) ? v : REDACTED_CODE;
}

function cleanLeaf(leaf: Leaf, v: unknown): unknown {
  switch (leaf.t) {
    case "count":
      return typeof v === "number" && Number.isInteger(v) && v >= 0 ? Math.min(v, MAX_COUNT) : undefined;
    case "ms":
      return typeof v === "number" && Number.isInteger(v) && v >= 0 ? Math.min(v, MAX_MS) : undefined;
    case "bool":
      return typeof v === "boolean" ? v : undefined;
    case "enum":
      return typeof v === "string" && leaf.values.includes(v) ? v : undefined;
    case "version":
      return typeof v === "string" && VERSION.test(v) ? v : undefined;
    case "code":
      return cleanCode(v);
    case "codes": {
      if (!Array.isArray(v)) return undefined;
      const out = [...new Set(v.slice(0, 32).map(cleanCode).filter((c): c is string => c !== undefined))];
      return out.length ? out.slice(0, MAX_CODES) : undefined;
    }
    case "prefixes": {
      if (!Array.isArray(v)) return undefined;
      const out = [
        ...new Set(v.filter((n): n is number => typeof n === "number" && Number.isInteger(n) && n >= 0 && n <= 128)),
      ].sort((a, b) => a - b);
      return out.slice(0, MAX_PREFIXES);
    }
    case "props": {
      if (!Array.isArray(v)) return undefined;
      const out = [...new Set(v.filter((k): k is string => (PROP_KEYS as readonly string[]).includes(k as string)))];
      return out.length ? out : undefined;
    }
  }
}

function cleanObject(spec: Spec, raw: Record<string, unknown>): Record<string, unknown> | undefined {
  const out: Record<string, unknown> = {};
  for (const [key, sub] of Object.entries(spec)) {
    if (!(key in raw)) continue;
    const v = raw[key];
    const cleaned = isLeaf(sub) ? cleanLeaf(sub, v) : isRecord(v) ? cleanObject(sub, v) : undefined;
    if (cleaned !== undefined) out[key] = cleaned;
  }
  return Object.keys(out).length ? out : undefined;
}

// The cleaned object as JSON text, or null when nothing in it was valid.
export function cleanDiag(raw: unknown): string | null {
  if (!isRecord(raw)) return null;
  const cleaned = cleanObject(DIAG_SPEC, raw);
  if (!cleaned) return null;
  const text = JSON.stringify(cleaned);
  // Cannot happen with the caps above; kept so a future spec change cannot
  // make a row unbounded.
  return text.length <= DIAG_MAX_BYTES ? text : null;
}
