import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { chromium } from "playwright";
import { fileURLToPath } from "node:url";
import { startViteServer } from "./helpers/vite-harness.mjs";

/// `v2/shared` sits outside the Vite root, so the browser reaches it by path.
const SHARED_SUPPORT = "/@fs" + fileURLToPath(new URL("../../shared/support.ts", import.meta.url));

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

const PROPS = {
  friendly_name: "Living Room",
  brand: "Amazon",
  model: "AFTMM",
  device_codename: "mantis",
  manufacturer: "Amazon",
  android_release: "9",
  sdk_level: "28",
  build_id: "PS7",
  board_platform: "mt8695",
  characteristics: "tv",
  serial_number: "G070VM1234567890",
};

/// A page running the real app against a scripted backend. `window.calls`
/// records every invoke; `window.replies[command]` overrides one answer.
async function createPage(t, options = {}) {
  const page = await browser.newPage({ viewport: { width: 384, height: 812 } });
  t.after(() => page.close());
  await page.addInitScript((options) => {
    localStorage.clear();
    window.calls = [];
    window.replies = {};
    window.telemetry = options.telemetry ?? { enabled: true, available: true, notice_seen: true };
    window.__TAURI_INTERNALS__ = {
      invoke: async (command, args) => {
        window.calls.push({ command, args });
        if (window.replies[command]) return window.replies[command](args);
        switch (command) {
          case "get_entitlement": return "pro";
          case "wireless_connect": return { ok: true };
          case "wireless_status": return { connected: true };
          case "list_devices": return [{
            id: 1,
            serial: "A:5555",
            name: "Living Room",
            model: "AFTMM",
            status: "device",
            connection: "network",
            device_type: "google_tv",
            tv_evidence: "tv",
            properties: options.properties ?? null,
            support: options.support,
          }];
          case "health_report": return {
            ram: { free_mb: 1024, total_mb: 4096 },
            storage: { used: "8.1G", total: "16G", used_percent: 51 },
            display: {},
            top_memory: [],
          };
          case "app_list_for_device": return [];
          case "package_states": return {};
          case "telemetry_settings": return window.telemetry;
          case "set_telemetry_enabled":
            window.telemetry = { ...window.telemetry, enabled: args.enabled };
            return window.telemetry;
          case "dismiss_telemetry_notice":
            window.telemetry = { ...window.telemetry, notice_seen: true };
            return window.telemetry;
          case "telemetry_action":
          case "telemetry_error":
            return null;
          default: return { ok: true, message: "done" };
        }
      },
    };
  }, options);
  await page.goto(origin);
  await page.evaluate(async () => {
    const { session } = await import("/src/lib/session.svelte.ts");
    const { router } = await import("/src/lib/router.svelte.ts");
    const { api } = await import("/src/lib/api.ts");
    const { onConnectionLost } = await import("/src/lib/connectionEvents.ts");
    window.session = session;
    window.router = router;
    window.api = api;
    window.lostEvents = 0;
    onConnectionLost(() => window.lostEvents++);
  });
  return page;
}

function reports(page, command) {
  return page.evaluate((command) => window.calls.filter((c) => c.command === command).map((c) => c.args), command);
}

async function waitForReports(page, command, count) {
  await page.waitForFunction(
    ([command, count]) => window.calls.filter((c) => c.command === command).length >= count,
    [command, count],
  );
  return reports(page, command);
}

test("an action that resolves ok:false is reported as a coded failure, and its result is untouched", async (t) => {
  const page = await createPage(t);
  const result = await page.evaluate(async () => {
    window.replies.disable_package = () => ({ ok: false, message: "Failure [DELETE_FAILED] for com.example.private" });
    return window.api.disablePackage("A:5555", "com.example.private");
  });
  assert.deepEqual(result, { ok: false, message: "Failure [DELETE_FAILED] for com.example.private" });
  const [report] = await waitForReports(page, "telemetry_action", 1);
  assert.equal(report.action, "disable");
  assert.equal(report.ok, false);
  assert.equal(report.errorCode, "pm_failure");
  assert.equal(report.serial, "A:5555");
});

test("a lost connection still flips the session and still throws, and is reported once", async (t) => {
  const page = await createPage(t);
  const outcome = await page.evaluate(async () => {
    window.replies.enable_package = () => { throw "Connection to the TV was lost: broken pipe"; };
    try {
      await window.api.enablePackage("A:5555", "com.x.y");
      return "resolved";
    } catch (e) {
      return String(e);
    }
  });
  assert.equal(outcome, "Connection to the TV was lost: broken pipe");
  assert.equal(await page.evaluate(() => window.lostEvents), 1);
  const [report] = await waitForReports(page, "telemetry_action", 1);
  assert.equal(report.action, "enable");
  assert.equal(report.errorCode, "connection_lost");
});

test("untracked failures are reported as errors, routine polling failures are not, and a broken report never surfaces", async (t) => {
  const page = await createPage(t);
  await page.evaluate(async () => {
    window.replies.telemetry_error = () => { throw new Error("report backend down"); };
    window.replies.wireless_status = () => { throw "timed out"; };
    window.replies.list_launchers = () => { throw "device offline"; };
    await window.api.wirelessStatus().catch(() => {});
    await window.api.listLaunchers("A:5555").catch(() => {});
    await window.api.getTweaks("A:5555");
  });
  const errors = await waitForReports(page, "telemetry_error", 1);
  assert.equal(errors.length, 1);
  assert.equal(errors[0].errorCode, "device_offline");
  assert.match(errors[0].message, /^list_launchers: /);
  // A successful untracked call says nothing at all.
  assert.equal((await reports(page, "telemetry_action")).length, 0);
});

test("an unsupported TV gets a prominent banner, and a destructive confirm warns without blocking", async (t) => {
  const page = await createPage(t, {
    properties: PROPS,
    support: { tier: "not_supported", note: "Fire OS blocks the changes this app makes." },
  });
  await page.evaluate(async () => {
    await window.session.connect("A", 5555);
    window.router.reset("dashboard");
  });
  const banner = page.getByRole("alert").filter({ hasText: "This TV isn't supported" });
  await banner.waitFor();
  assert.match(await banner.innerText(), /This TV isn't supported: Fire OS blocks the changes this app makes\./);

  await page.evaluate(async (sharedSupport) => {
    const { mount } = await import("/node_modules/svelte/src/index-client.js");
    const { destructiveWarning } = await import(sharedSupport);
    const ConfirmDialog = (await import("/src/components/ConfirmDialog.svelte")).default;
    const target = document.createElement("div");
    document.body.append(target);
    window.confirmed = 0;
    mount(ConfirmDialog, {
      target,
      props: {
        open: true,
        title: "Disable Foo?",
        message: "m",
        supportWarning: destructiveWarning(window.session.support),
        onConfirm: () => window.confirmed++,
        onCancel: () => {},
      },
    });
  }, SHARED_SUPPORT);
  const warning = page.getByTestId("support-warning");
  await warning.waitFor();
  assert.match(await warning.innerText(), /Changes may fail or behave unexpectedly/);
  await page.getByRole("dialog").getByRole("button", { name: "Confirm" }).click();
  assert.equal(await page.evaluate(() => window.confirmed), 1);
});

test("unknown and beta tiers say only what is known", async (t) => {
  for (const [support, expected] of [
    [undefined, "We haven't verified this TV model yet."],
    [{ tier: "beta", note: "TCL Google TV has had limited testing so far." }, "Beta: this TV may not work fully."],
    [{ tier: "fully_supported", note: "NVIDIA SHIELD is the device this app is built and tested on." }, "NVIDIA SHIELD is the device this app is built and tested on."],
  ]) {
    const page = await createPage(t, { properties: PROPS, support });
    await page.evaluate(async () => {
      await window.session.connect("A", 5555);
      window.router.reset("dashboard");
    });
    await page.getByText(expected, { exact: false }).first().waitFor();
    assert.equal(await page.getByText("This TV isn't supported").count(), 0);
    await page.close();
  }
});

test("More toggles anonymous reports and a build without an endpoint says so", async (t) => {
  const page = await createPage(t);
  await page.evaluate(async () => {
    await window.session.connect("A", 5555);
    window.router.reset("dashboard");
    window.router.navigate("more");
  });
  const toggle = page.getByRole("switch", { name: "Share anonymous device and error reports" });
  await toggle.waitFor();
  assert.equal(await toggle.isChecked(), true);
  await toggle.click();
  await page.getByText("Off. Nothing is sent.").waitFor();
  const [set] = await reports(page, "set_telemetry_enabled");
  assert.deepEqual(set, { enabled: false });

  const unavailable = await createPage(t, { telemetry: { enabled: true, available: false, notice_seen: true } });
  await unavailable.evaluate(async () => {
    await window.session.connect("A", 5555);
    window.router.reset("dashboard");
    window.router.navigate("more");
  });
  await unavailable.getByText("This build doesn't send reports.").waitFor();
  assert.equal(
    await unavailable.getByRole("switch", { name: "Share anonymous device and error reports" }).isDisabled(),
    true,
  );
});

test("the first-launch notice waits past onboarding, never blocks, and goes away for good", async (t) => {
  const page = await createPage(t, { telemetry: { enabled: true, available: true, notice_seen: false } });
  // Still on onboarding: nothing covers the connect buttons.
  assert.equal(await page.getByText("ATV Optimizer sends anonymous reports", { exact: false }).count(), 0);
  await page.evaluate(async () => {
    await window.session.connect("A", 5555);
    window.router.reset("dashboard");
  });
  const notice = page.getByRole("status").filter({ hasText: "ATV Optimizer sends anonymous reports" });
  await notice.waitFor();
  // Non-blocking: the screen underneath stays usable.
  await page.getByRole("button", { name: "More", exact: true }).click();
  await page.getByRole("switch", { name: "Share anonymous device and error reports" }).waitFor();
  await notice.getByRole("button", { name: "OK" }).click();
  await notice.waitFor({ state: "detached" });
  assert.equal((await reports(page, "dismiss_telemetry_notice")).length, 1);
});

test("a failed removal shows a coded error with a tier hint and Copy details", async (t) => {
  const page = await createPage(t, {
    properties: PROPS,
    support: { tier: "beta", note: "Limited testing." },
  });
  await page.evaluate(async () => {
    await window.session.connect("A", 5555);
    window.router.reset("dashboard");
  });
  await page.evaluate(async () => {
    const { mount } = await import("/node_modules/svelte/src/index-client.js");
    const ErrorNotice = (await import("/src/components/ErrorNotice.svelte")).default;
    const target = document.createElement("div");
    target.id = "notice-host";
    document.body.append(target);
    mount(ErrorNotice, { target, props: { error: "Failure [DELETE_FAILED_INTERNAL_ERROR]", title: "Couldn't uninstall Foo" } });
  });
  const notice = page.locator("#notice-host .error-notice");
  await notice.waitFor();
  const text = await notice.innerText();
  assert.match(text, /Couldn't uninstall Foo/);
  assert.match(text, /package manager reported a failure/);
  assert.match(text, /pm_failure/);
  assert.match(text, /beta support/);
  assert.ok(await notice.getByRole("button", { name: "Copy details" }).isVisible());
});
