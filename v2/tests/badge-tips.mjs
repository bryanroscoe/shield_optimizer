// Badges that explain themselves ($lib/tip) must not be hover-only: the
// explanation opens on click, tap and keyboard focus, and Escape dismisses it.
// One-off badges are Tab stops with the explanation as their accessible
// description. Per-row App List badges (REVIEW, RAM, last used) are not: a
// long list would gain a Tab stop per row and a screen reader would repeat the
// sentence on every row, so each column header carries one keyboard-reachable
// HelpTip instead. Clicking a badge inside a row shows its tip without
// toggling the row's detail.
//
// Run via `node tests/badge-tips.mjs` from v2/.
import { freePort } from "./lib/free-port.mjs";
import assert from "node:assert/strict";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const V2 = join(dirname(fileURLToPath(import.meta.url)), "..");
const SERIAL = "192.168.1.42:5555";

// The bubble is shared and fixed to the viewport; it is showing this badge's
// words, and it is not clipped to nothing.
const tipShown = (locator) =>
  locator.evaluate((el) => {
    const bubble = document.querySelector(".tip-bubble");
    if (!bubble || bubble.hidden) return false;
    const r = bubble.getBoundingClientRect();
    return bubble.textContent === el.dataset.tipText && r.width > 20 && r.height > 8;
  });

async function checkBadges(page, scope, label) {
  const badges = page.locator(`${scope} [data-tip-focus=""]`);
  const count = await badges.count();
  assert.ok(count > 0, `${label}: has explained badges`);
  const facts = await badges.evaluateAll((all) =>
    all.map((el) => ({
      tabindex: el.getAttribute("tabindex"),
      described: document.getElementById(el.getAttribute("aria-describedby") ?? "")?.textContent ?? null,
    })),
  );
  for (const f of facts) {
    assert.equal(f.tabindex, "0", `${label}: badge is focusable ${JSON.stringify(f)}`);
    assert.ok(f.described && f.described.length > 10, `${label}: the tip is the accessible description ${JSON.stringify(f)}`);
  }
  return { badges, count };
}

// Tab stops inside a table body that are not real controls.
const extraTabStops = (table) =>
  table.evaluate(
    (t) =>
      [...t.querySelectorAll("tbody [tabindex]")].filter(
        (el) => el.tabIndex >= 0 && !el.matches("button, a[href], input, select, textarea"),
      ).length,
  );

// Per body row: every Tab stop, and the Tab stops that are real controls.
// Badges must add nothing, so the two are equal however many badges a row has.
const rowTabStops = (table) =>
  table.evaluate((t) =>
    [...t.querySelectorAll("tbody > tr")]
      .filter((row) => !row.classList.contains("safety-detail-row"))
      .map((row) => {
        const stops = [...row.querySelectorAll("button, a[href], input, select, textarea, [tabindex]")].filter(
          (el) => el.tabIndex >= 0 && !el.disabled,
        );
        return {
          badges: row.querySelectorAll("[data-tip-focus]").length,
          stops: stops.length,
          controls: stops.filter((el) => el.matches("button, a[href], input, select, textarea")).length,
        };
      }),
  );

async function main() {
  const previous = new Map(["VITE_DEMO", "TAURI_DEV_HOST"].map((key) => [key, process.env[key]]));
  process.env.VITE_DEMO = "1";
  delete process.env.TAURI_DEV_HOST;
  let server;
  let browser;
  try {
    const { createServer } = await import("vite");
    const { chromium } = await import("playwright");
    server = await createServer({ root: V2, server: { host: "127.0.0.1", port: await freePort(), strictPort: true, hmr: false } });
    await server.listen();
    browser = await chromium.launch();
    const page = await browser.newPage({ viewport: { width: 1280, height: 1000 } });
    page.setDefaultTimeout(10000);
    const base = `http://127.0.0.1:${server.httpServer.address().port}`;

    await page.goto(`${base}/devices/${encodeURIComponent(SERIAL)}`, { waitUntil: "networkidle" });
    await page.locator("#tab-apps").click();
    const panel = page.locator("#tabpanel-apps");
    await panel.locator(".tag.review").first().waitFor();
    const table = panel.locator("table.app-table").first();
    const rowBadges = await panel.locator('[data-tip-focus="row"]').evaluateAll((all) =>
      all.map((el) => ({ tabindex: el.getAttribute("tabindex"), describedby: el.getAttribute("aria-describedby") })),
    );
    assert.ok(rowBadges.length > 3, "App List: rows carry explained badges");
    for (const b of rowBadges) {
      assert.deepEqual(b, { tabindex: "-1", describedby: null }, "a row badge is neither a Tab stop nor re-read per row");
    }
    assert.equal(await extraTabStops(table), 0, "App List rows add no Tab stops beyond their controls");
    const rows = await rowTabStops(table);
    const badgeRows = rows.filter((r) => r.badges > 0);
    assert.ok(badgeRows.length > 10, "many rows have badges");
    for (const r of badgeRows) {
      assert.equal(r.stops, r.controls, `a row's badges add no Tab stops ${JSON.stringify(r)}`);
    }

    const review = panel.locator(".tag.review").first();
    assert.equal(await tipShown(review), false, "tip starts hidden");
    await review.click();
    assert.ok(await tipShown(review), "a click opens the tip");
    assert.equal(await panel.locator(".safety-detail-row").count(), 0, "clicking the badge does not open the row");
    // Move the pointer off so only focus could be holding it open.
    await page.mouse.move(5, 5);
    assert.ok(await tipShown(review), "the tip stays open while the badge has focus");
    await page.keyboard.press("Escape");
    assert.equal(await tipShown(review), false, "Escape dismisses the tip");

    // The keyboard explanation is one HelpTip per column header.
    const heads = table.locator("thead .help-trigger");
    assert.deepEqual(
      await heads.evaluateAll((all) => all.map((el) => el.getAttribute("aria-label"))),
      ["About App", "About RAM", "About Last used"],
      "App, RAM and Last used each explain themselves once, in the header",
    );
    const usageHelp = table.locator('thead button[aria-label="About Last used"]');
    await usageHelp.evaluate((trigger) => {
      const order = [...document.querySelectorAll("button, a[href], input, select, textarea, [tabindex]")]
        .filter((el) => el.tabIndex >= 0 && !el.disabled && el.offsetParent !== null);
      order[order.indexOf(trigger) - 1].focus();
    });
    await page.keyboard.press("Tab");
    assert.ok(await usageHelp.evaluate((el) => el === document.activeElement), "Tab reaches the Last used help");
    await page.keyboard.press("Enter");
    const popover = page.locator(`#${await usageHelp.getAttribute("aria-controls")}`);
    await popover.waitFor({ state: "visible" });
    assert.match(await popover.innerText(), /about a year|roughly a year/i, "the header help explains the usage badges");
    const box = await popover.boundingBox();
    assert.ok(box && box.x >= 0 && box.x + box.width <= 1280, "the header help stays in the window");
    await page.keyboard.press("Escape");
    await popover.waitFor({ state: "hidden" });
    assert.ok(await usageHelp.evaluate((el) => el === document.activeElement), "Escape returns focus to the header help");

    await page.locator("#tab-health").click();
    await page.locator("#tabpanel-health .unconfirmed").first().waitFor();
    await checkBadges(page, "#tabpanel-health", "Health");
    const process = page.locator("#tabpanel-health .unconfirmed").first();
    await process.click();
    assert.ok(await tipShown(process), "a click opens 'not a package'");

    console.log("Badge tips passed: App List row badges add no Tab stops and open on click, the column headers explain them by keyboard, and Health badges are focusable and described.");
  } finally {
    await browser?.close().catch((e) => console.error("browser cleanup failed", e));
    await server?.close().catch((e) => console.error("Vite cleanup failed", e));
    for (const [key, value] of previous) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
