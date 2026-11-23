import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { chromium } from "playwright";
import { startViteServer } from "./helpers/vite-harness.mjs";

// More › Shell (desktop Shell tab parity): Pro only, nothing runs until the
// person ticks the per-TV risk box and presses Run, consent is remembered by
// verified hardware id and never by address, output stays text, a refusal
// reaches no TV, a late result never lands on another TV, and the typed
// command stays out of the debug log.

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

const TV_A = {
  id: 1, serial: "A:5555", name: "A", model: "Shield", status: "device",
  connection: "network", device_type: "shield", tv_evidence: "tv",
  properties: { serial_number: "HW-SHELL-A" },
};

async function open(t, screen, { entitlement = "pro", device = TV_A, storage = null, width = 384 } = {}) {
  const context = await browser.newContext({ viewport: { width, height: 1400 } });
  const page = await context.newPage();
  t.after(() => context.close());
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  t.after(() => assert.deepEqual(errors, []));
  await page.addInitScript(({ entitlement, device, storage }) => {
    if (storage && !sessionStorage.getItem("seeded")) {
      for (const [key, value] of Object.entries(storage)) localStorage.setItem(key, value);
      sessionStorage.setItem("seeded", "1");
    }
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
          case "get_entitlement": return entitlement;
          case "license_info": return null;
          case "list_devices": return [device];
          case "wireless_status": return { connected: true };
          case "wireless_discover": return { devices: [], message: "" };
          case "wireless_connect": return { ok: true, message: "connected" };
          case "telemetry_settings": return { enabled: false, available: true, notice_seen: true, support_id: null };
          case "read_debug_log": return "";
          case "billing_status":
          case "billing_refresh":
            return { available: false, owned: false, pending: false, offer: null, message: "" };
          case "run_shell":
            if (entitlement !== "pro") throw "LOCKED:shell_runner";
            return {
              stdout: "package:com.example.one\n<img src=x onerror=alert(1)>\n",
              stderr: "warning: user 0\n",
              exit_code: 0, termination: "completed", blocked: false, blocked_reason: null,
            };
          case "telemetry_action":
          case "telemetry_error":
            return null;
          default: return { ok: true, message: "done" };
        }
      },
    };
  }, { entitlement, device, storage });
  await page.goto(origin);
  await enter(page, screen);
  return page;
}

async function enter(page, screen) {
  await page.evaluate(async (target) => {
    const { session } = await import("/src/lib/session.svelte.ts");
    const { router } = await import("/src/lib/router.svelte.ts");
    await session.connect("A", 5555);
    router.reset("dashboard");
    router.navigate(target);
  }, screen);
}

const calls = (page, command) =>
  page.evaluate((c) => window.calls.filter((call) => call.command === c).map((call) => call.args), command);

const ack = (page) => page.getByRole("checkbox", { name: /I understand these risks/ });
const runButton = (page) => page.getByRole("button", { name: "Run", exact: true });

test("More lists Shell as a Pro tool, and Free sees the paywall instead of a prompt", async (t) => {
  const page = await open(t, "more", { entitlement: "free" });
  await page.getByRole("button", { name: /Shell/ }).click();
  await page.getByRole("heading", { name: "Shell is a Pro feature" }).waitFor();
  assert.equal(await page.getByLabel("Shell command").count(), 0);
  await page.getByRole("button", { name: "Unlock Pro" }).click();
  await page.getByRole("dialog").first().waitFor();
  assert.deepEqual(await calls(page, "run_shell"), []);
});

test("nothing runs until the box is ticked and Run is pressed; output stays text", async (t) => {
  const page = await open(t, "shell");
  await page.getByRole("heading", { name: "Shell" }).waitFor();
  assert.equal(await ack(page).isChecked(), false);

  await page.getByRole("button", { name: /Disabled packages/ }).click();
  assert.equal(await page.getByLabel("Shell command").inputValue(), "pm list packages -d --user 0");
  assert.equal(await runButton(page).isDisabled(), true, "Run waits for the acknowledgment");
  await page.getByText("Check the box above to enable Run.").waitFor();
  assert.deepEqual(await calls(page, "run_shell"), [], "a preset only fills the box");

  await ack(page).check();
  await page.getByLabel("Shell command").fill("  getprop ro.product.model  ");
  await runButton(page).click();
  await page.getByLabel("Command output").waitFor();
  assert.deepEqual(await calls(page, "run_shell"), [{ serial: "A:5555", command: "getprop ro.product.model" }]);
  assert.match(await page.getByLabel("Command output").innerText(), /<img src=x onerror=alert\(1\)>/);
  assert.equal(await page.locator(".output img").count(), 0, "output is never HTML");
  assert.match(await page.getByLabel("Error output").innerText(), /warning: user 0/);
  await page.getByText("exit 0").waitFor();
  await page.getByText("3 lines").waitFor();

  await page.getByRole("button", { name: "Copy" }).click();
  assert.equal(await page.evaluate(() => window.clipboardText),
    "package:com.example.one\n<img src=x onerror=alert(1)>\n\nwarning: user 0\n");
  await page.getByRole("button", { name: "Clear" }).click();
  assert.equal(await page.getByLabel("Command output").count(), 0);
});

test("consent is remembered by hardware id and asked again for a TV without one", async (t) => {
  const page = await open(t, "shell");
  await ack(page).check();
  await page.reload();
  await enter(page, "shell");
  await ack(page).waitFor();
  assert.equal(await ack(page).isChecked(), true, "the same TV keeps its consent");
  assert.deepEqual(
    JSON.parse(await page.evaluate(() => localStorage.getItem("shieldOptimizer.expertShellAcknowledged"))),
    ["HW-SHELL-A"],
  );

  const idless = await open(t, "shell", { device: { ...TV_A, properties: null } });
  await ack(idless).check();
  assert.equal(await idless.evaluate(() => localStorage.getItem("shieldOptimizer.expertShellAcknowledged")), null,
    "a TV with no hardware id is never filed under its address");
  await idless.reload();
  await enter(idless, "shell");
  await ack(idless).waitFor();
  assert.equal(await ack(idless).isChecked(), false);
});

test("a refused command is shown as a refusal with nothing to copy", async (t) => {
  const page = await open(t, "shell");
  await page.evaluate(() => {
    window.handlers.run_shell = () => ({
      stdout: "", stderr: "", exit_code: null, termination: "completed", blocked: true,
      blocked_reason: "Refused: this command could damage com.android.systemui, which is on the do-not-disable list.",
    });
  });
  await ack(page).check();
  await page.getByLabel("Shell command").fill("pm disable-user --user 0 com.android.systemui");
  await runButton(page).click();
  await page.getByText(/Refused: this command could damage com.android.systemui/).waitFor();
  assert.equal(await page.getByLabel("Command output").count(), 0);
  assert.equal(await page.getByRole("button", { name: "Copy" }).count(), 0);
});

test("a timed-out run says the output is partial and the TV may have changed", async (t) => {
  const page = await open(t, "shell");
  await page.evaluate(() => {
    window.handlers.run_shell = () => ({
      stdout: "partial\n", stderr: "", exit_code: null, termination: "timeout", blocked: false, blocked_reason: null,
    });
  });
  await ack(page).check();
  await page.getByLabel("Shell command").fill("logcat");
  await runButton(page).click();
  await page.getByText(/Stopped after 30 seconds\. Partial output is shown below\. The command may already have changed the device\./).waitFor();
});

test("a late result from the previous TV never lands on the new one", async (t) => {
  const page = await open(t, "shell");
  await page.evaluate(() => {
    window.pending = [];
    window.handlers.run_shell = () => new Promise((resolve) => window.pending.push(() => resolve({
      stdout: "LATE OLD TV OUTPUT\n", stderr: "", exit_code: 0, termination: "completed", blocked: false, blocked_reason: null,
    })));
  });
  await ack(page).check();
  await page.getByLabel("Shell command").fill("uptime");
  await runButton(page).click();
  await page.waitForFunction(() => window.pending.length === 1);
  await page.getByRole("button", { name: "Running…" }).waitFor();
  await page.evaluate(async () => {
    window.handlers.list_devices = () => [{
      id: 2, serial: "B:5555", name: "B", model: "Chromecast", status: "device",
      connection: "network", device_type: "google_tv", tv_evidence: "tv", properties: { serial_number: "HW-SHELL-B" },
    }];
    const { session } = await import("/src/lib/session.svelte.ts");
    await session.connect("B", 5555);
  });
  assert.equal(await ack(page).isChecked(), false, "consent for TV A is not consent for TV B");
  await page.evaluate(() => window.pending.shift()());
  await page.waitForTimeout(200);
  assert.doesNotMatch(await page.locator(".screen").innerText(), /LATE OLD TV OUTPUT/);
  assert.equal(await page.getByRole("button", { name: "Running…" }).count(), 0, "Run is released for the new TV");
});

test("a backend LOCKED answer opens the paywall", async (t) => {
  const page = await open(t, "shell");
  await page.evaluate(() => { window.handlers.run_shell = () => { throw "LOCKED:shell_runner"; }; });
  await ack(page).check();
  await page.getByLabel("Shell command").fill("uptime");
  await runButton(page).click();
  await page.getByRole("dialog").first().waitFor();
  assert.equal(await page.locator(".screen .error").count(), 0, "the lock is not shown as an error");
});

test("custom bookmarks are saved, edited and removed without running anything", async (t) => {
  const page = await open(t, "shell");
  await page.getByLabel("Shell command").fill("dumpsys battery");
  await page.getByLabel("Bookmark name (optional)").fill("Battery");
  await page.getByRole("button", { name: "Bookmark this command" }).click();
  await page.getByRole("button", { name: /^Battery/ }).waitFor();

  await page.getByLabel("Bookmark name (optional)").fill("Battery");
  await page.getByRole("button", { name: "Bookmark this command" }).click();
  await page.getByText("A custom bookmark already has that name. Edit it or choose another name.").waitFor();

  await page.getByRole("button", { name: "Edit the bookmark Battery" }).click();
  const form = page.getByRole("form", { name: "Edit custom bookmark" });
  await form.getByLabel("Name").fill("Power");
  await form.getByLabel("Command").fill("dumpsys power");
  await form.getByRole("button", { name: "Save changes" }).click();
  assert.deepEqual(
    JSON.parse(await page.evaluate(() => localStorage.getItem("shieldopt.shellBookmarks"))),
    [{ label: "Power", command: "dumpsys power" }],
  );

  await page.getByLabel("Shell command").fill("");
  await page.getByRole("button", { name: /^Power/ }).click();
  assert.equal(await page.getByLabel("Shell command").inputValue(), "dumpsys power");
  await page.getByRole("button", { name: "Remove the bookmark Power" }).click();
  assert.equal(await page.getByRole("button", { name: /^Power/ }).count(), 0);
  assert.deepEqual(await calls(page, "run_shell"), []);
});

test("the typed command never reaches the debug log", async (t) => {
  const page = await open(t, "shell");
  const summary = await page.evaluate(async () => {
    const { summarizeArgs } = await import("/src/lib/log.ts");
    return summarizeArgs({ serial: "A:5555", command: "settings put secure token SECRET" });
  });
  assert.equal(summary, "serial=A:5555 command=***");
});

test("at 150% text on a phone (275 CSS px) nothing scrolls sideways", async (t) => {
  const page = await open(t, "shell", { width: 275 });
  await ack(page).check();
  await page.getByLabel("Shell command").fill("dumpsys display | grep -E 'mBaseDisplayInfo|modeId' && pm list packages -f --user 0");
  await runButton(page).click();
  await page.getByLabel("Command output").waitFor();
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
  assert.ok(overflow <= 0, `page overflows by ${overflow}px`);
  for (const name of ["Copy", "Clear", "Run"]) {
    const box = await page.getByRole("button", { name, exact: true }).boundingBox();
    assert.ok(box.x >= 0 && box.x + box.width <= 275, `${name} stays on screen`);
    assert.ok(box.height >= 44, `${name} keeps a 44px touch target`);
  }
  if (process.env.SHELL_SHOT) await page.screenshot({ path: process.env.SHELL_SHOT, fullPage: true });
});
