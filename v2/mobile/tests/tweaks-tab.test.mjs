import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { chromium } from "playwright";
import { startViteServer } from "./helpers/vite-harness.mjs";

let server;
let browser;
let origin;
before(async () => {
  ({ server, origin } = await startViteServer());
  browser = await chromium.launch({ headless: true });
});
after(async () => {
  await browser?.close();
  await server?.close();
});

const HOOKS = "com.nvidia.shieldtech.hooks";
const BASIC = "com.android.dreams.basic/com.android.dreams.basic.BasicDream";
const VENDOR = "com.google.android.backdrop/.Backdrop";

async function open(t, screen, options = {}) {
  const page = await browser.newPage({ viewport: { width: 384, height: 812 } });
  t.after(() => page.close());
  await page.addInitScript((options) => {
    localStorage.clear();
    window.calls = [];
    window.handlers = {};
    window.activeHost = "A";
    window.tweaks = {
      hdmi_control_enabled: "1",
      match_content_frame_rate: "1",
      long_press_timeout: "400",
      screensaver_components: options.screensaver ?? "com.google.android.backdrop/.Backdrop",
      screensaver_enabled: "1",
    };
    window.hooks = options.hooks ?? "enabled";
    window.mic = options.mic ?? "granted";
    window.__TAURI_INTERNALS__ = { invoke: async (command, args) => {
      window.calls.push({ command, args });
      if (window.handlers[command]) return window.handlers[command](args);
      switch (command) {
        case "get_entitlement": return options.pro ? "pro" : "free";
        case "wireless_connect": window.activeHost = args.host; return { ok: true };
        case "wireless_status": return { connected: true };
        case "list_devices": return [{ id: 1, serial: `${window.activeHost}:5555`, name: window.activeHost, model: "TV", status: "device", connection: "network", device_type: options.deviceType ?? "shield", properties: null }];
        case "health_report": return { ram: { free_mb: 1024 }, storage: {}, display: {}, top_memory: [] };
        case "app_list_for_device": return [];
        case "package_states":
          return Object.fromEntries(args.packages.map((p) => [p, p === "com.nvidia.shieldtech.hooks" ? window.hooks : "enabled"]));
        case "get_tweaks": return { ...window.tweaks };
        case "get_private_dns": return { mode: "opportunistic", hostname: null };
        case "get_display_scaling": return { size: "Physical size: 1920x1080", density: "Physical density: 320" };
        case "app_permission_state": return window.mic;
        case "enable_package": window.hooks = "enabled"; return { ok: true, message: "enabled" };
        case "disable_package":
          if (!options.pro) throw "LOCKED:curated_debloat";
          window.hooks = "disabled";
          return { ok: true, message: "disabled" };
        case "set_app_permission":
          if (!options.pro) throw "LOCKED:app_permission_write";
          window.mic = args.grant ? "granted" : "revoked";
          return { ok: true, message: "done" };
        case "write_setting":
          if (!options.pro) throw "LOCKED:tweaks_write";
          window.tweaks[args.key] = args.value || null;
          return { ok: true, message: "done" };
        default: return { ok: true, message: "done" };
      }
    } };
  }, options);
  await page.goto(origin);
  await page.evaluate(async ({ screen, pro }) => {
    const { session } = await import("/src/lib/session.svelte.ts");
    const { router } = await import("/src/lib/router.svelte.ts");
    window.session = session;
    window.router = router;
    session.entitlement = pro ? "pro" : "free";
    await session.connect("A", 5555);
    router.reset("dashboard");
    if (screen !== "dashboard") router.navigate(screen);
  }, { screen, pro: !!options.pro });
  return page;
}

const writes = (page, command) =>
  page.evaluate((command) => calls.filter((c) => c.command === command).map((c) => c.args), command);

test("Free users see every current value, including hooks, mic and screensaver", async (t) => {
  const page = await open(t, "tweaks");
  await page.getByRole("heading", { name: "Tweaks" }).waitFor();
  await page.getByText("Nvidia System Hooks", { exact: true }).waitFor();
  assert.equal(await page.getByText("Hooks on", { exact: true }).count(), 1);
  assert.equal(await page.getByText("Mic on", { exact: true }).count(), 1);
  assert.equal(await page.locator('[data-tweak="screensaver"] .state').innerText(), VENDOR);
  assert.equal(await page.getByText("Reading settings is free. Changing them is a Pro feature.").count(), 1);
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), true);
});

test("Turning the hooks back on is free; turning them off opens the paywall", async (t) => {
  const page = await open(t, "tweaks", { hooks: "disabled" });
  const hooks = page.getByRole("group", { name: "Nvidia System Hooks" });
  await hooks.waitFor();
  await hooks.getByRole("button", { name: "On", exact: true }).click();
  await page.getByText("Hooks on", { exact: true }).waitFor();
  assert.deepEqual(await writes(page, "enable_package"), [{ serial: "A:5555", package: HOOKS }]);
  await hooks.getByRole("button", { name: "Off", exact: true }).click();
  await page.getByRole("heading", { name: /Unlock/ }).waitFor();
  assert.equal(await page.evaluate(() => window.hooks), "enabled");
});

test("hooks row is hidden when the package is missing and on non-Shield read failures", async (t) => {
  const page = await open(t, "tweaks", { hooks: "missing" });
  await page.getByText("Remote Assistant Button", { exact: true }).waitFor();
  assert.equal(await page.getByText("Nvidia System Hooks", { exact: true }).count(), 0);

  const tv = await open(t, "tweaks", { deviceType: "google_tv" });
  await tv.evaluate(() => { handlers.package_states = () => Promise.reject("pm failed"); });
  await tv.getByRole("button", { name: "Re-read settings" }).click();
  await tv.getByText("Remote Assistant Button", { exact: true }).waitFor();
  await tv.waitForFunction(() => calls.filter((c) => c.command === "package_states").length >= 2);
  assert.equal(await tv.getByText("Nvidia System Hooks", { exact: true }).count(), 0);
});

test("Assistant button revokes the mic through set_app_permission and re-reads", async (t) => {
  const page = await open(t, "tweaks", { pro: true });
  const group = page.getByRole("group", { name: "Remote Assistant Button" });
  await group.waitFor();
  await group.getByRole("button", { name: "Off", exact: true }).click();
  await page.getByText("Mic off", { exact: true }).waitFor();
  assert.deepEqual(await writes(page, "set_app_permission"), [{
    serial: "A:5555", package: "com.google.android.katniss",
    permission: "android.permission.RECORD_AUDIO", grant: false,
  }]);
});

test("screensaver: Basic Daydream, Off, then Restore previous brings the vendor one back", async (t) => {
  const page = await open(t, "tweaks", { pro: true });
  const row = page.locator('[data-tweak="screensaver"]');
  await row.waitFor();
  const restore = row.getByRole("button", { name: /Restore previous/ });
  assert.equal(await restore.isDisabled(), true);

  await row.getByRole("button", { name: "Basic Daydream", exact: true }).click();
  await page.waitForFunction((basic) => document.querySelector('[data-tweak="screensaver"] .state')?.textContent === "Basic Daydream" && window.tweaks.screensaver_components === basic, BASIC);
  await row.getByRole("button", { name: "Off", exact: true }).click();
  await page.waitForFunction(() => document.querySelector('[data-tweak="screensaver"] .state')?.textContent === "Off");

  // Leaving and re-entering the tab must not recapture the value we wrote.
  await page.evaluate(() => { router.navigate("dashboard"); router.navigate("tweaks"); });
  await row.waitFor();
  assert.match(await row.getByRole("button", { name: /Restore previous/ }).innerText(), /backdrop/);
  await row.getByRole("button", { name: /Restore previous/ }).click();
  await page.waitForFunction((vendor) => document.querySelector('[data-tweak="screensaver"] .state')?.textContent === vendor, VENDOR);

  const ss = (await writes(page, "write_setting")).map((a) => `${a.key}=${a.value}`);
  assert.deepEqual(ss, [
    "screensaver_enabled=1", `screensaver_components=${BASIC}`,
    "screensaver_enabled=0", "screensaver_components=",
    "screensaver_enabled=1", `screensaver_components=${VENDOR}`,
  ]);
});

test("a refused screensaver flag stops before the component is written", async (t) => {
  const page = await open(t, "tweaks", { pro: true });
  await page.locator('[data-tweak="screensaver"]').waitFor();
  await page.evaluate(() => {
    handlers.write_setting = (args) => args.key === "screensaver_enabled"
      ? { ok: false, message: "denied" }
      : { ok: true, message: "done" };
  });
  await page.locator('[data-tweak="screensaver"]').getByRole("button", { name: "Off", exact: true }).click();
  await page.getByText(/screensaver was left unchanged/).waitFor();
  assert.deepEqual((await writes(page, "write_setting")).map((a) => a.key), ["screensaver_enabled"]);
});

test("Dashboard has one Optimize call to action and Tune tiles for Tweaks and Launcher", async (t) => {
  const page = await open(t, "dashboard");
  const cta = page.getByRole("button", { name: /Review app choices/ });
  await cta.waitFor();
  assert.equal(await cta.count(), 1);
  assert.match(await cta.innerText(), /PRO/);
  assert.equal(await page.locator(".actions-grid").getByText("Optimize", { exact: true }).count(), 0);
  assert.match(await page.getByRole("button", { name: /Launcher/ }).innerText(), /PRO/);
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), true);

  await page.locator(".tune-tile", { hasText: "Tweaks" }).click();
  await page.getByRole("heading", { name: "Tweaks" }).waitFor();
  await page.locator(".bottom-tabs .tab-btn.active", { hasText: "Tweaks" }).waitFor();
  await page.evaluate(() => history.back());
  await page.waitForFunction(() => router.current === "dashboard");
  await cta.click();
  await page.getByRole("heading", { name: "Optimize" }).waitFor();
});

test("Pro dashboard drops the PRO tags", async (t) => {
  const page = await open(t, "dashboard", { pro: true });
  await page.getByRole("button", { name: /Review app choices/ }).waitFor();
  assert.equal(await page.locator(".pro-marker").count(), 0);
});

test("More is titled More, lists Optimize, and no longer duplicates the Tweaks tab", async (t) => {
  const page = await open(t, "more");
  await page.getByRole("heading", { name: "More", exact: true }).waitFor();
  assert.equal(await page.getByRole("heading", { name: "Settings" }).count(), 0);
  const tools = page.locator(".setting-row");
  assert.equal(await tools.filter({ hasText: "Tweaks" }).count(), 0);
  assert.equal(await tools.filter({ hasText: "Launcher" }).count(), 1);
  assert.equal(await tools.filter({ hasText: "Snapshots" }).count(), 1);
  await tools.filter({ hasText: "Optimize" }).click();
  await page.getByRole("heading", { name: "Optimize" }).waitFor();
});
