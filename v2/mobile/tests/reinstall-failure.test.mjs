import assert from "node:assert/strict";
import { mkdir, readFile } from "node:fs/promises";
import { after, before, test } from "node:test";
import { chromium } from "playwright";
import { startViteServer } from "./helpers/vite-harness.mjs";

let server, origin, browser;
const evidence = process.env.REINSTALL_FAILURE_EVIDENCE;
const catalog = JSON.parse(await readFile(new URL("../../crates/core/data/app-lists/common.json", import.meta.url)));
const music = catalog.find((entry) => entry.package === "com.google.android.music");
assert.ok(music?.defunct && !music.play_store);

before(async () => {
  ({ server, origin } = await startViteServer());
  browser = await chromium.launch();
  if (evidence) await mkdir(evidence, { recursive: true });
});
after(async () => { await browser?.close(); await server?.close(); });

async function open(t, screen, { entry = music, width = 384, capture = false } = {}) {
  const context = await browser.newContext({
    viewport: { width, height: 812 },
    ...(capture && evidence ? { recordVideo: { dir: `${evidence}/raw`, size: { width, height: 812 } } } : {}),
  });
  t.after(() => context.close());
  const page = await context.newPage();
  page.setDefaultTimeout(10000);
  await page.addInitScript(({ entry }) => {
    localStorage.clear();
    window.calls = [];
    window.removed = false;
    window.__TAURI_INTERNALS__ = { invoke: async (command, args) => {
      window.calls.push({ command, args });
      switch (command) {
        case "get_entitlement": return "pro";
        case "wireless_connect": return { ok: true };
        case "wireless_status": return { connected: true };
        case "list_devices": return [{ id: 1, serial: "fixture:5555", name: "Simulated TV", model: "TV", status: "device", connection: "network", device_type: "shield", tv_evidence: "tv", properties: { serial_number: "FIXTURE-ONLY" } }];
        case "health_report": return { ram: { total_mb: 2048, free_mb: 1024 }, storage: {}, display: {}, top_memory: [{ process: entry.package, package: entry.package, pid: 1, mb: 10 }] };
        case "app_list_for_device": return [entry];
        case "package_states": return Object.fromEntries(args.packages.map((p) => [p, window.removed ? "missing" : "enabled"]));
        case "list_installed_packages": return [{ package: entry.package, system: true, enabled: true }];
        case "list_other_packages": return [];
        case "prepare_optimize": return { mode: args.mode, items: [{ entry, action: { kind: "uninstall" }, memory_mb: 10 }] };
        case "safety_info": return { kind: "safe", reason: entry.optimize_description, source: "reviewed_catalog" };
        case "process_safety_info": return { kind: "unknown", reason: "Synthetic process." };
        case "app_memory_map": case "app_usage_map": case "app_storage_map": return {};
        case "app_apk_size": return { app_bytes: 4096, data_bytes: null, cache_bytes: null };
        case "uninstall_package": window.removed = true; return { ok: true, message: "" };
        case "reinstall_existing": return { ok: false, message: window.reinstallFailure ?? "" };
        default: return { ok: true, message: "" };
      }
    } };
  }, { entry });
  await page.goto(origin, { waitUntil: "networkidle" });
  await page.evaluate(async (screen) => {
    const { session } = await import("/src/lib/session.svelte.ts");
    const { router } = await import("/src/lib/router.svelte.ts");
    await session.connect("fixture", 5555);
    router.reset("dashboard"); router.navigate(screen);
  }, screen);
  await page.evaluate(() => {
    const badge = document.createElement("div");
    badge.textContent = "SIMULATION · no phone or TV";
    badge.style = "position:fixed;top:0;left:0;right:0;text-align:center;background:#132431;color:white;font:11px sans-serif;z-index:9999;padding:2px";
    document.body.append(badge);
  });
  if (screen === "apps") await page.locator(`.app-row[data-package="${entry.package}"]`).click();
  else await page.getByRole("button", { name: `Details for ${screen === "optimize" ? entry.name : entry.package}`, exact: true }).click();
  const sheet = page.getByRole("dialog", { name: `App details for ${entry.name}` });
  await sheet.waitFor();
  return { context, page, sheet };
}

async function capture(page, name) {
  if (!evidence) return;
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(450);
  await page.screenshot({ path: `${evidence}/${name}.png` });
}

const mutations = (page) => page.evaluate(() => calls.filter((call) => ["uninstall_package", "disable_package", "enable_package", "reinstall_existing", "apply_performance_settings"].includes(call.command)));

const cases = [
  { name: "discontinued", entry: music, guidance: /discontinued and has no store listing/ },
  { name: "store", entry: { ...music, name: "Store app", defunct: false, play_store: true }, guidance: /Get it back from the Play Store/ },
  { name: "sideload", entry: { ...music, name: "SmartTube", package: "org.smarttube.stable", defunct: false }, guidance: /reinstall SmartTube from https:/ },
];

for (const screen of ["apps", "optimize", "diagnostics"]) {
  for (const scenario of cases) {
    test(`${screen}: failed Reinstall keeps the ${scenario.name} recovery source`, async (t) => {
      const { entry, guidance } = scenario;
      const record = screen === "apps" && scenario.name === "discontinued";
      const { context, page, sheet } = await open(t, screen, { entry, capture: record });
      const dialog = page.locator(".dialog-card");
      await sheet.getByRole("button", { name: "Uninstall", exact: true }).click();
      await dialog.waitFor();
      assert.match(await dialog.innerText(), guidance);
      if (record) await capture(page, "discontinued-confirm");
      await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
      await dialog.waitFor({ state: "detached" });
      assert.deepEqual(await mutations(page), []);
      if (screen === "apps") await page.locator(`.app-row[data-package="${entry.package}"]`).click();
      await sheet.getByRole("button", { name: "Uninstall", exact: true }).click();
      await dialog.waitFor();
      await page.keyboard.press("Escape");
      await dialog.waitFor({ state: "detached" });
      assert.deepEqual(await mutations(page), []);
      if (screen === "apps") await page.locator(`.app-row[data-package="${entry.package}"]`).click();
      await sheet.getByRole("button", { name: "Uninstall", exact: true }).click();
      await dialog.getByRole("button", { name: "Uninstall", exact: true }).click();
      await sheet.getByRole("button", { name: /Reinstall/ }).click();
      const failure = screen === "apps" ? page.locator(".en-msg").filter({ hasText: /^Reinstall failed\./ }) : page.getByText(/^Reinstall failed\./);
      await failure.waitFor();
      assert.match(await failure.innerText(), guidance);
      assert.doesNotMatch(await failure.innerText(), /Try the Play Store/);
      assert.deepEqual((await mutations(page)).map((call) => [call.command, call.args]), [
        ["uninstall_package", { serial: "fixture:5555", package: entry.package }],
        ["reinstall_existing", { serial: "fixture:5555", package: entry.package }],
      ]);
      if (screen === "apps") await sheet.getByRole("button", { name: "Close", exact: true }).click();
      if (evidence && scenario.name === "discontinued") await capture(page, `${screen}-reinstall-failed`);
      if (record && evidence) {
        await page.waitForTimeout(1500);
        const video = page.video();
        await context.close();
        await video.saveAs(`${evidence}/reinstall-failure-simulated.webm`);
      }
    });
  }

  test(`${screen}: Reinstall preserves a specific backend failure`, async (t) => {
    const { page, sheet } = await open(t, screen);
    await page.evaluate(() => { window.reinstallFailure = "Package is not installed for user 0."; });
    await sheet.getByRole("button", { name: "Uninstall", exact: true }).click();
    await page.locator(".dialog-card").getByRole("button", { name: "Uninstall", exact: true }).click();
    await sheet.getByRole("button", { name: /Reinstall/ }).click();
    if (screen === "apps") {
      await sheet.getByRole("button", { name: "Close", exact: true }).click();
      await page.locator(".en-details summary").click();
    }
    await page.getByText("Package is not installed for user 0.", { exact: true }).waitFor();
    assert.equal(await page.getByText(/^Reinstall failed\./).count(), 0);
  });
}
