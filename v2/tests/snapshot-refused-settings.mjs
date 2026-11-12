// A snapshot file can be edited or shared, so a restore plan lists settings
// the file names that snapshots never restore (`settings_refused`). Both
// desktop previews say how many will be skipped, with the keys behind help
// on demand, and show nothing when the list is empty.
//
// Run via `node tests/snapshot-refused-settings.mjs` from v2/.
// REFUSED_SETTINGS_EVIDENCE=<dir> saves a screenshot of each warning.
import { freePort } from "./lib/free-port.mjs";
import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const V2 = join(dirname(fileURLToPath(import.meta.url)), "..");
const REFUSED = ["global:adb_enabled", "secure:install_non_market_apps"];
const evidence = process.env.REFUSED_SETTINGS_EVIDENCE;

async function main() {
  if (evidence) await mkdir(evidence, { recursive: true });
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
    const page = await browser.newPage({ viewport: { width: 1280, height: 1000 }, ...(evidence ? { recordVideo: { dir: evidence, size: { width: 1280, height: 1000 } } } : {}) });
    page.setDefaultTimeout(10000);
    const errors = [];
    page.on("pageerror", (error) => errors.push(error.message));
    await page.goto(`http://127.0.0.1:${server.httpServer.address().port}`, { waitUntil: "networkidle" });
    await page.evaluate((refused) => {
      const original = window.__TAURI_INTERNALS__.invoke;
      window.__REFUSED__ = refused;
      window.__TAURI_INTERNALS__.invoke = async (command, args) => {
        if (command === "apply_snapshot") throw new Error("Unexpected restore in a preview test");
        const result = await original(command, args);
        if (command === "preview_apply") return { ...result, settings_refused: [...window.__REFUSED__] };
        return result;
      };
    }, REFUSED);

    const line = "2 settings in this file are not ones snapshots restore and will be skipped.";
    async function checkWarning(scope, shot) {
      const absent = scope.locator('[data-plan-item="absent-skipped"]');
      await absent.getByText("Recorded absent apps are left unchanged because saved-TV hardware identity is unverified.").waitFor();
      await absent.locator("summary").click();
      await absent.getByText("tv.twitch.android.app", { exact: true }).waitFor();
      assert.equal(await scope.locator("table").getByText("tv.twitch.android.app", { exact: true }).count(), 0, "recorded absence never appears among actions");
      if (evidence) await scope.screenshot({ path: join(evidence, shot.replace("refused-settings", "absent-unchanged")) });
      const warning = scope.locator('[data-plan-item="refused"]');
      await warning.getByText(line).waitFor();
      const help = warning.locator("summary", { hasText: "Skipped settings" });
      assert.equal(await warning.getByText(REFUSED[0]).isVisible(), false, "keys stay behind help until asked");
      await help.focus();
      await page.keyboard.press("Enter");
      for (const key of REFUSED) await warning.getByText(key, { exact: true }).waitFor();
      if (evidence) await warning.screenshot({ path: join(evidence, shot) });
      await page.keyboard.press("Escape");
      assert.equal(await warning.getByText(REFUSED[0]).isVisible(), false, "Escape closes the list");
    }

    // Device page, Snapshot tab.
    await page.getByText("NVIDIA SHIELD", { exact: false }).first().click();
    await page.locator("#tab-snapshot").click();
    await page.locator(".snap-list li").first().getByRole("button", { name: "Preview restore", exact: true }).click();
    await page.getByRole("button", { name: "Restore this snapshot", exact: true }).waitFor();
    await checkWarning(page.locator("#tabpanel-snapshot"), "desktop-device-refused-settings.png");

    await page.evaluate(() => { window.__REFUSED__ = []; });
    await page.locator(".snap-list li").nth(1).getByRole("button", { name: "Preview restore", exact: true }).click();
    await page.getByRole("button", { name: "Restore this snapshot", exact: true }).waitFor();
    assert.equal(await page.locator('[data-plan-item="refused"]').count(), 0, "no warning when nothing is refused");

    // Snapshots page.
    await page.evaluate(() => { window.__REFUSED__ = ["global:adb_enabled"]; });
    await page.getByRole("link", { name: "Snapshots", exact: true }).click();
    const select = page.getByRole("combobox", { name: /^Restore .* to a device$/ }).first();
    await select.waitFor();
    const value = await select.locator("option").nth(1).getAttribute("value");
    await select.selectOption(value);
    const panel = page.locator(".preview-panel");
    await panel.getByText("1 setting in this file is not one snapshots restore and will be skipped.").waitFor();
    await page.evaluate(() => { window.__REFUSED__ = ["global:adb_enabled", "secure:install_non_market_apps"]; });
    await page.getByRole("button", { name: "Close", exact: true }).click();
    await select.selectOption(value);
    await checkWarning(panel, "desktop-snapshots-refused-settings.png");

    assert.deepEqual(errors, [], `no page errors: ${errors.join("\n")}`);
    console.log("Snapshot refused settings passed: both previews count skipped settings, list them on demand, and stay quiet when none.");
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
