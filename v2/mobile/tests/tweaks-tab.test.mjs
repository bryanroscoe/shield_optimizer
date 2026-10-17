import assert from "node:assert/strict";
import { mkdir, readFile, writeFile } from "node:fs/promises";
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
  const page = await browser.newPage({ viewport: { width: 384, height: 812 }, ...(options.daydreamEvidence ? { recordVideo: { dir: "evidence/daydream-order-2026-10-17/raw", size: { width: 384, height: 812 } } } : options.evidence ? { recordVideo: { dir: "evidence/settings-polish-2026-10-17/raw", size: { width: 384, height: 812 } } } : process.env.RECONNECT_VIDEO_DIR ? { recordVideo: { dir: `${process.env.RECONNECT_VIDEO_DIR}/${t.name.replace(/[^a-z0-9]+/gi, "-")}`, size: { width: 384, height: 812 } } } : {}) });
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
  assert.equal(await page.getByText("Additional tools for managing your TV.", { exact: true }).count(), 1);
  if (process.env.RELEASE_SCREENSHOT) {
    await page.waitForTimeout(400);
    await page.screenshot({ path: process.env.RELEASE_SCREENSHOT });
  }
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
    `screensaver_components=${BASIC}`, "screensaver_enabled=1",
    "screensaver_enabled=0", "screensaver_components=",
    `screensaver_components=${VENDOR}`, "screensaver_enabled=1",
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

test("endpoint reuse never restores another TV's screensaver and interrupted writes stop", async (t) => {
  const page = await open(t, "tweaks", { pro: true });
  const card = page.locator('[data-tweak="screensaver"]');
  await card.getByRole("button", { name: "Off", exact: true }).click();
  await page.waitForFunction(() => window.tweaks.screensaver_enabled === "0");
  await page.evaluate(async () => { window.tweaks.screensaver_components = "replacement/.Dream"; window.tweaks.screensaver_enabled = "1"; await window.session.connect("A", 5555); });
  await card.getByText("replacement/.Dream", { exact: true }).first().waitFor();
  assert.equal(await card.getByText("com.google.android.backdrop/.Backdrop", { exact: true }).count(), 0);
  await page.evaluate(() => { window.handlers.write_setting = () => new Promise(resolve => { window.finishWrite = resolve; }); });
  await card.getByRole("button", { name: "Off", exact: true }).click();
  await page.waitForFunction(() => !!window.finishWrite);
  const before = (await writes(page, "write_setting")).length;
  await page.evaluate(async () => { await window.session.connect("A", 5555); window.finishWrite({ ok: true, message: "done" }); });
  await page.waitForTimeout(150);
  assert.equal((await writes(page, "write_setting")).length, before);
});

test("verified TV retains originals across a rotated port, replacement hardware does not", async (t) => {
  const page = await open(t, "tweaks", { pro: true });
  await page.evaluate(async () => {
    window.hardware = "verified-A";
    window.handlers.list_devices = () => [{ id: 1, serial: `${window.session.host}:${window.session.connectPort}`, status: "device", device_type: "shield", properties: { serial_number: window.hardware } }];
    await window.session.connect("A", 5555);
  });
  const card = page.locator('[data-tweak="screensaver"]');
  await card.getByRole("button", { name: "Off", exact: true }).click();
  await page.waitForFunction(() => window.tweaks.screensaver_enabled === "0");
  await page.evaluate(async () => { await window.session.connect("A", 4321); });
  await card.getByRole("button", { name: /Restore previous/ }).click();
  await page.waitForFunction(() => window.tweaks.screensaver_components === "com.google.android.backdrop/.Backdrop" && window.tweaks.screensaver_enabled === "1");
  await page.evaluate(async () => { window.hardware = "verified-B"; window.tweaks.screensaver_components = "different/.Dream"; await window.session.connect("A", 4321); });
  await card.getByText("different/.Dream", { exact: true }).first().waitFor();
  assert.equal(await card.getByText("com.google.android.backdrop/.Backdrop", { exact: true }).count(), 0);
});

test("a reconnect between screensaver writes prevents the second write", async (t) => {
  const page = await open(t, "tweaks", { pro: true });
  await page.locator('[data-tweak="screensaver"]').waitFor();
  await page.evaluate(() => { window.handlers.write_setting = () => new Promise(resolve => { window.finishWrite = resolve; }); });
  await page.locator('[data-tweak="screensaver"]').getByRole("button", { name: "Off", exact: true }).click();
  await page.waitForFunction(() => !!window.finishWrite);
  await page.evaluate(async () => { await window.session.connect("A", 5555); window.finishWrite({ ok: true, message: "done" }); });
  await page.waitForTimeout(100);
  assert.equal((await writes(page, "write_setting")).length, 1);
});

test("current gap: Pro loss blocks restoring an app-changed screensaver even with the original known", async (t) => {
  const page = await open(t, "tweaks", { pro: true });
  await page.evaluate(() => {
    window.handlers.write_setting = (args) => {
      if (!window.session.isPro) throw "LOCKED:tweaks_write";
      window.tweaks[args.key] = args.value || null;
      return { ok: true, message: "done" };
    };
  });
  const card = page.locator('[data-tweak="screensaver"]');
  await card.getByRole("button", { name: "Off", exact: true }).click();
  await page.waitForFunction(() => document.querySelector('[data-tweak="screensaver"] .state')?.textContent === "Off");
  await page.evaluate(() => { window.session.entitlement = "free"; });
  await card.getByRole("button", { name: /Restore previous/ }).click();
  await page.getByRole("heading", { name: /Unlock ATV Optimizer Pro/ }).waitFor();
  assert.equal(await page.evaluate(() => window.tweaks.screensaver_enabled), "0");
  assert.equal(await page.evaluate(() => window.tweaks.screensaver_components), null);
  assert.equal(await page.evaluate(() => window.session.entitlement), "free");
  const calls = await writes(page, "write_setting");
  assert.equal(calls.length, 3, "two initial writes, then a denied restore component write only");
  assert.equal(calls[2].key, "screensaver_components");
  assert.equal(calls[2].value, VENDOR);
});

test("Free users retain emergency recovery without an unlock or payment", async (t) => {
  const page = await open(t, "more");
  await page.evaluate(() => { window.handlers.panic_recovery = () => ({ restored: ["com.example.app"], failed: [], message: "Package recovery completed" }); });
  await page.getByRole("button", { name: /Emergency recovery/ }).click();
  await page.getByRole("button", { name: "Re-enable all", exact: true }).click();
  await page.getByText("Package recovery completed", { exact: true }).first().waitFor();
  assert.equal(await page.getByRole("heading", { name: /Unlock ATV/ }).count(), 0);
  assert.equal(await page.evaluate(() => window.session.entitlement), "free");
  assert.deepEqual(await writes(page, "panic_recovery"), [{ serial: "A:5555" }]);
  assert.equal((await writes(page, "activate_license")).length, 0);
});

test("scaling readback matches both resolution and density and clears stale selection on failure", async (t) => {
  const page = await open(t, "tweaks", { pro: true });
  const card = page.locator('.tweak-row').filter({ has: page.getByText('Display scaling', { exact: true }) });
  await page.evaluate(() => { handlers.get_display_scaling = () => ({ size: 'Physical size: 3840x2160\nOverride size: 1920x1080', density: 'Physical density: 640\nOverride density: 320' }); });
  await page.getByRole('button', { name: 'Re-read settings' }).click();
  await page.waitForFunction(() => [...document.querySelectorAll('button')].some(b => b.textContent.trim() === '1080p' && b.getAttribute('aria-pressed') === 'true'));
  await page.evaluate(() => { handlers.get_display_scaling = () => ({ size: 'Physical size: 3840x2160\nOverride size: 1920x1080', density: 'Physical density: 640\nOverride density: 300' }); });
  await page.getByRole('button', { name: 'Re-read settings' }).click();
  await page.getByText(/Custom size \/ density/).waitFor();
  assert.equal(await card.locator('[aria-pressed="true"]').count(), 0);
  await page.evaluate(() => { handlers.get_display_scaling = () => Promise.reject('read failed'); });
  await page.getByRole('button', { name: 'Re-read settings' }).click();
  await page.getByText("Couldn't read the current size").waitFor();
  assert.equal(await card.locator('[aria-pressed="true"]').count(), 0);
});

test("Custom DNS editing does not claim the TV has applied it; failed read clears selection", async (t) => {
  const page = await open(t, "tweaks", { pro: true });
  const card = page.locator('.tweak-row').filter({ has: page.getByText('Private DNS', { exact: true }) });
  await card.getByRole('button', { name: 'Auto', exact: true }).waitFor();
  await card.getByRole('button', { name: 'Custom', exact: true }).click();
  assert.equal(await card.getByRole('button', { name: 'Custom', exact: true }).getAttribute('aria-pressed'), 'false');
  assert.equal(await card.getByRole('button', { name: 'Auto', exact: true }).getAttribute('aria-pressed'), 'true');
  await page.evaluate(() => { handlers.get_private_dns = () => Promise.reject('DNS read failed'); });
  await page.getByRole('button', { name: 'Re-read settings' }).click();
  await page.getByText("Couldn't read the current mode").waitFor();
  assert.equal(await card.locator('[aria-pressed="true"]').count(), 0);
});

test("equivalent Android animation number formats select the same speed", async (t) => {
  const page = await open(t, "tweaks", { pro: true });
  await page.evaluate(() => Object.assign(tweaks, { window_animation_scale: '1.0', transition_animation_scale: '1', animator_duration_scale: '1.00' }));
  await page.getByRole('button', { name: 'Re-read settings' }).click();
  const card = page.locator('.tweak-row').filter({ has: page.getByText('Animation speed', { exact: true }) });
  await page.waitForFunction(() => [...document.querySelectorAll('button')].some(b => b.textContent.trim() === '1×' && b.classList.contains('active')));
  assert.equal(await card.getByRole('button', { name: '1×', exact: true }).evaluate(b => b.classList.contains('active')), true);
});

if (process.env.MOBILE_POLISH_EVIDENCE === "1") test("capture simulated settings and optical icon comparison", async (t) => {
  const dir = "evidence/settings-polish-2026-10-17";
  await mkdir(dir, { recursive: true });
  const page = await open(t, "tweaks", { pro: true, evidence: true });
  await page.evaluate(() => {
    Object.assign(tweaks, { cached_process_limit: 32, background_process_limit: "2", window_animation_scale: "1.0", transition_animation_scale: "1", animator_duration_scale: "1.00" });
    handlers.get_display_scaling = () => ({ size: "Physical size: 3840x2160\nOverride size: 1920x1080", density: "Physical density: 640\nOverride density: 320" });
  });
  await page.getByRole('button', { name: 'Re-read settings' }).click();
  await page.getByText('Reported cached-process cap: 32', { exact: true }).waitFor();
  await page.screenshot({ path: `${dir}/settings-simulated.png`, fullPage: true });
  await page.waitForTimeout(700);
  const dns = page.locator('.tweak-row').filter({ has: page.getByText('Private DNS', { exact: true }) });
  await dns.scrollIntoViewIfNeeded();
  await dns.getByRole('button', { name: 'Custom', exact: true }).click();
  await page.waitForTimeout(700);
  await page.getByRole('textbox', { name: 'Private DNS hostname' }).fill('dns.example.test');
  await page.screenshot({ path: `${dir}/dns-draft-simulated.png` });
  await page.getByRole('button', { name: 'Auto', exact: true }).click();
  await page.getByRole('button', { name: 'Re-read settings' }).click();
  await page.waitForTimeout(700);
  const recording = page.video();
  await page.context().close();
  await recording.saveAs(`${dir}/settings-simulated.webm`);
  const icons = await browser.newPage({ viewport: { width: 840, height: 420 }, deviceScaleFactor: 2 });
  t.after(() => icons.close());
  const svg = await readFile('src-tauri/icons/icon.svg', 'utf8');
  const transparent = svg.replace(/<rect width="120" height="120" rx="28" fill="#C9F24E"\/>/, '');
  const art = transparent.replace(/<svg[^>]*>/, '').replace('</svg>', '');
  const adaptive = y => `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 108 108"><g transform="translate(14.4 ${y}) scale(.66)">${art}</g></svg>`;
  const current = adaptive(14.4);
  const shifted = adaptive(17.4);
  await icons.setContent(`<style>body{background:#16191e;color:#f2f5f8;font:16px system-ui;padding:20px} .row{display:flex;gap:42px;margin:22px} .icon{width:64px;height:64px;background:#c9f24e;overflow:hidden}.icon svg{width:64px;height:64px}.circle{border-radius:50%}.square{border-radius:18px}.squircle{border-radius:28%}span{width:180px}small{color:#bbb}</style><h2>Launcher optical alignment · simulated masks</h2><small>64 CSS px; candidate moves artwork down 1.8 px. No launcher/device test.</small>${['circle','square','squircle'].map(mask => `<div class="row"><span>${mask}</span><div class="icon ${mask}">${current}</div><div class="icon ${mask}">${shifted}</div></div>`).join('')}<p>Current ← → proposed optical shift (source assets unchanged)</p>`);
  await icons.screenshot({ path: `${dir}/icon-optical-comparison.png` });
});

test("DNS confirmed changes survive navigation and reconnect, while unset stays explicit", async (t) => {
  const page = await open(t, 'tweaks', { pro: true });
  await page.evaluate(() => {
    window.dnsState = { mode: 'opportunistic', hostname: null };
    handlers.get_private_dns = () => ({ ...dnsState });
    handlers.set_private_dns = args => { dnsState = { mode: args.mode, hostname: args.hostname ?? null }; return { ok: true, message: 'done' }; };
  });
  const card = page.locator('.tweak-row').filter({ has: page.getByText('Private DNS', { exact: true }) });
  await card.getByRole('button', { name: 'Off', exact: true }).click();
  await page.waitForFunction(() => [...document.querySelectorAll('button')].some(b => b.textContent.trim() === 'Off' && b.getAttribute('aria-pressed') === 'true'));
  await page.evaluate(() => router.navigate('more'));
  await page.evaluate(() => router.navigate('tweaks'));
  await card.getByRole('button', { name: 'Off', exact: true }).waitFor();
  assert.equal(await card.getByRole('button', { name: 'Off', exact: true }).getAttribute('aria-pressed'), 'true');
  await page.evaluate(async () => { await session.connect('B', 5555); dnsState = { mode: null, hostname: null }; });
  await page.getByRole('button', { name: 'Re-read settings' }).click();
  await page.getByText('Not explicitly set (TV default)', { exact: true }).waitFor();
  assert.equal(await card.locator('[aria-pressed="true"]').count(), 0);
});

for (const component of [VENDOR, null]) {
  for (const outcome of ["refuse", "reject"]) {
    test(`Daydream component ${outcome} never enables an off screensaver with ${component ? "previous" : "unset"} component`, async t => {
      const page = await open(t, "tweaks", { pro: true });
      await page.locator('[data-tweak="screensaver"]').waitFor();
      await page.evaluate(({ component, outcome }) => {
        tweaks.screensaver_enabled = "0"; tweaks.screensaver_components = component;
        handlers.write_setting = args => {
          if (args.key === "screensaver_components") {
            if (outcome === "reject") throw "Component write failed";
            return { ok: false, message: "Component write refused" };
          }
          tweaks[args.key] = args.value || null; return { ok: true, message: "done" };
        };
      }, { component, outcome });
      await page.locator('[data-tweak="screensaver"]').getByRole("button", { name: "Basic Daydream", exact: true }).click();
      await page.getByText(/Component write (failed|refused)/).waitFor();
      assert.equal(await page.evaluate(() => tweaks.screensaver_enabled), "0");
      assert.equal(await page.evaluate(() => tweaks.screensaver_components), component);
      assert.deepEqual((await writes(page, "write_setting")).map(a => a.key), ["screensaver_components"]);
    });
  }
}

for (const action of ["Basic Daydream", "Restore previous"]) {
  test(`${action} does not enable after a refused component; failed enable retains off with configured component`, async t => {
    const page = await open(t, "tweaks", { pro: true });
    const card = page.locator('[data-tweak="screensaver"]'); await card.waitFor();
    if (action === "Restore previous") {
      await card.getByRole("button", { name: "Off", exact: true }).click();
      await page.waitForFunction(() => document.querySelector('[data-tweak="screensaver"] .state')?.textContent === "Off");
    }
    await page.evaluate(() => {
      tweaks.screensaver_enabled = "0";
      calls.length = 0;
      handlers.write_setting = args => {
        if (args.key === "screensaver_components") return { ok: false, message: "Component refused" };
        tweaks[args.key] = args.value || null; return { ok: true, message: "done" };
      };
    });
    const button = () => card.getByRole("button", { name: action === "Restore previous" ? /Restore previous/ : action, exact: action !== "Restore previous" });
    await button().click(); await page.getByText(/Component refused/).waitFor();
    assert.equal(await page.evaluate(() => tweaks.screensaver_enabled), "0");
    assert.deepEqual((await writes(page, "write_setting")).map(a => a.key), ["screensaver_components"]);
    await page.evaluate(() => {
      calls.length = 0;
      handlers.write_setting = args => {
        if (args.key === "screensaver_enabled") return { ok: false, message: "Enable refused" };
        tweaks[args.key] = args.value || null; return { ok: true, message: "done" };
      };
    });
    await button().click(); await page.getByText(/switch was not confirmed/).waitFor();
    assert.equal(await page.evaluate(() => tweaks.screensaver_enabled), "0");
    assert.equal(await page.evaluate(() => tweaks.screensaver_components), action === "Restore previous" ? VENDOR : BASIC);
    assert.deepEqual((await writes(page, "write_setting")).map(a => a.key), ["screensaver_components", "screensaver_enabled"]);
  });
}

test("Off keeps the switch disabled if component clearing fails", async t => {
  const page = await open(t, "tweaks", { pro: true });
  await page.locator('[data-tweak="screensaver"]').waitFor();
  await page.evaluate(() => {
    handlers.write_setting = args => {
      if (args.key === "screensaver_components") return { ok: false, message: "Clear refused" };
      tweaks[args.key] = args.value || null; return { ok: true, message: "done" };
    };
  });
  await page.locator('[data-tweak="screensaver"]').getByRole("button", { name: "Off", exact: true }).click();
  await page.getByText(/component was not confirmed/).waitFor();
  assert.equal(await page.evaluate(() => tweaks.screensaver_enabled), "0");
  assert.equal(await page.evaluate(() => tweaks.screensaver_components), VENDOR);
  assert.deepEqual((await writes(page, "write_setting")).map(a => a.key), ["screensaver_enabled", "screensaver_components"]);
});

if (process.env.DAYDREAM_EVIDENCE === "1") test("capture simulated Daydream partial failure and recovery", async t => {
  const dir = "evidence/daydream-order-2026-10-17"; await mkdir(dir, { recursive: true });
  const page = await open(t, "tweaks", { pro: true, daydreamEvidence: true });
  const card = page.locator('[data-tweak="screensaver"]'); await card.waitFor();
  await card.getByRole("button", { name: "Off", exact: true }).click();
  await page.waitForFunction(() => document.querySelector('[data-tweak="screensaver"] .state')?.textContent === "Off");
  await card.scrollIntoViewIfNeeded();
  await page.evaluate(() => { handlers.write_setting = args => args.key === "screensaver_components" ? { ok: false, message: "Component write refused (simulated)" } : { ok: true, message: "done" }; });
  await card.getByRole("button", { name: "Basic Daydream", exact: true }).click();
  await page.getByText(/Component write refused \(simulated\)/).waitFor();
  await page.screenshot({ path: `${dir}/component-refused-off-simulated.png`, animations: "disabled" }); await page.waitForTimeout(800);
  assert.equal(await page.evaluate(() => tweaks.screensaver_enabled), "0");
  await page.evaluate(() => { delete handlers.write_setting; });
  await card.getByRole("button", { name: "Basic Daydream", exact: true }).click();
  await page.waitForFunction(() => document.querySelector('[data-tweak="screensaver"] .state')?.textContent === "Basic Daydream");
  await page.screenshot({ path: `${dir}/basic-daydream-readback-simulated.png`, animations: "disabled" }); await page.waitForTimeout(800);
  const video = page.video(); await page.close(); await video.saveAs(`${dir}/daydream-failure-recovery-simulated.webm`);
});

test("a reconnect after the Daydream component write prevents enabling", async t => {
  const page = await open(t, "tweaks", { pro: true });
  const card = page.locator('[data-tweak="screensaver"]'); await card.waitFor();
  await page.evaluate(() => {
    tweaks.screensaver_enabled = "0";
    handlers.write_setting = args => new Promise(resolve => { window.finishComponent = () => { tweaks[args.key] = args.value; resolve({ ok: true, message: "done" }); }; });
  });
  await card.getByRole("button", { name: "Basic Daydream", exact: true }).click();
  await page.waitForFunction(() => !!window.finishComponent);
  await page.evaluate(async () => { await session.connect("A", 5555); finishComponent(); });
  assert.deepEqual((await writes(page, "write_setting")).map(a => a.key), ["screensaver_components"]);
  assert.equal(await page.evaluate(() => tweaks.screensaver_enabled), "0");
});


test("density-only readback shows the override instead of physical density or device default", async t => {
  const page = await open(t, "tweaks");
  const card = page.locator('.tweak-row').filter({ has: page.getByText('Display scaling', { exact: true }) });
  await page.evaluate(() => { handlers.get_display_scaling = () => ({ size: 'Physical size: 3840x2160', density: 'Physical density: 640\nOverride density: 300' }); });
  await page.getByRole('button', { name: 'Re-read settings' }).click();
  await card.getByText(/Custom size \/ density/).waitFor();
  if (process.env.DENSITY_BEFORE === '1') { await mkdir('evidence/density-only-label-2026-10-17', { recursive: true }); await card.screenshot({ path: 'evidence/density-only-label-2026-10-17/before-simulated.png' }); }
  assert.equal(await card.locator('.t-info .t-desc').innerText(), '3840x2160 @ 300 · density override · Custom size / density');
  assert.equal(await card.locator('[aria-pressed="true"]').count(), 0);
  assert.equal(await card.getByText(/device default/).count(), 0);
  assert.deepEqual(await writes(page, 'set_display_scaling'), []);
  assert.deepEqual(await writes(page, 'write_setting'), []);
});


test("density-only matching preset, absent size and reset readbacks retain their distinct states", async t => {
  const page = await open(t, "tweaks");
  const card = page.locator('.tweak-row').filter({ has: page.getByText('Display scaling', { exact: true }) });
  for (const fixture of [
    { size: 'Physical size: 1920x1080', density: 'Physical density: 640\nOverride density: 320', text: '1920x1080 @ 320 · density override', selected: '1080p' },
    { size: '', density: 'Physical density: 640\nOverride density: 300', text: 'Size not reported @ 300 · density override · Custom size / density', selected: null },
    { size: 'Physical size: 3840x2160', density: 'Physical density: 640', text: '3840x2160 @ 640 · device default', selected: 'Reset' },
    { size: 'Physical size: 3840x2160\nOverride size: 1920x1080', density: 'Physical density: 320', text: '1920x1080 @ 320 · panel 3840x2160', selected: '1080p' },
    { size: '', density: '', text: '—', selected: null },
  ]) {
    await page.evaluate(fixture => { handlers.get_display_scaling = () => ({ size: fixture.size, density: fixture.density }); }, fixture);
    await page.getByRole('button', { name: 'Re-read settings' }).click();
    await card.getByText(fixture.text, { exact: true }).waitFor();
    assert.equal(await card.locator('[aria-pressed="true"]').count(), fixture.selected ? 1 : 0);
    if (fixture.selected) assert.equal(await card.getByRole('button', { name: fixture.selected, exact: true }).getAttribute('aria-pressed'), 'true');
  }
  assert.deepEqual(await writes(page, 'set_display_scaling'), []);
  assert.deepEqual(await writes(page, 'write_setting'), []);
});

if (process.env.DENSITY_EVIDENCE === '1') test("capture density-only label at narrow width and enlarged summary text", async t => {
  const page = await open(t, "tweaks");
  const card = page.locator('.tweak-row').filter({ has: page.getByText('Display scaling', { exact: true }) });
  await page.evaluate(() => { handlers.get_display_scaling = () => ({ size: 'Physical size: 3840x2160', density: 'Physical density: 640\nOverride density: 300' }); });
  await page.getByRole('button', { name: 'Re-read settings' }).click();
  await card.getByText('3840x2160 @ 300 · density override · Custom size / density', { exact: true }).waitFor();
  const dir = 'evidence/density-only-label-2026-10-17'; await mkdir(dir, { recursive: true });
  await card.screenshot({ path: `${dir}/after-384-simulated.png` });
  await page.setViewportSize({ width: 320, height: 812 });
  await card.evaluate(el => {
    const nodes = [el, ...el.querySelectorAll('*')];
    const sizes = nodes.map(node => parseFloat(getComputedStyle(node).fontSize));
    nodes.forEach((node, i) => { if (!node.classList.contains('msr')) node.style.fontSize = `${sizes[i] * 2}px`; });
  });
  await card.screenshot({ path: `${dir}/known-full-card-text-200-simulated.png` });
  const overflow = await card.evaluate(el => [...el.querySelectorAll('*')].filter(node => node.scrollWidth > node.clientWidth + 1).map(node => ({ text: node.textContent.trim(), width: node.clientWidth, content: node.scrollWidth })));
  await writeFile(`${dir}/full-card-text-200-observations.json`, JSON.stringify(overflow, null, 2) + '\n');
  await card.evaluate(el => {
    [el, ...el.querySelectorAll('*')].forEach(node => node.style.fontSize = '');
    const summary = el.querySelector('.t-info .t-desc');
    summary.style.fontSize = `${parseFloat(getComputedStyle(summary).fontSize) * 2}px`;
  });
  assert.equal(await card.evaluate(el => el.scrollWidth <= el.clientWidth), true);
  await card.screenshot({ path: `${dir}/after-320-summary-text-200-simulated.png` });
  assert.deepEqual(await writes(page, 'set_display_scaling'), []);
  assert.deepEqual(await writes(page, 'write_setting'), []);
});


for (const [width, scale] of [[320, 1], [384, 1], [320, 2], [384, 2]]) test(`scaling preset row fits ${width}px at ${scale * 100}% text and preserves controls`, async t => {
  const page = await open(t, 'tweaks', { pro: true });
  const card = page.locator('.tweak-row').filter({ has: page.getByText('Display scaling', { exact: true }) });
  await page.evaluate(() => { handlers.get_display_scaling = () => ({ size: 'Physical size: 3840x2160\nOverride size: 1920x1080', density: 'Physical density: 640\nOverride density: 320' }); });
  await page.getByRole('button', { name: 'Re-read settings' }).click();
  const row = card.locator('.segmented');
  await page.waitForFunction(() => [...document.querySelectorAll('button')].some(el => el.textContent.trim() === '1080p' && el.getAttribute('aria-pressed') === 'true'));
  await page.setViewportSize({ width, height: 812 });
  if (scale !== 1) await card.evaluate((el, scale) => {
    const nodes = [el, ...el.querySelectorAll('*')];
    const sizes = nodes.map(node => parseFloat(getComputedStyle(node).fontSize));
    nodes.forEach((node, i) => { if (!node.classList.contains('msr')) node.style.fontSize = `${sizes[i] * scale}px`; });
  }, scale);
  await row.evaluate(el => el.scrollIntoView({ block: 'center' }));
  const dir = 'evidence/scaling-preset-wrap-2026-10-17';
  if (process.env.SCALING_WRAP_BEFORE === '1') { await mkdir(dir, { recursive: true }); await row.screenshot({ path: `${dir}/before-${width}-${scale * 100}-simulated.png` }); }
  const dimensions = await row.evaluate(el => ({ width: el.clientWidth, content: el.scrollWidth }));
  assert.ok(dimensions.content <= dimensions.width, JSON.stringify(dimensions));
  assert.equal(await card.evaluate(el => el.scrollWidth <= el.clientWidth), true);
  assert.deepEqual((await row.getByRole('button').allTextContents()).map(x => x.trim()), ['4K', '1080p', '720p', 'Reset']);
  const buttons = row.getByRole('button');
  await buttons.first().focus();
  for (let i = 0; i < 4; i++) {
    const button = buttons.nth(i);
    assert.equal(await button.evaluate(el => document.activeElement === el), true);
    assert.equal(await button.evaluate(el => {
      const r = el.getBoundingClientRect();
      return r.width >= 44 && r.height >= 44 && r.left >= 0 && r.right <= innerWidth && r.top >= 0 && r.bottom <= innerHeight && el.scrollWidth <= el.clientWidth;
    }), true);
    await page.keyboard.press('Enter');
    const dialog = page.getByRole('dialog', { name: i === 3 ? 'Reset display scaling?' : 'Change display scaling?', exact: true });
    await dialog.waitFor(); await dialog.getByRole('button', { name: 'Cancel', exact: true }).click(); await dialog.waitFor({ state: 'detached' });
    assert.equal(await button.evaluate(el => document.activeElement === el), true);
    if (i < 3) await page.keyboard.press('Tab');
  }
  assert.equal(await row.getByRole('button', { name: '1080p', exact: true }).getAttribute('aria-pressed'), 'true');
  assert.deepEqual(await writes(page, 'set_display_scaling'), []);
  assert.deepEqual(await writes(page, 'write_setting'), []);
  if (process.env.SCALING_WRAP_EVIDENCE === '1') {
    await mkdir(dir, { recursive: true }); await row.evaluate(el => el.scrollIntoView({ block: 'center' }));
    await row.screenshot({ path: `${dir}/after-${width}-${scale * 100}-simulated.png` });
  }
});
