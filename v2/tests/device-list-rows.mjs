// The Devices list has to be honest about several things at once:
//   - one physical device reached two ways is ONE row (the duplicate-transport
//     regression: adb auto-connects a paired mDNS device under its service
//     name, and dialling it again by address makes a second transport);
//   - a device that is positively not an Android TV says so and does not open
//     the TV tools until its owner confirms "Open anyway" (remembered per
//     hardware id); one that never said either way still opens, labelled
//     UNCONFIRMED TV (#120); and one we cannot read at all claims nothing;
//   - a network device is not told to look for a *USB* dialog;
//   - every network row can be forgotten, and no USB row can, and forgetting
//     from a clickable row does not navigate into the device it just dropped.
//
// Rendering is where all of it becomes visible, so this drives the real screen.
import assert from "node:assert/strict";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const V2 = join(HERE, "..");

function serverURL(server) {
  const address = server.httpServer?.address();
  if (!address || typeof address === "string") {
    throw new Error("Vite did not expose its bound TCP address");
  }
  const host = address.address.includes(":") ? `[${address.address}]` : address.address;
  return `http://${host}:${address.port}`;
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

const tvProperties = {
  friendly_name: "Bedroom Shield", brand: "NVIDIA", model: "SHIELD Android TV",
  device_codename: "mdarcy", manufacturer: "NVIDIA", android_release: "11",
  sdk_level: "30", build_id: "PPR1", board_platform: "tegra",
  characteristics: "tv", serial_number: "1324619053514", leanback: true,
};

// A phone SAYS it is a phone. That is the only thing that earns the label —
// `nosdcard` alone used to be read as "not a TV" and locked out real boxes.
const phoneProperties = {
  friendly_name: null, brand: "google", model: "Pixel 10 Pro",
  device_codename: "blazer", manufacturer: "Google", android_release: "16",
  sdk_level: "36", build_id: "BP41", board_platform: "zuma",
  characteristics: "nosdcard,phone", serial_number: "58040DLCH005YV",
  leanback: false,
};

// The #120 device: readable, and it never said either way.
const oddBoxProperties = {
  friendly_name: "Living Room Box", brand: "Xiaomi", model: "MiBOX4",
  device_codename: "cezanne", manufacturer: "Xiaomi", android_release: "12",
  sdk_level: "31", build_id: "STTE", board_platform: "amlogic",
  characteristics: "nosdcard", serial_number: "9AB4C21D7E03", leanback: null,
};

const ROWS = [
  { id: 1, serial: "192.168.42.196:5555", name: "Bedroom Shield",
    model: "Shield TV Pro (2019)", device_type: "shield", tv_evidence: "tv",
    status: "device", connection: "network", properties: tvProperties },
  // It reported a non-TV form factor = positively not a TV.
  { id: 2, serial: "192.168.42.211:34083", name: "Bryan Pixel 10 Pro",
    model: "Pixel 10 Pro", device_type: "unknown", tv_evidence: "not_tv",
    status: "device", connection: "network", properties: phoneProperties },
  // No properties = we do not know what this is yet. Claim nothing.
  { id: 3, serial: "192.168.42.143:5555", name: "192.168.42.143:5555",
    model: "", device_type: "unknown", tv_evidence: "unknown",
    status: "unauthorized", connection: "network", properties: null },
  // Same, but over USB — the one place the USB wording is right.
  { id: 4, serial: "0323220012345", name: "0323220012345",
    model: "", device_type: "unknown", tv_evidence: "unknown",
    status: "unauthorized", connection: "usb", properties: null },
  // Readable, said neither way: openable, labelled, and not accused.
  { id: 5, serial: "192.168.42.77:5555", name: "Living Room Box",
    model: "MiBOX4", device_type: "unknown", tv_evidence: "unknown",
    status: "device", connection: "network", properties: oddBoxProperties },
  // A cabled TV. `adb disconnect` has nothing to do here.
  { id: 6, serial: "0323220054321", name: "Workshop Shield",
    model: "Shield TV (2019 Tube)", device_type: "shield", tv_evidence: "tv",
    status: "device", connection: "usb", properties: tvProperties },
  // Unauthorized under a Wireless debugging key: the only kind that appears
  // in that screen's Paired devices list.
  { id: 7, serial: "adb-9XK2-abc._adb-tls-connect._tcp", name: "adb-9XK2-abc._adb-tls-connect._tcp",
    model: "", device_type: "unknown", tv_evidence: "unknown",
    status: "unauthorized", connection: "network", properties: null },
];

const rowFor = (page, name) =>
  page.locator("li", { has: page.getByText(name, { exact: true }) }).first();

async function exercise({ browser, base }) {
  const page = await browser.newPage({ viewport: { width: 1280, height: 1400 } });
  await page.goto(base, { waitUntil: "networkidle" });
  await page.getByText("NVIDIA SHIELD", { exact: false }).first().waitFor();

  // First, the demo fixture itself: the gallery has to show the openable
  // unconfirmed row, or a regression here would only ever be seen on a real
  // device nobody on the team owns.
  const demoOdd = rowFor(page, "Living Room Box");
  assert.equal(await demoOdd.locator("a.device-row").count(), 1,
    "the demo's unconfirmed box must still open");
  assert.equal(await demoOdd.getByText("UNCONFIRMED TV").count(), 1,
    "and must say that it never confirmed what it is");

  await page.evaluate((rows) => {
    const bridge = window.__TAURI_INTERNALS__;
    const original = bridge.invoke.bind(bridge);
    bridge.invoke = async (command, args = {}) =>
      command === "list_devices" ? rows : original(command, args);
  }, ROWS);
  await page.getByRole("button", { name: "Refresh", exact: true }).click();
  await page.getByText("Bryan Pixel 10 Pro", { exact: true }).waitFor();

  // A real TV opens the tools and carries no "not a TV" tag.
  const tv = rowFor(page, "Bedroom Shield");
  assert.equal(await tv.locator("a.device-row").count(), 1, "a TV row must be a link");
  assert.equal(await tv.getByText("NOT AN ANDROID TV").count(), 0);

  // A phone is listed, labelled, and not openable.
  const phone = rowFor(page, "Bryan Pixel 10 Pro");
  assert.equal(await phone.getByText("NOT AN ANDROID TV").count(), 1,
    "a device that reported it is not a TV must say so");
  assert.equal(await phone.locator("a.device-row").count(), 0,
    "the TV tools must not open for a phone");
  assert.equal(await phone.getByText("Pixel 10 Pro").count() > 0, true,
    "it stays visible — labelling is not hiding");

  // An unauthorized NETWORK device: no USB wording.
  const net = rowFor(page, "192.168.42.143:5555");
  const netHelp = await net.locator(".unauthorized-help").innerText();
  assert.match(netHelp, /"Allow debugging\?"/, netHelp);
  assert.doesNotMatch(netHelp.split("Revoke")[0], /Allow USB debugging/,
    "a network device must not be told to look for a USB dialog");
  // We do not know what it is, so we must not label it.
  assert.equal(await net.getByText("NOT AN ANDROID TV").count(), 0,
    "an unreadable device is unknown, not known-not-a-TV");

  // No-dialog fallback: the app's own controls, never a terminal adb (the
  // bundled adb is not on PATH, and a different one restarts the shared
  // server and drops every device). Wake first; Revoke says what it costs.
  assert.match(netHelp, /Wake the TV; the prompt can be hidden behind the screensaver\./, netHelp);
  assert.match(netHelp, /Click Forget here, then Add by IP again; the prompt reappears\./, netHelp);
  assert.match(netHelp, /Revoke USB debugging authorizations\. That un-trusts every computer, not just this one\./, netHelp);
  assert.doesNotMatch(netHelp, /terminal|adb disconnect/, "no terminal instructions");
  assert.doesNotMatch(netHelp, /Paired devices|tap this computer/,
    "a :5555 transport is legacy network debugging and never appears under Wireless debugging");
  const wireless = rowFor(page, "adb-9XK2-abc._adb-tls-connect._tcp");
  const wirelessHelp = await wireless.locator(".unauthorized-help").innerText();
  assert.match(wirelessHelp, /or Wireless debugging → tap this computer → Forget/, wirelessHelp);

  // An unauthorized USB device: the USB wording is correct and must survive.
  const usb = rowFor(page, "0323220012345");
  const usbHelp = await usb.locator(".unauthorized-help").innerText();
  assert.match(usbHelp, /"Allow USB debugging\?"/, usbHelp);
  assert.doesNotMatch(usbHelp, /Click Forget/, "a USB row has no Forget");
  assert.doesNotMatch(usbHelp, /terminal|adb disconnect/);

  // #120: a box that never said what it is still opens, and says so. This is
  // the row 2.2.0 locked its owner out of.
  const odd = rowFor(page, "Living Room Box");
  assert.equal(await odd.locator("a.device-row").count(), 1,
    "a device that said neither way must still open");
  assert.equal(await odd.getByText("UNCONFIRMED TV").count(), 1,
    "and must say that it never confirmed what it is");
  assert.equal(await odd.getByText("NOT AN ANDROID TV").count(), 0,
    "saying nothing is not the same as saying no");
  assert.equal(await odd.getByRole("button", { name: "Copy diagnostics" }).count(), 1,
    "the row we are unsure about is the one worth reporting");

  // Forget is on EVERY network row now, including the online, clickable ones.
  for (const name of ["Bedroom Shield", "Bryan Pixel 10 Pro", "192.168.42.143:5555", "Living Room Box", "adb-9XK2-abc._adb-tls-connect._tcp"]) {
    assert.equal(await rowFor(page, name).getByRole("button", { name: "Forget" }).count(), 1,
      `${name} is on the network, so it can be forgotten`);
  }
  // …and on no USB row: `adb disconnect` has nothing to drop over a cable.
  for (const name of ["0323220012345", "Workshop Shield"]) {
    assert.equal(await rowFor(page, name).getByRole("button", { name: "Forget" }).count(), 0,
      `${name} is cabled, so Forget would mean nothing`);
  }

  // Forgetting from inside the row link must not follow the link. The row is
  // an <a>; without stopping the click, "Forget" would open the device it had
  // just disconnected.
  const before = page.url();
  await rowFor(page, "Bedroom Shield").getByRole("button", { name: "Forget" }).click();
  await page.waitForTimeout(300);
  assert.equal(page.url(), before, "Forget must not navigate into the device");

  // The not-a-TV row offers "Open anyway", and backing out of the confirm
  // leaves it exactly as inert as before.
  await phone.getByRole("button", { name: "Open anyway" }).click();
  await phone.getByText("The tools are built for", { exact: false }).waitFor();
  await phone.getByRole("button", { name: "Cancel" }).click();
  assert.equal(await phone.getByText("The tools are built for", { exact: false }).count(), 0);
  assert.equal(await phone.locator("a.device-row").count(), 0, "Cancel must not open it");

  // Forget is one backend call that drops every alias of the device, and when
  // adb will re-attach it (still advertising Wireless debugging) the screen
  // says so, then shows it back rather than pretending it is gone.
  await page.evaluate((rows) => {
    const bridge = window.__TAURI_INTERNALS__;
    const previous = bridge.invoke;
    window.__FORGET_CALLS__ = [];
    let forgotten = false;
    let lists = 0;
    bridge.invoke = async (command, args = {}) => {
      if (command === "disconnect_device") throw new Error("Forget must not use the single-key disconnect");
      if (command === "forget_device") {
        window.__FORGET_CALLS__.push(args.serial);
        forgotten = true;
        return {
          ok: true,
          disconnected: [args.serial, "adb-58040DLCH005YV-jBeCEe._adb-tls-connect._tcp"],
          still_advertised: true,
          message: "Disconnected, but the device is still advertising Wireless debugging, so adb will reconnect it by itself within a few seconds.",
        };
      }
      if (command === "list_devices" && forgotten) {
        lists += 1;
        return lists === 1 ? rows.filter((r) => r.name !== "Bryan Pixel 10 Pro") : rows;
      }
      return previous(command, args);
    };
  }, ROWS);
  await phone.getByRole("button", { name: "Forget" }).click();
  await page.getByText("still advertising Wireless debugging, so adb will reconnect it", { exact: false }).waitFor();
  await page.getByText("Bryan Pixel 10 Pro reconnected by itself", { exact: false }).waitFor({ timeout: 10000 });
  assert.match(
    await page.locator(".connect-message").first().innerText(),
    /turn off Wireless debugging on the device, or remove this computer under Wireless debugging → Paired devices/,
  );
  assert.deepEqual(await page.evaluate(() => window.__FORGET_CALLS__), ["192.168.42.211:34083"]);
  await page.getByText("Bryan Pixel 10 Pro", { exact: true }).waitFor();
  await page.close();

  await exerciseOpenAnyway({ browser, base });

  console.log(
    "Device list rows passed: TVs open, a known non-TV is labelled and inert until Open anyway is confirmed (then remembered by hardware id across a reload), a device that said neither way opens with an UNCONFIRMED TV tag, only USB devices are told about a USB dialog, and Forget is on every network row without navigating.",
  );
}

// #120 follow-up: a device that really is not a TV can still be opened, on
// purpose, and the choice sticks to that hardware — not to its address.
async function exerciseOpenAnyway({ browser, base }) {
  const page = await browser.newPage({ viewport: { width: 1280, height: 1400 } });
  await page.addInitScript(() => localStorage.setItem("shieldopt.demo.notTv", "1"));
  await page.goto(base, { waitUntil: "networkidle" });
  await page.getByText("Pixel Tablet", { exact: true }).first().waitFor();

  let tablet = rowFor(page, "Pixel Tablet");
  assert.equal(await tablet.locator("a.device-row").count(), 0,
    "a device that said it is not a TV must not be a link at first");
  assert.equal(await tablet.getByText("NOT AN ANDROID TV").count(), 1);

  await tablet.getByRole("button", { name: "Open anyway" }).click();
  await tablet.getByText("some of them may not apply", { exact: false }).waitFor();
  await tablet.getByRole("button", { name: "Open tools" }).click();
  await page.waitForURL(/\/devices\/192\.168\.1\.88%3A37015/);

  const stored = await page.evaluate(() => localStorage.getItem("shieldopt.openNonTv"));
  assert.deepEqual(JSON.parse(stored ?? "[]"), ["3A171FDJH00ZX4"],
    "the choice is filed under the hardware id, never the address");

  await page.goto(base, { waitUntil: "networkidle" });
  await page.getByText("Pixel Tablet", { exact: true }).first().waitFor();
  tablet = rowFor(page, "Pixel Tablet");
  assert.equal(await tablet.locator("a.device-row").count(), 1,
    "after a reload the remembered device opens directly");
  assert.equal(await tablet.getByText("NOT A TV", { exact: true }).count(), 1,
    "and still says what it is");
  assert.equal(await tablet.getByRole("button", { name: "Open anyway" }).count(), 0);

  await tablet.locator("a.device-row").click();
  await page.waitForURL(/\/devices\/192\.168\.1\.88%3A37015/);
  await page.close();
}

async function main() {
  const restoreEnvironment = setHarnessEnvironment();
  let server;
  let browser;
  try {
    const { createServer } = await import("vite");
    const { chromium } = await import("playwright");
    server = await createServer({
      root: V2,
      server: { host: "127.0.0.1", port: 0, strictPort: false, hmr: false },
    });
    await server.listen();
    browser = await chromium.launch();
    await exercise({ browser, base: serverURL(server) });
  } finally {
    await browser?.close().catch((e) => console.error("browser cleanup failed", e));
    await server?.close().catch((e) => console.error("Vite cleanup failed", e));
    restoreEnvironment();
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
