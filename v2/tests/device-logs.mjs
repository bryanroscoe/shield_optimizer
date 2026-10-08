import assert from "node:assert/strict";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
const V2 = join(dirname(fileURLToPath(import.meta.url)), "..");

async function exercise(browser, base) {
  const page = await browser.newPage({ viewport: { width: 1500, height: 1000 } });
  const errors = [];
  page.on("pageerror", e => errors.push(String(e)));
  await page.addInitScript(() => {
    window.__LOG_TEST__ = { calls: [], hold: false, pending: [], stdout: "First synthetic entry\nSecond <img src=x onerror=alert(1)> entry\n" };
    const timer = setInterval(() => {
      const bridge = window.__TAURI_INTERNALS__;
      if (!bridge?.invoke) return;
      clearInterval(timer);
      const original = bridge.invoke.bind(bridge);
      bridge.invoke = async (command, args = {}) => {
        if (command !== "read_device_logs") return original(command, args);
        const state = window.__LOG_TEST__;
        state.calls.push({ command, args });
        const stdout = state.stdout;
        if (state.hold) await new Promise((resolve, reject) => state.pending.push({ resolve, reject }));
        return { package: args.options.package, pid: args.options.package ? 123 : null, output: { stdout, stderr: "", exit_code: 0, termination: "completed" } };
      };
    }, 0);
  });
  await page.goto(base, { waitUntil: "networkidle" });
  await page.getByText("NVIDIA SHIELD", { exact: false }).first().click();
  await page.getByRole("tab", { name: "Logs", exact: true }).click();
  const logs = page.locator("#tabpanel-logs");
  const output = logs.getByLabel("Log output");
  const count = () => page.evaluate(() => window.__LOG_TEST__.calls.length);
  assert.equal(await count(), 0, "entering the view never starts a device read");
  await logs.getByLabel("Minimum severity").selectOption("warning");
  await logs.getByLabel("Recent entries").selectOption("500");
  await logs.getByLabel("Tag (optional)").fill(" ActivityManager ");
  await logs.getByLabel("App package (optional)").fill("com.example.app");
  await logs.getByRole("button", { name: "Read logs", exact: true }).click();
  await output.waitFor();
  assert.deepEqual(await page.evaluate(() => window.__LOG_TEST__.calls[0].args.options), {
    priority: "warning", lines: 500, tag: "ActivityManager", package: "com.example.app",
  });
  assert.match(await output.innerText(), /<img src=x/);
  assert.equal(await output.locator("img").count(), 0, "log text never becomes HTML");
  await logs.getByLabel("Find in this snapshot").fill("second");
  assert.doesNotMatch(await output.innerText(), /First/);
  assert.match(await output.innerText(), /Second/);
  assert.equal(await count(), 1, "text search is local");

  // Stopping cannot kill the bounded native read, but must discard its late result.
  await page.evaluate(() => { window.__LOG_TEST__.hold = true; window.__LOG_TEST__.stdout = "LATE STOPPED OUTPUT"; });
  await logs.getByRole("button", { name: "Auto-refresh", exact: true }).click();
  await page.waitForFunction(() => window.__LOG_TEST__.pending.length === 1);
  assert.equal(await logs.getByLabel("Minimum severity").isDisabled(), true);
  await logs.getByRole("button", { name: "Stop refreshing", exact: true }).click();
  await page.evaluate(() => window.__LOG_TEST__.pending.shift().resolve());
  await page.waitForFunction(() => !document.querySelector("#tabpanel-logs button").disabled);
  assert.doesNotMatch(await logs.innerText(), /LATE STOPPED OUTPUT/);

  // An old view's in-flight logs cannot appear after leaving and reopening it.
  await logs.getByRole("button", { name: "Read logs", exact: true }).click();
  await page.waitForFunction(() => window.__LOG_TEST__.pending.length === 1);
  await page.getByRole("tab", { name: "Overview", exact: true }).click();
  await page.getByRole("tab", { name: "Logs", exact: true }).click();
  await page.evaluate(() => window.__LOG_TEST__.pending.shift().resolve());
  assert.equal(await output.count(), 0, "new view starts with no old snapshot");

  // A failed capture stops automatic retries instead of hammering an unsupported TV.
  await logs.getByRole("button", { name: "Auto-refresh", exact: true }).click();
  await page.waitForFunction(() => window.__LOG_TEST__.pending.length === 1);
  await page.evaluate(() => window.__LOG_TEST__.pending.shift().reject("logcat option unsupported"));
  await logs.getByRole("alert").waitFor();
  assert.match(await logs.getByRole("alert").innerText(), /unsupported/);
  assert.equal(await logs.getByRole("button", { name: "Stop refreshing" }).count(), 0);

  // Hiding the document also stops and invalidates a read, without automatic resume.
  await logs.getByRole("button", { name: "Auto-refresh", exact: true }).click();
  await page.waitForFunction(() => window.__LOG_TEST__.pending.length === 1);
  await page.evaluate(() => {
    Object.defineProperty(document, "visibilityState", { configurable: true, value: "hidden" });
    document.dispatchEvent(new Event("visibilitychange"));
    window.__LOG_TEST__.pending.shift().resolve();
  });
  await page.waitForFunction(() => !document.querySelector("#tabpanel-logs [role=status]").textContent.includes("Reading"));
  assert.equal(await logs.getByRole("button", { name: "Auto-refresh", exact: true }).isDisabled(), true);
  assert.equal(await output.count(), 0);
  assert.deepEqual(errors, []);
  await page.close();
  console.log("Device logs: exact filters, escaped raw text, local search, stop/navigation/hidden lifecycle and error handling pass.");
}

const keys = ["VITE_DEMO", "TAURI_DEV_HOST"];
const previous = new Map(keys.map(key => [key, process.env[key]]));
process.env.VITE_DEMO = "1";
delete process.env.TAURI_DEV_HOST;
let browser, server;
try {
  const { createServer } = await import("vite");
  const { chromium } = await import("playwright");
  server = await createServer({ root: V2, server: { host: "127.0.0.1", port: 0, strictPort: false, hmr: false } });
  await server.listen();
  const address = server.httpServer.address();
  browser = await chromium.launch();
  await exercise(browser, `http://127.0.0.1:${address.port}`);
} finally {
  await browser?.close(); await server?.close();
  for (const [key, value] of previous) { if (value === undefined) delete process.env[key]; else process.env[key] = value; }
}
