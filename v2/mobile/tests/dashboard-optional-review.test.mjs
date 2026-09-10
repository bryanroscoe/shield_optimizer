import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import { createServer } from "vite";

let server;
let browser;
let origin;

before(async () => {
  const testPort = 14000 + (process.pid % 20000);
  server = await createServer({
    root: fileURLToPath(new URL("../", import.meta.url)),
    logLevel: "silent",
    server: { host: "127.0.0.1", port: testPort },
  });
  await server.listen();
  origin = `http://127.0.0.1:${server.httpServer.address().port}`;
  browser = await chromium.launch({ headless: true });
});

after(async () => {
  await browser?.close();
  await server?.close();
});

const entry = (name, options = {}) => ({
  package: `com.example.${name.toLowerCase().replaceAll(" ", "-")}`,
  name,
  method: options.method ?? "disable",
  risk: "safe",
  optimize_description: `${name} is optional on this TV.`,
  restore_description: `Restore ${name}.`,
  default_optimize: options.defaultOptimize ?? false,
  default_restore: options.defaultRestore ?? false,
  play_store: true,
});

const item = (name, options = {}) => ({
  entry: entry(name, options),
  action: options.action ?? { kind: options.method === "uninstall" ? "uninstall" : "disable" },
  memory_mb: options.memoryMb ?? null,
});

async function createPage(t, options = {}) {
  const page = await browser.newPage({ viewport: options.viewport ?? { width: 384, height: 812 } });
  t.after(() => page.close());
  await page.addInitScript((options) => {
    localStorage.clear();
    window.calls = [];
    window.handlers = {};
    window.pendingStates = [];
    window.pendingSafety = [];
    window.options = options;
    window.__TAURI_INTERNALS__ = {
      invoke: async (command, args) => {
        window.calls.push({ command, args });
        if (window.handlers[command]) return window.handlers[command](args);
        switch (command) {
          case "get_entitlement": return options.entitlement ?? "pro";
          case "wireless_connect": return { ok: true };
          case "wireless_status": return { connected: true };
          case "list_devices": return [{
            id: 1,
            serial: "A:5555",
            name: "Living Room",
            model: "Shield",
            status: "device",
            connection: "network",
            device_type: "shield",
            properties: null,
          }];
          case "health_report": return {
            ram: { free_mb: 1024 }, storage: {}, display: {}, top_memory: [],
          };
          case "app_list_for_device": return options.catalog ?? [];
          case "package_states": return options.states ?? {};
          case "prepare_optimize": return options.plans?.[args.mode] ?? { mode: args.mode, items: [] };
          case "safety_info": {
            const verdict = options.safety?.[args.package] ?? {
              kind: "caution", reason: "Review whether you use this app.",
            };
            if (options.deferSafety) {
              return new Promise((resolve) => window.pendingSafety.push({ resolve, verdict }));
            }
            return verdict;
          }
          default: return { ok: true, message: "done", transport: "channel" };
        }
      },
    };
  }, options);
  await page.goto(origin);
  await page.evaluate(async () => {
    const { session } = await import("/src/lib/session.svelte.ts");
    const { router } = await import("/src/lib/router.svelte.ts");
    window.session = session;
    window.router = router;
    await session.connect("A", 5555);
  });
  return page;
}

async function openScreen(page, screen) {
  await page.evaluate((screen) => {
    window.router.reset("dashboard");
    if (screen !== "dashboard") window.router.navigate(screen);
  }, screen);
}

test("Dashboard counts enabled among installed recommended apps", async (t) => {
  const first = entry("Prime Video", { defaultOptimize: true });
  const second = entry("Photos", { defaultOptimize: true });
  const missing = entry("Music", { defaultOptimize: true });
  const optional = entry("Optional Player");
  const page = await createPage(t, {
    entitlement: "free",
    catalog: [first, second, missing, optional],
    states: {
      [first.package]: "enabled",
      [second.package]: "disabled",
      [missing.package]: "missing",
    },
  });
  await openScreen(page, "dashboard");
  await page.getByText("1 of 2 installed recommended apps are enabled.").waitFor();
  assert.equal(await page.getByText("Enabled", { exact: true }).count(), 1);
  assert.equal(await page.getByText("System optimized", { exact: true }).count(), 0);
  const cta = page.getByRole("button", { name: /Review app choices PRO/ });
  await cta.click();
  await page.getByRole("heading", { name: "Optimize" }).waitFor();
  assert.equal(
    await page.evaluate(() => window.calls.some((call) =>
      ["disable_package", "uninstall_package", "apply_performance_settings"].includes(call.command))),
    false,
  );
});

test("Dashboard treats incomplete package state replies as unavailable", async (t) => {
  const first = entry("Prime Video", { defaultOptimize: true });
  const second = entry("Photos", { defaultOptimize: true });
  const page = await createPage(t, {
    catalog: [first, second],
    states: { [first.package]: "enabled" },
  });
  await openScreen(page, "dashboard");
  await page.getByRole("button", { name: /Retry/ }).waitFor();
  assert.match(await page.locator(".health-card").innerText(), /incomplete/i);
  assert.doesNotMatch(await page.locator(".health-card").innerText(), /0 of 0/);
});

test("a stale same-serial count cannot overwrite a newer generation", async (t) => {
  const recommended = entry("Prime Video", { defaultOptimize: true });
  const page = await createPage(t, { catalog: [recommended], states: {} });
  await page.evaluate(() => {
    window.handlers.package_states = () => new Promise((resolve) => window.pendingStates.push(resolve));
  });
  await openScreen(page, "dashboard");
  await page.waitForFunction(() => window.pendingStates.length === 1);
  await page.evaluate(async () => {
    await window.session.connect("A", 5555);
    void window.session.loadBloat(true);
  });
  await page.waitForFunction(() => window.pendingStates.length === 2);
  await page.evaluate(() => window.pendingStates[1]({ "com.example.prime-video": "disabled" }));
  await page.waitForFunction(() => window.session.bloatLoaded && window.session.bloatTotal === 1);
  await page.evaluate(() => window.pendingStates[0]({ "com.example.prime-video": "enabled" }));
  await page.waitForTimeout(20);
  assert.deepEqual(
    await page.evaluate(() => ({ enabled: window.session.bloatCount, installed: window.session.bloatTotal })),
    { enabled: 0, installed: 1 },
  );
});

test("zero recommended candidates opens Optional apps with Keep selected", async (t) => {
  const prime = item("Prime Video");
  const page = await createPage(t, {
    plans: { optimize: { mode: "optimize", items: [prime] } },
  });
  await openScreen(page, "optimize");
  await page.getByRole("button", { name: "Optional apps", exact: true }).waitFor();
  assert.equal(await page.getByRole("button", { name: "Keep", exact: true }).getAttribute("aria-pressed"), "true");
  assert.equal(await page.getByRole("button", { name: "Select all", exact: true }).count(), 0);
  assert.equal(await page.getByRole("button", { name: /Apply optimization/ }).isDisabled(), true);
  assert.equal(await page.getByText(prime.entry.package, { exact: true }).count(), 1);
});

test("Caution defaults are recommended while Unknown defaults move to Optional", async (t) => {
  const cautionDefault = item("Caution Default", { defaultOptimize: true });
  const unknownDefault = item("Unknown Default", { defaultOptimize: true });
  const page = await createPage(t, {
    plans: { optimize: { mode: "optimize", items: [cautionDefault, unknownDefault] } },
    safety: {
      [cautionDefault.entry.package]: { kind: "caution", reason: "Safe to review as a recommendation." },
      [unknownDefault.entry.package]: { kind: "unknown", reason: "No audited rule matched this package." },
    },
  });
  await openScreen(page, "optimize");
  await page.getByText("1 selected · 1 recommended · 0 optional", { exact: false }).waitFor();
  assert.equal(await page.getByText("Caution Default", { exact: true }).count(), 1);
  assert.equal(await page.getByText("Unknown Default", { exact: true }).count(), 0);
  await page.getByRole("button", { name: "Optional apps", exact: true }).click();
  assert.equal(await page.getByText("Caution Default", { exact: true }).count(), 0);
  assert.equal(await page.getByText("Unknown Default", { exact: true }).count(), 1);
  assert.equal(
    await page.getByRole("group", { name: "Choice for Unknown Default" })
      .getByRole("button", { name: "Keep", exact: true }).getAttribute("aria-pressed"),
    "true",
  );
});

test("a plan containing only Unknown defaults opens Optional after safety resolves", async (t) => {
  const unknownDefault = item("Unknown Only", { defaultOptimize: true });
  const page = await createPage(t, {
    plans: { optimize: { mode: "optimize", items: [unknownDefault] } },
    safety: {
      [unknownDefault.entry.package]: { kind: "unknown", reason: "No audited rule matched this package." },
    },
  });
  await openScreen(page, "optimize");
  const optionalTab = page.getByRole("button", { name: "Optional apps", exact: true });
  await page.getByText("Unknown Only", { exact: true }).waitFor();
  assert.match(await optionalTab.getAttribute("class"), /active/);
  assert.equal(
    await page.getByRole("group", { name: "Choice for Unknown Only" })
      .getByRole("button", { name: "Keep", exact: true }).getAttribute("aria-pressed"),
    "true",
  );
});

test("a user's tab choice survives pending safety resolution", async (t) => {
  const cautionDefault = item("Late Caution", { defaultOptimize: true });
  const optional = item("Visible Optional");
  const page = await createPage(t, {
    deferSafety: true,
    plans: { optimize: { mode: "optimize", items: [cautionDefault, optional] } },
  });
  await openScreen(page, "optimize");
  await page.waitForFunction(() => window.pendingSafety.length === 2);
  const optionalTab = page.getByRole("button", { name: "Optional apps", exact: true });
  await optionalTab.click();
  await page.getByText("Visible Optional", { exact: true }).waitFor();
  await page.evaluate(() => {
    for (const pending of window.pendingSafety) pending.resolve(pending.verdict);
  });
  await page.getByText("Review whether you use this app.", { exact: true }).waitFor();
  assert.match(await optionalTab.getAttribute("class"), /active/);
  assert.equal(await page.getByText("Late Caution", { exact: true }).count(), 0);
});

test("optional choices survive tabs and confirmation names every actual action", async (t) => {
  const recommended = item("Recommended One", { defaultOptimize: true, memoryMb: 80 });
  const optionalDisable = item("Optional Disable");
  const optionalUninstall = item("Optional Uninstall", { method: "uninstall" });
  const absent = item("Absent Optional", { action: { kind: "skip", reason: "not_installed" } });
  const page = await createPage(t, {
    plans: { optimize: { mode: "optimize", items: [recommended, optionalDisable, optionalUninstall, absent] } },
  });
  await openScreen(page, "optimize");
  await page.getByText("1 selected · 1 recommended · 0 optional", { exact: false }).waitFor();
  await page.getByRole("button", { name: "Optional apps", exact: true }).click();
  assert.equal(await page.getByText("Absent Optional", { exact: true }).count(), 0);
  await page.getByRole("group", { name: "Choice for Optional Disable" })
    .getByRole("button", { name: "Disable", exact: true }).click();
  await page.getByRole("group", { name: "Choice for Optional Uninstall" })
    .getByRole("button", { name: "Uninstall for this user", exact: true }).click();
  await page.getByRole("button", { name: "Recommended", exact: true }).click();
  await page.getByText("3 selected · 1 recommended · 2 optional", { exact: false }).waitFor();
  await page.getByRole("button", { name: "Apply optimization", exact: false }).click();
  const dialog = page.getByRole("heading", { name: "Apply 3 selected changes?" }).locator("..");
  const text = await dialog.innerText();
  assert.match(text, /Recommended One — Disable/);
  assert.match(text, /Optional Disable — Disable/);
  assert.match(text, /Optional Uninstall — Uninstall for this user/);
  assert.match(text, /2 apps will be disabled/);
  assert.match(text, /1 app will be uninstalled/);
  assert.match(text, /animation scales are set to 0.5×/);
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  assert.equal(
    await page.evaluate(() => window.calls.some((call) =>
      ["disable_package", "uninstall_package", "apply_performance_settings"].includes(call.command))),
    false,
  );
});

test("protected and unresolved optional actions remain blocked while Unknown is explicit", async (t) => {
  const protectedItem = item("Protected App");
  const unknownItem = item("Unknown App");
  const page = await createPage(t, {
    plans: { optimize: { mode: "optimize", items: [protectedItem, unknownItem] } },
    safety: {
      [protectedItem.entry.package]: { kind: "never_disable", reason: "Required for the system UI." },
      [unknownItem.entry.package]: { kind: "unknown", reason: "No audited rule matched this package." },
    },
  });
  await openScreen(page, "optimize");
  const protectedAction = page.getByRole("group", { name: "Choice for Protected App" })
    .getByRole("button", { name: "Disable", exact: true });
  const unknownAction = page.getByRole("group", { name: "Choice for Unknown App" })
    .getByRole("button", { name: "Disable", exact: true });
  await protectedAction.waitFor();
  assert.equal(await protectedAction.isDisabled(), true);
  assert.equal(await unknownAction.isDisabled(), false);
  assert.equal(await unknownAction.getAttribute("aria-pressed"), "false");
  await unknownAction.click();
  assert.equal(await unknownAction.getAttribute("aria-pressed"), "true");
  assert.match(await page.locator(".optimize-list").innerText(), /Required for the system UI/);
  assert.match(await page.locator(".optimize-list").innerText(), /No audited rule matched/);
});

test("Restore keeps Recommended and All curated tabs with its prior defaults", async (t) => {
  const defaultRestore = item("Default Restore", {
    defaultRestore: true,
    action: { kind: "enable" },
  });
  const otherRestore = item("Other Restore", { action: { kind: "enable" } });
  const page = await createPage(t, {
    plans: {
      optimize: { mode: "optimize", items: [] },
      restore: { mode: "restore", items: [defaultRestore, otherRestore] },
    },
  });
  await openScreen(page, "optimize");
  await page.getByRole("button", { name: "Restore", exact: true }).click();
  await page.getByRole("button", { name: "All curated", exact: true }).waitFor();
  assert.equal(await page.getByText("Default Restore", { exact: true }).count(), 1);
  assert.equal(await page.getByText("Other Restore", { exact: true }).count(), 0);
  await page.getByRole("button", { name: "All curated", exact: true }).click();
  assert.equal(await page.getByText("Other Restore", { exact: true }).count(), 1);
  assert.match(await page.locator(".optimize-summary-card").innerText(), /1 selected/);
});
