import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { chromium } from "playwright";
import { startViteServer } from "./helpers/vite-harness.mjs";

// Device logs (desktop Logs tab parity) and the bug report on More: reads
// start only on request, filters reach the backend trimmed, log text never
// becomes HTML, a failed or partial read says so, a stopped read's late result
// is dropped, and the bug report is reviewed before it reaches the clipboard.

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

async function open(t, screen) {
  const page = await browser.newPage({ viewport: { width: 384, height: 1400 } });
  t.after(() => page.close());
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  t.after(() => assert.deepEqual(errors, []));
  await page.addInitScript(() => {
    window.calls = [];
    window.handlers = {};
    window.clipboardText = null;
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: async (text) => { window.clipboardText = text; } },
    });
    window.__TAURI_INTERNALS__ = {
      transformCallback: () => 0,
      invoke: async (command, args) => {
        window.calls.push({ command, args });
        if (window.handlers[command]) return window.handlers[command](args);
        switch (command) {
          case "get_entitlement": return "free";
          case "license_info": return null;
          case "list_devices": return [{
            id: 1, serial: "A:5555", name: "A", model: "Shield", status: "device",
            connection: "network", device_type: "shield", properties: null,
          }];
          case "wireless_status": return { connected: true };
          case "wireless_connect": return { ok: true, message: "connected" };
          case "telemetry_settings": return { enabled: false, available: true, notice_seen: true, support_id: null };
          case "read_debug_log": return "";
          case "read_device_logs": return {
            package: args.options.package, pid: args.options.package ? 123 : null,
            output: { stdout: "First entry\nSecond <img src=x onerror=alert(1)> entry\n", stderr: "", exit_code: 0, termination: "completed" },
          };
          case "collect_diagnostics": return "## ATV Optimizer diagnostics\n\n- App version: test\n";
          case "telemetry_action":
          case "telemetry_error":
            return null;
          default: return { ok: true, message: "done" };
        }
      },
    };
  });
  await page.goto(origin);
  await page.evaluate(async (target) => {
    const { session } = await import("/src/lib/session.svelte.ts");
    const { router } = await import("/src/lib/router.svelte.ts");
    await session.connect("A", 5555);
    router.reset("dashboard");
    router.navigate(target);
  }, screen);
  return page;
}

const calls = (page, command) =>
  page.evaluate((c) => window.calls.filter((call) => call.command === c).map((call) => call.args), command);

test("More lists Device logs as a Free tool", async (t) => {
  const page = await open(t, "more");
  const row = page.getByRole("button", { name: /Device logs/ });
  await row.click();
  await page.getByRole("heading", { name: "Device logs" }).waitFor();
});

test("logs are read only on request, with trimmed filters, as text", async (t) => {
  const page = await open(t, "logs");
  await page.getByRole("heading", { name: "Device logs" }).waitFor();
  assert.deepEqual(await calls(page, "read_device_logs"), [], "entering the screen never reads the TV");
  await page.getByLabel("Minimum severity").selectOption("warning");
  await page.getByLabel("Recent entries").selectOption("500");
  await page.getByLabel("Tag (optional)").fill(" ActivityManager ");
  await page.getByLabel("App package (optional)").fill(" com.example.app ");
  await page.getByRole("button", { name: "Read logs", exact: true }).click();
  const output = page.getByLabel("Log output");
  await output.waitFor();
  assert.deepEqual(await calls(page, "read_device_logs"), [{
    serial: "A:5555",
    options: { priority: "warning", lines: 500, tag: "ActivityManager", package: "com.example.app" },
  }]);
  assert.match(await output.innerText(), /<img src=x/);
  assert.equal(await output.locator("img").count(), 0, "log text never becomes HTML");
  assert.match(await page.getByText(/Snapshot at/).innerText(), /com\.example\.app \(PID 123\)/);

  await page.getByLabel("Find in this snapshot").fill("second");
  assert.doesNotMatch(await output.innerText(), /First/);
  assert.equal((await calls(page, "read_device_logs")).length, 1, "search is local");
});

test("a failed read is an error and a partial read is labelled", async (t) => {
  const page = await open(t, "logs");
  await page.evaluate(() => {
    window.handlers.read_device_logs = () => ({
      package: null, pid: null,
      output: { stdout: "", stderr: "logcat: unknown option", exit_code: 1, termination: "completed" },
    });
  });
  await page.getByRole("button", { name: "Read logs", exact: true }).click();
  await page.getByRole("alert").filter({ hasText: "logcat: unknown option" }).waitFor();
  assert.equal(await page.getByLabel("Log output").count(), 0);

  await page.evaluate(() => {
    window.handlers.read_device_logs = () => ({
      package: null, pid: null,
      output: { stdout: "cut short\n", stderr: "", exit_code: null, termination: "timeout" },
    });
  });
  await page.getByRole("button", { name: "Read logs", exact: true }).click();
  await page.getByRole("alert").filter({ hasText: "Partial snapshot: the read timed out." }).waitFor();
  assert.match(await page.getByLabel("Log output").innerText(), /cut short/);
});

test("Stop drops a late auto-refresh result", async (t) => {
  const page = await open(t, "logs");
  await page.evaluate(() => {
    window.pending = [];
    window.handlers.read_device_logs = () => new Promise((resolve) => window.pending.push(() => resolve({
      package: null, pid: null,
      output: { stdout: "LATE STOPPED OUTPUT\n", stderr: "", exit_code: 0, termination: "completed" },
    })));
  });
  await page.getByRole("button", { name: "Auto-refresh", exact: true }).click();
  await page.waitForFunction(() => window.pending.length === 1);
  assert.equal(await page.getByLabel("Minimum severity").isDisabled(), true);
  await page.getByRole("button", { name: "Stop refreshing", exact: true }).click();
  await page.evaluate(() => window.pending.shift()());
  await page.getByRole("button", { name: "Read logs", exact: true }).and(page.locator(":enabled")).waitFor();
  assert.doesNotMatch(await page.locator(".screen").innerText(), /LATE STOPPED OUTPUT/);
});

test("switching TVs and then backgrounding drops the old snapshot and the late read", async (t) => {
  const page = await open(t, "logs");
  await page.getByRole("button", { name: "Read logs", exact: true }).click();
  await page.getByLabel("Log output").waitFor();
  await page.evaluate(() => {
    window.pending = [];
    window.handlers.read_device_logs = () => new Promise((resolve) => window.pending.push(() => resolve({
      package: null, pid: null,
      output: { stdout: "LATE OLD TV OUTPUT\n", stderr: "", exit_code: 0, termination: "completed" },
    })));
  });
  await page.getByRole("button", { name: "Auto-refresh", exact: true }).click();
  await page.waitForFunction(() => window.pending.length === 1);
  await page.evaluate(async () => {
    window.handlers.list_devices = () => [{
      id: 2, serial: "B:5555", name: "B", model: "Chromecast", status: "device",
      connection: "network", device_type: "google_tv", properties: null,
    }];
    const { session } = await import("/src/lib/session.svelte.ts");
    await session.connect("B", 5555);
  });
  assert.equal(await page.getByLabel("Log output").count(), 0, "the old TV's snapshot is cleared");
  assert.equal(await page.getByLabel("Minimum severity").isDisabled(), true, "the old read is still running and says so");
  await page.evaluate(() => {
    Object.defineProperty(document, "visibilityState", { configurable: true, get: () => "hidden" });
    document.dispatchEvent(new Event("visibilitychange"));
  });
  await page.evaluate(() => window.pending.shift()());
  await page.getByLabel("Minimum severity").and(page.locator(":enabled")).waitFor();
  assert.doesNotMatch(await page.locator(".screen").innerText(), /LATE OLD TV OUTPUT/);
  await page.waitForTimeout(3500);
  assert.equal((await calls(page, "read_device_logs")).length, 2, "nothing refreshes in the background");
  assert.equal(await page.getByRole("button", { name: "Read logs", exact: true }).isDisabled(), true, "hidden screens don't read");

  await page.evaluate(() => {
    Object.defineProperty(document, "visibilityState", { configurable: true, get: () => "visible" });
    document.dispatchEvent(new Event("visibilitychange"));
    window.handlers.read_device_logs = null;
  });
  await page.getByRole("button", { name: "Read logs", exact: true }).click();
  await page.getByLabel("Log output").waitFor();
  assert.equal((await calls(page, "read_device_logs")).at(-1).serial, "B:5555");
});

test("a bug report is dropped when the TV changes", async (t) => {
  const page = await open(t, "more");
  await page.getByRole("button", { name: "Review bug report" }).click();
  await page.getByLabel("Bug report", { exact: true }).waitFor();
  await page.evaluate(async () => {
    const { session } = await import("/src/lib/session.svelte.ts");
    await session.connect("A", 5555);
  });
  assert.equal(await page.getByLabel("Bug report", { exact: true }).count(), 0);
  assert.equal(await page.getByRole("button", { name: "Copy bug report" }).count(), 0);
});

test("the bug report is reviewed, then copied, and names the connected TV", async (t) => {
  const page = await open(t, "more");
  assert.deepEqual(await calls(page, "collect_diagnostics"), [], "opening More never builds a report");
  await page.getByRole("button", { name: "Review bug report" }).click();
  const report = page.getByLabel("Bug report", { exact: true });
  await report.waitFor();
  assert.deepEqual(await calls(page, "collect_diagnostics"), [{ serial: "A:5555" }]);
  assert.match(await report.innerText(), /ATV Optimizer diagnostics/);
  assert.equal(await page.evaluate(() => window.clipboardText), null, "review alone copies nothing");
  await page.getByRole("button", { name: "Copy bug report" }).click();
  await page.waitForFunction(() => window.clipboardText !== null);
  assert.match(await page.evaluate(() => window.clipboardText), /App version: test/);
});
