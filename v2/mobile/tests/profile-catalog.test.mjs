import assert from "node:assert/strict";
import { test } from "node:test";
import { mkdir } from "node:fs/promises";
import { join } from "node:path";
import { chromium } from "playwright";
import { startViteServer } from "./helpers/vite-harness.mjs";
import { exerciseProfileCatalog, labelSyntheticCapture } from "../../shared/profile-catalog-browser-test.mjs";

test("mobile catalog exposes attributed observations without package mutations", async () => {
  const previousDemo = process.env.VITE_DEMO;
  process.env.VITE_DEMO = "1";
  const evidence = process.env.SHIELD_CATALOG_EVIDENCE;
  let server, browser, context, video;
  try {
    if (evidence) await mkdir(evidence, { recursive: true });
    const started = await startViteServer();
    server = started.server;
    browser = await chromium.launch();
    context = await browser.newContext({ viewport: { width: 412, height: 915 }, isMobile: true, hasTouch: true,
      ...(evidence ? { recordVideo: { dir: evidence, size: { width: 412, height: 915 } } } : {}) });
    const page = await context.newPage();
    video = page.video();
    const errors = [];
    page.on("pageerror", (error) => errors.push(error.message));
    await page.goto(started.origin, { waitUntil: "networkidle" });
    await page.evaluate(async () => {
      const { session } = await import("/src/lib/session.svelte.ts");
      const { router } = await import("/src/lib/router.svelte.ts");
      await session.connect("192.168.1.42", 5555);
      router.reset("dashboard");
      router.navigate("more");
      window.catalogCalls = [];
      const invoke = window.__TAURI_INTERNALS__.invoke;
      window.__TAURI_INTERNALS__.invoke = async (command, args) => {
        window.catalogCalls.push({ command, args });
        return invoke(command, args);
      };
    });
    await page.getByRole("button", { name: /Device & package catalog/ }).click();
    if (evidence) await labelSyntheticCapture(page);
    await exerciseProfileCatalog(page, () => page.evaluate(() => window.catalogCalls.filter((c) => c.command === "plugin:opener|open_url").map((c) => c.args.url)));
    const writes = await page.evaluate(() => window.catalogCalls.filter((c) => /^(disable_package|uninstall_package|apply_|optimize|set_home|shell_run)/.test(c.command)));
    assert.deepEqual(writes, []);
    if (evidence) {
      await page.evaluate(() => window.scrollTo(0, 0));
      await page.waitForTimeout(900);
      await page.screenshot({ path: join(evidence, "mobile-catalog.png"), fullPage: true });
      await page.setViewportSize({ width: 320, height: 800 });
      await page.addStyleTag({ content: "html{font-size:22px}" });
      assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), "mobile catalog fits narrow and enlarged text");
      await page.screenshot({ path: join(evidence, "mobile-catalog-narrow.png"), fullPage: true });
      await page.waitForTimeout(900);
    }
    await page.getByRole("button", { name: "← Back", exact: true }).click();
    await page.getByRole("button", { name: /Device & package catalog/ }).waitFor();
    assert.deepEqual(errors, []);
  } finally {
    await context?.close();
    if (video && evidence) await video.saveAs(join(evidence, "mobile-catalog-demo.webm"));
    await browser?.close();
    await server?.close();
    if (previousDemo === undefined) delete process.env.VITE_DEMO;
    else process.env.VITE_DEMO = previousDemo;
  }
});
