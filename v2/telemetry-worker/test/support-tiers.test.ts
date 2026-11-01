import { describe, expect, it } from "vitest";
import repoFile from "../../crates/core/data/device-support.json";
import { checkTierFile, classify, rulesEqual, type TierDoc, type TierRule } from "../scripts/tiers-lib.ts";
import type { TierDoc as ServedDoc } from "../src/tiers";
import { ADMIN, call, post } from "./helpers";

const auth = { authorization: `Bearer ${ADMIN}` };
const TODAY = "2026-11-01";

function doc(rules: unknown[], updated = "2026-10-31"): Record<string, unknown> {
  return { schema: 1, updated, rules };
}
const rule = (match: Record<string, unknown>, tier = "beta", note = "n") => ({ match, tier, note });

describe("the repo's device-support.json", () => {
  it("passes the ship check", () => {
    const res = checkTierFile(repoFile, { today: TODAY });
    expect(res.errors).toEqual([]);
    expect(res.doc).not.toBeNull();
  });

  it("is exactly what the Worker serves after a PUT (the sync contract)", async () => {
    // The sync workflow sends the file's rules as-is; the served rules must
    // come back byte for byte, or the post-sync comparison would fail.
    const res = await post("/v1/admin/support-tiers", { rules: repoFile.rules }, { method: "PUT", headers: auth });
    expect(res.status).toBe(200);
    const served = (await (await call("/v1/support-tiers")).json()) as ServedDoc;
    expect(served.rules).toEqual(repoFile.rules);
    expect(rulesEqual(repoFile.rules as TierRule[], served.rules)).toBe(true);
    // The apps compare dates as text and drop a served list older than theirs.
    expect(served.updated >= repoFile.updated).toBe(true);
    // The whole embedded document is also accepted as a PUT body.
    const whole = await post("/v1/admin/support-tiers", repoFile, { method: "PUT", headers: auth });
    expect(whole.status).toBe(200);
  });
});

describe("checkTierFile", () => {
  it("rejects what either side would reject, trim or clamp", () => {
    const cases: Array<[unknown, string]> = [
      [{ ...doc([]), schema: 2 }, "schema must be 1"],
      [{ ...doc([]), extra: 1 }, 'unknown top-level key "extra"'],
      [doc([], "2026-11-1"), "YYYY-MM-DD"],
      [doc([], "2026-02-30"), "YYYY-MM-DD"],
      [doc([], "2026-10-31T00:00:00Z"), "YYYY-MM-DD"],
      [doc([rule({})]), "at least one key"],
      [doc([rule({ manufacturer: "  " })]), "non-blank"],
      [doc([rule({ manufacturer: " TCL" })]), "whitespace"],
      [doc([rule({ manufacturer: "TCL" }, "beta", "note ")]), "note has leading or trailing whitespace"],
      [doc([rule({ manufacturer: "TCL" }, "great")]), "tier must be one of"],
      [doc([rule({ manufacturer: "TCL" }, "beta", "")]), "note is required"],
      [doc([rule({ manufacturer: "TCL" }, "beta", "x".repeat(201))]), "over 200"],
      [doc([rule({ manufacturer: "x".repeat(65) })]), "over 64"],
      [doc([rule({ sdk_min: "30" })]), 'unknown match key "sdk_min"'],
      [doc([{ ...rule({ brand: "a" }), extra: 1 }]), 'unknown key "extra"'],
      [doc([rule({ model: "unknown" })]), "unknown match key"],
      [doc([rule({ manufacturer: "Unknown" })]), "can never match"],
      [doc(Array.from({ length: 201 }, (_, i) => rule({ device: `d${i}` }))), "more than 200"],
    ];
    for (const [input, message] of cases) {
      const res = checkTierFile(input, { today: TODAY });
      expect(res.doc, JSON.stringify(input).slice(0, 120)).toBeNull();
      expect(res.errors.join("\n")).toContain(message);
    }
  });

  it("finds rules an earlier, broader rule makes dead", () => {
    const res = checkTierFile(
      doc([rule({ manufacturer: "TCL" }), rule({ manufacturer: "tcl", model_prefix: "QM7" }, "supported")]),
      { today: TODAY },
    );
    expect(res.errors).toEqual([expect.stringContaining("rule 1 can never match: rule 0 comes first")]);
    const prefix = checkTierFile(
      doc([rule({ model_prefix: "SHIELD" }), rule({ model_prefix: "shield android", manufacturer: "NVIDIA" })]),
      { today: TODAY },
    );
    expect(prefix.errors).toEqual([expect.stringContaining("rule 1 can never match")]);
    // Specific first is the right order.
    const ok = checkTierFile(
      doc([rule({ manufacturer: "TCL", model_prefix: "QM7" }, "supported"), rule({ manufacturer: "TCL" })]),
      { today: TODAY },
    );
    expect(ok.errors).toEqual([]);
  });

  it("refuses a future date, which would hide the live list from apps", () => {
    expect(checkTierFile(doc([], "2026-11-02"), { today: TODAY }).errors.join()).toContain("in the future");
    expect(checkTierFile(doc([], TODAY), { today: TODAY }).errors).toEqual([]);
  });

  it("requires updated to move forward when the rules change", () => {
    const base = doc([rule({ manufacturer: "TCL" })], "2026-10-31");
    const changed = [rule({ manufacturer: "TCL" }, "supported")];

    const stale = checkTierFile(doc(changed, "2026-10-31"), { base, today: TODAY });
    expect(stale.errors.join()).toContain("updated is still 2026-10-31");

    expect(checkTierFile(doc(changed, TODAY), { base, today: TODAY }).errors).toEqual([]);
    // A note edit is a change too.
    const note = checkTierFile(doc([rule({ manufacturer: "TCL" }, "beta", "other")], "2026-10-31"), { base, today: TODAY });
    expect(note.errors.join()).toContain("updated is still");
    // Unchanged rules don't need a new date.
    expect(checkTierFile(base, { base, today: TODAY }).errors).toEqual([]);
    // Two changes on the same day share that day's date.
    const sameDayBase = doc([rule({ manufacturer: "TCL" })], TODAY);
    expect(checkTierFile(doc(changed, TODAY), { base: sameDayBase, today: TODAY }).errors).toEqual([]);
    // Never backwards.
    const back = checkTierFile(doc(changed, "2026-10-30"), { base, today: TODAY });
    expect(back.errors.join()).toContain("went backwards");
  });

  it("warns that the device report can't evaluate brand rules", () => {
    const res = checkTierFile(doc([rule({ brand: "onn" })]), { today: TODAY });
    expect(res.errors).toEqual([]);
    expect(res.warnings.join()).toContain("brand");
  });
});

describe("classify mirrors the apps (engine/support.rs)", () => {
  const rules = (checkTierFile(repoFile, { today: TODAY }).doc as TierDoc).rules;
  const props = (manufacturer: string, brand: string, model: string, device: string) => ({ manufacturer, brand, model, device });

  it("classifies the same fixtures the Rust tests use", () => {
    expect(classify(rules, props("NVIDIA", "NVIDIA", "SHIELD Android TV", "mdarcy")).tier).toBe("fully_supported");
    expect(classify(rules, props("TCL", "TCL", "QM7L Pro", "x")).tier).toBe("beta");
    expect(classify(rules, props("Amazon", "Amazon", "AFTMM", "mantis")).tier).toBe("not_supported");
    expect(classify(rules, props("Amazon", "Amazon", "KFTRWI", "trona")).tier).toBe("unknown");
    expect(classify(rules, props("Google", "google", "Pixel 10 Pro", "blazer")).tier).toBe("unknown");
  });

  it("unreadable or unreported properties claim nothing", () => {
    expect(classify(rules, null)).toEqual({ tier: "unknown", rule: null });
    expect(classify(rules, props("", "", "", "")).tier).toBe("unknown");
    expect(classify(rules, props("unknown", "unknown", "unknown", "unknown")).tier).toBe("unknown");
    expect(classify(rules, { manufacturer: null, model: null, device: null }).tier).toBe("unknown");
  });

  it("is case-insensitive, first match wins, and model_prefix is a prefix", () => {
    const r: TierRule[] = [
      { match: { brand: "acme", device: "boxy" }, tier: "supported", note: "a" },
      { match: { brand: "ACME" }, tier: "beta", note: "b" },
    ];
    expect(classify(r, props("x", "Acme", "M1", "BOXY")).tier).toBe("supported");
    expect(classify(r, props("x", "acme", "M1", "other")).tier).toBe("beta");
    const p: TierRule[] = [{ match: { model_prefix: "ab" }, tier: "beta", note: "n" }];
    expect(classify(p, props("", "", "é", "")).tier).toBe("unknown");
    expect(classify(p, props("", "", "ABC", "")).tier).toBe("beta");
  });

  it("says undetermined, not a guess, when a brand rule decides and brand is unknown", () => {
    const r: TierRule[] = [
      { match: { manufacturer: "onn", brand: "onn" }, tier: "supported", note: "a" },
      { match: { manufacturer: "onn" }, tier: "beta", note: "b" },
    ];
    expect(classify(r, { manufacturer: "onn", model: "4K" }, { brandKnown: false })).toEqual({ tier: "undetermined", rule: 0 });
    // A brand rule whose other keys don't match is skipped as usual.
    expect(classify(r, { manufacturer: "TCL", model: "4K" }, { brandKnown: false }).tier).toBe("unknown");
  });
});
