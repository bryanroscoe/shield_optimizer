import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import { createServer } from "vite";

let server;
let browser;
let origin;
before(async () => {
  server = await createServer({
    root: fileURLToPath(new URL("../", import.meta.url)),
    logLevel: "silent",
    server: { host: "127.0.0.1", port: 0 },
  });
  await server.listen();
  origin = `http://127.0.0.1:${server.httpServer.address().port}`;
  browser = await chromium.launch({ headless: true });
});
after(async () => {
  await browser?.close();
  await server?.close();
});

async function open(t, screen = "remote") {
  const page = await browser.newPage({ viewport: { width: 384, height: 812 } });
  t.after(() => page.close());
  await page.addInitScript(() => {
    window.calls = [];
    window.handlers = {};
    window.device = (host) => ({
      id: 1, serial: `${host}:5555`, name: host, model: "Shield",
      status: "device", connection: "network", device_type: "shield", properties: null,
    });
    window.activeHost = "A";
    window.__TAURI_INTERNALS__ = { invoke: async (command, args) => {
      window.calls.push({ command, args });
      if (window.handlers[command]) return window.handlers[command](args);
      switch (command) {
        case "get_entitlement": return "pro";
        case "list_devices": return window.activeHost ? [window.device(window.activeHost)] : [];
        case "wireless_status": return { connected: true };
        case "wireless_connect": window.activeHost = args.host; return { ok: true };
        case "wireless_disconnect": window.activeHost = ""; return { ok: true };
        case "wireless_cancel_connect": return;
        case "remote_warm": return { transport: "channel", message: "" };
        case "health_report": return { ram: { free_mb: 512 }, storage: {}, display: {}, top_memory: [] };
        case "app_list_for_device": return [];
        case "safety_info": return { kind: "safe" };
        case "prepare_optimize": return { mode: "optimize", items: ["one", "two"].map((name) => ({
          entry: { package: `com.example.${name}`, name, default_optimize: true, risk: "safe", method: "disable" },
          action: { kind: "disable" },
        })) };
        default: return { ok: true, transport: "channel", message: "done" };
      }
    } };
  });
  await page.goto(origin);
  await page.evaluate(async (screen) => {
    const { session } = await import("/src/lib/session.svelte.ts");
    const { router } = await import("/src/lib/router.svelte.ts");
    window.session = session;
    window.router = router;
    session.entitlement = "pro";
    await session.connect("A", 5555);
    router.reset("dashboard");
    router.navigate(screen);
  }, screen);
  return page;
}

test("hardware Back and direct navigation keep Optimize cancellable", async (t) => {
  const page = await open(t, "optimize");
  await page.evaluate(() => {
    window.handlers.disable_package = () => new Promise((resolve) => { window.release = resolve; });
    window.backEvents = 0;
    window.addEventListener("popstate", () => window.backEvents++);
  });
  await page.getByRole("button", { name: "Apply optimization", exact: false }).click();
  await page.getByRole("button", { name: "Apply", exact: true }).click();
  await page.waitForFunction(() => !!window.release);
  await page.evaluate(() => history.back());
  await page.waitForFunction(() => window.backEvents > 0);
  await page.evaluate(() => window.router.navigate("devices"));
  assert.equal(await page.evaluate(() => window.router.current), "optimize");
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await page.evaluate(() => window.release({ ok: true }));
  await page.waitForFunction(() => !window.session.applyInProgress);
  assert.equal(await page.evaluate(() => window.calls.filter((c) => c.command === "disable_package").length), 1);
  assert.equal(await page.evaluate(() => window.calls.some((c) => c.command === "apply_performance_settings")), false);
});

test("remote queue is discarded after navigation and a TV switch", async (t) => {
  const page = await open(t);
  await page.evaluate(() => {
    window.handlers.send_key = () => new Promise((resolve) => { window.release = resolve; });
  });
  await page.getByRole("button", { name: "Volume up", exact: true }).click();
  await page.getByRole("button", { name: "Volume down", exact: true }).click();
  await page.evaluate(async () => {
    window.router.navigate("dashboard");
    await window.session.connect("B", 5555);
    window.release({ ok: true, transport: "channel" });
  });
  await page.evaluate(() => new Promise((resolve) => setTimeout(resolve, 30)));
  const calls = await page.evaluate(() => window.calls.filter((c) => c.command === "send_key"));
  assert.deepEqual(calls.map((c) => c.args.serial), ["A:5555"]);
});

test("ordinary remote taps cannot accumulate an unlimited backlog", async (t) => {
  const page = await open(t);
  await page.evaluate(() => {
    window.handlers.send_key = async () => {
      if (!window.release) await new Promise((resolve) => { window.release = resolve; });
      return { ok: true, transport: "channel" };
    };
  });
  await page.getByRole("button", { name: "Volume up", exact: true }).click();
  await page.getByRole("button", { name: "Volume down", exact: true }).evaluate((button) => {
    for (let i = 0; i < 30; i++) button.click();
  });
  await page.evaluate(() => window.release());
  await page.waitForFunction(() => window.calls.filter((c) => c.command === "send_key").length === 8);
  assert.equal(await page.evaluate(() => window.calls.filter((c) => c.command === "send_key").length), 8);
});

test("failed health refresh preserves the last report and exposes the error", async (t) => {
  const page = await open(t);
  const state = await page.evaluate(async () => {
    await window.session.loadHealth();
    window.handlers.health_report = () => { throw new Error("Health report timed out"); };
    await window.session.loadHealth(true);
    return { free: window.session.health.ram.free_mb, error: window.session.healthError };
  });
  assert.equal(state.free, 512);
  assert.match(state.error, /timed out/);
});

test("one failed recovery stays lost until an explicit retry", async (t) => {
  const page = await open(t);
  const result = await page.evaluate(async () => {
    window.calls = [];
    window.handlers.wireless_status = () => ({ connected: false });
    window.handlers.wireless_connect = () => ({ ok: false });
    await window.session.checkLiveness();
    await window.session.checkLiveness();
    await window.session.recoverOrMarkLost();
    const automatic = window.calls.filter((c) => c.command === "wireless_connect").length;
    const lost = window.session.liveness;
    delete window.handlers.wireless_connect;
    await window.session.reconnect();
    return { automatic, lost, retried: window.session.isConnected };
  });
  assert.deepEqual(result, { automatic: 1, lost: "lost", retried: true });
});

test("canceled connect cannot update the session when its old reply arrives", async (t) => {
  const page = await open(t);
  const result = await page.evaluate(async () => {
    window.handlers.wireless_connect = () => new Promise((resolve) => { window.releaseConnect = resolve; });
    const old = window.session.connect("B", 5555);
    await window.session.cancelConnect();
    window.releaseConnect({ ok: true });
    const canceled = await old;
    window.handlers.wireless_connect = () => ({ ok: false });
    await window.session.connect("C", 5555);
    return { canceled: canceled.ok, serial: window.session.serial, host: window.session.host,
      canceledNative: window.calls.some((c) => c.command === "wireless_cancel_connect") };
  });
  assert.deepEqual(result, { canceled: false, serial: "A:5555", host: "A", canceledNative: true });
});

test("stale reboot completion cannot disconnect a newer session", async (t) => {
  const page = await open(t);
  const result = await page.evaluate(async () => {
    const generation = window.session.generation;
    await window.session.connect("B", 5555);
    window.calls = [];
    const finished = await window.session.finishReboot(generation);
    return { finished, serial: window.session.serial,
      disconnected: window.calls.some((c) => c.command === "wireless_disconnect") };
  });
  assert.deepEqual(result, { finished: false, serial: "B:5555", disconnected: false });
});

test("cancel and retry of the same saved TV ignores the first completion", async (t) => {
  const page = await open(t);
  await page.evaluate(() => {
    window.pendingConnects = [];
    window.handlers.wireless_connect = () => new Promise((resolve) => window.pendingConnects.push(resolve));
    window.router.reset("onboarding");
  });
  await page.waitForFunction(() => window.pendingConnects.length === 1);
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await page.locator("button.device").first().click();
  await page.waitForFunction(() => window.pendingConnects.length === 2);
  await page.evaluate(() => window.pendingConnects[0]({ ok: true }));
  await page.evaluate(() => new Promise((resolve) => setTimeout(resolve, 30)));
  assert.equal(await page.getByRole("button", { name: "Open dashboard" }).count(), 0);
  assert.equal(await page.getByRole("button", { name: "Cancel", exact: true }).count(), 1);
  await page.evaluate(() => window.pendingConnects[1]({ ok: true }));
  await page.getByRole("button", { name: "Open dashboard" }).waitFor();
});

test("a late transport error from A cannot start recovery on B", async (t) => {
  const page = await open(t);
  await page.evaluate(() => {
    window.handlers.send_key = () => new Promise((_, reject) => { window.rejectPress = reject; });
  });
  await page.getByRole("button", { name: "Volume up", exact: true }).click();
  await page.evaluate(async () => {
    await window.session.connect("B", 5555);
    window.calls = [];
    window.rejectPress(new Error("Connection to the TV was lost"));
  });
  await page.evaluate(() => new Promise((resolve) => setTimeout(resolve, 30)));
  assert.deepEqual(await page.evaluate(() => ({
    serial: window.session.serial,
    live: window.session.isConnected,
    redials: window.calls.filter((c) => c.command === "wireless_connect").length,
  })), { serial: "B:5555", live: true, redials: 0 });
});
