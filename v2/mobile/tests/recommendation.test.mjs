import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { startViteServer } from "./helpers/vite-harness.mjs";

let server;
let origin;
let rec;
let report;
let safety;

before(async () => {
  ({ server, origin } = await startViteServer());
  rec = await server.ssrLoadModule("/src/lib/recommendation.ts");
  report = await server.ssrLoadModule("/src/lib/app-report.ts");
  safety = await server.ssrLoadModule("/src/lib/safety.ts");
});

after(async () => {
  await server?.close();
});

const entry = (overrides = {}) => ({
  package: "com.example.app",
  name: "Example",
  method: "uninstall",
  risk: "safe",
  optimize_description: "",
  restore_description: "",
  default_optimize: true,
  default_restore: false,
  play_store: false,
  ...overrides,
});

const ready = (kind) => ({ status: "ready", verdict: { kind, reason: "r", source: "reviewed_catalog" } });

test("the shared sideload catalog is served to the webview", async () => {
  // recommendation.ts reads desktop's sideload-catalog.json rather than a
  // copy; the dev server must be allowed to serve it outside the mobile root.
  const res = await fetch(`${origin}/src/lib/recommendation.ts`);
  assert.equal(res.status, 200);
  const body = await res.text();
  const jsonImport = body.match(/from\s+"([^"]*sideload-catalog\.json[^"]*)"/);
  assert.ok(jsonImport, "recommendation.ts imports the sideload catalog");
  const json = await fetch(new URL(jsonImport[1], origin));
  assert.equal(json.status, 200);
});

test("uninstall is only recommended when the store can give the app back", () => {
  assert.equal(rec.effectiveMethod(entry()), "disable");
  assert.equal(rec.effectiveMethod(entry({ play_store: true })), "uninstall");
  assert.equal(rec.effectiveMethod(entry({ defunct: true })), "uninstall");
  assert.deepEqual(rec.recommendation(entry(), "enabled", ready("safe")), {
    kind: "act",
    label: "Disable",
    action: "disable",
  });
  assert.deepEqual(rec.recommendation(entry({ play_store: true }), "enabled", ready("safe")), {
    kind: "act",
    label: "Uninstall",
    action: "uninstall",
  });
  assert.equal(rec.canOfferUninstall(entry()), false);
  assert.equal(rec.canOfferUninstall(entry({ package: "org.smarttube.stable" })), true);
});

test("no recommendation label ever says Remove", () => {
  const states = ["enabled", "disabled", "missing", null];
  const verdicts = [ready("safe"), ready("caution"), ready("unknown"), ready("never_disable"), undefined];
  for (const e of [entry(), entry({ play_store: true }), entry({ default_optimize: false, review: true })]) {
    for (const s of states) {
      for (const v of verdicts) {
        assert.doesNotMatch(rec.recommendation(e, s, v).label, /remove/i);
      }
    }
  }
});

test("unknown and unresolved safety never read as a default action", () => {
  assert.equal(rec.recommendation(entry(), "enabled", ready("unknown")).kind, "review");
  assert.equal(rec.recommendation(entry(), "enabled", undefined).kind, "unavailable");
  assert.equal(rec.recommendation(entry(), "enabled", { status: "checking" }).kind, "unavailable");
  assert.equal(rec.recommendation(entry(), "enabled", ready("never_disable")).label, "Protected");
});

test("safety labels match the desktop vocabulary", () => {
  assert.equal(safety.SAFETY_TIERS.safe.label, "Safe to remove");
  assert.equal(safety.SAFETY_TIERS.unknown.label, "Unknown");
  assert.equal(safety.SAFETY_TIERS.caution.label, "Caution");
  assert.equal(safety.SAFETY_TIERS.never_disable.label, "Protected");
  assert.equal(safety.safetyLabel(undefined), "Safety unavailable");
  assert.equal(safety.safetyLabel({ status: "checking" }), "Checking…");
});

test("an app report carries no device identifiers and is labelled mobile", () => {
  const built = report.buildAppReport({
    package: "com.example.app",
    appName: "Example",
    reason: "wrong_verdict",
    note: "seen on 192.168.1.20 serial ABCD1234 next to com.other.app",
    appVersion: "0.1.0",
    device: { family: "android_tv", androidVersion: "12", redact: ["ABCD1234", "unknown"] },
    verdict: null,
    includeState: false,
    state: { status: null, running: null, ramMb: null, storage: null },
    now: new Date("2026-10-06T00:00:00Z"),
  });
  assert.equal(built.source, "mobile_app_report");
  const [record] = built.records;
  assert.equal(record.reason, "user_report_wrong_verdict");
  assert.equal(record.current_verdict, "unavailable");
  assert.doesNotMatch(record.note, /192\.168|ABCD1234|com\.other\.app/);
  assert.equal(report.reportFamily("unknown", undefined), "unknown");
  assert.equal(report.reportFamily("unknown", "tv"), "android_tv");
  assert.equal(report.buildAppReport({ ...built, package: "not a package" }), null);
  const { url, prefilled } = report.appReportIssueUrl("com.example.app", "other", "{}");
  assert.ok(prefilled);
  assert.match(url, /template=app_report\.yml/);
});

test("uninstallOutcome names how each kind of app comes back", () => {
  const store = rec.uninstallOutcome("com.example.app", entry({ play_store: true }));
  assert.equal(store.short, "Get it back from the Play Store");
  assert.match(store.detail, /^Uninstall removes it for this TV's user\. Get it back from the Play Store\./);

  const sideload = rec.uninstallOutcome("org.smarttube.stable", entry({ package: "org.smarttube.stable" }));
  assert.equal(sideload.short, "Not on the Play Store: reinstall from SmartTube");
  assert.match(sideload.detail, /https:\/\/github\.com\/yuliskov\/SmartTube/);

  const defunct = rec.uninstallOutcome("com.example.app", entry({ defunct: true }));
  assert.equal(defunct.short, "Discontinued: no way to download it again");

  const unknown = rec.uninstallOutcome("com.example.unlisted", null);
  assert.equal(unknown.short, "May not be on the Play Store");
  assert.match(unknown.detail, /back it up first\./);

  // Every detail says what Uninstall does, and, because the phone's app sheet
  // offers Reinstall for any app that is gone, that Reinstall may work.
  for (const o of [store, sideload, defunct, unknown]) {
    assert.match(o.detail, /^Uninstall removes it for this TV's user\./);
    assert.match(o.detail, /Reinstall in its app sheet can bring it back while its APK is still on the TV\.$/);
  }
  assert.equal(rec.uninstallNote("com.example.app", entry({ defunct: true })), defunct.detail);
});

test("a review row suggests Uninstall only for a store app the user installed", () => {
  const store = entry({ method: "disable", play_store: true, review: true, default_optimize: false });
  assert.equal(rec.suggestedMethod(store, "user"), "uninstall");
  assert.equal(rec.suggestedMethod({ ...store, method: "uninstall" }, "preinstalled"), "disable");
  assert.equal(rec.suggestedMethod(store, "unknown"), "disable");
  for (const other of [{ play_store: false }, { defunct: true }, { default_restore: true }, { risk: "high" }, { risk: "advanced" }]) {
    assert.equal(rec.suggestedMethod({ ...store, ...other }, "user"), "disable", JSON.stringify(other));
  }
  assert.equal(rec.suggestedMethod({ ...store, risk: "medium" }, "user"), "uninstall");
  assert.deepEqual(rec.recommendation(store, "enabled", ready("safe"), "user"), {
    kind: "review", label: "Review: uninstall if unused", action: "uninstall",
  });
  assert.deepEqual(rec.recommendation(store, "enabled", ready("caution")), {
    kind: "review", label: "Review: disable if unused", action: "disable",
  });
  // A default Optimize row keeps its store-gated method whatever the source.
  const defunct = entry({ play_store: false, defunct: true });
  assert.equal(rec.recommendation(defunct, "enabled", ready("safe"), "preinstalled").action, "uninstall");
  // A failed or empty third-party read claims nothing.
  assert.equal(rec.installSource("com.example.app", null), "unknown");
  assert.equal(rec.installSource("com.example.app", new Set()), "unknown");
  assert.equal(rec.installSource("com.example.app", new Set(["com.example.app"])), "user");
  assert.equal(rec.installSource("com.example.app", new Set(["com.other"])), "preinstalled");
});
