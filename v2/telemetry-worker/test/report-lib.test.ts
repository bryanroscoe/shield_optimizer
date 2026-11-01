import { exports } from "cloudflare:workers";
import { describe, expect, it } from "vitest";
import repoFile from "../../crates/core/data/device-support.json";
import {
  analyze,
  fetchReport,
  issueMarkdown,
  ISSUE_MARKER,
  suggest,
  type ReportBody,
  type ReportRow,
} from "../scripts/report-lib.ts";
import type { Classification, TierRule } from "../scripts/tiers-lib.ts";
import { ADMIN, event, newInstallId, post } from "./helpers";

const rules = repoFile.rules as TierRule[];

function row(over: Partial<ReportRow> = {}): ReportRow {
  return {
    manufacturer: "Acme",
    model: "Box 4K",
    device: "boxy",
    installs: 0,
    action_ok: 0,
    action_fail: 0,
    top_errors: [],
    last_seen: "2026-10-30T12:00:00Z",
    ...over,
  };
}
const at = (tier: Classification["tier"]): Classification =>
  tier === "undetermined" ? { tier, rule: 0 } : { tier, rule: null };

describe("suggest", () => {
  it("unknown -> beta at 3 installs and 80% success", () => {
    expect(suggest(at("unknown"), row({ installs: 3, action_ok: 8, action_fail: 2 }), undefined)).toMatchObject({ kind: "move", to: "beta" });
    expect(suggest(at("unknown"), row({ installs: 2, action_ok: 8, action_fail: 2 }), undefined)).toMatchObject({ kind: "keep" });
    expect(suggest(at("unknown"), row({ installs: 3, action_ok: 7, action_fail: 3 }), undefined).why).toContain("80% success");
    // Too few actions to judge success at all.
    expect(suggest(at("unknown"), row({ installs: 4, action_ok: 4 }), undefined).why).toContain("5 actions");
  });

  it("beta -> supported needs 10 installs and a clean 30 days", () => {
    const all = row({ installs: 10, action_ok: 200, action_fail: 5 });
    const clean = row({ installs: 6, action_ok: 96, action_fail: 4, top_errors: [{ code: "timeout", count: 30 }, { code: "pm_failure", count: 4 }] });
    expect(suggest(at("beta"), all, clean)).toMatchObject({ kind: "move", to: "supported" });

    // Explained codes (network, pairing) never count against the TV, but an
    // unexplained one over 5% does.
    const noisy = row({ ...clean, top_errors: [{ code: "permission_denied", count: 6 }] });
    expect(suggest(at("beta"), all, noisy).why).toContain("`permission_denied` is 6%");
    expect(suggest(at("beta"), all, row({ action_ok: 94, action_fail: 6 })).why).toContain("95% success in 30 days");
    expect(suggest(at("beta"), row({ ...all, installs: 9 }), clean).why).toContain("10 installs (has 9)");
    expect(suggest(at("beta"), all, undefined).why).toContain("20 actions in 30 days (has 0)");
  });

  it("does not assume a code cut from the top-5 list is under the bar", () => {
    const all = row({ installs: 12, action_ok: 100 });
    const recent = row({
      action_ok: 100,
      top_errors: ["timeout", "unreachable", "wrong_pin", "device_offline", "connection_lost"].map((code) => ({ code, count: 6 })),
    });
    expect(suggest(at("beta"), all, recent).why).toContain("cut at 5 codes");
  });

  it("flags not_supported for the owner to confirm, on low success or a systematic code", () => {
    const low = row({ installs: 5, action_ok: 3, action_fail: 7 });
    expect(suggest(at("beta"), low, undefined)).toMatchObject({ kind: "not_supported" });
    expect(suggest(at("unknown"), low, undefined).why).toContain("confirm");
    const systematic = row({ installs: 6, action_ok: 50, action_fail: 50, top_errors: [{ code: "permission_denied", count: 50 }] });
    expect(suggest(at("supported"), systematic, undefined)).toMatchObject({ kind: "not_supported" });
    // Explained codes are never systematic, and four installs are too few.
    expect(suggest(at("unknown"), row({ installs: 6, action_ok: 50, action_fail: 50, top_errors: [{ code: "unreachable", count: 50 }] }), undefined).kind).not.toBe("not_supported");
    expect(suggest(at("unknown"), row({ ...low, installs: 4 }), undefined).kind).not.toBe("not_supported");
  });

  it("never suggests fully_supported, only a candidate for a physical test", () => {
    const all = row({ installs: 25, action_ok: 500 });
    const recent = row({ action_ok: 100 });
    expect(suggest(at("supported"), all, recent)).toMatchObject({ kind: "fully_supported_candidate" });
    expect(suggest(at("supported"), row({ ...all, installs: 24 }), recent)).toMatchObject({ kind: "keep" });
    expect(suggest(at("beta"), all, recent)).toMatchObject({ kind: "move", to: "supported" });
  });

  it("asks for a review when a supported TV starts failing or a not_supported one works", () => {
    expect(suggest(at("supported"), row({ installs: 30, action_ok: 500 }), row({ action_ok: 7, action_fail: 3 }))).toMatchObject({ kind: "review" });
    expect(suggest(at("fully_supported"), row({ installs: 30, action_ok: 500 }), row({ action_ok: 70, action_fail: 30 }))).toMatchObject({ kind: "review" });
    expect(suggest(at("not_supported"), row({ installs: 4, action_ok: 9, action_fail: 1 }), undefined)).toMatchObject({ kind: "review" });
    expect(suggest(at("not_supported"), row({ installs: 9, action_ok: 1, action_fail: 9 }), undefined)).toMatchObject({ kind: "keep" });
  });

  it("never gives a tier to a TV that didn't say what it is", () => {
    const busy = { installs: 50, action_ok: 500 };
    for (const ids of [
      { manufacturer: null, model: "X" },
      { manufacturer: "Acme", model: null },
      { manufacturer: "unknown", model: "unknown" },
      { manufacturer: "[redacted]", model: "X" },
      { manufacturer: " ", model: "X" },
    ]) {
      expect(suggest(at("unknown"), row({ ...busy, ...ids }), undefined)).toMatchObject({ kind: "none" });
    }
  });
});

const body = (rows: ReportRow[], since: string): ReportBody => ({ group: ["manufacturer", "model", "device"], since, truncated: false, rows });

describe("issueMarkdown", () => {
  const opts = { generatedAt: new Date("2026-11-02T14:23:00Z"), rulesUpdated: "2026-10-31", docsUrl: "https://example.test/docs" };

  it("applies the repo rules, lists suggested changes first and escapes device strings", () => {
    const all = body(
      [
        row({ manufacturer: "NVIDIA", model: "SHIELD Android TV", device: "mdarcy", installs: 40, action_ok: 900 }),
        row({ manufacturer: "TCL", model: "QM7L Pro", device: "x", installs: 3, action_ok: 9, action_fail: 1 }),
        row({ manufacturer: "Evil|Co", model: "@owner `x` <b>", device: null, installs: 4, action_ok: 10 }),
        row({ manufacturer: null, model: null, device: null, installs: 9, action_ok: 0, action_fail: 9 }),
      ],
      "2016-11-04",
    );
    const recent = body([row({ manufacturer: "NVIDIA", model: "SHIELD Android TV", device: "mdarcy", installs: 10, action_ok: 100 })], "2026-10-04");
    const rows = analyze(rules, all, recent);
    expect(rows.map((r) => r.current.tier)).toEqual(["fully_supported", "beta", "unknown", "unknown"]);
    const md = issueMarkdown(rows, all, recent, opts);
    expect(md.startsWith(ISSUE_MARKER)).toBe(true);
    expect(md).toContain("**Suggestions only.**");
    expect(md).toContain("## Suggested changes (1)");
    expect(md).toContain("| `Evil\\|Co` | `@owner  x  <b>` | _(not reported)_ | unknown | 4 |");
    expect(md).toContain("**beta**");
    expect(md).toContain("none (unidentified)");
    expect(md).toContain("## All TVs (4)");
    // Every table line has the same column count, pipes escaped.
    for (const line of md.split("\n").filter((l) => l.startsWith("| "))) {
      expect(line.replace(/\\\|/g, "").split("|").length).toBe(13);
    }
  });

  it("stays under GitHub's issue size limit", () => {
    const many = Array.from({ length: 2000 }, (_, i) => row({ model: `Model ${i} ${"x".repeat(40)}`, installs: 1 }));
    const md = issueMarkdown(analyze(rules, body(many, "a"), body([], "b")), body(many, "a"), body([], "b"), opts);
    expect(md.length).toBeLessThan(65_536);
    expect(md).toMatch(/\d+ quieter TVs are not shown/);
  });
});

describe("fetchReport against the Worker", () => {
  it("reads the admin device report the issue is built from", async () => {
    const dev = { manufacturer: "IssueCo", brand: "IssueCo", model: "IC 1", device: "ic1", profile: "googletv", support_tier: "unknown" };
    for (let i = 0; i < 3; i++) {
      const id = newInstallId();
      await post("/v1/events", {
        events: [
          event({ install_id: id, device: dev }),
          event({ install_id: id, device: dev, kind: "action_result", action: "disable", ok: true }),
          event({ install_id: id, device: dev, kind: "action_result", action: "disable", ok: true }),
        ],
      });
    }
    const viaWorker = ((input: RequestInfo | URL, init?: RequestInit) => exports.default.fetch(new Request(input, init))) as typeof fetch;
    const all = await fetchReport("https://telemetry.test", ADMIN, { group: "manufacturer,model,device", days: 3650, limit: 5000 }, viaWorker);
    const recent = await fetchReport("https://telemetry.test", ADMIN, { group: "manufacturer,model,device", days: 30, limit: 5000 }, viaWorker);
    const ic = analyze(rules, all, recent).find((r) => r.all.manufacturer === "IssueCo");
    expect(ic?.all).toMatchObject({ installs: 3, action_ok: 6, action_fail: 0 });
    expect(ic?.current.tier).toBe("unknown");
    expect(ic?.suggestion).toMatchObject({ kind: "move", to: "beta" });
  });
});
