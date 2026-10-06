import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { startViteServer } from "./helpers/vite-harness.mjs";

let server;
let rows;
let mem;
let snap;

before(async () => {
  ({ server } = await startViteServer());
  rows = await server.ssrLoadModule("/src/lib/optimizeRows.ts");
  mem = await server.ssrLoadModule("/src/lib/memorySuggestion.ts");
  snap = await server.ssrLoadModule("/src/lib/snapshotPreview.ts");
});

after(async () => {
  await server?.close();
});

const entry = (overrides = {}) => ({
  package: "com.example.app",
  name: "Example",
  method: "disable",
  risk: "safe",
  optimize_description: "",
  restore_description: "",
  default_optimize: true,
  default_restore: false,
  play_store: true,
  ...overrides,
});
const item = (action, overrides = {}) => ({ entry: entry(overrides), action, memory_mb: null });
const ready = (kind) => ({ status: "ready", verdict: { kind, reason: "r" } });

test("Optimize recommends exactly what desktop pre-selects", () => {
  const disable = { kind: "disable" };
  assert.equal(rows.isPlanRecommended(item(disable), "optimize", ready("safe")), true);
  assert.equal(rows.isPlanRecommended(item(disable), "optimize", ready("caution")), true);
  // Unknown is a review, never a default; Protected and unresolved never are.
  assert.equal(rows.isPlanRecommended(item(disable), "optimize", ready("unknown")), false);
  assert.equal(rows.isPlanRecommended(item(disable), "optimize", ready("never_disable")), false);
  assert.equal(rows.isPlanRecommended(item(disable), "optimize", { status: "checking" }), false);
  assert.equal(rows.isPlanRecommended(item(disable), "optimize", undefined), false);
  // Not on the default list: never pre-selected, whatever the verdict.
  assert.equal(
    rows.isPlanRecommended(item(disable, { default_optimize: false, review: true }), "optimize", ready("safe")),
    false,
  );
  // Restore follows default_restore.
  const enable = { kind: "enable" };
  assert.equal(rows.isPlanRecommended(item(enable, { default_restore: true }), "restore", undefined), true);
  assert.equal(rows.isPlanRecommended(item(enable), "restore", undefined), false);
});

test("an uninstall the store can't give back runs as Disable", () => {
  const uninstall = { kind: "uninstall" };
  assert.equal(rows.naturalAction(item(uninstall, { method: "uninstall", play_store: false })), "disable");
  assert.equal(rows.naturalAction(item(uninstall, { method: "uninstall", play_store: false, defunct: true })), "uninstall");
  assert.equal(rows.naturalAction(item(uninstall, { method: "uninstall" })), "uninstall");
  assert.equal(rows.naturalAction(item({ kind: "skip", reason: "not_installed" })), null);
});

test("review pill only for resolved, unprotected review rows", () => {
  const review = item({ kind: "disable" }, { default_optimize: false, review: true });
  assert.equal(rows.isReviewRow(review, ready("caution")), true);
  assert.equal(rows.isReviewRow(review, ready("never_disable")), false);
  assert.equal(rows.isReviewRow(review, { status: "checking" }), false);
  assert.equal(rows.isReviewRow(item({ kind: "disable" }), ready("caution")), false);
});

test("plan search matches name or package, case-insensitively", () => {
  const it = item({ kind: "disable" }, { name: "Prime Video", package: "com.amazon.amazonvideo.livingroom" });
  assert.equal(rows.matchesPlanQuery(it, ""), true);
  assert.equal(rows.matchesPlanQuery(it, "  prime "), true);
  assert.equal(rows.matchesPlanQuery(it, "AMAZONVIDEO"), true);
  assert.equal(rows.matchesPlanQuery(it, "netflix"), false);
});

test("row safety never reads unresolved as a verdict", () => {
  assert.deepEqual(rows.rowSafety(undefined, true, false), { status: "checking" });
  assert.equal(rows.rowSafety(undefined, false, true).status, "unavailable");
  assert.equal(rows.rowSafety({ kind: "safe", reason: "r" }, true, false).status, "ready");
});

test("the sheet sees the plan's state and catalog entry", () => {
  const app = rows.planItemApp(item({ kind: "skip", reason: "already_disabled" }));
  assert.equal(app.state, "disabled");
  assert.equal(app.entry.package, "com.example.app");
  assert.equal(app.system, null);
});

const row = (process, pkg = process) => ({ process, pid: 1, package: pkg, mb: 100 });
const installedReady = (pairs) => ({ status: "ready", packages: new Map(pairs) });

test("Health: a confirmed catalog app gets the App List's recommendation", () => {
  const catalog = { status: "ready", entries: new Map([["com.example.app", entry()]]) };
  const s = mem.memorySuggestion(
    row("com.example.app:remote", "com.example.app"),
    installedReady([["com.example.app", "enabled"]]),
    catalog,
    ready("safe"),
  );
  assert.equal(s.kind, "recommendation");
  assert.equal(s.rec.label, "Disable");
  assert.deepEqual(mem.suggestionDisplay(s), { label: "Disable", tone: "act" });
  const disabled = mem.memorySuggestion(
    row("com.example.app"),
    installedReady([["com.example.app", "disabled"]]),
    catalog,
    ready("safe"),
  );
  assert.equal(disabled.rec.label, "Already disabled");
});

test("Health: a name no installed package carries is a process, not an app", () => {
  const catalog = { status: "ready", entries: new Map([["media.codec", entry({ package: "media.codec" })]]) };
  const s = mem.memorySuggestion(row("media.codec"), installedReady([]), catalog, ready("safe"));
  assert.equal(s.kind, "process");
  assert.equal(mem.suggestionDisplay(s).label, "Not an app");
  assert.deepEqual(mem.safetyQuery(row("media.codec"), installedReady([])), { kind: "process", name: "media.codec" });
  assert.deepEqual(
    mem.safetyQuery(row("com.x:svc", "com.x"), installedReady([["com.x", "enabled"]])),
    { kind: "package", name: "com.x" },
  );
});

test("Health: an unreadable installed list confirms nothing and says so", () => {
  const catalog = { status: "ready", entries: new Map() };
  const s = mem.memorySuggestion(row("com.example.app"), { status: "failed" }, catalog, ready("safe"));
  assert.equal(s.kind, "unconfirmed");
  assert.notEqual(mem.suggestionDisplay(s).label, "Not an app");
  const bare = mem.memorySuggestion(row("/system/bin/surfaceflinger", null), { status: "failed" }, catalog, undefined);
  assert.equal(bare.kind, "process");
  assert.equal(mem.memorySuggestion(row("x"), { status: "loading" }, catalog, undefined).kind, "checking");
  assert.equal(mem.safetyQuery(row("x"), { status: "loading" }), null);
});

test("Health: an installed package outside the catalog shows its verdict", () => {
  const s = mem.memorySuggestion(
    row("com.other"),
    installedReady([["com.other", "enabled"]]),
    { status: "ready", entries: new Map() },
    ready("unknown"),
  );
  assert.equal(s.kind, "verdict");
  assert.equal(mem.suggestionDisplay(s).label, "Unknown");
  const pending = mem.memorySuggestion(
    row("com.other"),
    installedReady([["com.other", "enabled"]]),
    { status: "loading" },
    ready("unknown"),
  );
  assert.equal(pending.kind, "checking");
  assert.equal(
    mem.suggestionDisplay({ kind: "verdict", pkg: "com.other", status: { status: "unavailable", reason: "x" } }).label,
    "Safety unavailable",
  );
});

const plan = (overrides = {}) => ({
  cross_device_warning: null,
  packages_to_disable: [],
  packages_already_disabled: [],
  packages_not_installed: [],
  launcher_to_set: null,
  settings_to_write: {},
  settings_to_delete: [],
  settings_already_set: [],
  current_values: {},
  current_launcher: null,
  launcher_not_installed: null,
  ...overrides,
});

test("snapshot preview: Now is what the TV reported, and matching settings read Already set", () => {
  const { acting, unchanged } = snap.previewRows(
    plan({
      packages_to_disable: ["com.a"],
      packages_already_disabled: ["com.b"],
      packages_not_installed: ["com.c"],
      settings_to_write: { "global:x": "1" },
      settings_to_delete: ["secure:y"],
      settings_already_set: ["system:z"],
      current_values: { "global:x": "0", "secure:y": "5", "system:z": "2" },
      current_launcher: "com.home",
    }),
    "com.home",
  );
  assert.deepEqual(
    acting.map((r) => [r.item, r.now, r.result]),
    [
      ["com.a", "enabled", "Disable"],
      ["global:x", "0", "Set → 1"],
      ["secure:y", "5", "Reset → device default"],
    ],
  );
  assert.deepEqual(
    unchanged.map((r) => [r.item, r.now, r.result]),
    [
      ["Home app", "com.home", "Already Home"],
      ["system:z", "2", "Already set"],
      ["com.b", "disabled", "Already disabled"],
      ["com.c", "not installed", "Not on device"],
    ],
  );
});

test("snapshot preview: launcher change, skipped launcher, and an unset Now", () => {
  const set = snap.previewRows(
    plan({ launcher_to_set: "com.new", current_launcher: null, settings_to_write: { k: "v" } }),
    "com.new",
  );
  assert.deepEqual(set.acting[0], { item: "Home app", now: "—", result: "Set Home → com.new", kind: "launcher" });
  assert.equal(set.acting[1].now, "unset");
  const skipped = snap.previewRows(plan({ launcher_not_installed: "com.gone", current_launcher: "com.home" }), "com.gone");
  assert.equal(skipped.unchanged[0].result, "com.gone isn't installed; skipped");
  // No recorded launcher: no Home row at all.
  assert.equal(snap.previewRows(plan(), null).unchanged.length, 0);
  // An older backend without current_values says "unset", never invents one.
  const legacy = plan({ settings_already_set: ["k"] });
  delete legacy.current_values;
  assert.equal(snap.previewRows(legacy, null).unchanged[0].now, "unset");
});

test("snapshot preview summary matches desktop's wording", () => {
  assert.equal(
    snap.previewSummary(plan({ packages_to_disable: ["a"], settings_to_write: { k: "v" }, launcher_to_set: "h" })),
    "1 to disable · 0 already disabled · 0 not on device · 1 setting to write · 0 to reset · Home app",
  );
});
