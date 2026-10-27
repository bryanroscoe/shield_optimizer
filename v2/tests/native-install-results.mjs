import assert from "node:assert/strict";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";
import { chromium } from "playwright";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const fixtures = JSON.parse(await readFile(resolve(root, "tests/fixtures/native-install-ui.json"), "utf8"));
const evidence = process.env.NATIVE_INSTALL_UI_EVIDENCE;
const previousDemo = process.env.VITE_DEMO;
process.env.VITE_DEMO = "1";
let server, browser, context;
try {
  if (evidence) await mkdir(evidence, { recursive: true });
  server = await createServer({ root, server: { host: "127.0.0.1", port: 0, hmr: false } });
  await server.listen();
  browser = await chromium.launch();
  context = await browser.newContext({
    viewport: { width: 1440, height: 1000 },
    ...(evidence ? { recordVideo: { dir: resolve(evidence, "raw") } } : {}),
  });
  await context.addInitScript(() => {
    localStorage.setItem("shieldopt.lastApkFolder", "/simulation");
    document.addEventListener("DOMContentLoaded", () => {
      const banner = document.createElement("div");
      banner.id = "native-install-simulation";
      banner.textContent = "SIMULATION — demo app and Rust command fixtures; no TV or APK installation";
      banner.style.cssText = "position:fixed;top:0;left:0;right:0;z-index:99999;background:#182238;color:white;padding:8px 16px;font:14px system-ui";
      document.body.append(banner);
    });
  });
  const page = await context.newPage();
  const receipts = [];
  for (const fixture of fixtures.cases) {
    await page.goto(`http://127.0.0.1:${server.httpServer.address().port}`, { waitUntil: "networkidle" });
    await page.evaluate((fixture) => {
      const original = window.__TAURI_INTERNALS__.invoke;
      window.__INSTALL_CALLS__ = [];
      window.__TAURI_INTERNALS__.invoke = async (command, args) => {
        if (command === "adb_status") return { backend: "direct", available: true, path: null, last_probe: null };
        if (command === "list_apks_in_folder") return [{ path: "/simulation/native-fixture.apk", name: "native-fixture.apk", size_bytes: 8193, package: null }];
        if (command === "install_apk") {
          window.__INSTALL_CALLS__.push(args);
          await new Promise((resolve) => setTimeout(resolve, 400));
          if (fixture.error) throw fixture.error;
          return fixture.result;
        }
        return original(command, args);
      };
      const banner = document.getElementById("native-install-simulation");
      banner.textContent = `SIMULATION — ${fixture.name}; Rust command fixtures, no TV or APK installation`;
    }, fixture);
    await page.getByRole("button", { name: "Refresh", exact: true }).click();
    await page.getByText("NVIDIA SHIELD", { exact: false }).first().click();
    await page.getByRole("tab", { name: "Install APK", exact: true }).click();
    await page.getByRole("button", { name: "Scan folder", exact: true }).click();
    const install = page.locator(".apk-list").getByRole("button", { name: "Install", exact: true });
    await install.waitFor();
    assert.equal(await page.evaluate(() => window.__INSTALL_CALLS__.length), 0);
    await install.focus();
    await install.press("Enter");
    assert.equal(await page.getByRole("button", { name: "Installing…", exact: true })
      .evaluateAll((buttons) => buttons.length === 2 && buttons.every((button) => button.disabled)), true);
    const result = page.locator(".apk-list .install-result");
    await result.waitFor();
    const text = await result.innerText();
    if (fixture.result?.ok) {
      assert.equal(await result.evaluate((el) => el.classList.contains("ok")), true);
      assert.match(text, /Installed\./);
    } else {
      assert.equal(await result.evaluate((el) => el.classList.contains("bad")), true);
      assert.equal(await page.locator(".install-result.ok").count(), 0);
      assert.doesNotMatch(text, /Installed\./);
      if (fixture.error) assert.ok(text.includes(fixture.error));
      else if (fixture.name === "unconfirmed") {
        assert.match(text, /Installation not confirmed\./);
        assert.match(text, /Check installed apps before retrying\./);
        assert.equal(fixture.result.hint, null);
      } else if (fixture.result.hint) assert.ok(text.includes(fixture.result.hint));
    }
    await page.waitForTimeout(250);
    const calls = await page.evaluate(() => window.__INSTALL_CALLS__);
    assert.deepEqual(calls, [{ serial: "192.168.1.42:5555", apkPath: "/simulation/native-fixture.apk", reinstall: true }]);
    await result.scrollIntoViewIfNeeded();
    if (evidence) {
      await page.screenshot({ path: resolve(evidence, `${fixture.name}-simulated.png`) });
      await page.waitForTimeout(900);
    }
    receipts.push({ case: fixture.name, text, calls });
    console.log(`PASS ${fixture.name}: truthful result and one explicit install attempt`);
  }
  const video = page.video();
  await context.close();
  context = undefined;
  if (evidence) {
    await video.saveAs(resolve(evidence, "native-install-results-simulated.webm"));
    await writeFile(resolve(evidence, "ui-receipt.json"), JSON.stringify({ simulation: true, fixtures: fixtures.provenance, cases: receipts }, null, 2));
  }
} finally {
  await context?.close();
  await browser?.close();
  await server?.close();
  if (previousDemo === undefined) delete process.env.VITE_DEMO;
  else process.env.VITE_DEMO = previousDemo;
}
