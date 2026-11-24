import assert from "node:assert/strict";
import { test } from "node:test";
import { mkdir } from "node:fs/promises";
import { join } from "node:path";
import { chromium } from "playwright";
import { startViteServer } from "./helpers/vite-harness.mjs";

// The phone's Optimize ranks the TV's CleanYourTV list after the catalog the
// same way desktop does: its rows on Optional apps under the list's name,
// after the catalog's, starting on Keep with Disable as the only removal.
test("the TV's CleanYourTV list adds unselected Disable-only rows under its name", async () => {
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
    const heading = page.locator('[data-cleanyourtv-rows="nvidia-shield-tv-pro-2019"]');
    // The codename's list is used without asking, ranked after the catalog.
    await page.waitForFunction(() =>
      document.querySelector("[data-cleanyourtv-card]")?.textContent?.replace(/\s+/g, " ")
        .includes("Sources: ATV Optimizer catalog, then CleanYourTV's list for NVIDIA SHIELD TV Pro 2019"));
    if (evidence) await card.screenshot({ path: join(evidence, "mobile-sources.png") });
    await page.getByRole("button", { name: /^Optional/ }).click();
    await heading.waitFor();
    assert.match(await heading.innerText(), /CleanYourTV list for NVIDIA SHIELD TV Pro 2019/);
    assert.match(await heading.innerText(), /Community list, not reviewed by ATV Optimizer · Disable only/);
    assert.match(await page.locator(".listed-heading").first().innerText(), /^ATV Optimizer catalog/, "the catalog comes first");
    const row = page.locator(".optimize-item").filter({ has: page.getByText("android.autoinstalls.config.nvidia", { exact: true }) });
    await row.scrollIntoViewIfNeeded();
    assert.equal(await row.getByRole("button", { name: "Keep", exact: true }).getAttribute("aria-pressed"), "true");
    assert.equal(await row.getByRole("button", { name: "Disable", exact: true }).count(), 1);
    assert.equal(await row.getByRole("button", { name: /Uninstall/ }).count(), 0, "a list row never offers Uninstall");
    const explain = row.getByRole("button", { name: /^If you remove it/ });
    if ((await explain.getAttribute("aria-expanded")) !== "true") await explain.click();
    assert.ok((await row.innerText()).includes("List author's note: Play auto-installs of partner apps"), "the list author's note shows");
    await heading.getByRole("button", { name: "View list" }).click();
    const opened = await page.evaluate(() => window.openedUrls);
    assert.match(opened.at(-1) ?? "", /examples\/nvidia-shield-tv-pro-2019\/debloat\.txt$/);
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
    }
    assert.deepEqual(errors, []);
  } finally {
    await browser?.close();
    await server?.close();
    if (previousDemo === undefined) delete process.env.VITE_DEMO;
    else process.env.VITE_DEMO = previousDemo;
  }
});
