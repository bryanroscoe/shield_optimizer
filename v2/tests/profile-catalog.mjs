import assert from "node:assert/strict";
import { readFile, mkdir } from "node:fs/promises";
import { stripTypeScriptTypes } from "node:module";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import { exerciseProfileCatalog, labelSyntheticCapture } from "../shared/profile-catalog-browser-test.mjs";

const root = fileURLToPath(new URL("../", import.meta.url));
const source = await readFile(join(root, "shared/profile-catalog.ts"), "utf8");
const helper = await import(`data:text/javascript;base64,${Buffer.from(stripTypeScriptTypes(source)).toString("base64")}`);
const data = JSON.parse(await readFile(join(root, "catalogs/cleanyourtv/catalog.json"), "utf8"));
const a = data.profiles.find((p) => p.profile === "acer-ar3-r3");
const b = data.profiles.find((p) => p.profile === "acer-ar32ap2841hdfl");
const pkg = "com.realtek.leanbacklauncher.partnercustomizer";
assert.equal(helper.matchingProfiles(data.profiles, "acer-ar3-r3", pkg)[0], a);
assert.deepEqual(helper.matchingProfiles(data.profiles, "unknown firmware and platform", pkg), []);
assert.ok(helper.packageObservations(a, pkg)[0].entries.every((entry) => entry.upstream_form === "comment_reference"));
assert.ok(helper.packageObservations(b, pkg)[0].entries.some((entry) => entry.upstream_form === "active_disable"));
assert.ok(helper.packageObservations(a, "com.example.only_on_another_model").length === 0);
assert.equal(data.actionable, false);
for (const profile of data.profiles) {
  assert.equal(profile.verified_model, null);
  assert.equal(profile.verified_firmware, null);
  assert.equal(profile.verified_platform, null);
}
assert.equal(JSON.parse(await readFile(join(root, "package.json"), "utf8")).license, "UNLICENSED");
assert.equal(JSON.parse(await readFile(join(root, "mobile/package.json"), "utf8")).license, "UNLICENSED");
assert.match(await readFile(join(root, "../LICENSE"), "utf8"), /All rights reserved/);

const priorDemo = process.env.VITE_DEMO;
process.env.VITE_DEMO = "1";
const evidence = process.env.SHIELD_CATALOG_EVIDENCE;
let server, browser, context, video;
try {
  if (evidence) await mkdir(evidence, { recursive: true });
  const { createServer } = await import("vite");
  const { chromium } = await import("playwright");
  server = await createServer({ root, server: { host: "127.0.0.1", port: 0, strictPort: false, hmr: false } });
  await server.listen();
  const address = server.httpServer.address();
  const base = `http://127.0.0.1:${address.port}`;
  browser = await chromium.launch();
  context = await browser.newContext({ viewport: { width: 1280, height: 920 }, colorScheme: "dark", ...(evidence ? { recordVideo: { dir: evidence, size: { width: 1280, height: 920 } } } : {}) });
  const page = await context.newPage();
  video = page.video();
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.route(/plugin-updater/, (r) => r.fulfill({ contentType: "application/javascript", body: "export async function check(){return null}" }));
  await page.route(/plugin-opener/, (r) => r.fulfill({ contentType: "application/javascript", body: "export async function openUrl(url){(window.openedSources??=[]).push(url)} export async function revealItemInDir(){}" }));
  await page.route(/plugin-process/, (r) => r.fulfill({ contentType: "application/javascript", body: "export async function relaunch(){}" }));
  await page.addInitScript(() => localStorage.clear());
  await page.goto(base, { waitUntil: "networkidle" });
  await page.getByRole("link", { name: "Catalog", exact: true }).click();
  await page.waitForURL("**/catalog");
  if (evidence) await labelSyntheticCapture(page);
  await exerciseProfileCatalog(page, () => page.evaluate(() => window.openedSources ?? []));
  if (evidence) {
    for (const theme of ["dark", "light"]) {
      await page.emulateMedia({ colorScheme: theme });
      for (const layout of [{ name: "", width: 1280, font: 16 }, { name: "minimum-", width: 720, font: 16 }, { name: "enlarged-", width: 720, font: 24 }]) {
        await page.setViewportSize({ width: layout.width, height: 980 });
        await page.addStyleTag({ content: `html{font-size:${layout.font}px}` });
        await page.evaluate(() => window.scrollTo(0, 0));
        await page.waitForTimeout(700);
        assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), `catalog fits ${layout.width}px with ${layout.font}px text in ${theme}`);
        await page.screenshot({ path: join(evidence, `desktop-catalog-${layout.name}${theme}.png`), fullPage: true });
      }
    }
  }
  await page.goto(`${base}/catalog?package=${encodeURIComponent(pkg)}`, { waitUntil: "networkidle" });
  assert.equal(await page.getByRole("searchbox", { name: "Search package IDs" }).inputValue(), pkg);
  assert.equal(await page.locator("[data-package-source]").count(), 0, "a package link still requires an explicit source profile");
  assert.deepEqual(errors, []);
  console.log("Profile catalog filtering, source context isolation, notices, license and desktop navigation passed");
} finally {
  await context?.close();
  if (video && evidence) await video.saveAs(join(evidence, "desktop-catalog-demo.webm"));
  await browser?.close();
  await server?.close();
  if (priorDemo === undefined) delete process.env.VITE_DEMO;
  else process.env.VITE_DEMO = priorDemo;
}
