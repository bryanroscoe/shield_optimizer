import assert from "node:assert/strict";
import { test } from "node:test";
import { readFile, mkdir } from "node:fs/promises";
import { join } from "node:path";
import { chromium } from "playwright";
import { startViteServer } from "./helpers/vite-harness.mjs";

const reviewed = JSON.parse(await readFile(new URL("../../tests/fixtures/reviewed-optimize-packages.json", import.meta.url), "utf8"));

test("reviewed source leads appear in mobile Optimize as unchecked Caution choices", async () => {
  const previousDemo = process.env.VITE_DEMO;
  process.env.VITE_DEMO = "1";
  let server, browser;
  try {
    const started = await startViteServer();
    server = started.server;
    browser = await chromium.launch();
    const evidence = process.env.OPTIMIZE_CATALOG_EVIDENCE;
    if (evidence) await mkdir(evidence, { recursive: true });
    const page = await browser.newPage({ viewport: { width: 412, height: 915 }, isMobile: true, hasTouch: true,
      ...(evidence ? { recordVideo: { dir: join(evidence, "raw"), size: { width: 412, height: 915 } } } : {}) });
    const errors = [];
    page.on("pageerror", (error) => errors.push(error.message));
    await page.addInitScript(() => localStorage.setItem("shieldopt.demo.reviewedCatalog", "1"));
    await page.goto(started.origin, { waitUntil: "networkidle" });
    if (evidence) await page.evaluate(() => {
      const label = document.createElement("div");
      label.textContent = "SIMULATED Optimize · no TV or removal";
      label.style.cssText = "position:fixed;top:0;right:0;z-index:99999;padding:8px;background:#1a2540;color:white;font:14px sans-serif";
      document.body.appendChild(label);
    });
    await page.evaluate(async () => {
      const { session } = await import("/src/lib/session.svelte.ts");
      const { router } = await import("/src/lib/router.svelte.ts");
      await session.connect("192.168.1.42", 5555);
      window.optimizeReviewCalls = [];
      const invoke = window.__TAURI_INTERNALS__.invoke;
      window.__TAURI_INTERNALS__.invoke = async (command, args) => {
        window.optimizeReviewCalls.push(command);
        return invoke(command, args);
      };
      router.reset("dashboard");
      router.navigate("optimize");
    });
    await page.getByRole("button", { name: /^Optional/ }).click();
    for (const entry of reviewed) {
      const row = page.locator(".optimize-item").filter({ has: page.getByText(entry.package, { exact: true }) });
      await row.scrollIntoViewIfNeeded();
      await row.locator(".tier-chip.caution").waitFor();
      assert.equal(await row.getByRole("button", { name: "Keep", exact: true }).getAttribute("aria-pressed"), "true", entry.package);
      assert.equal(await row.getByRole("button", { name: "Disable", exact: true }).getAttribute("aria-pressed"), "false", entry.package);
      await row.getByText("Review: disable if unused", { exact: true }).waitFor();
      for (const fragment of entry.consequence_fragments ?? []) {
        assert.ok((await row.innerText()).includes(fragment), `visible consequence: ${entry.package}: ${fragment}`);
      }
    }
    const focusPackage = process.env.OPTIMIZE_CATALOG_FOCUS || reviewed[0].package;
    assert.ok(reviewed.some((entry) => entry.package === focusPackage), "evidence focus must be a reviewed package");
    const first = page.locator(".optimize-item").filter({ has: page.getByText(focusPackage, { exact: true }) });
    if (evidence) {
      await first.scrollIntoViewIfNeeded();
      await page.waitForTimeout(600);
    }
    await first.getByRole("button", { name: "Disable", exact: true }).click();
    assert.equal(await first.getByRole("button", { name: "Disable", exact: true }).getAttribute("aria-pressed"), "true");
    if (evidence) await page.waitForTimeout(400);
    await first.getByRole("button", { name: "Keep", exact: true }).click();
    await page.getByRole("button", { name: /^Recommended/ }).click();
    for (const entry of reviewed) assert.equal(await page.getByRole("button", { name: `Details for ${entry.name}`, exact: true }).count(), 0);
    await page.getByRole("button", { name: /^Optional/ }).click();
    if (evidence) {
      await first.scrollIntoViewIfNeeded();
      await page.waitForTimeout(400);
      await first.scrollIntoViewIfNeeded();
      await page.waitForTimeout(400);
      await page.screenshot({ path: join(evidence, "mobile-optimize-reviewed.png") });
      await page.setViewportSize({ width: 320, height: 800 });
      await page.addStyleTag({ content: "html{font-size:22px}" });
      await first.scrollIntoViewIfNeeded();
      await page.waitForTimeout(400);
      await first.scrollIntoViewIfNeeded();
      await page.waitForTimeout(400);
      assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), "new optional choices fit narrow enlarged text");
      await page.screenshot({ path: join(evidence, "mobile-optimize-narrow.png") });
    }
    assert.deepEqual(await page.evaluate(() => window.optimizeReviewCalls.filter((c) => /^(disable_package|uninstall_package|enable_package|apply_)/.test(c))), []);
    assert.deepEqual(errors, []);
    const video = page.video();
    await page.close();
    if (evidence) await video.saveAs(join(evidence, "mobile-optimize-reviewed.webm"));
  } finally {
    await browser?.close();
    await server?.close();
    if (previousDemo === undefined) delete process.env.VITE_DEMO;
    else process.env.VITE_DEMO = previousDemo;
  }
});
