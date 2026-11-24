import assert from "node:assert/strict";
import { test } from "node:test";
import { mkdir } from "node:fs/promises";
import { join } from "node:path";
import { chromium } from "playwright";
import { startViteServer } from "./helpers/vite-harness.mjs";

// The phone's Optimize offers the TV's CleanYourTV list the same way desktop
// does: suggested, not used until chosen, its rows on Optional apps under
// their own heading, starting on Keep with Disable as the only removal.
test("a chosen CleanYourTV list adds unselected Disable-only rows under their own heading", async () => {
  const previousDemo = process.env.VITE_DEMO;
  process.env.VITE_DEMO = "1";
  let server, browser;
  try {
    const started = await startViteServer();
    server = started.server;
    browser = await chromium.launch();
    const evidence = process.env.CLEANYOURTV_EVIDENCE;
    if (evidence) await mkdir(evidence, { recursive: true });
    const page = await browser.newPage({ viewport: { width: 412, height: 915 }, isMobile: true, hasTouch: true });
    const errors = [];
    page.on("pageerror", (error) => errors.push(error.message));
    await page.goto(started.origin, { waitUntil: "networkidle" });
    await page.evaluate(async () => {
      const { session } = await import("/src/lib/session.svelte.ts");
      const { router } = await import("/src/lib/router.svelte.ts");
      await session.connect("192.168.1.42", 5555);
      window.openedUrls = [];
      const invoke = window.__TAURI_INTERNALS__.invoke;
      window.__TAURI_INTERNALS__.invoke = async (command, args) => {
        if (command === "plugin:opener|open_url") {
          window.openedUrls.push(args.url);
          return null;
        }
        return invoke(command, args);
      };
      router.reset("dashboard");
      router.navigate("optimize");
    });
    const card = page.locator("[data-cleanyourtv-card]");
    await card.getByRole("button", { name: "Use this list" }).waitFor();
    assert.match(await card.innerText(), /NVIDIA SHIELD TV Pro 2019/);
    assert.equal(await page.locator("[data-cleanyourtv-rows]").count(), 0, "nothing from the list before it is chosen");
    if (evidence) await card.screenshot({ path: join(evidence, "mobile-card-suggested.png") });

    await card.getByRole("button", { name: "Use this list" }).click();
    await card.getByText(/1 more app from/).waitFor();
    await page.getByRole("button", { name: /^Optional/ }).click();
    const heading = page.locator("[data-cleanyourtv-rows]");
    await heading.waitFor();
    assert.match(await heading.innerText(), /From the CleanYourTV list for NVIDIA SHIELD TV Pro/);
    const row = page.locator(".optimize-item").filter({ has: page.getByText("android.autoinstalls.config.nvidia", { exact: true }) });
    await row.scrollIntoViewIfNeeded();
    assert.equal(await row.getByRole("button", { name: "Keep", exact: true }).getAttribute("aria-pressed"), "true");
    assert.equal(await row.getByRole("button", { name: "Disable", exact: true }).count(), 1);
    assert.equal(await row.getByRole("button", { name: /Uninstall/ }).count(), 0, "a list row never offers Uninstall");
    await row.locator(".tier-chip.unknown, .tier-chip").first().waitFor();
    const explain = row.getByRole("button", { name: /^If you remove it/ });
    if ((await explain.getAttribute("aria-expanded")) !== "true") await explain.click();
    assert.ok((await row.innerText()).includes("Play auto-installs of partner apps"), "the list author's note shows");
    await row.getByRole("button", { name: /^Line \d+ of the CleanYourTV list$/ }).click();
    const opened = await page.evaluate(() => window.openedUrls);
    assert.match(opened.at(-1) ?? "", /examples\/nvidia-shield-tv-pro-2019\/debloat\.txt#L\d+$/);
    for (const width of [412, 320]) {
      await page.setViewportSize({ width, height: 900 });
      await page.waitForTimeout(200);
      assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), `no sideways scroll at ${width}px`);
    }
    if (evidence) {
      await page.setViewportSize({ width: 412, height: 915 });
      await heading.scrollIntoViewIfNeeded();
      await page.waitForTimeout(300);
      await page.screenshot({ path: join(evidence, "mobile-optional-list-rows.png") });
      await card.scrollIntoViewIfNeeded();
      await card.screenshot({ path: join(evidence, "mobile-card-chosen.png") });
    }
    assert.deepEqual(errors, []);
  } finally {
    await browser?.close();
    await server?.close();
    if (previousDemo === undefined) delete process.env.VITE_DEMO;
    else process.env.VITE_DEMO = previousDemo;
  }
});
