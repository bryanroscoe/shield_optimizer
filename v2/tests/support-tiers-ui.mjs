// Device support tiers, the anonymous-report setting and the shared error
// presentation, driven through the real desktop screens against the demo
// fixture layer. Nothing here can reach a network: the demo backend records
// what the UI hands to `telemetry_*` in `window.__TELEMETRY__`.
import assert from "node:assert/strict";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const V2 = join(HERE, "..");

function serverURL(server) {
  const address = server.httpServer?.address();
  if (!address || typeof address === "string") throw new Error("no address");
  const host = address.address.includes(":") ? `[${address.address}]` : address.address;
  return `http://${host}:${address.port}`;
}

function setHarnessEnvironment() {
  const keys = ["VITE_DEMO", "TAURI_DEV_HOST"];
  const previous = new Map(keys.map((k) => [k, { present: Object.hasOwn(process.env, k), value: process.env[k] }]));
  process.env.VITE_DEMO = "1";
  delete process.env.TAURI_DEV_HOST;
  return () => {
    for (const [k, prior] of previous) {
      if (prior.present) process.env[k] = prior.value;
      else delete process.env[k];
    }
  };
}

const stub = (body) => ({ status: 200, contentType: "application/javascript", body });

async function newPage(browser, flags = []) {
  const page = await browser.newPage({ viewport: { width: 1200, height: 900 } });
  await page.route(/plugin-updater/, (r) => r.fulfill(stub(`export async function check() { return null; }`)));
  await page.route(/plugin-opener/, (r) =>
    r.fulfill(stub(`export async function openUrl() {} export async function revealItemInDir() {}`)),
  );
  await page.route(/plugin-process/, (r) => r.fulfill(stub(`export async function relaunch() {}`)));
  await page.addInitScript((flags) => {
    localStorage.clear();
    for (const f of flags) localStorage.setItem(`shieldopt.demo.${f}`, "1");
  }, flags);
  return page;
}

const telemetryCalls = (page) => page.evaluate(() => window.__TELEMETRY__ ?? []);

async function tiersOnTheDeviceList({ browser, base }) {
  const page = await newPage(browser, ["fireTv"]);
  await page.goto(base, { waitUntil: "networkidle" });

  const shield = page.locator('[data-serial="192.168.1.42:5555"]');
  await shield.waitFor();
  assert.equal(await shield.locator(".support-badge").innerText(), "Fully supported");
  assert.match(await shield.innerText(), /NVIDIA SHIELD is the device this app is built and tested on\./);

  // No rule matched: it says so, and claims nothing either way.
  const box = page.locator('[data-serial="192.168.1.77:5555"]');
  assert.equal(await box.locator(".support-badge").innerText(), "Not verified");
  assert.match(await box.innerText(), /We haven't verified this TV model yet\./);

  // Unsupported: a prominent banner, and the row still opens.
  const fire = page.locator('[data-serial="192.168.1.91:5555"]');
  assert.equal(await fire.locator(".support-badge").innerText(), "Not supported");
  const banner = fire.locator(".support-banner");
  assert.equal(await banner.getAttribute("role"), "alert");
  assert.match(await banner.innerText(), /This TV isn't supported: Fire OS blocks the changes this app makes\./);
  assert.equal(await fire.evaluate((el) => el.tagName), "A", "an unsupported TV is not blocked from opening");

  // The device page header carries the same badge and banner.
  await page.goto(`${base}/devices/${encodeURIComponent("192.168.1.91:5555")}`, { waitUntil: "networkidle" });
  await page.locator(".device-header .support-banner").waitFor();
  assert.equal(await page.locator(".device-header .support-badge").first().innerText(), "Not supported");
  await page.close();
}

async function telemetrySetting({ browser, base }) {
  // First launch: a non-blocking notice, dismissed once.
  const page = await newPage(browser, ["telemetryNotice"]);
  await page.goto(base, { waitUntil: "networkidle" });
  const notice = page.locator(".telemetry-notice");
  await notice.waitFor();
  const text = await notice.innerText();
  assert.match(text, /anonymous reports/);
  assert.match(text, /No serial numbers, IP addresses, TV names or app lists\./);
  assert.match(text, /Report a bug/);
  assert.doesNotMatch(text, /in Settings/, "desktop points at where the switch really is");
  // Non-blocking: the device list is usable underneath it.
  await page.locator('[data-serial="192.168.1.42:5555"]').waitFor();
  await notice.getByRole("button", { name: "Got it" }).click();
  await notice.waitFor({ state: "detached" });

  // The switch lives in the Report a bug dialog, on by default.
  await page.getByRole("button", { name: "Report a bug" }).click();
  const toggle = page.getByRole("dialog").getByRole("checkbox", { name: "Share anonymous device and error reports" });
  assert.equal(await toggle.isChecked(), true, "on by default");
  await toggle.uncheck();
  await page.waitForFunction(async () => {
    const internals = window.__TAURI_INTERNALS__;
    const s = await internals.invoke("telemetry_settings", {});
    return s.enabled === false;
  });
  assert.equal(await toggle.isChecked(), false);
  await page.close();

  // A build without an endpoint says so instead of implying it reports, and
  // shows no first-launch notice.
  const offline = await newPage(browser, ["telemetryNotice", "telemetryUnavailable"]);
  await offline.goto(base, { waitUntil: "networkidle" });
  await offline.locator('[data-serial="192.168.1.42:5555"]').waitFor();
  assert.equal(await offline.locator(".telemetry-notice").count(), 0);
  await offline.getByRole("button", { name: "Report a bug" }).click();
  const off = offline.getByRole("dialog").getByRole("checkbox", { name: "Share anonymous device and error reports" });
  assert.equal(await off.isDisabled(), true);
  assert.match(await offline.getByRole("dialog").innerText(), /This build doesn't send reports\./);
  await offline.close();
}

async function errorsAndReports({ browser, base }) {
  const page = await newPage(browser);
  await page.goto(base, { waitUntil: "networkidle" });
  await page.locator('[data-serial="192.168.1.42:5555"]').waitFor();

  // A tracked action that succeeds is reported as such.
  await page.getByRole("button", { name: "Scan LAN" }).click();
  await page.waitForFunction(() => (window.__TELEMETRY__ ?? []).some((c) => c.args.action === "scan"));
  const scan = (await telemetryCalls(page)).find((c) => c.args.action === "scan");
  assert.deepEqual(scan.args, { action: "scan", serial: null, ok: true, errorCode: null, message: null });

  // A failed Add by IP: the explanation, the code, raw text behind Details,
  // Copy details — and a failure report with the same code.
  await page.evaluate(() => {
    const internals = window.__TAURI_INTERNALS__;
    const prev = internals.invoke;
    internals.invoke = async (command, args = {}) =>
      command === "connect_device"
        ? { ok: false, message: "failed to connect to '192.168.1.88:5555': Connection refused" }
        : prev(command, args);
  });
  await page.getByPlaceholder("IP[:port] — e.g. 192.168.42.71").fill("192.168.1.88");
  await page.locator(".connect-form").getByRole("button", { name: "Add by IP" }).click();
  const notice = page.locator(".error-notice");
  await notice.waitFor();
  assert.equal(await notice.getAttribute("data-error-code"), "connection_refused");
  assert.match(await notice.locator(".error-text").innerText(), /^Couldn't reach 192\.168\.1\.88\./);
  assert.equal(await notice.locator(".error-code").innerText(), "connection_refused");
  await notice.locator("summary").click();
  assert.match(await notice.locator("pre").innerText(), /Connection refused/);
  await notice.getByRole("button", { name: "Copy details" }).waitFor();
  await page.waitForFunction(() =>
    (window.__TELEMETRY__ ?? []).some((c) => c.args.action === "connect" && c.args.ok === false),
  );
  const connect = (await telemetryCalls(page)).find((c) => c.args.action === "connect");
  assert.equal(connect.args.errorCode, "connection_refused");
  assert.equal(connect.command, "telemetry_action");
  await page.close();
}

async function main() {
  const restore = setHarnessEnvironment();
  let server, browser;
  try {
    const { createServer } = await import("vite");
    const { chromium } = await import("playwright");
    server = await createServer({ root: V2, server: { host: "127.0.0.1", port: 0, strictPort: false, hmr: false } });
    await server.listen();
    browser = await chromium.launch();
    const base = serverURL(server);
    await tiersOnTheDeviceList({ browser, base });
    await telemetrySetting({ browser, base });
    await errorsAndReports({ browser, base });
    console.log(
      "Support tiers UI passed: badges and one-line copy per tier, the not-supported banner on list and header (row still opens), the one-time report notice and the on-by-default switch (disabled when the build has no endpoint), and a failed connect shown with its code, Details and Copy details while the same code is reported.",
    );
  } finally {
    await browser?.close().catch((e) => console.error("browser cleanup failed", e));
    await server?.close().catch((e) => console.error("Vite cleanup failed", e));
    restore();
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
