import assert from "node:assert/strict";
import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { after, before, test } from "node:test";
import { chromium } from "playwright";
import { startViteServer } from "./helpers/vite-harness.mjs";

let server, browser, origin;
before(async () => {
  ({ server, origin } = await startViteServer());
  browser = await chromium.launch();
});
after(async () => { await browser?.close(); await server?.close(); });

test("public store/developer links survive the real mobile report sheet, clipboard and Details", async () => {
  const evidence = process.env.APP_REPORT_EVIDENCE_DIR;
  if (evidence) mkdirSync(evidence, { recursive: true });
  const context = await browser.newContext({ viewport: { width: 430, height: 932 },
    ...(evidence ? { recordVideo: { dir: evidence, size: { width: 430, height: 932 } } } : {}) });
  const page = await context.newPage();
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  try {
    await page.addInitScript(() => {
      localStorage.clear();
      window.__OPENED__ = [];
      window.__COPIED__ = [];
      window.__TAURI_INTERNALS__ = { invoke: async (command, args) => {
        if (command === "plugin:opener|open_url") window.__OPENED__.push(args.url);
        if (command === "get_entitlement") return "free";
        if (command === "wireless_status") return { connected: false, host: null, serial: null };
        return null;
      } };
      Object.defineProperty(navigator, "clipboard", { configurable: true,
        value: { writeText: async (text) => void window.__COPIED__.push(text) } });
    });
    await page.goto(origin);
    await page.evaluate(async () => {
      const { mountReport } = await import("/tests/helpers/report-sheet.ts");
      const target = document.createElement("div");
      document.body.append(target);
      mountReport(target, { package: "com.weathergroup.localnow", appName: "Local Now", verdict: null,
        device: { family: "shield", androidVersion: "11", redact: ["SECRET123", "192.0.2.10:5555"] },
        appState: { status: null, running: null, ramMb: null, storage: null }, appVersion: "0.1.2", onClose() {} });
      const label = document.createElement("div");
      label.textContent = "SIMULATION • report sheet fixture • no TV • issue intercepted";
      label.style.cssText = "position:fixed;top:0;left:0;right:0;z-index:99999;background:#ffda7b;color:#111;padding:6px;text-align:center;font:14px system-ui;pointer-events:none";
      document.body.append(label);
    });
    const dialog = page.getByRole("dialog", { name: "Report this app" });
    const store = "https://play.google.com/store/apps/details?id=com.weathergroup.localnow";
    const developer = "https://developer.example.com/support";
    await dialog.getByLabel("Note (optional)").fill(`${store}\n${developer}\nSECRET123 com.private.app 192.0.2.10`);
    await dialog.getByRole("button", { name: "Show the full report" }).click();
    const text = await dialog.getByLabel("Report preview").inputValue();
    const note = JSON.parse(text).records[0].note;
    assert.equal(note, `${store}\n${developer}\n[redacted] [redacted] [redacted]`);
    await dialog.getByRole("button", { name: /Copy report/ }).click();
    assert.equal(await page.evaluate(() => window.__COPIED__.at(-1)), text);
    await dialog.getByRole("button", { name: "Open GitHub issue" }).click();
    await page.waitForFunction(() => window.__OPENED__.length > 0);
    const url = new URL(await page.evaluate(() => window.__OPENED__.at(-1)));
    assert.equal(url.searchParams.get("report"), text);
    assert.equal(url.searchParams.get("details"), note);
    if (evidence) await page.screenshot({ path: join(evidence, "mobile-public-link-simulated.png") });
    assert.deepEqual(errors, []);
  } finally {
    await context.close();
    if (evidence) await page.video().saveAs(join(evidence, "mobile-report-flow-simulated.webm"));
  }
});
