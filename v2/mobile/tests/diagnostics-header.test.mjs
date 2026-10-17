import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
import { before, after, test } from "node:test";
import { chromium } from "playwright";
import { startViteServer } from "./helpers/vite-harness.mjs";
let server, origin, browser;
before(async () => { ({ server, origin } = await startViteServer()); browser = await chromium.launch({ headless: true }); });
after(async () => { await browser?.close(); await server?.close(); });
const dir = "evidence/diagnostics-header-2026-10-17";
async function open(t, { width = 320, scale = 1, deviceType = "shield", connected = true, record = false } = {}) {
  const page = await browser.newPage({ viewport: { width, height: 812 }, ...(record ? { recordVideo: { dir: `${dir}/raw`, size: { width: 384, height: 812 } } } : {}) });
  page.setDefaultTimeout(10000); t.after(() => page.close());
  await page.addInitScript(({ deviceType }) => {
    localStorage.clear(); window.calls = [];
    window.__TAURI_INTERNALS__ = { invoke: async (command, args) => {
      calls.push({ command, args });
      switch(command) {
        case "get_entitlement": return "pro";
        case "wireless_connect": return { ok: true };
        case "wireless_status": return { connected: true };
        case "list_devices": return [{ id: 1, serial: "MOCK:5555", name: "Synthetic TV", model: "Fixture TV", status: "device", connection: "network", device_type: deviceType, properties: null }];
        case "health_report": return { ram: { free_mb: 1024, total_mb: 2048 }, storage: { used_percent: 25 }, display: {}, top_memory: [] };
        case "app_list_for_device": case "list_installed_packages": return [];
        case "package_states": return {};
        case "find_remote": throw Error("The fixture must cancel remote ringing");
        default: return { ok: true, message: "fixture" };
      }
    } };
  }, { deviceType });
  await page.goto(origin);
  await page.evaluate(async ({ scale, connected }) => {
    const { session } = await import("/src/lib/session.svelte.ts"); const { router } = await import("/src/lib/router.svelte.ts");
    window.session = session; window.router = router;
    if (connected) await session.connect("MOCK", 5555);
    router.reset("dashboard"); router.navigate("diagnostics"); document.documentElement.style.fontSize = `${16 * scale}px`;
  }, { scale, connected });
  await page.getByRole("heading", { name: "Diagnostics", exact: true }).waitFor(); return page;
}
async function withinViewport(locator) {
  return locator.evaluate(el => { const r = el.getBoundingClientRect(); return r.left >= 0 && r.right <= innerWidth && r.top >= 0 && r.bottom <= innerHeight; });
}
for (const [width, scale] of [[320, 1], [384, 1], [320, 2], [384, 2]]) {
  test(`Diagnostics header fits ${width}px at ${scale * 100}% text with every control usable`, async t => {
    const page = await open(t, { width, scale }); const header = page.locator(".topline");
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
    assert.equal(await header.evaluate(el => el.scrollWidth <= el.clientWidth), true);
    const back = header.getByRole("button", { name: "Back", exact: true });
    const find = header.getByRole("button", { name: "Find remote", exact: true }); const live = header.locator(".live-refresh-btn");
    assert.equal(await live.getAttribute("aria-pressed"), "false");
    for (const control of [back, find, live]) {
      assert.equal(await withinViewport(control), true);
      const rect = await control.boundingBox(); assert.ok(rect.width >= 44 && rect.height >= 44);
    }
    assert.equal(await header.getByRole("heading", { name: "Diagnostics", exact: true }).evaluate(el => getComputedStyle(el).fontSize), `${22 * scale}px`);
    assert.equal(await live.evaluate(el => getComputedStyle(el).fontSize), `${11 * scale}px`);
    const names = await header.locator("button").allTextContents();
    assert.deepEqual(names.map(x => x.trim()), ["arrow_back", "settings_remote", "refreshlive"]);
    assert.equal(await page.locator('meta[name="viewport"]').getAttribute("content"), "width=device-width, initial-scale=1.0, viewport-fit=cover");
    await back.focus(); await page.keyboard.press("Tab");
    assert.equal(await find.evaluate(el => document.activeElement === el), true);
    await page.keyboard.press("Tab"); assert.equal(await live.evaluate(el => document.activeElement === el), true);
    if (process.env.HEADER_EVIDENCE === "1") {
      await mkdir(dir, { recursive: true });
      await page.screenshot({ path: `${dir}/header-${width}-${scale * 100}-simulated.png`, animations: "disabled" });
    }
    const before = await page.evaluate(() => calls.filter(c => c.command === "health_report").length);
    await live.click(); await page.waitForFunction(before => calls.filter(c => c.command === "health_report").length > before, before);
    assert.equal(await live.getAttribute("aria-pressed"), "true"); await live.click(); assert.equal(await live.getAttribute("aria-pressed"), "false");
    await find.click(); const confirm = page.getByRole("dialog", { name: "Ring the remote?", exact: true }); await confirm.waitFor();
    await confirm.getByRole("button", { name: "Cancel", exact: true }).click(); await confirm.waitFor({ state: "detached" });
    assert.equal(await page.evaluate(() => calls.some(c => c.command === "find_remote")), false);
    await back.click(); await page.locator(".actions-grid").waitFor();
  });
}

test("non-Shield and disconnected headers keep the applicable controls without overflow", async t => {
  for (const setup of [{ deviceType: "google_tv" }, { connected: false }]) {
    const page = await open(t, { ...setup, scale: 2 }); const header = page.locator(".topline");
    assert.equal(await header.getByRole("button", { name: "Find remote" }).count(), 0);
    assert.equal(await header.getByRole("button", { name: "Back" }).count(), 1);
    assert.equal(await header.locator(".live-refresh-btn").count(), setup.connected === false ? 0 : 1);
    assert.equal(await header.evaluate(el => el.scrollWidth <= el.clientWidth), true);
  }
});

if (process.env.HEADER_EVIDENCE === "1") test("capture synthetic header layout and control continuity", async t => {
  await mkdir(dir, { recursive: true }); const page = await open(t, { width: 384, record: true });
  for (const [width, scale] of [[384, 1], [384, 2]]) {
    await page.setViewportSize({ width, height: 812 }); await page.evaluate(scale => { document.documentElement.style.fontSize = `${16 * scale}px`; }, scale);
    await page.screenshot({ path: `${dir}/header-${width}-${scale * 100}-simulated.png`, animations: "disabled" }); await page.waitForTimeout(700);
  }
  const live = page.locator(".live-refresh-btn"); await live.click(); assert.equal(await live.getAttribute("aria-pressed"), "true");
  await page.screenshot({ path: `${dir}/live-on-simulated.png`, animations: "disabled" }); await page.waitForTimeout(700); await live.click();
  await page.getByRole("button", { name: "Find remote", exact: true }).click();
  const confirm = page.getByRole("dialog", { name: "Ring the remote?", exact: true }); await confirm.waitFor();
  await page.screenshot({ path: `${dir}/remote-confirm-cancel-only-simulated.png`, animations: "disabled" }); await page.waitForTimeout(700);
  await confirm.getByRole("button", { name: "Cancel", exact: true }).click();
  assert.equal(await page.evaluate(() => calls.some(c => c.command === "find_remote")), false);
  const video = page.video(); await page.close(); await video.saveAs(`${dir}/header-layout-controls-simulated.webm`);
});
