import { describe, expect, it } from "vitest";
import { validateBatch, validateEvent } from "../src/validate";
import { event } from "./helpers";

function reason(raw: unknown): string | null {
  const r = validateEvent(raw);
  return r.ok ? null : r.reason;
}

describe("validateEvent", () => {
  it("accepts the contract example", () => {
    const r = validateEvent(event());
    expect(r.ok).toBe(true);
  });

  it("rejects unknown kind, app, tier and schema", () => {
    expect(reason(event({ kind: "crash" }))).toBe("unknown_kind");
    expect(reason(event({ app: "web" }))).toBe("unknown_app");
    expect(reason(event({ schema: 2 }))).toBe("unsupported_schema");
    expect(reason(event({ device: { manufacturer: "NVIDIA", support_tier: "great" } }))).toBe("unknown_support_tier");
  });

  it("requires a v4 uuid install id, a version, an RFC 3339 timestamp and an os", () => {
    expect(reason(event({ install_id: "0323220012345" }))).toBe("bad_install_id");
    expect(reason(event({ install_id: "5f0c2b1e-8d3a-1c6b-9e2f-000000000001" }))).toBe("bad_install_id");
    expect(reason(event({ app_version: "latest" }))).toBe("bad_app_version");
    expect(reason(event({ ts: "yesterday" }))).toBe("bad_ts");
    expect(reason(event({ ts: "2026-10-31T12:00:00" }))).toBe("bad_ts");
    expect(reason(event({ os: "" }))).toBe("bad_os");
    expect(reason(event({ os: "10.0.0.5" }))).toBe("bad_os");
  });

  it("enforces per-kind required fields", () => {
    expect(reason(event({ device: undefined }))).toBe("device_seen_without_device");
    expect(reason(event({ kind: "action_result", ok: true }))).toBe("action_result_without_action");
    expect(reason(event({ kind: "action_result", action: "disable" }))).toBe("action_result_without_ok");
    expect(reason(event({ kind: "error" }))).toBe("error_without_error_code");
    expect(reason(event({ kind: "error", error_code: "Has Spaces" }))).toBe("bad_error_code");
    expect(reason(event({ kind: "action_result", action: "disable", ok: "yes" }))).toBe("bad_ok");
  });

  it("allows errors without a device", () => {
    const r = validateEvent(event({ kind: "error", device: undefined, error_code: "adb_missing" }));
    expect(r.ok && r.value.device).toBe(null);
  });

  it("maps unknown profile and action to unknown/other instead of dropping", () => {
    const r = validateEvent(
      event({ kind: "action_result", action: "teleport", ok: true, device: { manufacturer: "X", profile: "tizen" } }),
    );
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.value.action).toBe("other");
      expect(r.value.device?.profile).toBe("unknown");
    }
  });

  it("drops unknown fields and never carries forbidden data", () => {
    const r = validateEvent(
      event({
        serial: "0323220012345",
        ip: "192.168.1.5",
        tv_name: "Living Room",
        packages: ["com.a.b", "com.c.d"],
        license: { licensee: "someone" },
        device: { ...(event().device as object), serial: "0323220012345", name: "Bedroom TV" },
      }),
    );
    expect(r.ok).toBe(true);
    const text = JSON.stringify(r.ok ? r.value : null);
    for (const forbidden of ["0323220012345", "192.168.1.5", "Living Room", "Bedroom TV", "com.a.b", "licensee"]) {
      expect(text).not.toContain(forbidden);
    }
  });

  it("clamps and re-redacts strings", () => {
    const r = validateEvent(
      event({
        kind: "error",
        error_code: "connect_failed",
        message: `connect 192.168.1.5:5555 failed ${"y".repeat(900)}`,
        device: { ...(event().device as object), model: "M".repeat(300), device: "0323220012345" },
      }),
    );
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.value.message?.startsWith("connect [ip] failed")).toBe(true);
      expect(r.value.message?.length).toBeLessThanOrEqual(500);
      expect(r.value.device?.model).toHaveLength(64);
      expect(r.value.device?.device).toBe("[redacted]");
    }
  });

  it("normalises the install id and timestamp", () => {
    const r = validateEvent(event({ install_id: "5F0C2B1E-8D3A-4C6B-9E2F-0000000000AA", ts: "2026-10-31T08:00:00-04:00" }));
    expect(r.ok && r.value.install_id).toBe("5f0c2b1e-8d3a-4c6b-9e2f-0000000000aa");
    expect(r.ok && r.value.client_ts).toBe("2026-10-31T12:00:00.000Z");
  });
});

describe("validateBatch", () => {
  it("rejects bad envelopes", () => {
    expect(validateBatch([]).ok).toBe(false);
    expect(validateBatch({ events: [] }).ok).toBe(false);
    expect(validateBatch({ events: Array.from({ length: 51 }, () => event()) }).ok).toBe(false);
  });

  it("keeps valid events and counts rejected ones by reason", () => {
    const r = validateBatch({ events: [event(), event({ kind: "nope" }), event({ app: "web" })] });
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.value.events).toHaveLength(1);
      expect(r.value.rejected).toEqual({ unknown_kind: 1, unknown_app: 1 });
    }
  });
});
