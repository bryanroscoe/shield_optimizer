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

for (const [width, scale] of [[360, 1], [320, 2]]) test(`help opens in a bottom sheet by tap and keyboard, at ${width}px / ${scale * 100}% text`, async t => {
  const page = await open(t, "tweaks", width, scale);
  const trigger = page.getByRole("button", { name: "About System Hooks", exact: true });
  await trigger.waitFor();
  assert.equal(await trigger.getAttribute("aria-expanded"), "false");
  assert.equal(await trigger.getAttribute("aria-haspopup"), "dialog");
  assert.ok((await trigger.boundingBox()).height >= 44);
  const sheet = page.getByRole("dialog", { name: "About System Hooks" });
  const info = sheet.getByText(/Turning the hooks off can fix/);
  assert.equal(await sheet.count(), 0, "help is on demand, not in the screen");
  const before = await page.evaluate(() => document.documentElement.scrollHeight);
  for (let i = 0; i < 3; i++) {
    await trigger.tap(); await info.waitFor();
    assert.equal(await trigger.getAttribute("aria-expanded"), "true");
    if (i % 2) await page.touchscreen.tap(width / 2, 60); // above the sheet
    else await sheet.getByRole("button", { name: "Done", exact: true }).tap();
    await sheet.waitFor({ state: "detached" });
  }
  assert.equal(await page.evaluate(() => document.documentElement.scrollHeight), before, "opening help never grows the screen");
  await trigger.focus(); await page.keyboard.press("Space");
  await info.waitFor();
  await sheet.evaluate(el => Promise.all(el.getAnimations().map(a => a.finished)));
  assert.equal(await info.evaluate(el => parseFloat(getComputedStyle(el).fontSize)), 14 * scale);
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), true);
  const box = await sheet.boundingBox();
  assert.ok(box.x >= 0 && box.x + box.width <= width + 0.5, "sheet fits the width");
  if (evidence) await page.screenshot({ path: `${evidence}/tweaks-help-${width}-${scale * 100}-simulated.png` });
  await page.keyboard.press("Escape");
  await sheet.waitFor({ state: "detached" });
  assert.equal(await trigger.evaluate(el => el === document.activeElement), true, "focus returns to the help link");
  assert.equal(await trigger.evaluate(el => getComputedStyle(el).outlineStyle), "solid");
  // Risk and permission consequences stay beside the action rather than inside Help.
  const micRisk = page.getByText(/Off revokes Google's microphone permission/);
  await micRisk.scrollIntoViewIfNeeded(); assert.equal(await micRisk.isVisible(), true);
  assert.equal(await micRisk.evaluate(el => !!el.closest("details, [role=dialog]")), false);
  const audioRisk = page.getByText(/Manual overrides can cause silence/);
  assert.equal(await audioRisk.isVisible(), true);
  assert.equal(await audioRisk.evaluate(el => !!el.closest("details, [role=dialog]")), false);
  const longPressRisk = page.getByText("Shorter reacts sooner but makes accidental long presses more likely.", { exact: true });
  await longPressRisk.scrollIntoViewIfNeeded();
  assert.equal(await longPressRisk.isVisible(), true);
  assert.equal(await longPressRisk.evaluate(el => !!el.closest("details, [role=dialog]")), false);
  assert.equal(await page.getByRole("button", { name: "About long presses", exact: true }).getAttribute("aria-expanded"), "false");
  await page.goBack();
  await page.locator(".device-name").waitFor();
  await page.evaluate(() => router.navigate("tweaks"));
  await page.getByRole("heading", { name: "Tweaks", exact: true }).waitFor();
  assert.equal(await page.getByRole("dialog").count(), 0);
  if (evidence) {
    const video = page.video(); await page.context().close();
    await video.saveAs(`${evidence}/mobile-help-${width}-${scale * 100}-simulated.webm`);
  }
});

test("app descriptions remain available on tap while row safety stays visible", async t => {
  const page = await open(t, "apps");
  await page.getByRole("button", { name: "Using the app list", exact: true }).tap();
  const help = page.getByRole("dialog", { name: "Using the app list" });
  await help.getByText(/Tap for descriptions, consequences and safety details/).waitFor();
  await help.getByRole("button", { name: "Done", exact: true }).tap();
  await help.waitFor({ state: "detached" });
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
  const trigger = page.getByRole("button", { name: "How samples are measured", exact: true });
  assert.equal(await page.getByText(/VPN traffic can also appear/).count(), 0);
  await trigger.tap();
  await page.getByRole("dialog", { name: "How samples are measured" }).getByText(/VPN traffic can also appear/).waitFor();
  assert.equal(await page.getByText("No network counters available.", { exact: true }).count(), 0);
  assert.deepEqual(await page.evaluate(() => helpCalls.filter(command => ["resource_sample", "media_report"].includes(command))), []);
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
});

test("pinch zoom is not disabled by mobile viewport configuration", async () => {
  const html = await readFile(new URL("../index.html", import.meta.url), "utf8");
  assert.doesNotMatch(html, /user-scalable\s*=\s*no|maximum-scale\s*=\s*1/);
});
