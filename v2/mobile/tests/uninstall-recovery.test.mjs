import assert from "node:assert/strict";
import { mkdir, readFile } from "node:fs/promises";
import { after, before, test } from "node:test";
import { chromium } from "playwright";
import { startViteServer } from "./helpers/vite-harness.mjs";

let server, origin, browser;
const evidence = process.env.UNINSTALL_RECOVERY_EVIDENCE;
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
        case "reinstall_existing": return { ok: false, message: "" };
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

for (const screen of ["apps", "optimize", "diagnostics"]) {
  test(`${screen}: discontinued-app confirmation and failed recovery never promise Play Store`, async (t) => {
    const { context, page, sheet } = await open(t, screen, { capture: screen === "apps" });
    assert.doesNotMatch(await sheet.innerText(), /anything else needs the Play Store/);
    await sheet.getByRole("button", { name: "Uninstall", exact: true }).click();
    const dialog = page.locator(".dialog-card");
    await dialog.waitFor();
    const text = await dialog.innerText();
    assert.match(text, /discontinued and has no Play Store listing/);
    assert.match(text, /recovery requires a saved APK/);
    assert.match(text, /Reinstall works only while its APK remains on the TV/);
    assert.doesNotMatch(text, /otherwise use the Play Store|Get it back from the Play Store/);
    await capture(page, `${screen}-discontinued-confirm`);
    await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
    await dialog.waitFor({ state: "detached" });
    assert.deepEqual(await mutations(page), []);
    if (screen === "apps") await page.locator(`.app-row[data-package="${music.package}"]`).click();
    await sheet.getByRole("button", { name: "Uninstall", exact: true }).click();
    await dialog.waitFor();
    await page.keyboard.press("Escape");
    await dialog.waitFor({ state: "detached" });
    assert.deepEqual(await mutations(page), []);
    if (screen === "apps") await page.locator(`.app-row[data-package="${music.package}"]`).click();
    await sheet.getByRole("button", { name: "Uninstall", exact: true }).click();
    await dialog.getByRole("button", { name: "Uninstall", exact: true }).click();
    await sheet.getByRole("button", { name: /Reinstall/ }).click();
    await page.getByText(/Reinstall failed\. This service is discontinued/).waitFor();
    assert.deepEqual((await mutations(page)).map((call) => call.command), ["uninstall_package", "reinstall_existing"]);
    if (screen === "apps") await sheet.getByRole("button", { name: "Close", exact: true }).click();
    await capture(page, `${screen}-recovery-failed`);
    if (screen === "apps" && evidence) {
      await page.waitForTimeout(1200);
      const video = page.video();
      await context.close();
      await video.saveAs(`${evidence}/uninstall-recovery-simulated.webm`);
    }
  });
}

test("discontinued warning fits narrow text-equivalent viewport and Cancel stays reachable", async (t) => {
  const { page, sheet } = await open(t, "apps", { width: 275 });
  await sheet.getByRole("button", { name: "Uninstall", exact: true }).click();
  const dialog = page.locator(".dialog-card");
  await dialog.waitFor();
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
  const cancel = dialog.getByRole("button", { name: "Cancel", exact: true });
  const box = await cancel.boundingBox();
  assert.ok(box.y >= 0 && box.y + box.height <= 812);
  await capture(page, "narrow-discontinued-confirm");
  await cancel.click();
  assert.deepEqual(await mutations(page), []);
});

for (const [name, entry, expected] of [
  ["store", { ...music, name: "Store app", defunct: false, play_store: true }, /Get it back from the Play Store/],
  ["sideload", { ...music, name: "SmartTube", package: "org.smarttube.stable", defunct: false }, /Not on the Play Store.*SmartTube.*https:/s],
]) {
  test(`${name}: Apps confirmation preserves the actual recovery source`, async (t) => {
    const { page, sheet } = await open(t, "apps", { entry });
    await sheet.getByRole("button", { name: "Uninstall", exact: true }).click();
    const dialog = page.locator(".dialog-card");
    await dialog.waitFor();
    assert.match(await dialog.innerText(), expected);
    assert.doesNotMatch(await dialog.innerText(), /discontinued and has no Play Store listing/);
    await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
    assert.deepEqual(await mutations(page), []);
  });
}

test("Optimize run confirmation warns that discontinued apps may be unavailable", async (t) => {
  const { page, sheet } = await open(t, "optimize");
  await sheet.getByRole("button", { name: "Close", exact: true }).click();
  await page.getByRole("button", { name: /Apply optimization/ }).click();
  const dialog = page.locator(".dialog-card");
  await dialog.waitFor();
  assert.match(await dialog.innerText(), /Uninstall Google Play Music/);
  assert.match(await dialog.innerText(), /discontinued apps may be unavailable/);
  assert.doesNotMatch(await dialog.innerText(), /otherwise use the Play Store/i);
  await capture(page, "optimize-run-confirm");
  await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
  await dialog.waitFor({ state: "detached" });
  assert.deepEqual(await mutations(page), []);
});
