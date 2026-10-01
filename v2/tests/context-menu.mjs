// The webview's own right-click menu (#129) offers "Open Link in New Window",
// "Download Linked File" and friends on a device row — none of which mean
// anything in this app. Release builds suppress it, and the rows where a
// right-click has an obvious meaning get a small app menu instead. Text fields
// must keep their native menu: Cut / Copy / Paste are the whole point there.
//
// The dev server keeps the native menu (Inspect Element), so this sets the
// dev-only switch that previews the release behaviour.
import assert from "node:assert/strict";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const V2 = join(HERE, "..");
const DEMO_SERIAL = "192.168.1.42:5555";

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

const ROWS = [
  { id: 1, serial: "192.168.42.196:5555", name: "Bedroom Shield",
    model: "Shield TV Pro (2019)", device_type: "shield", tv_evidence: "tv",
    status: "device", connection: "network", properties: tvProperties },
  { id: 2, serial: "0323220054321", name: "Workshop Shield",
    model: "Shield TV (2019 Tube)", device_type: "shield", tv_evidence: "tv",
    status: "device", connection: "usb",
    properties: { ...tvProperties, friendly_name: "Workshop Shield", serial_number: "0323220054321" } },
  // Wireless debugging key: an mDNS name, so there is no IP to copy.
  { id: 3, serial: "adb-9XK2-abc._adb-tls-connect._tcp", name: "adb-9XK2-abc._adb-tls-connect._tcp",
    model: "", device_type: "unknown", tv_evidence: "unknown",
    status: "unauthorized", connection: "network", properties: null },
];

const INIT = () => {
  localStorage.setItem("shieldopt.dev.suppressContextMenu", "1");
  window.__COPIED__ = [];
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { writeText: async (text) => { window.__COPIED__.push(text); } },
  });
  // Registered on window, so it runs after the app's document listener and
  // sees the final verdict for every right-click.
  window.__CTX__ = [];
  window.addEventListener("contextmenu", (e) => {
    window.__CTX__.push({ tag: e.target?.tagName ?? null, prevented: e.defaultPrevented });
  });
};

const rowFor = (page, name) =>
  page.locator("li", { has: page.getByText(name, { exact: true }) }).first();

const lastContextMenu = (page) => page.evaluate(() => window.__CTX__.at(-1));
const lastCopied = (page) => page.evaluate(() => window.__COPIED__.at(-1));

async function exercise({ browser, base }) {
  const page = await browser.newPage({ viewport: { width: 1280, height: 1000 } });
  await page.addInitScript(INIT);
  await page.goto(base, { waitUntil: "networkidle" });
  await page.getByText("NVIDIA SHIELD", { exact: false }).first().waitFor();

  await page.evaluate((rows) => {
    const bridge = window.__TAURI_INTERNALS__;
    const original = bridge.invoke.bind(bridge);
    window.__FORGET__ = [];
    bridge.invoke = async (command, args = {}) => {
      if (command === "list_devices") return rows;
      if (command === "forget_device") {
        window.__FORGET__.push(args.serial);
        return { ok: true, disconnected: [args.serial], still_advertised: false, message: "" };
      }
      return original(command, args);
    };
  }, ROWS);
  await page.getByRole("button", { name: "Refresh", exact: true }).click();
  await page.getByText("Workshop Shield", { exact: true }).waitFor();

  const menu = page.getByRole("menu");

  // Right-click on a device row: the app menu, not the webview's.
  const tv = rowFor(page, "Bedroom Shield");
  await tv.locator(".device-name").click({ button: "right" });
  await menu.waitFor();
  assert.equal((await lastContextMenu(page)).prevented, true,
    "the row's right-click must not reach the webview's own menu");
  assert.deepEqual(
    await menu.getByRole("menuitem").allInnerTexts(),
    ["Open", "Copy IP", "Copy serial", "Copy diagnostics", "Forget"],
  );

  // Keyboard: focus lands in the menu, and Tab cannot walk out of it.
  const focusedRole = () => page.evaluate(() => document.activeElement?.getAttribute("role"));
  await page.waitForFunction(() => document.activeElement?.getAttribute("role") === "menuitem",
    null, { timeout: 2000 }).catch(() => {});
  assert.equal(await focusedRole(), "menuitem", "the first item takes focus");
  assert.equal(await page.evaluate(() => document.activeElement?.textContent), "Open");
  for (let i = 0; i < 7; i += 1) await page.keyboard.press("Tab");
  assert.equal(await focusedRole(), "menuitem", "Tab stays inside the open menu");
  await page.keyboard.press("Escape");
  await menu.waitFor({ state: "detached" });

  // Copy IP copies the address part of the adb key, not the port.
  await tv.locator(".device-name").click({ button: "right" });
  await menu.getByRole("menuitem", { name: "Copy IP" }).click();
  await menu.waitFor({ state: "detached" });
  assert.equal(await lastCopied(page), "192.168.42.196");
  await page.getByRole("status").filter({ hasText: "Copied IP address" }).waitFor();

  // Copy serial is the hardware serial, never the address.
  await tv.locator(".device-name").click({ button: "right" });
  await menu.getByRole("menuitem", { name: "Copy serial" }).click();
  assert.equal(await lastCopied(page), "1324619053514");

  // Escape closes it, with no item run.
  const copies = await page.evaluate(() => window.__COPIED__.length);
  await tv.locator(".device-name").click({ button: "right" });
  await menu.waitFor();
  await page.keyboard.press("Escape");
  await menu.waitFor({ state: "detached" });
  assert.equal(await page.evaluate(() => window.__COPIED__.length), copies, "Escape runs nothing");

  // A USB row has no IP and nothing to Forget.
  await rowFor(page, "Workshop Shield").locator(".device-name").click({ button: "right" });
  await menu.waitFor();
  assert.equal(await menu.getByRole("menuitem", { name: "Copy IP" }).getAttribute("aria-disabled"), "true");
  assert.equal(await menu.getByRole("menuitem", { name: "Forget" }).count(), 0);
  await page.keyboard.press("Escape");

  // An mDNS transport is on the network but has no address to offer, and an
  // unreadable one has no hardware serial either. Unknown claims nothing.
  const mdns = rowFor(page, "adb-9XK2-abc._adb-tls-connect._tcp");
  await mdns.locator(".device-name").click({ button: "right" });
  await menu.waitFor();
  assert.equal(await menu.getByRole("menuitem", { name: "Copy IP" }).getAttribute("aria-disabled"), "true");
  assert.equal(await menu.getByRole("menuitem", { name: "Copy serial" }).getAttribute("aria-disabled"), "true");
  assert.equal(await menu.getByRole("menuitem", { name: "Open" }).getAttribute("aria-disabled"), "true");
  await page.keyboard.press("Escape");

  // Forget from the menu is the same backend call as the button, keyed on the
  // row's adb serial (the backend resolves the hardware id from it), and it
  // does not open the device on the way.
  const before = page.url();
  await tv.locator(".device-name").click({ button: "right" });
  await menu.getByRole("menuitem", { name: "Forget" }).click();
  await page.waitForTimeout(300);
  assert.deepEqual(await page.evaluate(() => window.__FORGET__), ["192.168.42.196:5555"]);
  assert.equal(page.url(), before, "Forget must not navigate into the device");

  // A text field keeps the native menu: no preventDefault, no app menu.
  const input = page.getByPlaceholder("IP[:port] — e.g. 192.168.42.71");
  await input.click({ button: "right" });
  const onInput = await lastContextMenu(page);
  assert.equal(onInput.tag, "INPUT");
  assert.equal(onInput.prevented, false, "an input must still get the webview's own menu");
  assert.equal(await menu.count(), 0, "and no app menu on top of it");

  // Plain chrome with nothing selected: the release build swallows the menu.
  await page.locator("h1, h2").first().click({ button: "right" });
  assert.equal((await lastContextMenu(page)).prevented, true,
    "release builds suppress the default menu outside text fields");
  assert.equal(await menu.count(), 0);

  // Selected text offers Copy rather than the webview's link-and-share menu.
  await page.evaluate(() => {
    const el = document.querySelector(".device-list .device-meta");
    const range = document.createRange();
    range.selectNodeContents(el);
    const sel = window.getSelection();
    sel.removeAllRanges();
    sel.addRange(range);
  });
  const selected = await page.evaluate(() => window.getSelection().toString());
  await page.locator(".device-list .device-meta").first().click({ button: "right" });
  await menu.waitFor();
  assert.equal(await menu.getByRole("menuitem").first().innerText(), "Copy");
  await menu.getByRole("menuitem", { name: "Copy", exact: true }).click();
  assert.equal(await lastCopied(page), selected);
  await page.close();

  await exercisePackageRow({ browser, base });

  console.log(
    "Context menu passed: a device row opens the app menu (Open, Copy IP, Copy serial, Copy diagnostics, Forget) with focus trapped and Escape closing it, Copy IP and Copy serial copy the right values, Forget keys on the adb serial, a package row offers Copy package name, inputs keep the native menu, and plain chrome suppresses it.",
  );
}

async function exercisePackageRow({ browser, base }) {
  const page = await browser.newPage({ viewport: { width: 1280, height: 1000 } });
  await page.addInitScript(INIT);
  await page.goto(`${base}/devices/${encodeURIComponent(DEMO_SERIAL)}`, { waitUntil: "networkidle" });
  await page.locator("#tab-apps").click();
  const pkg = page.locator("tr .pkg-id").first();
  await pkg.waitFor();
  const name = await pkg.innerText();
  await pkg.click({ button: "right" });
  const menu = page.getByRole("menu");
  await menu.getByRole("menuitem", { name: "Copy package name" }).click();
  assert.equal(await lastCopied(page), name);
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
