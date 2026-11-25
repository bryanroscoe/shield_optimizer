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

async function openOptimize(page, base, homeNow, { dismissed = null } = {}) {
  await page.addInitScript(({ homeNow, dismissed, KEY }) => {
    if (dismissed) localStorage.setItem(KEY, JSON.stringify(dismissed));
    window.homeNow = homeNow;
    window.holdHome = false;
    window.offerCalls = [];
    let internals;
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true, get: () => internals,
      set(value) {
        const original = value.invoke.bind(value);
        value.invoke = async (command, args) => {
          window.offerCalls.push(command);
          if (command === "current_launcher") {
            if (window.holdHome) await new Promise((resolve) => (window.releaseHome = resolve));
            if (window.homeNow === "error") throw new Error("SIMULATED unreadable Home app");
            return { package: window.homeNow, activity: ".Main" };
          }
          if (command === "set_default_launcher") {
            window.homeNow = args.package;
            return { ok: true, strategy: "role", last_error: null, diagnostics: [] };
          }
          return original(command, args);
        };
        internals = value;
      },
    });
  }, { homeNow, dismissed, KEY });
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

const homeReads = (page) => page.evaluate(() => window.offerCalls.filter((c) => c === "current_launcher").length);

async function runPlan(page, panel, label = "Optimize") {
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
  await dialog.getByRole("button", { name: `Run ${label}`, exact: true }).click();
  await panel.getByText(new RegExp(`${label} complete`)).waitFor({ timeout: 15000 });
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
    assert.ok(await offer.isVisible(), "visiting Launcher without changing Home keeps the offer");
    await dismiss.click();
    assert.equal(await offer.count(), 0, "Dismiss hides it");
    assert.deepEqual(await page.evaluate((k) => JSON.parse(localStorage.getItem(k)), KEY), ["0323220012345"],
      "the dismissal is filed under the Shield's hardware id");
    if (evidence) {
      const video = page.video();
      await context.close();
      await video.saveAs(join(evidence, "home-offer-simulated.webm"));
    } else await context.close();
  }

  // A dismissed TV isn't asked again, nor is its Home app read.
  {
    const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
    const panel = await openOptimize(page, base, STOCK, { dismissed: ["0323220012345"] });
    await runPlan(page, panel);
    await page.waitForTimeout(500);
    assert.equal(await panel.locator(".home-offer").count(), 0, "a dismissed TV is not asked again");
    assert.equal(await homeReads(page), 0, "nor is its Home app read");
    await page.close();
  }

  // Running the same plan again changes nothing, so it reads nothing; and a
  // lookup still in flight when the second run starts never lands.
  {
    const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
    const panel = await openOptimize(page, base, STOCK);
    await page.evaluate(() => (window.holdHome = true));
    await runPlan(page, panel);
    await page.waitForFunction(() => typeof window.releaseHome === "function");
    await runPlan(page, panel);
    await page.evaluate(() => { window.holdHome = false; window.releaseHome(); });
    await page.waitForTimeout(500);
    assert.equal(await panel.locator(".home-offer").count(), 0, "the first run's late lookup doesn't land on the second run");
    assert.equal(await homeReads(page), 1, "a run that changed nothing reads nothing");
    await page.close();
  }

  // Changing Home in the Launcher tab takes the offer back.
  {
    const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
    const panel = await openOptimize(page, base, STOCK);
    await runPlan(page, panel);
    await panel.locator(".home-offer").getByRole("button", { name: "Get a better home screen", exact: true }).click();
    const launcher = page.locator("#tabpanel-launcher");
    await launcher.waitFor();
    const reads = await homeReads(page);
    await launcher.locator("tr, li, .launcher-row").filter({ hasText: "Projectivy Launcher" }).first()
      .getByRole("button", { name: /^Set as default$/ }).click();
    await page.waitForFunction((n) => window.offerCalls.filter((c) => c === "current_launcher").length > n, reads);
    await page.getByRole("tab", { name: "Optimize" }).click();
    await panel.locator("table.optimize-table").waitFor();
    assert.equal(await panel.locator(".home-offer").count(), 0, "a custom Home app retracts the offer");
    await page.close();
  }

  // A Restore run never reads Home.
  {
    const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
    const panel = await openOptimize(page, base, STOCK);
    await panel.locator(".mode-box").getByRole("button", { name: "Restore", exact: true }).click();
    const enableRow = panel.locator("tr").filter({ has: page.locator(".radio-label", { hasText: /^Enable$/ }) }).first();
    await enableRow.waitFor();
    await enableRow.locator(".radio-pill", { has: page.locator(".radio-label", { hasText: /^Enable$/ }) }).click();
    await runPlan(page, panel, "Restore");
    await page.waitForTimeout(500);
    assert.equal(await homeReads(page), 0, "Restore never reads Home");
    assert.equal(await panel.locator(".home-offer").count(), 0);
    await page.close();
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

  console.log("Home screen offer passed: shown only after a run that changed the TV on stock Home, opens Launcher by keyboard, fits 1280/720/larger text in both themes, Dismiss remembered per hardware id, no read for a dismissed TV, a no-op rerun or Restore, a late lookup never lands, a Home change in Launcher retracts it, none for a custom or unreadable Home.");
} finally {
  await browser?.close();
  await server?.close();
}
