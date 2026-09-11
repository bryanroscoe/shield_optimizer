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

async function createPage(t, options = {}) {
  const page = await browser.newPage({ viewport: { width: 384, height: 812 } });
  t.after(() => page.close());
  await page.addInitScript((options) => {
    localStorage.removeItem("atv.savedDevices.v1");
    localStorage.removeItem("atv.autoConnect.v1");
    if (options.savedDevices) {
      localStorage.setItem("atv.savedDevices.v1", JSON.stringify(options.savedDevices));
    }
    if (options.autoConnect === false) localStorage.setItem("atv.autoConnect.v1", "0");
    window.calls = [];
    window.handlers = {};
    window.pendingConnects = [];
    window.pendingProfiles = [];
    window.connectMode = options.connectMode ?? "success";
    window.profileMode = options.profileMode ?? "matching";
    window.device = (host) => ({
      id: 1, serial: `${host}:5555`, name: host, model: "Shield",
      status: "device", connection: "network", device_type: "shield", properties: null,
    });
    window.activeHost = options.activeHost ?? "";
    window.__TAURI_INTERNALS__ = { invoke: async (command, args) => {
      window.calls.push({ command, args });
      if (window.handlers[command]) return window.handlers[command](args);
      switch (command) {
        case "get_entitlement": return "pro";
        case "list_devices": {
          if (window.profileMode === "deferred") {
            return new Promise((resolve) => window.pendingProfiles.push({
              host: window.activeHost,
              resolve,
            }));
          }
          if (!window.activeHost || window.profileMode === "missing") return [];
          const host = window.profileMode === "wrong" ? "wrong-host" : window.activeHost;
          return [window.device(host)];
        }
        case "wireless_status": return { connected: window.activeHost !== "" };
        case "wireless_connect": {
          if (window.connectMode === "deferred") {
            return new Promise((resolve, reject) => window.pendingConnects.push({
              args,
              resolve: (result) => {
                if (result.ok) window.activeHost = args.host;
                resolve(result);
              },
              reject,
            }));
          }
          if (window.connectMode === "false") {
            return { ok: false, message: "Authorization was not accepted." };
          }
          if (window.connectMode === "reject") throw new Error("Connection request rejected");
          window.activeHost = args.host;
          return { ok: true };
        }
        case "wireless_disconnect": window.activeHost = ""; return { ok: true };
        case "wireless_cancel_connect": return;
        case "wireless_discover": return { devices: [], warnings: [] };
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
  }, options);
  await page.goto(origin);
  await page.evaluate(async () => {
    const { session } = await import("/src/lib/session.svelte.ts");
    const { router } = await import("/src/lib/router.svelte.ts");
    window.session = session;
    window.router = router;
    session.entitlement = "pro";
  });
  return page;
}

async function open(t, screen = "remote") {
  const page = await createPage(t, { activeHost: "A" });
  await page.evaluate(async (screen) => {
    await session.connect("A", 5555);
    router.reset("dashboard");
    router.navigate(screen);
  }, screen);
  return page;
}

const savedA = {
  host: "A",
  connectPort: 5555,
  name: "Living Room",
  deviceType: "shield",
  lastUsed: "2026-09-05T12:00:00.000Z",
};
const savedB = {
  host: "B",
  connectPort: 5555,
  name: "Bedroom",
  deviceType: "google_tv",
  lastUsed: "2026-09-04T12:00:00.000Z",
};

async function connectManually(page, host) {
  await page.getByRole("button", { name: "Enter IP address manually" }).click();
  await page.getByRole("textbox", { name: "TV IP address" }).fill(host);
  await page.getByRole("button", { name: "Connect (no code)" }).click();
  await page.getByRole("button", { name: "Open dashboard" }).waitFor();
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

test("cold single saved TV waits for its profile, then opens a clean dashboard stack", async (t) => {
  const page = await createPage(t, { savedDevices: [savedA], profileMode: "deferred" });
  await page.waitForFunction(() => window.pendingProfiles.length === 1);
  await page.evaluate(() => {
    const reset = window.router.reset.bind(window.router);
    window.dashboardResets = 0;
    window.router.reset = (screen) => {
      if (screen === "dashboard") window.dashboardResets += 1;
      reset(screen);
    };
  });
  assert.deepEqual(await page.evaluate(() => ({
    stack: [...window.router.stack],
    serial: window.session.serial,
    host: window.session.host,
    connected: window.session.isConnected,
    connects: window.calls.filter((c) => c.command === "wireless_connect").length,
    profiles: window.calls.filter((c) => c.command === "list_devices").length,
  })), {
    stack: ["onboarding"], serial: "", host: "A", connected: false, connects: 1, profiles: 1,
  });

  await page.evaluate(() => window.pendingProfiles[0].resolve([window.device("A")]));
  await page.waitForFunction(() => window.router.current === "dashboard");
  assert.deepEqual(await page.evaluate(() => ({
    stack: [...window.router.stack],
    serial: window.session.serial,
    host: window.session.host,
    resets: window.dashboardResets,
    canceled: window.calls.filter((c) => c.command === "wireless_cancel_connect").length,
  })), { stack: ["dashboard"], serial: "A:5555", host: "A", resets: 1, canceled: 0 });
});

test("cold multi-TV launch waits for a choice and opens the chosen TV", async (t) => {
  const page = await createPage(t, { savedDevices: [savedA, savedB] });
  await page.getByRole("heading", { name: "Which TV?" }).waitFor();
  await page.evaluate(() => new Promise((resolve) => setTimeout(resolve, 30)));
  assert.equal(
    await page.evaluate(() => window.calls.filter((c) => c.command === "wireless_connect").length),
    0,
  );

  await page.getByRole("button", { name: /Bedroom/ }).click();
  await page.waitForFunction(() => window.router.current === "dashboard");
  assert.deepEqual(await page.evaluate(() => ({
    stack: [...window.router.stack],
    serial: window.session.serial,
    host: window.session.host,
    targets: window.calls
      .filter((c) => c.command === "wireless_connect")
      .map((c) => c.args.host),
    canceled: window.calls.filter((c) => c.command === "wireless_cancel_connect").length,
  })), { stack: ["dashboard"], serial: "B:5555", host: "B", targets: ["B"], canceled: 0 });
});

for (const [name, options] of [
  ["missing profile", { profileMode: "missing" }],
  ["wrong profile", { profileMode: "wrong" }],
  ["rejected request", { connectMode: "reject" }],
]) {
  test(`cold saved TV ${name} stays on onboarding`, async (t) => {
    const page = await createPage(t, { savedDevices: [savedA], ...options });
    await page.getByRole("heading", { name: "Couldn't reconnect" }).waitFor();
    assert.deepEqual(await page.evaluate(() => ({
      stack: [...window.router.stack],
      connected: window.session.isConnected,
      connects: window.calls.filter((c) => c.command === "wireless_connect").length,
      canceled: window.calls.filter((c) => c.command === "wireless_cancel_connect").length,
    })), { stack: ["onboarding"], connected: false, connects: 1, canceled: 0 });
  });
}

test("authorization failure stays honest and a deliberate retry can open Dashboard", async (t) => {
  const page = await createPage(t, { savedDevices: [savedA], connectMode: "false" });
  await page.getByRole("heading", { name: "Couldn't reconnect" }).waitFor();
  assert.equal(await page.getByText(/Always allow/).count(), 1);
  assert.deepEqual(await page.evaluate(() => [...window.router.stack]), ["onboarding"]);

  await page.evaluate(() => { window.connectMode = "success"; });
  await page.getByRole("button", { name: /Living Room/ }).click();
  await page.waitForFunction(() => window.router.current === "dashboard");
  assert.deepEqual(await page.evaluate(() => ({
    stack: [...window.router.stack],
    connects: window.calls.filter((c) => c.command === "wireless_connect").length,
  })), { stack: ["dashboard"], connects: 2 });
});

test("explicit disconnect suppresses automatic saved-TV redial", async (t) => {
  const page = await open(t);
  await page.evaluate(async () => {
    await window.session.disconnect();
    window.calls = [];
    window.router.reset("onboarding");
  });
  await page.getByRole("heading", { name: "Which TV?" }).waitFor();
  await page.evaluate(() => new Promise((resolve) => setTimeout(resolve, 30)));
  assert.deepEqual(await page.evaluate(() => ({
    stack: [...window.router.stack],
    connects: window.calls.filter((c) => c.command === "wireless_connect").length,
  })), { stack: ["onboarding"], connects: 0 });
});

test("first-time connection keeps the connected interstitial", async (t) => {
  const page = await createPage(t);
  await connectManually(page, "C");
  assert.deepEqual(await page.evaluate(() => ({
    stack: [...window.router.stack],
    serial: window.session.serial,
  })), { stack: ["onboarding"], serial: "C:5555" });
});

test("add-TV connection keeps the connected interstitial", async (t) => {
  const page = await createPage(t);
  await page.evaluate(() => window.router.reset("addtv"));
  await connectManually(page, "B");
  assert.deepEqual(await page.evaluate(() => ({
    stack: [...window.router.stack],
    serial: window.session.serial,
  })), { stack: ["addtv"], serial: "B:5555" });
});

test("saved TV success opens its dashboard once without canceling the connection", async (t) => {
  const page = await open(t);
  await page.evaluate(() => {
    window.calls = [];
    window.handlers.wireless_connect = () => new Promise((resolve) => { window.releaseConnect = resolve; });
    const reset = window.router.reset.bind(window.router);
    window.dashboardResets = 0;
    window.router.reset = (screen) => {
      if (screen === "dashboard") window.dashboardResets += 1;
      reset(screen);
    };
    window.router.reset("onboarding");
  });
  await page.waitForFunction(() => !!window.releaseConnect);
  await page.evaluate(() => window.releaseConnect({ ok: true }));
  await page.waitForFunction(() => window.router.current === "dashboard");
  assert.deepEqual(await page.evaluate(() => ({
    resets: window.dashboardResets,
    serial: window.session.serial,
    host: window.session.host,
    canceled: window.calls.filter((c) => c.command === "wireless_cancel_connect").length,
  })), { resets: 1, serial: "A:5555", host: "A", canceled: 0 });
});

test("leaving onboarding cancels a saved TV connection that is still pending", async (t) => {
  const page = await open(t);
  await page.evaluate(() => {
    window.calls = [];
    window.handlers.wireless_connect = () => new Promise((resolve) => { window.releaseConnect = resolve; });
    window.router.reset("onboarding");
  });
  await page.waitForFunction(() => !!window.releaseConnect);
  await page.evaluate(() => window.router.reset("dashboard"));
  await page.waitForFunction(() => window.calls.some((c) => c.command === "wireless_cancel_connect"));
  await page.evaluate(() => window.releaseConnect({ ok: true }));
  await page.evaluate(() => new Promise((resolve) => setTimeout(resolve, 30)));
  assert.equal(await page.evaluate(() => window.router.current), "dashboard");
  assert.equal(
    await page.evaluate(() => window.calls.filter((c) => c.command === "wireless_cancel_connect").length),
    1,
  );
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
  assert.equal(await page.evaluate(() => window.router.current), "onboarding");
  assert.equal(await page.getByRole("button", { name: "Cancel", exact: true }).count(), 1);
  await page.evaluate(() => window.pendingConnects[1]({ ok: true }));
  await page.waitForFunction(() => window.router.current === "dashboard");
  assert.deepEqual(await page.evaluate(() => ({
    serial: window.session.serial,
    host: window.session.host,
    canceled: window.calls.filter((c) => c.command === "wireless_cancel_connect").length,
  })), { serial: "A:5555", host: "A", canceled: 1 });
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
