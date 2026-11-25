// After an Optimize run, a TV still on its stock Home app gets one quiet link to
// the Launcher tab. It never shows before a run, never for a TV already on
// another Home app or one whose Home app can't be read, and once dismissed it
// stays gone for that TV's hardware id (never its address).
//
// SIMULATED: demo fixtures with current_launcher overridden, no TV.
import { freePort } from "./lib/free-port.mjs";
import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";
import { chromium } from "playwright";

const V2 = join(dirname(fileURLToPath(import.meta.url)), "..");
const evidence = process.env.HOME_OFFER_EVIDENCE;
process.env.VITE_DEMO = "1";
delete process.env.TAURI_DEV_HOST;

const STOCK = "com.google.android.tvlauncher";
const CUSTOM = "com.spocky.projengmenu";
const KEY = "atvopt.homeScreenOfferDismissed";

function memoryStorage(fail = false) {
  const map = new Map();
  return {
    getItem: (k) => (map.has(k) ? map.get(k) : null),
    setItem: (k, v) => {
      if (fail) throw new Error("quota");
      map.set(k, String(v));
    },
    removeItem: (k) => map.delete(k),
    map,
  };
}

async function moduleChecks(server) {
  const { onStockHome, homeOfferDismissed, dismissHomeOffer } = await server.ssrLoadModule("/shared/home-screen-offer.ts");
  const rows = [
    { entry: { package: STOCK }, stock: true, setup_helper: false },
    { entry: { package: CUSTOM }, stock: false, setup_helper: false },
    { entry: { package: "com.google.android.tungsten.setupwraith" }, stock: false, setup_helper: true },
  ];
  assert.equal(onStockHome(rows, STOCK), true);
  assert.equal(onStockHome(rows, CUSTOM), false, "another Home app is already a choice");
  assert.equal(onStockHome(rows, "com.google.android.tungsten.setupwraith"), null, "Setup Wraith claims nothing");
  assert.equal(onStockHome(rows, "not.listed"), null, "a Home app outside the list is unknown");
  assert.equal(onStockHome(rows, null), null, "an unreadable Home app is unknown");
  assert.equal(onStockHome(null, STOCK), null, "no list is unknown");

  const storage = memoryStorage();
  assert.equal(homeOfferDismissed(storage, "ABC123"), false);
  assert.equal(dismissHomeOffer(storage, " ABC123 "), true);
  assert.equal(homeOfferDismissed(storage, "ABC123"), true, "remembered under the trimmed hardware id");
  assert.equal(homeOfferDismissed(storage, "XYZ999"), false, "another TV is not dismissed");
  for (const id of [null, undefined, "", "  ", "unknown", "UNKNOWN"]) {
    assert.equal(dismissHomeOffer(storage, id), false, `no id (${JSON.stringify(id)}) is never remembered`);
    assert.equal(homeOfferDismissed(storage, id), false);
  }
  assert.deepEqual(JSON.parse(storage.getItem(KEY)), ["ABC123"], "only hardware ids are stored, never addresses");
  storage.setItem(KEY, "{not json");
  assert.equal(homeOfferDismissed(storage, "ABC123"), false, "unreadable storage dismisses nothing");
  assert.equal(dismissHomeOffer(memoryStorage(true), "ABC123"), false, "a refused write reports false");
  console.log("Home screen offer module passed: stock/custom/unknown Home, per-id dismissal, no-id and storage failures.");
}

async function openOptimize(page, base, homeNow) {
  await page.addInitScript(({ homeNow }) => {
    window.homeNow = homeNow;
    window.offerCalls = [];
    let internals;
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true, get: () => internals,
      set(value) {
        const original = value.invoke.bind(value);
        value.invoke = async (command, args) => {
          window.offerCalls.push(command);
          if (command === "current_launcher") {
            if (window.homeNow === "error") throw new Error("SIMULATED unreadable Home app");
            return { package: window.homeNow, activity: ".Main" };
          }
          return original(command, args);
        };
        internals = value;
      },
    });
  }, { homeNow });
  await page.goto(base, { waitUntil: "networkidle" });
  await page.getByText("NVIDIA SHIELD", { exact: false }).first().click();
  await page.getByRole("tab", { name: "Optimize" }).click();
  const panel = page.locator("#tabpanel-optimize");
  await panel.locator("table.optimize-table tbody tr").first().waitFor();
  await page.waitForFunction(() => {
    const cells = [...document.querySelectorAll("#tabpanel-optimize td.verdict-cell")];
    return cells.length > 0 && cells.every((c) => c.dataset.verdict !== "checking");
  }, null, { timeout: 15000 });
  return panel;
}

async function runPlan(page, panel) {
  const armed = await panel.evaluate((root) => [...root.querySelectorAll("table.optimize-table tbody tr")]
    .some((tr) => {
      const label = tr.querySelector(".action-radio .radio-pill.active .radio-label")?.textContent?.trim();
      return label && label !== "Keep";
    }));
  if (!armed) {
    await panel.locator("tr").filter({ has: page.locator(".radio-label", { hasText: /^Disable$/ }) }).first()
      .locator(".radio-pill", { has: page.locator(".radio-label", { hasText: /^Disable$/ }) }).click();
  }
  await panel.getByRole("button", { name: /^Run plan/ }).click();
  const dialog = page.locator("[data-run-plan-dialog]");
  await dialog.waitFor();
  await dialog.getByRole("button", { name: "Run Optimize", exact: true }).click();
  await panel.getByText(/Optimize complete/).waitFor({ timeout: 15000 });
}

async function settle(page) {
  await page.waitForFunction(() => window.offerCalls.length > 0);
  await page.waitForTimeout(300);
}

let server, browser;
try {
  server = await createServer({ root: V2, logLevel: "error", server: { host: "127.0.0.1", port: await freePort(), strictPort: true, hmr: false } });
  await server.listen();
  await moduleChecks(server);
  const base = `http://127.0.0.1:${server.httpServer.address().port}`;
  browser = await chromium.launch();
  if (evidence) await mkdir(evidence, { recursive: true });

  // Stock Home: the link appears only after the run, opens Launcher, and Dismiss sticks for this TV.
  {
    const context = await browser.newContext({ viewport: { width: 1280, height: 900 },
      ...(evidence ? { recordVideo: { dir: join(evidence, "raw"), size: { width: 1280, height: 900 } } } : {}) });
    const page = await context.newPage();
    const panel = await openOptimize(page, base, STOCK);
    if (evidence) await page.evaluate(() => {
      const label = document.createElement("div");
      label.textContent = "SIMULATED · demo fixtures, Home overridden to stock, no TV";
      label.style.cssText = "position:fixed;bottom:0;right:0;z-index:99999;padding:6px 10px;background:#1a2540;color:white;font:13px sans-serif";
      document.body.append(label);
    });
    const offer = panel.locator(".home-offer");
    assert.equal(await offer.count(), 0, "nothing before a run, so nothing on connect");
    assert.equal(await page.evaluate(() => window.offerCalls.includes("current_launcher")), false, "Home isn't read before a run");

    await runPlan(page, panel);
    await offer.waitFor();
    assert.match(await offer.innerText(), /Still on the stock home screen\./);
    const link = offer.getByRole("button", { name: "Get a better home screen", exact: true });
    const dismiss = offer.getByRole("button", { name: "Dismiss", exact: true });
    assert.ok(await link.isVisible());
    if (evidence) {
      await page.evaluate(() => document.documentElement.setAttribute("data-theme", "dark"));
      await offer.evaluate((el) => el.scrollIntoView({ block: "center" }));
      await page.screenshot({ path: join(evidence, "offer-in-context-1280-dark.png") });
    }

    for (const scheme of ["dark", "light"]) {
      await page.evaluate((s) => document.documentElement.setAttribute("data-theme", s), scheme);
      for (const [width, size] of [[1280, "100%"], [720, "100%"], [720, "150%"]]) {
        await page.setViewportSize({ width, height: 900 });
        await page.evaluate((s) => (document.documentElement.style.fontSize = s), size);
        await offer.scrollIntoViewIfNeeded();
        const box = await offer.boundingBox();
        assert.ok(box.x >= 0 && box.x + box.width <= width, `${scheme} ${width}px ${size}: the offer fits`);
        const clipped = await offer.evaluate((el) => [...el.querySelectorAll("span, button")]
          .filter((n) => n.scrollWidth > n.clientWidth + 1).map((n) => n.textContent.trim()));
        assert.deepEqual(clipped, [], `${scheme} ${width}px ${size}: nothing is clipped`);
        const [l, d] = [await link.boundingBox(), await dismiss.boundingBox()];
        assert.ok(l.width > 0 && d.width > 0);
        if (evidence) await offer.screenshot({ path: join(evidence, `offer-${width}-${size.replace("%", "")}-${scheme}.png`) });
      }
    }
    await page.evaluate(() => (document.documentElement.style.fontSize = ""));
    await page.setViewportSize({ width: 1280, height: 900 });

    await link.focus();
    await page.keyboard.press("Enter");
    await page.locator("#tabpanel-launcher").waitFor();
    assert.equal(await page.getByRole("tab", { name: "Launcher" }).getAttribute("aria-selected"), "true");
    if (evidence) await page.screenshot({ path: join(evidence, "after-link-launcher-tab.png") });

    await page.getByRole("tab", { name: "Optimize" }).click();
    await offer.waitFor();
    await dismiss.click();
    assert.equal(await offer.count(), 0, "Dismiss hides it");
    assert.deepEqual(await page.evaluate((k) => JSON.parse(localStorage.getItem(k)), KEY), ["0323220012345"],
      "the dismissal is filed under the Shield's hardware id");

    await page.evaluate(() => (window.offerCalls = []));
    await runPlan(page, panel);
    await page.waitForTimeout(500);
    assert.equal(await offer.count(), 0, "a dismissed TV is not asked again");
    assert.equal(await page.evaluate(() => window.offerCalls.includes("current_launcher")), false, "nor is its Home app read");
    if (evidence) {
      const video = page.video();
      await context.close();
      await video.saveAs(join(evidence, "home-offer-simulated.webm"));
    } else await context.close();
  }

  // Another Home app, or an unreadable one: no link.
  for (const homeNow of [CUSTOM, "error"]) {
    const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
    const panel = await openOptimize(page, base, homeNow);
    await runPlan(page, panel);
    await settle(page);
    await page.waitForFunction(() => window.offerCalls.includes("current_launcher"));
    await page.waitForTimeout(300);
    assert.equal(await panel.locator(".home-offer").count(), 0, `no link when Home is ${homeNow}`);
    await page.close();
  }

  // Restore mode never offers it.
  {
    const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
    const panel = await openOptimize(page, base, STOCK);
    await panel.locator(".mode-box").getByRole("button", { name: "Restore", exact: true }).click();
    await page.waitForTimeout(500);
    assert.equal(await panel.locator(".home-offer").count(), 0);
    await page.close();
  }
  console.log("Home screen offer passed: shown only after a run on stock Home, opens Launcher by keyboard, fits 1280/720/larger text in both themes, Dismiss remembered per hardware id, none for a custom or unreadable Home.");
} finally {
  await browser?.close();
  await server?.close();
}
