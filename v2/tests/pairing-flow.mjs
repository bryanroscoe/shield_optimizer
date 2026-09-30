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

const TV_ROW = {
  id: 9,
  serial: CONNECT_ADDRESS,
  name: MDNS_NAME,
  model: "TCL QM7L Pro",
  device_type: "google_tv",
  tv_evidence: "tv",
  status: "device",
  connection: "network",
  properties: {
    friendly_name: MDNS_NAME, brand: "TCL", model: "QM7L Pro", device_codename: "x",
    manufacturer: "TCL", android_release: "14", sdk_level: "34", build_id: "X",
    board_platform: "mt", characteristics: "tv", serial_number: "DEMO0001", leanback: true,
  },
};

/// Replace the demo bridge for one scenario. `probeReplies` is handed back in
/// order (sticking on the last); `connectOk` decides what connect_device says;
/// the new row joins list_devices once a connect has succeeded.
async function installBridge(page, { probeReplies, connectOk = true, pairError = null, wrongDevice = false, listed = false }) {
  await page.evaluate(({ probeReplies, connectOk, pairError, wrongDevice, listed, row }) => {
    const demo = window.__TAURI_INTERNALS__;
    const originalInvoke = demo.invoke.bind(demo);
    window.__PAIRING_FLOW_CALLS__ = [];
    let probes = 0;
    let connected = listed;
    demo.invoke = async (command, args = {}) => {
      window.__PAIRING_FLOW_CALLS__.push({ command, args });
      if (command === "pair_device" && pairError) throw pairError;
      if (command === "probe_paired_connect") {
        const reply = probeReplies[Math.min(probes, probeReplies.length - 1)];
        probes += 1;
        return reply;
      }
      if (command === "connect_paired" && wrongDevice) {
        return {
          ok: false,
          not_the_paired_device: true,
          message: `${args.address} is a different device (serial OLDTV0001, expected DEMO0001), so it was disconnected.`,
        };
      }
      if (command === "connect_device" || command === "connect_paired") {
        if (!connectOk) {
          return { ok: false, not_the_paired_device: false, message: "failed to connect to explicit endpoint" };
        }
        connected = true;
        return { ok: true, not_the_paired_device: false, message: `connected to ${args.address}` };
      }
      const result = await originalInvoke(command, args);
      if (command === "list_devices" && connected) return [...result, row];
      return result;
    };
  }, { probeReplies, connectOk, pairError, wrongDevice, listed, row: TV_ROW });
}

async function openAndPair(page, address = PAIR_ADDRESS) {
  await page.locator(".connect-form").getByRole("button", { name: "Pair PIN" }).click();
  await page.getByPlaceholder("IP:pair_port — e.g. 192.168.42.71:43219").fill(address);
  await page.getByPlaceholder("6-digit PIN").fill(PIN);
  await page.getByRole("button", { name: "Pair", exact: true }).click();
}

const calls = (page, name) =>
  page.evaluate((n) => window.__PAIRING_FLOW_CALLS__.filter((c) => c.command === n), name);

async function freshPage(browser, base, { clock = false } = {}) {
  const page = await browser.newPage();
  if (clock) await page.clock.install();
  await page.goto(base, { waitUntil: "networkidle" });
  await page.getByText("NVIDIA SHIELD", { exact: false }).first().waitFor();
  return page;
}

async function exercisePairingFlow({ browser, base }) {
  const connectBox = (page) => page.getByPlaceholder("IP[:port] — e.g. 192.168.42.71");
  const pairedHost = PAIR_ADDRESS.split(":")[0];

  // 1. Success: the connect service appears after the dialog closes, the app
  // dials exactly that advertised port, the panel closes and the new row is
  // highlighted with an Open call to action.
  {
    const page = await freshPage(browser, base);
    await installBridge(page, {
      probeReplies: [
        { state: "waiting" },
        { state: "waiting" },
        { state: "endpoint", address: CONNECT_ADDRESS },
      ],
    });
    await openAndPair(page);
    await page.getByText("Close the pairing dialog on your phone/TV.", { exact: false }).waitFor();
    await page.getByText("Waiting for the device to advertise its connect port…", { exact: false }).waitFor();
    assert.equal(await page.getByText("connecting is a separate step", { exact: false }).count(), 0,
      "the old 'more work to do' success line is gone");
    // The manual fallback is pre-filled from the start, with the host only.
    assert.equal(await connectBox(page).inputValue(), `${pairedHost}:`);

    const row = page.locator(`[data-serial="${CONNECT_ADDRESS}"]`);
    await row.waitFor({ timeout: 15000 });
    await page.getByText(`Connected ${MDNS_NAME}.`, { exact: true }).waitFor();
    assert.equal(await page.locator(".pair-form").count(), 0, "the pairing panel closes once connected");
    assert.match(await row.getAttribute("class"), /\bflash\b/, "the new row is highlighted");
    assert.equal(await row.getByRole("button", { name: "Open", exact: true }).count(), 1,
      "the new TV row carries an Open call to action");
    assert.equal(await connectBox(page).inputValue(), "");

    assert.deepEqual(await calls(page, "pair_device"),
      [{ command: "pair_device", args: { pairAddress: PAIR_ADDRESS, pin: PIN } }]);
    const probes = await calls(page, "probe_paired_connect");
    assert.equal(probes.length, 3);
    assert.ok(probes.every((c) => c.args.pairAddress === PAIR_ADDRESS));
    assert.ok(probes.every((c) => c.args.instance === "adb-DEMO0001-a1B2c3"),
      "the probe is keyed on the paired device's mDNS instance, not its address");
    assert.deepEqual(await calls(page, "connect_paired"),
      [{ command: "connect_paired", args: { address: CONNECT_ADDRESS, instance: "adb-DEMO0001-a1B2c3" } }],
      "only the advertised connect port is dialled, and its serial is checked against the pairing");
    assert.equal((await calls(page, "connect_device")).length, 0);
    assert.notEqual(CONNECT_ADDRESS, PAIR_ADDRESS);

    await row.getByRole("button", { name: "Open", exact: true }).click();
    await page.waitForURL(/\/devices\//);
    await page.close();
  }

  // 1b. adb attached it by itself under its mDNS name: nothing is dialled.
  {
    const page = await freshPage(browser, base);
    await installBridge(page, {
      probeReplies: [{ state: "attached", serial: "192.168.1.42:5555" }],
    });
    await openAndPair(page);
    await page.locator('[data-serial="192.168.1.42:5555"].flash').waitFor({ timeout: 10000 });
    assert.equal((await calls(page, "connect_device")).length, 0);
    assert.equal((await calls(page, "connect_paired")).length, 0);
    assert.equal(await page.locator(".pair-form").count(), 0);
    await page.close();
  }

  // 1c. The advertised endpoint answers with a different ro.serialno: it is
  // dropped, no row is announced, and the manual box takes over at once.
  {
    const page = await freshPage(browser, base);
    await installBridge(page, {
      probeReplies: [{ state: "endpoint", address: CONNECT_ADDRESS }],
      wrongDevice: true,
    });
    await openAndPair(page);
    await page.getByText(`${CONNECT_ADDRESS} is a different device`, { exact: false }).waitFor();
    assert.equal(await page.locator(".pair-waiting").count(), 0);
    assert.equal(await page.locator(`[data-serial="${CONNECT_ADDRESS}"]`).count(), 0);
    assert.equal(await connectBox(page).inputValue(), `${pairedHost}:`);
    assert.equal((await calls(page, "connect_paired")).length, 1, "no retry against the same stranger");
    await page.close();
  }

  // 2. Timeout: nothing is advertised within 45 s. The manual box takes over,
  // pre-filled with the host and focused (#88), and nothing is dialled.
  {
    const page = await freshPage(browser, base, { clock: true });
    await installBridge(page, { probeReplies: [{ state: "waiting" }] });
    await openAndPair(page);
    await page.getByText("Waiting for the device to advertise its connect port…", { exact: false }).waitFor();
    for (let i = 0; i < 40 && (await page.locator(".pair-waiting").count()) > 0; i++) {
      await page.clock.runFor(1500);
    }
    await page.getByText("No connect port appeared within 45 seconds.", { exact: true }).waitFor();
    await page.getByText(
      "Enter the port shown on the TV or phone's main Wireless debugging screen, then Add by IP.",
      { exact: true },
    ).waitFor();
    assert.equal(await connectBox(page).inputValue(), `${pairedHost}:`);
    assert.equal(await connectBox(page).evaluate((el) => el === document.activeElement), true,
      "the connect box must have focus after the fallback");
    assert.equal((await calls(page, "connect_device")).length + (await calls(page, "connect_paired")).length, 0, "the fallback never connects by itself");

    // The fallback still works by hand, and a failure is explained with the
    // raw adb text behind Details.
    await page.evaluate(() => {
      const demo = window.__TAURI_INTERNALS__;
      const prev = demo.invoke;
      demo.invoke = async (command, args = {}) =>
        command === "connect_device"
          ? { ok: false, message: "failed to connect to '192.168.1.88:37123': Connection refused" }
          : prev(command, args);
    });
    await connectBox(page).pressSequentially(CONNECT_ADDRESS.split(":")[1]);
    await page.locator(".connect-form").getByRole("button", { name: "Add by IP" }).click();
    await page.getByText(
      `Couldn't reach ${CONNECT_ADDRESS}. Check the IP and port match the device's main Wireless debugging screen (not the pairing dialog), and that both are on the same Wi-Fi.`,
      { exact: true },
    ).waitFor();
    await page.locator(".connect-details summary").click();
    await page.getByText("failed to connect to '192.168.1.88:37123': Connection refused", { exact: true }).waitFor();
    await page.close();
  }

  // 2b. The paired device's mDNS identity was never seen: nothing advertised
  // can be tied to it, so it goes straight to the manual box without dialling.
  {
    const page = await freshPage(browser, base);
    await installBridge(page, { probeReplies: [{ state: "unidentified" }] });
    await openAndPair(page);
    await page.getByText("couldn't tell which advertised device this is", { exact: false }).waitFor();
    assert.equal(await page.locator(".pair-waiting").count(), 0);
    assert.equal(await connectBox(page).inputValue(), `${pairedHost}:`);
    assert.equal((await calls(page, "connect_device")).length, 0);
    await page.close();
  }

  // 1d. adb reports the device under its mDNS key, but the list collapsed it
  // into a row kept under IP:port. The row is found by the verified
  // ro.serialno that the pairing instance embeds, and highlighted.
  {
    const page = await freshPage(browser, base);
    await installBridge(page, {
      probeReplies: [{ state: "attached", serial: "adb-DEMO0001-zZ9._adb-tls-connect._tcp" }],
      listed: true,
    });
    await openAndPair(page);
    const row = page.locator(`[data-serial="${CONNECT_ADDRESS}"]`);
    await page.locator(`[data-serial="${CONNECT_ADDRESS}"].flash`).waitFor({ timeout: 10000 });
    await page.getByText(`Connected ${MDNS_NAME}.`, { exact: true }).waitFor();
    assert.equal(await row.getByRole("button", { name: "Open", exact: true }).count(), 1);
    await page.close();
  }

  // 2c. The endpoint is advertised on every poll but keeps refusing. At the
  // timeout that failure is what the user is told, not "nothing appeared".
  {
    const page = await freshPage(browser, base, { clock: true });
    await installBridge(page, {
      probeReplies: [{ state: "endpoint", address: CONNECT_ADDRESS }],
      connectOk: false,
    });
    await openAndPair(page);
    await page.getByText("Waiting for the device to advertise its connect port…", { exact: false }).waitFor();
    for (let i = 0; i < 40 && (await page.locator(".pair-waiting").count()) > 0; i++) {
      await page.clock.runFor(1500);
    }
    await page.getByText(`It advertised ${CONNECT_ADDRESS}, but connecting kept failing.`, { exact: false }).waitFor();
    assert.equal(await page.getByText("No connect port appeared", { exact: false }).count(), 0);
    await page.locator(".pair-form .adb-details summary").click();
    await page.locator(".pair-form .adb-details").getByText("failed to connect to explicit endpoint", { exact: true }).waitFor();
    assert.equal(await connectBox(page).inputValue(), `${pairedHost}:`);
    await page.close();
  }

  // 1e. adb holds a transport the advertisement points at, but it answered
  // with a different ro.serialno: refused, manual box, nothing highlighted.
  {
    const page = await freshPage(browser, base);
    await installBridge(page, {
      probeReplies: [{
        state: "not_the_paired_device",
        message: "192.168.1.42:5555 is attached, but it isn't the device that was just paired, so it wasn't picked.",
      }],
    });
    await openAndPair(page);
    await page.getByText("isn't the device that was just paired", { exact: false }).waitFor();
    assert.equal(await page.locator(".pair-waiting").count(), 0);
    assert.equal(await page.locator(".device-row.flash").count(), 0);
    assert.equal(await connectBox(page).inputValue(), `${pairedHost}:`);
    await page.close();
  }

  // 3. Cancel hands over to the manual box straight away.
  {
    const page = await freshPage(browser, base);
    await installBridge(page, { probeReplies: [{ state: "waiting" }] });
    await openAndPair(page);
    await page.locator(".pair-waiting").getByRole("button", { name: "Cancel" }).click();
    await page.getByText("Stopped waiting.", { exact: true }).waitFor();
    assert.equal(await page.locator(".pair-waiting").count(), 0);
    assert.equal(await connectBox(page).inputValue(), `${pairedHost}:`);
    const before = (await calls(page, "probe_paired_connect")).length;
    await page.waitForTimeout(2000);
    assert.equal((await calls(page, "probe_paired_connect")).length, before, "Cancel stops polling");
    await page.close();
  }

  // 4. The owner's typo: a warning before sending, then plain guidance after,
  // with adb's words behind Details.
  {
    const page = await freshPage(browser, base);
    await installBridge(page, {
      probeReplies: [{ state: "waiting" }],
      pairError:
        "adb pair: adb process failed (exit code Some(1)): error: protocol fault (couldn't read status message): Undefined error: 0",
    });
    await page.locator(".connect-form").getByRole("button", { name: "Pair PIN" }).click();
    await page.getByPlaceholder("IP:pair_port — e.g. 192.168.42.71:43219").fill("182.168.42.211:45439");
    await page.getByText("This computer is on 192.168.1.x, and 182.168.42.211 isn't on it. Typo?", { exact: true }).waitFor();
    await page.getByPlaceholder("6-digit PIN").fill(PIN);
    await page.getByRole("button", { name: "Pair", exact: true }).click();
    await page.getByText(
      "Couldn't reach 182.168.42.211:45439. Check the IP and pairing port match the pairing dialog, that it's still open, and that both are on the same Wi-Fi.",
      { exact: true },
    ).waitFor();
    const details = page.locator(".pair-form .adb-details");
    assert.equal(await details.evaluate((el) => el.open), false, "raw text stays folded away");
    await details.locator("summary").click();
    await details.getByText("error: protocol fault (couldn't read status message): Undefined error: 0", { exact: true }).waitFor();
    assert.equal((await calls(page, "probe_paired_connect")).length, 0, "a failed pair never polls");
    // Same subnet: no warning.
    await page.getByPlaceholder("IP:pair_port — e.g. 192.168.42.71:43219").fill("192.168.1.88:45439");
    await page.waitForTimeout(200);
    assert.equal(await page.locator(".pair-warning").count(), 0);
    await page.close();
  }

  // 5. The home callout's inline actions open the pair panel and focus the
  // connect box.
  {
    const page = await freshPage(browser, base);
    await page.locator(".devices-note").getByRole("button", { name: "Pair PIN" }).click();
    await page.locator(".pair-form").waitFor();
    assert.equal(await page.getByPlaceholder("IP:pair_port — e.g. 192.168.42.71:43219")
      .evaluate((el) => el === document.activeElement), true);
    await page.locator(".devices-note").getByRole("button", { name: "Add by IP" }).click();
    assert.equal(await connectBox(page).evaluate((el) => el === document.activeElement), true);
    assert.equal(await page.getByText("nothing is sent anywhere else", { exact: false }).count(), 0);
    await page.close();
  }

  console.log(
    "Pairing flow passed: auto-connect to the advertised port (and to adb's own attach) closes the panel and highlights the row, a 45 s timeout and Cancel fall back to the pre-filled focused connect box without dialling, pair/connect failures are explained with raw Details, the typo warning fires off-subnet, and the home callout's actions work.",
  );
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
