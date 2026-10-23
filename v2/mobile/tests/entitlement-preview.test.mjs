import assert from "node:assert/strict";
import { before, after, test } from "node:test";
import { chromium } from "playwright";
import { readFile } from "node:fs/promises";
import { startViteServer } from "./helpers/vite-harness.mjs";
import { SimulatedStore } from "./helpers/simulated-store.mjs";
let server, origin, browser, model;
before(async () => { ({ server, origin } = await startViteServer()); model = await server.ssrLoadModule("/src/lib/entitlementPreview.ts"); browser = await chromium.launch({ headless: true }); });
after(async () => { await browser?.close(); await server?.close(); });
test("matrix covers every registered mobile command and opener exactly once", async () => {
  const matrix = JSON.parse(await readFile(new URL("../BILLING-COMMAND-MATRIX.json", import.meta.url)));
  const source = await readFile(new URL("../src-tauri/src/lib.rs", import.meta.url), "utf8");
  const block = source.split("tauri::generate_handler![")[1].split("])")[0];
  const commands = [...block.matchAll(/^\s*([\w:]+),/gm)].map(x => x[1].split("::").at(-1)); commands.push("plugin:opener|open_url");
  assert.deepEqual(matrix.commands.map(x => x.command).sort(), commands.sort());
  assert.equal(new Set(commands).size, commands.length);
  for (const row of matrix.commands) { assert.ok(row.backend); assert.ok(row.gate); assert.ok(row.api); }
});
test("store observations never unlock; stale, duplicate and ack events are isolated", () => {
  let s = model.initialPreview(); const store = new SimulatedStore(e => { s = model.reducePreview(s, e); });
  store.begin(1); store.respond(1, "pending"); assert.equal(model.previewIsPro(s), false);
  store.respond(1, "purchased"); assert.equal(model.previewIsPro(s), false);
  store.begin(2, "restore"); const fresh = s; store.respond(1, "error"); assert.deepEqual(s, fresh);
  assert.throws(() => store.respond(2, "verified")); store.respond(2, "purchased");
  s = model.reducePreview(s, { type: "verified", request: 2 }); assert.equal(model.previewIsPro(s), true); assert.equal(s.acknowledged, false);
  s = model.reducePreview(s, { type: "acknowledged", request: 2 }); assert.equal(s.acknowledged, true);
  assert.deepEqual(model.reducePreview(s, { type: "verified", request: 2 }), s);
});
test("errors/cancel preserve access; revocation preserves independent legacy source", () => {
  let s = model.initialPreview();
  for (const e of [{ type: "begin", request: 1, operation: "buy" }, { type: "purchased", request: 1 }, { type: "verified", request: 1 }, { type: "begin", request: 2, operation: "restore" }, { type: "error", request: 2 }]) s = model.reducePreview(s, e);
  assert.equal(model.previewIsPro(s), true); s = model.reducePreview(s, { type: "legacy", valid: true }); s = model.reducePreview(s, { type: "revoked" });
  assert.equal(s.playPro, false); assert.equal(model.previewIsPro(s), true);
  s = model.reducePreview(s, { type: "legacy", valid: false }); assert.equal(model.previewIsPro(s), false);
  assert.equal(model.reducePreview(s, { type: "verified", request: 2 }).playPro, false);
});
test("cancel and errors never create access; acknowledgment cannot precede verification", () => {
  for (const type of ["canceled", "error"]) {
    let s = model.reducePreview(model.initialPreview(), { type: "begin", request: 1, operation: "buy" });
    s = model.reducePreview(s, { type, request: 1 });
    assert.equal(s.phase, type); assert.equal(model.previewIsPro(s), false);
    assert.equal(model.reducePreview(s, { type: "acknowledged", request: 1 }).acknowledged, false);
    assert.equal(model.reducePreview(s, { type: "verified", request: 1 }).playPro, false);
  }
});
test("simulated UI keeps recovery usable through pending/error/revocation", async (t) => {
  const page = await browser.newPage({ viewport: { width: 384, height: 812 }, ...(process.env.ENTITLEMENT_VIDEO_DIR ? { recordVideo: { dir: process.env.ENTITLEMENT_VIDEO_DIR, size: { width: 384, height: 812 } } } : {}) }); t.after(() => page.close()); await page.goto(origin);
  await page.evaluate(async () => {
    const { mount } = await import("/node_modules/svelte/src/index-client.js");
    const Harness = (await import("/tests/helpers/EntitlementHarness.svelte")).default;
    const target = document.createElement("div"); document.body.replaceChildren(target); window.recovered = 0;
    window.instance = mount(Harness, { target, props: { recovered: () => window.recovered++ } });
    window.update = event => window.instance.dispatch(event);
  });
  await page.getByRole("button", { name: "Simulate Pro purchase" }).click(); await page.evaluate(() => window.update({ type: "pending", request: 1 }));
  assert.equal(await page.getByRole("button", { name: "Simulate Pro purchase" }).isDisabled(), true);
  if (process.env.ENTITLEMENT_VIDEO_DIR) {
    await page.screenshot({ path: "/tmp/shield-entitlement-preview.png" });
    await page.waitForTimeout(1000);
  }
  for (const event of [null, { type: "error", request: 1 }, { type: "revoked" }]) {
    if (event) await page.evaluate(e => window.update(e), event);
    if (process.env.ENTITLEMENT_VIDEO_DIR) await page.waitForTimeout(1000);
    await page.getByRole("button", { name: "Free recovery" }).click();
  }
  assert.equal(await page.evaluate(() => window.recovered), 3);
  assert.equal(await page.getByRole("button", { name: "Support tips unavailable" }).isDisabled(), true);
});
