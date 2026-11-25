// After an Optimize run, a TV still on its stock Home app gets one quiet link to
// Launcher. Never before a run, never on a custom or unreadable Home app, never
// after Restore or a stopped run, and once dismissed never again for that TV's
// hardware id (never its address). SIMULATED: scripted invoke, no phone or TV.
import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
import { after, before, test } from "node:test";
import { chromium } from "playwright";
import { startViteServer } from "./helpers/vite-harness.mjs";

const evidence = process.env.MOBILE_HOME_OFFER_EVIDENCE;
const STOCK = "com.google.android.tvlauncher";
const KEY = "atvopt.homeScreenOfferDismissed";

let server, origin, browser;
before(async () => {
  ({ server, origin } = await startViteServer());
  browser = await chromium.launch({ headless: true });
  if (evidence) await mkdir(evidence, { recursive: true });
});
after(async () => { await browser?.close(); await server?.close(); });

const catalog = Array.from({ length: 3 }, (_, i) => ({
  package: `com.example.bloat${i}`,
  name: `Bloat ${i}`,
  method: "disable",
  risk: "safe",
  optimize_description: "Optional.",
  restore_description: "",
  default_optimize: true,
  default_restore: false,
  play_store: true,
}));

async function openOptimize(t, { home = STOCK, hardwareId = "HW-HOME-1", width = 412, scale = 1, dismissed = null, video = false, keeper = false } = {}) {
  const context = await browser.newContext({ viewport: { width, height: 860 }, isMobile: true, hasTouch: true,
    ...(video && evidence ? { recordVideo: { dir: `${evidence}/raw`, size: { width, height: 860 } } } : {}) });
  const page = await context.newPage();
  page.setDefaultTimeout(10000);
  t.after(() => context.close());
  const apps = keeper ? [...catalog, { ...catalog[0], package: "com.example.keeper", name: "Keeper", default_optimize: false }] : catalog;
  await page.addInitScript(({ catalog, home, hardwareId, dismissed, KEY }) => {
    localStorage.clear();
    window.holdHome = false;
    window.disableHook = null;
    if (dismissed) localStorage.setItem(KEY, JSON.stringify(dismissed));
    window.calls = [];
    window.home = home;
    window.states = Object.fromEntries(catalog.map((e) => [e.package, "enabled"]));
    window.__TAURI_INTERNALS__ = { invoke: async (command, args) => {
      window.calls.push(command);
      switch (command) {
        case "get_entitlement": return "pro";
        case "wireless_connect": return { ok: true };
        case "wireless_status": return { connected: true };
        case "list_devices": return [{ id: 1, serial: "A:5555", name: "Living Room", model: "Shield", status: "device", connection: "network", device_type: "shield", tv_evidence: "tv", properties: hardwareId ? { serial_number: hardwareId } : {} }];
        case "health_report": return { ram: { free_mb: 1024 }, storage: {}, display: {}, top_memory: [] };
        case "app_list_for_device": return catalog;
        case "package_states": return Object.fromEntries(args.packages.map((p) => [p, window.states[p] ?? "missing"]));
        case "prepare_optimize": return {
          mode: args.mode,
          items: catalog.map((e) => ({ entry: e, action: args.mode === "optimize"
            ? (window.states[e.package] === "enabled" ? { kind: "disable" } : { kind: "skip", reason: "already_disabled" })
            : (window.states[e.package] === "disabled" ? { kind: "enable" } : { kind: "skip", reason: "already_enabled" }), memory_mb: null })),
        };
        case "safety_info": return { kind: "safe", reason: "Reviewed and safe to remove.", source: "reviewed_catalog" };
        case "disable_package":
          if (window.disableHook) await window.disableHook(args);
          window.states[args.package] = "disabled";
          return { ok: true, message: "" };
        case "enable_package": window.states[args.package] = "enabled"; return { ok: true, message: "" };
        case "list_launchers": return [
          { entry: { name: "Google TV Home", package: "com.google.android.tvlauncher", source_url: null }, installed: true, enabled: true, stock: true, other: false, setup_helper: false },
          { entry: { name: "Projectivy Launcher", package: "com.spocky.projengmenu", source_url: null }, installed: true, enabled: true, stock: false, other: false, setup_helper: false },
        ];
        case "current_launcher":
          if (window.holdHome) await new Promise((resolve) => (window.releaseHome = resolve));
          if (window.home === "error") throw "SIMULATED unreadable Home app";
          return { package: window.home, activity: ".Main" };
        default: return { ok: true, message: "done" };
      }
    } };
  }, { catalog: apps, home, hardwareId, dismissed, KEY });
  await page.goto(origin);
  await page.evaluate(async (scale) => {
    const { session } = await import("/src/lib/session.svelte.ts");
    const { router } = await import("/src/lib/router.svelte.ts");
    window.session = session; window.router = router;
    await session.connect("A", 5555);
    await session.loadBloat(true);
    router.reset("dashboard");
    router.navigate("optimize");
    if (scale > 1) document.documentElement.style.fontSize = `${16 * scale}px`;
    const label = document.createElement("div");
    label.textContent = "SIMULATION · scripted TV, no phone or TV";
    label.style.cssText = "position:fixed;inset:0 0 auto;z-index:99999;background:#182238;color:white;padding:4px;font:10px sans-serif;text-align:center";
    document.body.append(label);
  }, scale);
  await page.getByText("3 selected", { exact: false }).waitFor();
  return page;
}

async function apply(page) {
  await page.getByRole("button", { name: /Apply optimization/ }).click();
  await page.locator(".dialog-card").getByRole("button", { name: "Apply", exact: true }).click();
}

async function launcherReads(page) {
  return page.evaluate(() => calls.filter((c) => c === "current_launcher").length);
}

for (const [width, scale] of [[412, 1], [320, 1], [320, 2]]) {
  test(`stock Home after a run: link opens Launcher and fits at ${width}px / ${scale * 100}% text`, async (t) => {
    const page = await openOptimize(t, { width, scale, video: width === 412 });
    const offer = page.locator(".home-offer");
    assert.equal(await offer.count(), 0, "nothing before a run");
    assert.equal(await launcherReads(page), 0, "Home isn't read before a run");
    await apply(page);
    await offer.waitFor();
    assert.match(await offer.innerText(), /Still on the stock home screen\./);
    const link = offer.getByRole("button", { name: "Get a better home screen", exact: true });
    const dismiss = offer.getByRole("button", { name: "Dismiss", exact: true });
    for (const b of [link, dismiss]) assert.ok((await b.boundingBox()).height >= 44, "touch target at least 44px");
    const box = await offer.boundingBox();
    assert.ok(box.x >= 0 && box.x + box.width <= width, "the offer fits the screen");
    const clipped = await offer.evaluate((el) => [...el.querySelectorAll("span, button")]
      .filter((n) => n.scrollWidth > n.clientWidth + 1).map((n) => n.textContent.trim()));
    assert.deepEqual(clipped, [], "nothing is clipped");
    const covered = await link.evaluate((el) => {
      el.scrollIntoView({ block: "center" });
      const r = el.getBoundingClientRect();
      const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
      return hit && !el.contains(hit) ? hit.className : null;
    });
    assert.equal(covered, null, "scrolled into view, the link isn't under the run's toast");
    if (evidence) {
      // The phone app has one (dark) theme.
      await page.screenshot({ path: `${evidence}/offer-${width}-${scale * 100}.png` });
    }
    await link.tap();
    await page.waitForFunction(() => router.current === "launcher");
    if (evidence && width === 412) {
      await page.waitForTimeout(400);
      await page.screenshot({ path: `${evidence}/after-link-launcher.png` });
      const video = page.video();
      await page.context().close();
      await video.saveAs(`${evidence}/home-offer-mobile-simulated.webm`);
    }
  });
}

test("Dismiss is remembered for this TV's hardware id and stops the next check", async (t) => {
  const page = await openOptimize(t);
  await apply(page);
  await page.locator(".home-offer").getByRole("button", { name: "Dismiss", exact: true }).click();
  assert.equal(await page.locator(".home-offer").count(), 0);
  assert.deepEqual(await page.evaluate((k) => JSON.parse(localStorage.getItem(k)), KEY), ["HW-HOME-1"]);

  const again = await openOptimize(t, { dismissed: ["HW-HOME-1"] });
  await apply(again);
  await again.getByText(/Optimization complete/).waitFor();
  await again.waitForTimeout(300);
  assert.equal(await again.locator(".home-offer").count(), 0, "a dismissed TV isn't asked again");
  assert.equal(await launcherReads(again), 0, "nor is its Home app read");

  const other = await openOptimize(t, { hardwareId: "HW-HOME-2", dismissed: ["HW-HOME-1"] });
  await apply(other);
  await other.locator(".home-offer").waitFor();
});

test("a TV with no hardware id is never remembered, only hidden for the open screen", async (t) => {
  const page = await openOptimize(t, { hardwareId: null });
  await apply(page);
  await page.locator(".home-offer").getByRole("button", { name: "Dismiss", exact: true }).click();
  assert.equal(await page.evaluate((k) => localStorage.getItem(k), KEY), null);
  await page.evaluate((states) => Object.assign(window.states, states), { "com.example.bloat0": "enabled" });
  await page.evaluate(() => router.navigate("optimize"));
  await page.waitForTimeout(300);
  assert.equal(await page.locator(".home-offer").count(), 0);
});

for (const home of ["com.spocky.projengmenu", "error"]) {
  test(`no link when Home is ${home === "error" ? "unreadable" : "already custom"}`, async (t) => {
    const page = await openOptimize(t, { home });
    await apply(page);
    await page.waitForFunction(() => calls.includes("current_launcher"));
    await page.waitForTimeout(300);
    assert.equal(await page.locator(".home-offer").count(), 0);
  });
}

test("switching to Restore drops it", async (t) => {
  const page = await openOptimize(t);
  await apply(page);
  await page.locator(".home-offer").waitFor();
  await page.getByRole("button", { name: "Restore", exact: true }).click();
  await page.getByRole("button", { name: /Apply restore/ }).waitFor();
  assert.equal(await page.locator(".home-offer").count(), 0, "switching to Restore drops it");
});

test("a run stopped by Cancel or by losing Pro reads nothing", async (t) => {
  const canceled = await openOptimize(t);
  await canceled.evaluate(() => (window.disableHook = () => new Promise((r) => setTimeout(r, 400))));
  await apply(canceled);
  await canceled.getByRole("button", { name: "Cancel", exact: true }).click();
  await canceled.getByText(/^Stopped after/).waitFor();
  await canceled.waitForTimeout(300);
  assert.equal(await launcherReads(canceled), 0, "Cancel: no Home read");
  assert.equal(await canceled.locator(".home-offer").count(), 0);

  const lost = await openOptimize(t);
  await lost.evaluate(() => { let n = 0; window.disableHook = () => { if (++n === 1) window.session.entitlement = "free"; }; });
  await apply(lost);
  await lost.getByText(/Stopped — Pro is required/).waitFor();
  await lost.waitForTimeout(300);
  assert.equal(await launcherReads(lost), 0, "Pro lost: no Home read");
});

test("a plan reload from an app sheet drops a Home lookup still in flight", async (t) => {
  const page = await openOptimize(t, { keeper: true });
  await page.evaluate(() => (window.holdHome = true));
  await apply(page);
  await page.waitForFunction(() => typeof window.releaseHome === "function");
  await page.getByRole("button", { name: "Optional apps", exact: true }).click().catch(() => {});
  await page.getByRole("button", { name: "Details for Keeper", exact: true }).click();
  const sheet = page.getByRole("dialog", { name: /App details for/ });
  await sheet.getByRole("button", { name: "Disable", exact: true }).click();
  await page.locator(".dialog-card").getByRole("button", { name: /^Disable/ }).click();
  await page.waitForFunction(() => window.states["com.example.keeper"] === "disabled");
  await page.waitForFunction(() => calls.filter((c) => c === "prepare_optimize").length >= 3);
  await page.evaluate(() => { window.holdHome = false; window.releaseHome(); });
  await page.waitForTimeout(400);
  assert.equal(await page.locator(".home-offer").count(), 0, "the stale lookup didn't land on the reloaded plan");
});
