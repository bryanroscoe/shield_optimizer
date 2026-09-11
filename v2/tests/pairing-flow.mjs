import assert from "node:assert/strict";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const V2 = join(HERE, "..");
const PAIR_ADDRESS = "192.168.1.88:43219";
const CONNECT_ADDRESS = "192.168.1.88:37123";
const PIN = "123456";
const MDNS_NAME = "Living Room TV (mDNS)";

function serverURL(server) {
  const address = server.httpServer?.address();
  if (!address || typeof address === "string") {
    throw new Error("Vite did not expose its bound TCP address");
  }
  const host = address.address.includes(":") ? `[${address.address}]` : address.address;
  return `http://${host}:${address.port}`;
}

async function withOwnedRuntime({ makeServer, launchBrowser, exercise, reportCleanupError = console.error }) {
  let server;
  let browser;
  let result;
  let primaryError;

  try {
    server = await makeServer();
    await server.listen();
    const base = serverURL(server);
    browser = await launchBrowser();
    result = await exercise({ browser, base });
  } catch (error) {
    primaryError = error;
  }

  const cleanupErrors = [];
  if (browser) {
    try {
      await browser.close();
    } catch (error) {
      cleanupErrors.push(error);
      reportCleanupError("browser cleanup failed", error);
    }
  }
  if (server) {
    try {
      await server.close();
    } catch (error) {
      cleanupErrors.push(error);
      reportCleanupError("Vite cleanup failed", error);
    }
  }

  if (primaryError) throw primaryError;
  if (cleanupErrors.length) {
    throw new AggregateError(cleanupErrors, "pairing-flow runtime cleanup failed");
  }
  return result;
}

function setHarnessEnvironment() {
  const keys = ["VITE_DEMO", "TAURI_DEV_HOST"];
  const previous = new Map(keys.map((key) => [
    key,
    { present: Object.hasOwn(process.env, key), value: process.env[key] },
  ]));

  process.env.VITE_DEMO = "1";
  delete process.env.TAURI_DEV_HOST;

  return () => {
    for (const [key, prior] of previous) {
      if (prior.present) process.env[key] = prior.value;
      else delete process.env[key];
    }
  };
}

async function exercisePairingFlow({ browser, base }) {
  const page = await browser.newPage();
  await page.goto(base, { waitUntil: "networkidle" });
  await page.getByText("NVIDIA SHIELD", { exact: false }).first().waitFor();

  await page.evaluate(({ mdnsName }) => {
    const demo = window.__TAURI_INTERNALS__;
    const originalInvoke = demo.invoke.bind(demo);
    window.__PAIRING_FLOW_CALLS__ = [];
    demo.invoke = async (command, args = {}) => {
      window.__PAIRING_FLOW_CALLS__.push({ command, args });
      if (command === "connect_device") {
        return { ok: false, message: "failed to connect to explicit endpoint" };
      }
      const result = await originalInvoke(command, args);
      if (command === "list_devices") {
        return [
          ...result,
          {
            id: 2,
            serial: "192.168.1.88:37123",
            name: mdnsName,
            model: "TCL QM7L Pro",
            device_type: "google_tv",
            status: "device",
            connection: "network",
            properties: null,
          },
        ];
      }
      return result;
    };
  }, { mdnsName: MDNS_NAME });

  await page.getByRole("button", { name: "Refresh", exact: true }).click();
  await page.getByText(MDNS_NAME, { exact: true }).waitFor();
  await page.evaluate(() => { window.__PAIRING_FLOW_CALLS__ = []; });

  await page.getByRole("button", { name: "Pair PIN" }).click();
  const pairAddress = page.getByPlaceholder("IP:pair_port — e.g. 192.168.42.71:43219");
  const pairPin = page.getByPlaceholder("6-digit PIN");
  await pairAddress.fill(PAIR_ADDRESS);
  await pairPin.fill(PIN);
  await page.getByRole("button", { name: "Pair", exact: true }).click();
  const pairedMessage = page.getByText("Paired successfully.", { exact: false });
  await pairedMessage.waitFor();
  await page.getByText(MDNS_NAME, { exact: true }).waitFor();

  let calls = await page.evaluate(() => window.__PAIRING_FLOW_CALLS__);
  assert.deepEqual(
    calls.filter(({ command }) => command === "pair_device"),
    [{ command: "pair_device", args: { pairAddress: PAIR_ADDRESS, pin: PIN } }],
  );
  assert.equal(calls.filter(({ command }) => command === "connect_device").length, 0);
  assert.equal(calls.filter(({ command }) => command === "list_devices").length, 1);
  assert.equal(await pairAddress.inputValue(), "");
  assert.equal(await pairPin.inputValue(), "");

  const connectAddress = page.getByPlaceholder("IP[:port] — e.g. 192.168.42.71");
  await connectAddress.fill(CONNECT_ADDRESS);
  await page.getByRole("button", { name: "Connect IP" }).click();
  await page.getByText("failed to connect to explicit endpoint", { exact: true }).waitFor();

  calls = await page.evaluate(() => window.__PAIRING_FLOW_CALLS__);
  assert.deepEqual(
    calls.filter(({ command }) => command === "connect_device"),
    [{ command: "connect_device", args: { address: CONNECT_ADDRESS } }],
  );
  assert.notEqual(CONNECT_ADDRESS, PAIR_ADDRESS);
  assert.equal(await connectAddress.inputValue(), CONNECT_ADDRESS);
  await assert.doesNotReject(() => pairedMessage.waitFor({ state: "visible" }));
  await assert.doesNotReject(() => page.getByText(MDNS_NAME, { exact: true }).waitFor());

  console.log("Pairing flow passed: explicit endpoints, retained trust status, and mDNS refresh verified.");
}

async function main() {
  const restoreEnvironment = setHarnessEnvironment();
  try {
    const { createServer } = await import("vite");
    const { chromium } = await import("playwright");
    await withOwnedRuntime({
      makeServer: () => createServer({
        root: V2,
        server: {
          host: "127.0.0.1",
          port: 0,
          strictPort: false,
          hmr: false,
        },
      }),
      launchBrowser: () => chromium.launch(),
      exercise: exercisePairingFlow,
    });
  } finally {
    restoreEnvironment();
  }
}

async function runLifecycleSelfTests() {
  const quiet = () => {};
  const makeServer = (events, { listenError, closeError, port = 43127 } = {}) => ({
    httpServer: { address: () => ({ address: "127.0.0.1", family: "IPv4", port }) },
    async listen() {
      events.push("server.listen");
      if (listenError) throw listenError;
    },
    async close() {
      events.push("server.close");
      if (closeError) throw closeError;
    },
  });
  const makeBrowser = (events, closeError) => ({
    async close() {
      events.push("browser.close");
      if (closeError) throw closeError;
    },
  });

  {
    const events = [];
    const failure = new Error("listen failed");
    await assert.rejects(withOwnedRuntime({
      makeServer: async () => makeServer(events, { listenError: failure }),
      launchBrowser: async () => makeBrowser(events),
      exercise: async () => {},
      reportCleanupError: quiet,
    }), (error) => error === failure);
    assert.deepEqual(events, ["server.listen", "server.close"]);
  }
  {
    const events = [];
    const failure = new Error("browser launch failed");
    await assert.rejects(withOwnedRuntime({
      makeServer: async () => makeServer(events),
      launchBrowser: async () => { throw failure; },
      exercise: async () => {},
      reportCleanupError: quiet,
    }), (error) => error === failure);
    assert.deepEqual(events, ["server.listen", "server.close"]);
  }
  {
    const events = [];
    const failure = new Error("assertion failed");
    await assert.rejects(withOwnedRuntime({
      makeServer: async () => makeServer(events),
      launchBrowser: async () => makeBrowser(events, new Error("browser close failed")),
      exercise: async ({ base }) => {
        events.push(base);
        throw failure;
      },
      reportCleanupError: quiet,
    }), (error) => error === failure);
    assert.deepEqual(events, [
      "server.listen",
      "http://127.0.0.1:43127",
      "browser.close",
      "server.close",
    ]);
  }
  {
    const events = [];
    const result = await withOwnedRuntime({
      makeServer: async () => makeServer(events, { port: 49152 }),
      launchBrowser: async () => makeBrowser(events),
      exercise: async ({ base }) => {
        events.push(base);
        return "ok";
      },
      reportCleanupError: quiet,
    });
    assert.equal(result, "ok");
    assert.deepEqual(events, [
      "server.listen",
      "http://127.0.0.1:49152",
      "browser.close",
      "server.close",
    ]);
  }
  {
    const events = [];
    const browserCloseFailure = new Error("browser close failed");
    await assert.rejects(withOwnedRuntime({
      makeServer: async () => makeServer(events),
      launchBrowser: async () => makeBrowser(events, browserCloseFailure),
      exercise: async () => "ok",
      reportCleanupError: quiet,
    }), (error) => error instanceof AggregateError && error.errors[0] === browserCloseFailure);
    assert.deepEqual(events, ["server.listen", "browser.close", "server.close"]);
  }
  {
    const oldDemo = process.env.VITE_DEMO;
    const hadDemo = Object.hasOwn(process.env, "VITE_DEMO");
    const oldHost = process.env.TAURI_DEV_HOST;
    const hadHost = Object.hasOwn(process.env, "TAURI_DEV_HOST");
    process.env.VITE_DEMO = "caller-demo";
    process.env.TAURI_DEV_HOST = "foreign.example";
    const restore = setHarnessEnvironment();
    assert.equal(process.env.VITE_DEMO, "1");
    assert.equal(Object.hasOwn(process.env, "TAURI_DEV_HOST"), false);
    restore();
    assert.equal(process.env.VITE_DEMO, "caller-demo");
    assert.equal(process.env.TAURI_DEV_HOST, "foreign.example");
    if (hadDemo) process.env.VITE_DEMO = oldDemo;
    else delete process.env.VITE_DEMO;
    if (hadHost) process.env.TAURI_DEV_HOST = oldHost;
    else delete process.env.TAURI_DEV_HOST;
  }
  console.log("Pairing-flow lifecycle stubs passed: listen, launch, assertion, cleanup, and success paths.");
}

const run = process.env.PAIRING_FLOW_LIFECYCLE_SELF_TEST === "1"
  ? runLifecycleSelfTests
  : main;
run().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
