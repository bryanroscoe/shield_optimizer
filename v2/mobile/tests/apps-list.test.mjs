import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { after, before, test } from "node:test";
import { startViteServer } from "./helpers/vite-harness.mjs";

// The real catalogue, so these tests fail if katniss ever leaves it or its
// name stops saying what it is.
const common = JSON.parse(
  readFileSync(new URL("../../crates/core/data/app-lists/common.json", import.meta.url), "utf8"),
);
const katniss = common.find((entry) => entry.package === "com.google.android.katniss");

let server;
let list;

before(async () => {
  ({ server } = await startViteServer());
  list = await server.ssrLoadModule("/src/lib/appsList.ts");
});

after(async () => {
  await server?.close();
});

const filters = (overrides = {}) => ({
  query: "",
  status: "all",
  showSystem: false,
  hideNotInstalled: true,
  ...overrides,
});

const others = [
  { package: "com.example.streambox", name: "StreamBox", system: false, enabled: true },
  { package: "com.android.helper", name: "System Helper", system: true, enabled: false },
];

function searchAll(states, query, extra = {}) {
  const catalog = list.catalogItems([katniss], states);
  const rest = list.otherItems(others, new Set(catalog.map((i) => i.package)));
  const f = filters({ query, ...extra });
  return [...list.filterCatalog(catalog, f), ...list.filterOthers(rest, f)].map((i) => i.package);
}

test("katniss is in the shared catalogue", () => {
  assert.ok(katniss, "com.google.android.katniss must stay in common.json");
});

for (const state of ["enabled", "disabled"]) {
  test(`katniss is found by package and by name when ${state}`, () => {
    const states = { [katniss.package]: state };
    for (const query of ["katniss", "assistant", "  ASSISTANT ", "com.google.android.katn"]) {
      assert.deepEqual(searchAll(states, query), [katniss.package], query);
    }
    assert.deepEqual(searchAll(states, "assistant", { status: state }), [katniss.package]);
    // A system-app toggle never hides a recognised app.
    assert.deepEqual(searchAll(states, "katniss", { showSystem: false }), [katniss.package]);
  });
}

test("a disabled katniss is reachable from the Disabled chip so it can be re-enabled", () => {
  const states = { [katniss.package]: "disabled" };
  assert.deepEqual(searchAll(states, "", { status: "disabled" }), [katniss.package]);
  assert.deepEqual(searchAll(states, "katniss", { status: "enabled" }), []);
});

test("not-installed catalogue rows hide by default and are counted as hidden", () => {
  const states = { [katniss.package]: "missing" };
  assert.deepEqual(searchAll(states, "katniss"), []);
  assert.deepEqual(searchAll(states, "katniss", { hideNotInstalled: false }), [katniss.package]);
  const catalog = list.catalogItems([katniss], states);
  const hidden = list.hiddenMatches(catalog, [], filters({ query: "assistant" }));
  assert.equal(hidden.notInstalled, 1);
  // A missing app is neither enabled nor disabled.
  assert.deepEqual(searchAll(states, "katniss", { hideNotInstalled: false, status: "disabled" }), []);
});

test("an unread state is unknown: shown under All, never under Enabled", () => {
  const catalog = list.catalogItems([katniss], {});
  assert.equal(catalog[0].state, null);
  assert.equal(list.filterCatalog(catalog, filters()).length, 1);
  assert.equal(list.filterCatalog(catalog, filters({ status: "enabled" })).length, 0);
});

test("system packages in Everything else follow the toggle and are counted when hidden", () => {
  const rest = list.otherItems(others, new Set());
  assert.deepEqual(list.filterOthers(rest, filters()).map((i) => i.package), ["com.example.streambox"]);
  assert.equal(list.filterOthers(rest, filters({ showSystem: true })).length, 2);
  assert.equal(list.hiddenMatches([], rest, filters({ query: "helper" })).system, 1);
});

test("a package in both lists is listed once, as recognised", () => {
  const catalog = list.catalogItems([katniss, katniss], { [katniss.package]: "enabled" });
  assert.equal(catalog.length, 1);
  const rest = list.otherItems(
    [...others, { package: katniss.package, name: null, system: true, enabled: true }],
    new Set(catalog.map((i) => i.package)),
  );
  assert.equal(rest.some((i) => i.package === katniss.package), false);
  assert.equal(catalog[0].entry.package, katniss.package);
  assert.equal(catalog[0].description, katniss.optimize_description);
});

test("validatedStates drops malformed values instead of guessing", () => {
  const states = list.validatedStates(["a.b", "c.d", "e.f"], { "a.b": "enabled", "c.d": "weird", "x.y": "disabled" });
  assert.deepEqual(states, { "a.b": "enabled" });
  assert.deepEqual(list.validatedStates(["a.b"], null), {});
  assert.deepEqual(list.validatedStates(["a.b"], ["enabled"]), {});
});

test("onDeviceCount leaves out only rows known to be missing", () => {
  const catalog = list.catalogItems(
    [katniss, { ...katniss, package: "com.example.gone" }, { ...katniss, package: "com.example.unread" }],
    { [katniss.package]: "enabled", "com.example.gone": "missing" },
  );
  assert.equal(list.onDeviceCount(catalog), 2);
});
