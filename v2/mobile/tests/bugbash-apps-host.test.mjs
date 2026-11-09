// The app sheet opened from Optimize and Diagnostics (AppSheetHost) must offer
// what the Apps screen and desktop do on every removal and detail panel, and
// Diagnostics' memory rows must keep the process name readable on a phone.
import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { chromium } from "playwright";
import { startViteServer } from "./helpers/vite-harness.mjs";

let server, origin, browser;
before(async () => { ({ server, origin } = await startViteServer()); browser = await chromium.launch({ headless: true }); });
after(async () => { await browser?.close(); await server?.close(); });

const pkg = "com.example.video";

async function open(t, screen, { entitlement = "pro", viewport = { width: 384, height: 812 }, topMemory = null } = {}) {
  const context = await browser.newContext({ viewport });
  const page = await context.newPage();
  page.setDefaultTimeout(10000);
  t.after(() => context.close());
  await page.addInitScript(({ entitlement, topMemory }) => {
    localStorage.clear();
    window.calls = [];
    window.handlers = {};
    window.copied = [];
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText: async (text) => void window.copied.push(text) } });
    const entry = { package: "com.example.video", name: "Example video", method: "disable", risk: "safe", optimize_description: "Synthetic optional video app.", restore_description: "Restore.", default_optimize: true, default_restore: false, play_store: true };
    window.__TAURI_INTERNALS__ = { invoke: async (command, args) => {
      window.calls.push({ command, args });
      if (window.handlers[command]) return window.handlers[command](args);
      switch (command) {
        case "get_entitlement": return entitlement;
        case "wireless_connect": return { ok: true };
        case "wireless_status": return { connected: true };
        case "list_devices": return [{ id: 1, serial: "A:5555", name: "Mock TV", model: "TV", status: "device", connection: "network", device_type: "shield", tv_evidence: "tv", support: { tier: "not_supported", note: "Synthetic unsupported model." }, properties: { android_release: "11", serial_number: "HW-SYNTH-1" } }];
        case "health_report": return { ram: { total_mb: 2048, free_mb: 1024 }, storage: {}, display: {}, top_memory: topMemory ?? [{ process: "com.example.video:remote", package: "com.example.video", pid: 1, mb: 55 }] };
        case "app_list_for_device": return [entry];
        case "package_states": return Object.fromEntries(args.packages.map((p) => [p, "enabled"]));
        case "list_installed_packages": return [{ package: entry.package, system: false, enabled: true }];
        case "list_other_packages": return [];
        case "prepare_optimize": return { mode: args.mode, items: [{ entry, action: { kind: "disable" }, memory_mb: 55 }] };
        case "safety_info": return { kind: "safe", reason: "Synthetic optional app.", source: "reviewed_catalog" };
        case "process_safety_info": return { kind: "unknown", reason: "Unresolved process." };
        case "app_memory_map": return { "com.example.video": 55 };
        case "app_usage_map": return {};
        case "app_storage_map": return { "com.example.video": { app_bytes: 4096, data_bytes: 1024, cache_bytes: 0 } };
        case "app_apk_size": return { app_bytes: 4096, data_bytes: null, cache_bytes: null };
        default: return { ok: true, message: "mock result" };
      }
    } };
  }, { entitlement, topMemory });
  await page.goto(origin);
  await page.evaluate(async (screen) => {
    const { session } = await import("/src/lib/session.svelte.ts");
    const { router } = await import("/src/lib/router.svelte.ts");
    window.session = session; window.router = router;
    await session.connect("A", 5555); router.reset("dashboard"); router.navigate(screen);
  }, screen);
  return page;
}

async function openSheet(page, screen) {
  const name = screen === "optimize" ? "Details for Example video" : "Details for com.example.video:remote";
  await page.getByRole("button", { name, exact: true }).click();
  const sheet = page.getByRole("dialog", { name: /App details for/ });
  await sheet.locator(".app-pkg", { hasText: pkg }).waitFor();
  await page.waitForFunction(() => {
    const b = [...document.querySelectorAll(".sheet button")].find((el) => /^Disable/.test(el.textContent.trim()));
    return b && !b.disabled;
  });
  return sheet;
}

for (const screen of ["optimize", "diagnostics"]) {
  test(`${screen}: the sheet's removal confirm carries the support-tier warning`, async (t) => {
    const page = await open(t, screen);
    const sheet = await openSheet(page, screen);
    await sheet.getByRole("button", { name: "Disable", exact: true }).click();
    const dialog = page.locator(".dialog-card");
    await dialog.waitFor();
    assert.match(await dialog.getByTestId("support-warning").innerText(), /This TV isn't supported: Synthetic unsupported model\./);
    await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
    assert.equal(await page.evaluate(() => calls.some((c) => c.command === "disable_package")), false);
  });

  test(`${screen}: the sheet offers Report this app with the shown verdict and the live state`, async (t) => {
    const page = await open(t, screen);
    const sheet = await openSheet(page, screen);
    await sheet.getByRole("button", { name: "Report this app" }).click();
    const report = page.getByRole("dialog", { name: "Report this app", exact: true });
    await report.getByText(pkg, { exact: true }).waitFor();
    await report.getByRole("checkbox").check();
    await report.getByRole("button", { name: "Show the full report" }).click();
    const preview = report.getByRole("textbox", { name: "Report preview" });
    const record = async () => JSON.parse(await preview.inputValue()).records[0];
    const first = await record();
    assert.equal(first.current_verdict, "safe");
    assert.deepEqual({ installed: first.state.installed, enabled: first.state.enabled }, { installed: true, enabled: true });
    assert.equal(first.state.storage.source, "diskstats");
    assert.doesNotMatch(await preview.inputValue(), /HW-SYNTH-1|A:5555/);
    // While a run is changing the TV, the state it read earlier may be stale.
    await page.evaluate(() => { session.applyInProgress = true; });
    await page.waitForFunction(() => JSON.parse(document.querySelector('[aria-label="Report preview"]').value).records[0].state.enabled === null);
    assert.equal((await record()).state.installed, null);
    await page.evaluate(() => { session.applyInProgress = false; });
    await report.getByRole("button", { name: "Close", exact: true }).click();
    await report.waitFor({ state: "detached" });
    await sheet.waitFor();
  });

  test(`${screen}: Research sources opens the catalog at this package`, async (t) => {
    const page = await open(t, screen);
    const sheet = await openSheet(page, screen);
    await sheet.getByRole("button", { name: "Research sources" }).click();
    await page.waitForFunction(() => router.current === "catalog");
    assert.equal(await page.evaluate(() => router.catalogPackage), pkg);
    assert.equal(await page.locator(".sheet").count(), 0);
  });

  test(`${screen}: Disable carries the PRO marker only on Free, like Uninstall`, async (t) => {
    const page = await open(t, screen, { entitlement: "free" });
    await page.evaluate(() => { session.entitlement = "free"; });
    const sheet = await openSheet(page, screen);
    const disable = sheet.locator(".act-btn", { hasText: /^Disable/ });
    await disable.locator(".pro-badge", { hasText: "PRO" }).waitFor();
    await page.evaluate(() => { session.entitlement = "pro"; });
    await disable.locator(".pro-badge").waitFor({ state: "detached" });
    assert.equal(await disable.innerText(), "Disable");
  });

  test(`${screen}: a removal whose reply lands after a silent redial still drops the shared caches`, async (t) => {
    const page = await open(t, screen);
    await page.evaluate(() => session.loadBloat(true));
    assert.equal(await page.evaluate(() => session.bloatLoaded), true);
    const sheet = await openSheet(page, screen);
    await page.evaluate(() => { handlers.disable_package = () => new Promise((resolve) => { window.finishDisable = resolve; }); });
    await sheet.getByRole("button", { name: "Disable", exact: true }).click();
    await page.locator(".dialog-card").getByRole("button", { name: "Disable", exact: true }).click();
    await page.waitForFunction(() => !!window.finishDisable);
    const before = await page.evaluate(() => session.generation);
    assert.equal(await page.evaluate(() => session.recoverOrMarkLost()), true);
    assert.notEqual(await page.evaluate(() => session.generation), before);
    assert.equal(await page.evaluate(() => session.bloatLoaded), true, "the redial alone keeps the cache");
    await page.evaluate(() => finishDisable({ ok: true, message: "" }));
    await page.waitForFunction(() => session.bloatLoaded === false && session.healthLoaded === false);
  });
}

for (const screen of ["optimize", "diagnostics"]) {
  test(`${screen}: Back up APK saves to this phone and says where, with no export claim`, async (t) => {
    const page = await open(t, screen);
    await page.evaluate(() => {
      handlers.backup_apk = (args) => ({ package: args.package, path: "/data/backups/video", size_bytes: 4096, apk_count: 2, complete: true, saved_at: "2026-11-08T00:00:00Z" });
    });
    const sheet = await openSheet(page, screen);
    // The removal line names how this Play Store app comes back.
    await sheet.getByText("After Uninstall: Get it back from the Play Store.", { exact: false }).waitFor();
    await sheet.getByRole("button", { name: "Back up APK" }).click();
    const note = sheet.getByRole("status").filter({ hasText: "APK parts" });
    await note.waitFor();
    assert.match(await note.innerText(), /Saved 2 APK parts to this phone, in ATV Optimizer's private storage\./);
    assert.doesNotMatch(await note.innerText(), /export|share|download/i);
    assert.deepEqual(
      await page.evaluate(() => calls.filter((c) => c.command === "backup_apk").map((c) => c.args)),
      [{ serial: "A:5555", package: pkg }],
    );
    await note.getByRole("button", { name: "View in Backups" }).click();
    await page.waitForFunction(() => router.current === "backups");
    assert.equal(await page.locator(".sheet").count(), 0);
  });
}

test("diagnostics: memory rows give the process name the row at 320px and show the whole suggestion", async (t) => {
  const page = await open(t, "diagnostics", {
    viewport: { width: 320, height: 640 },
    topMemory: [
      { process: "com.netflix.ninja.synthetic.long.process", package: "com.netflix.ninja.synthetic.long.process", pid: 2201, mb: 312 },
      { process: "vendor.nvidia.hardware.graphics.composer", package: null, pid: 3405, mb: 246 },
    ],
  });
  const rows = page.locator(".consumer-row");
  await rows.nth(1).waitFor();
  await page.waitForFunction(() => [...document.querySelectorAll(".risk-badge")].every((b) => !/Checking/.test(b.textContent)));
  const layout = await page.evaluate(() => [...document.querySelectorAll(".consumer-row")].map((row) => {
    const r = row.getBoundingClientRect();
    const name = row.querySelector(".consumer-name");
    const badge = row.querySelector(".risk-badge");
    const n = name.getBoundingClientRect();
    const b = badge.getBoundingClientRect();
    return {
      nameShare: n.width / r.width,
      nameClipped: name.scrollWidth > name.clientWidth + 1,
      badgeClipped: badge.scrollWidth > badge.clientWidth + 1,
      badgeInside: b.left >= r.left - 0.5 && b.right <= r.right + 0.5,
      pageOverflow: document.documentElement.scrollWidth > innerWidth,
    };
  }));
  for (const row of layout) {
    assert.ok(row.nameShare > 0.6, `process name gets ${Math.round(row.nameShare * 100)}% of the row`);
    assert.equal(row.nameClipped, false);
    assert.equal(row.badgeClipped, false);
    assert.equal(row.badgeInside, true);
    assert.equal(row.pageOverflow, false);
  }
});
