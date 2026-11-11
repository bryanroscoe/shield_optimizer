// Badges that explain themselves ($lib/tip) must not be hover-only: the
// explanation opens on click, tap and keyboard focus, Escape dismisses it, and
// it is the badge's accessible description. Clicking a badge inside an App
// List row shows its tip without toggling the row's detail.
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
    const words = document.getElementById(el.getAttribute("aria-describedby") ?? "")?.textContent;
    return bubble.textContent === words && r.width > 20 && r.height > 8;
  });

async function checkBadges(page, scope, label) {
  const badges = page.locator(`${scope} [data-tip-focus]`);
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
    await checkBadges(page, "#tabpanel-apps", "App List");

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

    // Tab from the control just before it lands on it.
    await review.evaluate((badge) => {
      const order = [...document.querySelectorAll("button, a[href], input, select, textarea, [tabindex]")]
        .filter((el) => el.tabIndex >= 0 && !el.disabled && el.offsetParent !== null);
      order[order.indexOf(badge) - 1].focus();
    });
    await page.keyboard.press("Tab");
    assert.ok(await review.evaluate((el) => el === document.activeElement), "keyboard reaches the badge");
    assert.ok(await tipShown(review), "keyboard focus opens the tip");

    await page.locator("#tab-health").click();
    await page.locator("#tabpanel-health .unconfirmed").first().waitFor();
    await checkBadges(page, "#tabpanel-health", "Health");
    const process = page.locator("#tabpanel-health .unconfirmed").first();
    await process.click();
    assert.ok(await tipShown(process), "a click opens 'not a package'");

    console.log("Badge tips passed: App List and Health badges are focusable, described, open on click and keyboard, and close on Escape.");
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
