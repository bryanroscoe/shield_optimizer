import assert from "node:assert/strict";
import { mkdir, readFile } from "node:fs/promises";
import { after, before, test } from "node:test";
import { chromium } from "playwright";
import { startViteServer } from "./helpers/vite-harness.mjs";

process.env.VITE_DEMO = "1";
const evidence = process.env.MOBILE_HELP_EVIDENCE;
let server, browser, origin;
before(async () => {
  ({ server, origin } = await startViteServer());
  browser = await chromium.launch();
  if (evidence) await mkdir(evidence, { recursive: true });
});
after(async () => { await browser?.close(); await server?.close(); });

async function open(t, screen, width = 360, scale = 1) {
  const context = await browser.newContext({ viewport: { width, height: 812 }, isMobile: true, hasTouch: true,
    ...(evidence && screen === "tweaks" ? { recordVideo: { dir: `${evidence}/raw`, size: { width, height: 812 } } } : {}) });
  const page = await context.newPage();
  page.setDefaultTimeout(10000);
  t.after(() => context.close());
  await page.addInitScript(() => { localStorage.clear(); localStorage.setItem("atv.autoConnect.v1", "0"); });
  await page.goto(origin, { waitUntil: "networkidle" });
  await page.evaluate(async ({ screen, scale }) => {
    const { session } = await import("/src/lib/session.svelte.ts");
    const { router } = await import("/src/lib/router.svelte.ts");
    window.router = router;
    await session.connect("192.168.1.42", 5555);
    router.reset("dashboard"); router.navigate(screen);
    const invoke = window.__TAURI_INTERNALS__.invoke;
    window.helpCalls = [];
    window.__TAURI_INTERNALS__.invoke = (command, args) => {
      window.helpCalls.push(command);
      return invoke(command, args);
    };
    if (scale > 1) document.documentElement.style.fontSize = `${16 * scale}px`;
    const label = document.createElement("div");
    label.textContent = "SIMULATION · demo fixtures · no phone or TV";
    label.style.cssText = "position:fixed;inset:0 0 auto;z-index:99999;background:#182238;color:white;padding:4px;font:10px sans-serif;text-align:center";
    document.body.append(label);
  }, { screen, scale });
  await page.getByRole("heading", { name: screen === "tweaks" ? "Tweaks" : screen === "apps" ? "Apps" : "Diagnostics", exact: true }).waitFor();
  return page;
}

for (const [width, scale] of [[360, 1], [320, 2]]) test(`tap and keyboard help stays local and wraps at ${width}px / ${scale * 100}% text`, async t => {
  const page = await open(t, "tweaks", width, scale);
  const details = page.locator("details.help-disclosure").filter({ has: page.locator("summary", { hasText: "About System Hooks" }) });
  const summary = details.locator("summary");
  await summary.waitFor();
  assert.equal(await details.getAttribute("open"), null);
  assert.ok((await summary.boundingBox()).height >= 44);
  const info = details.locator(".t-note");
  assert.equal(await info.isVisible(), false);
  for (let i = 0; i < 3; i++) {
    await summary.tap(); assert.equal(await info.isVisible(), true);
    await summary.tap(); assert.equal(await info.isVisible(), false);
  }
  await summary.focus(); await page.keyboard.press("Space");
  assert.equal(await info.isVisible(), true);
  assert.equal(await info.evaluate(el => parseFloat(getComputedStyle(el).fontSize)), 13 * scale);
  assert.equal(await summary.evaluate(el => el === document.activeElement), true);
  assert.equal(await summary.evaluate(el => getComputedStyle(el).outlineStyle), "solid");
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), true);
  // Risk and permission consequences stay beside the action rather than inside Help.
  const micRisk = page.getByText(/Off revokes Google's microphone permission/);
  await micRisk.scrollIntoViewIfNeeded(); assert.equal(await micRisk.isVisible(), true);
  assert.equal(await micRisk.evaluate(el => !!el.closest("details")), false);
  const audioRisk = page.getByText(/Manual overrides can cause silence/);
  assert.equal(await audioRisk.isVisible(), true);
  assert.equal(await audioRisk.evaluate(el => !!el.closest("details")), false);
  if (evidence) {
    await summary.scrollIntoViewIfNeeded();
    await page.screenshot({ path: `${evidence}/tweaks-help-${width}-${scale * 100}-simulated.png` });
  }
  await summary.focus(); await page.keyboard.press("Enter"); assert.equal(await info.isVisible(), false);
  await page.goBack();
  await page.locator(".device-name").waitFor();
  await page.evaluate(() => router.navigate("tweaks"));
  await page.getByRole("heading", { name: "Tweaks", exact: true }).waitFor();
  assert.equal(await page.getByText("About System Hooks", { exact: true }).evaluate(el => el.parentElement.open), false);
  if (evidence) {
    const video = page.video(); await page.context().close();
    await video.saveAs(`${evidence}/mobile-help-${width}-${scale * 100}-simulated.webm`);
  }
});

test("app descriptions remain available on tap while row safety stays visible", async t => {
  const page = await open(t, "apps");
  const help = page.getByText("Using the app list", { exact: true });
  await help.tap(); await page.getByText(/Tap for descriptions, consequences and safety details/).waitFor();
  await help.tap();
  const row = page.locator('.app-row[data-package="com.google.android.videos"]');
  await row.waitFor();
  assert.equal(await row.locator(".app-desc").count(), 0);
  assert.ok(await row.locator(".verdict-chip").isVisible());
  await row.tap();
  await page.locator(".sheet .reason-text").waitFor();
  const entries = JSON.parse(await readFile(new URL("../../crates/core/data/app-lists/common.json", import.meta.url), "utf8"));
  const description = entries.find(entry => entry.package === "com.google.android.videos").optimize_description;
  assert.ok((await page.locator(".sheet").innerText()).includes(description));
});

test("playback uncertainty remains visible and optional measurement context does not sample", async t => {
  const page = await open(t, "diagnostics");
  await page.getByText("Advertised configuration, not a playback test.", { exact: true }).waitFor();
  const summary = page.getByText("How samples are measured", { exact: true });
  const details = summary.locator("..");
  assert.equal(await details.getAttribute("open"), null);
  await summary.tap();
  await page.getByText(/VPN traffic can also appear/).waitFor();
  assert.equal(await page.getByText("No network counters available.", { exact: true }).count(), 0);
  assert.deepEqual(await page.evaluate(() => helpCalls.filter(command => ["resource_sample", "media_report"].includes(command))), []);
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
});

test("pinch zoom is not disabled by mobile viewport configuration", async () => {
  const html = await readFile(new URL("../index.html", import.meta.url), "utf8");
  assert.doesNotMatch(html, /user-scalable\s*=\s*no|maximum-scale\s*=\s*1/);
});
