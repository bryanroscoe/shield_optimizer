import { exports } from "cloudflare:workers";

export const ADMIN = "test-admin-token-0123456789";

let counter = 0;
export function newInstallId(): string {
  counter += 1;
  const tail = counter.toString(16).padStart(12, "0");
  return `5f0c2b1e-8d3a-4c6b-9e2f-${tail}`;
}

export function shieldDevice(): Record<string, unknown> {
  return {
    manufacturer: "NVIDIA",
    brand: "NVIDIA",
    model: "SHIELD Android TV",
    device: "mdarcy",
    product: "mdarcy",
    android_release: "11",
    sdk: 30,
    profile: "shield",
    support_tier: "fully_supported",
  };
}

export function event(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    schema: 1,
    install_id: newInstallId(),
    app: "desktop",
    app_version: "2.3.1",
    os: "macos 15.1",
    ts: "2026-10-31T12:00:00Z",
    kind: "device_seen",
    device: shieldDevice(),
    ...overrides,
  };
}

export function post(
  path: string,
  body: unknown,
  opts: { headers?: Record<string, string>; method?: string; cf?: Record<string, unknown> } = {},
): Promise<Response> {
  return exports.default.fetch(`https://telemetry.test${path}`, {
    method: opts.method ?? "POST",
    headers: { "content-type": "application/json", ...(opts.headers ?? {}) },
    body: typeof body === "string" ? body : JSON.stringify(body),
    cf: opts.cf,
  } as RequestInit);
}

export function call(path: string, init: RequestInit = {}): Promise<Response> {
  return exports.default.fetch(`https://telemetry.test${path}`, init);
}
