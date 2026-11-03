// The Settings screen, driven against the demo fixture layer. What has to hold:
//   - it is reachable from the header, and the first-run notice lands on it;
//   - every switch on it is the same setting as its other copy (header theme,
//     the Report a bug dialog, Devices' Stop ADB when quitting), not a second
//     one that can disagree;
//   - the support ID is shown only when one exists, and says why when not;
//   - Send logs uploads nothing until the person confirms, then shows the ID
//     to quote, and a failure is shown through the shared error notice.
// Nothing here can reach a network: the demo backend records the upload.
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

async function newPage(browser, flags = [], viewport = { width: 1200, height: 900 }) {
  const page = await browser.newPage({ viewport });
  await page.route(/plugin-updater/, (r) => r.fulfill(stub(`export async function check() { return null; }`)));
  await page.route(/plugin-opener/, (r) =>
    r.fulfill(
      stub(
        `export async function openUrl(url) { (window.__OPENED__ ??= []).push(url); } export async function revealItemInDir() {}`,
      ),
    ),
  );
  await page.route(/plugin-process/, (r) => r.fulfill(stub(`export async function relaunch() {}`)));
  await page.addInitScript((flags) => {
    localStorage.clear();
    for (const f of flags) localStorage.setItem(`shieldopt.demo.${f}`, "1");
    window.__COPIED__ = [];
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText: async (text) => void window.__COPIED__.push(text) },
    });
  }, flags);
  return page;
}

const uploads = (page) =>
  page.evaluate(() => (window.__TELEMETRY__ ?? []).filter((c) => c.command === "upload_diagnostics").length);

async function reachAndReports({ browser, base }) {
  const page = await newPage(browser);
  await page.goto(base, { waitUntil: "networkidle" });

  // The gear in the header is how Settings is reached.
  await page.getByRole("link", { name: "Settings" }).click();
  await page.waitForURL("**/settings");
  await page.getByRole("heading", { name: "Settings", level: 1 }).waitFor();
  assert.match(await page.getByRole("link", { name: "Settings" }).getAttribute("class"), /active/);

  const privacy = page.locator("#privacy");
  const toggle = privacy.getByRole("checkbox", { name: "Share anonymous device and error reports" });
  assert.equal(await toggle.isChecked(), true, "reports are on by default");
  const id = privacy.locator("[data-support-id]");
  await id.waitFor();
  assert.equal(await id.innerText(), "7F3A-9C21", "the support ID is the short form of the install id");
  assert.doesNotMatch(await privacy.innerText(), /Quote this ID/);
  await privacy.getByRole("button", { name: "About Support ID", exact: true }).click();
  await privacy.getByText("Quote this ID when you report a problem so the developer can find your reports.").waitFor();
  await privacy.getByRole("button", { name: "About Support ID", exact: true }).press("Escape");
  await privacy.getByRole("button", { name: "Copy", exact: true }).click();
  assert.deepEqual(await page.evaluate(() => window.__COPIED__), ["7F3A-9C21"]);

  // Off: no ID, and it says why. The dialog's copy of the switch agrees.
  await toggle.uncheck();
  await privacy.getByText("None while reports are off").waitFor();
  assert.equal(await id.count(), 0);
  await page.getByRole("button", { name: "Report a bug" }).click();
  const dialog = page.getByRole("dialog");
  const dialogToggle = dialog.getByRole("checkbox", { name: "Share anonymous device and error reports" });
  assert.equal(await dialogToggle.isChecked(), false, "one setting, shown in two places");
  assert.match(await dialog.innerText(), /No support ID while reports are off\./);
  await dialogToggle.check();
  await dialog.getByText("7F3A-9C21").waitFor();
  assert.match(await dialog.innerText(), /Quote it in the issue so the developer can find your reports\./);
  await dialog.getByRole("button", { name: /Copy ID/ }).click();
  assert.equal((await page.evaluate(() => window.__COPIED__)).at(-1), "7F3A-9C21");
  await dialog.getByRole("button", { name: "Close" }).click();
  assert.equal(await toggle.isChecked(), true, "turned back on from the dialog, Settings shows it on");
  await id.waitFor();

  // The dialog points at Settings.
  await page.getByRole("link", { name: "Devices" }).click();
  await page.waitForURL((u) => u.pathname === "/");
  await page.getByRole("button", { name: "Report a bug" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Send logs and more in Settings" }).click();
  await page.waitForURL("**/settings");
  assert.equal(await page.getByRole("dialog").count(), 0, "the dialog closes on the way");
  await page.close();
}

async function sendLogs({ browser, base }) {
  const page = await newPage(browser);
  await page.goto(`${base}/settings`, { waitUntil: "networkidle" });
  const privacy = page.locator("#privacy");

  // Asking is not sending.
  await privacy.getByRole("button", { name: "Send logs…" }).click();
  const confirm = privacy.getByRole("group", { name: "Send logs to the developer?" });
  await confirm.waitFor();
  const said = await confirm.innerText();
  assert.match(said, /diagnostics bundle and this app's recent logs/);
  assert.match(said, /addresses, serial numbers, names and file paths removed/);
  assert.match(said, /labeled with your support ID/);
  assert.equal(await uploads(page), 0, "nothing is uploaded before the person confirms");
  await confirm.getByRole("button", { name: "Cancel" }).click();
  await confirm.waitFor({ state: "detached" });
  assert.equal(await uploads(page), 0, "Cancel sends nothing");

  await privacy.getByRole("button", { name: "Send logs…" }).click();
  await confirm.getByRole("button", { name: "Send logs", exact: true }).click();
  const sent = privacy.locator(".sent");
  await sent.waitFor();
  assert.match(await sent.innerText(), /Sent 47\.1 KB\. Quote support ID 7F3A-9C21 when you report the problem\./);
  assert.equal(await uploads(page), 1, "one confirmation, one upload");
  await sent.getByRole("button", { name: "Copy ID" }).click();
  assert.deepEqual(await page.evaluate(() => window.__COPIED__), ["7F3A-9C21"]);
  await page.close();

  // Allowed with reports off, and the result still carries an ID to quote.
  const off = await newPage(browser);
  await off.goto(`${base}/settings`, { waitUntil: "networkidle" });
  await off.locator("#privacy").getByRole("checkbox", { name: "Share anonymous device and error reports" }).uncheck();
  await off.getByText("None while reports are off").waitFor();
  await off.getByRole("button", { name: "Send logs…" }).click();
  await off.getByRole("button", { name: "Send logs", exact: true }).click();
  await off.locator(".sent").getByText("7F3A-9C21").waitFor();
  await off.close();

  // A failed upload goes through the shared error notice.
  const failing = await newPage(browser, ["uploadFails"]);
  await failing.goto(`${base}/settings`, { waitUntil: "networkidle" });
  await failing.getByRole("button", { name: "Send logs…" }).click();
  await failing.getByRole("button", { name: "Send logs", exact: true }).click();
  const notice = failing.locator("#privacy .error-notice");
  await notice.waitFor();
  assert.match(await notice.innerText(), /Send logs failed\./);
  assert.match(await notice.innerText(), /upload_failed/);
  assert.doesNotMatch(await notice.innerText(), /internet connection/);
  assert.equal(await failing.locator(".sent").count(), 0, "no success shown for a failed upload");
  await failing.close();
}

async function unavailableBuild({ browser, base }) {
  const page = await newPage(browser, ["telemetryUnavailable"]);
  await page.goto(`${base}/settings`, { waitUntil: "networkidle" });
  const privacy = page.locator("#privacy");
  const toggle = privacy.getByRole("checkbox", { name: "Share anonymous device and error reports" });
  await toggle.waitFor();
  assert.equal(await toggle.isDisabled(), true);
  await privacy.getByText("This build doesn't send reports.").waitFor();
  await privacy.getByText("None in this build").waitFor();
  await page.close();
}

async function noticeLandsOnSettings({ browser, base }) {
  const page = await newPage(browser, ["telemetryNotice"]);
  await page.goto(base, { waitUntil: "networkidle" });
  const notice = page.locator(".telemetry-notice");
  await notice.getByRole("button", { name: "Change setting" }).click();
  await page.waitForURL("**/settings");
  await notice.waitFor({ state: "detached" });
  await page
    .locator("#privacy")
    .getByRole("checkbox", { name: "Share anonymous device and error reports" })
    .waitFor();
  await page.close();
}

async function sharedPreferences({ browser, base }) {
  const page = await newPage(browser);
  await page.goto(`${base}/settings`, { waitUntil: "networkidle" });

  // Theme: Settings and the header switch are one choice.
  await page.locator("#appearance").getByRole("button", { name: "Light" }).click();
  assert.equal(await page.evaluate(() => document.documentElement.dataset.theme), "light");
  assert.equal(await page.locator("header").getByRole("button", { name: "Light" }).getAttribute("aria-pressed"), "true");
  await page.locator("header").getByRole("button", { name: "Dark" }).click();
  assert.equal(await page.locator("#appearance").getByRole("button", { name: "Dark" }).getAttribute("aria-pressed"), "true");
  assert.equal(await page.evaluate(() => localStorage.getItem("shieldopt.theme")), "dark");

  // Auto-update keeps its stored key, read at launch as before.
  const auto = page.getByRole("checkbox", { name: "Auto-update" });
  assert.equal(await auto.isChecked(), false);
  await auto.check();
  assert.equal(await page.evaluate(() => localStorage.getItem("shieldopt.autoUpdate")), "true");
  await page.getByRole("link", { name: "Snapshots" }).click();
  await page.getByRole("link", { name: "Settings" }).click();
  assert.equal(await page.getByRole("checkbox", { name: "Auto-update" }).isChecked(), true);

  // A manual check reports what it found and installs nothing.
  await page.getByRole("button", { name: "Check for updates" }).click();
  await page.locator("#updates").getByText(/No update found\./).waitFor();

  // Stop ADB when quitting: the same setting as on Devices.
  const stop = page.locator("#adb").getByRole("checkbox", { name: /Stop ADB when quitting/ });
  await page.waitForFunction((el) => el && !el.disabled, await stop.elementHandle());
  assert.equal(await stop.isChecked(), false);
  page.once("dialog", (dialog) => dialog.accept());
  await stop.click();
  await page.waitForFunction(() => localStorage.getItem("shieldopt.stopAdbOnQuit") === "true");
  await page.waitForFunction((el) => el && !el.disabled && el.checked, await stop.elementHandle());
  await page.getByRole("button", { name: "About Stop ADB when quitting", exact: true }).click();
  assert.match(await page.locator("#adb").innerText(), /Android Studio, scrcpy/);
  await page.getByRole("link", { name: "Devices" }).click();
  const devicesBox = page.getByRole("checkbox", { name: "Stop ADB when quitting" });
  await page.waitForFunction((el) => el && !el.disabled, await devicesBox.elementHandle());
  assert.equal(await devicesBox.isChecked(), true, "Devices shows what Settings set");

  // ADB location and the logs folder are shown, debug logging is a real switch.
  await page.getByRole("link", { name: "Settings" }).click();
  await page.locator("#adb").getByText("/opt/homebrew/bin/adb").waitFor();
  await page.locator("#logs").getByText(/logs$/).first().waitFor();
  const debug = page.getByRole("checkbox", { name: "Debug logging" });
  assert.equal(await debug.isChecked(), false);
  await debug.check();
  assert.equal(await page.evaluate(() => window.__TAURI_INTERNALS__.invoke("get_debug_logging", {})), true);

  // About opens the bug dialog the header does.
  await page.locator("#about").getByRole("button", { name: "Open bug report…" }).click();
  await page.getByRole("dialog").getByLabel("Diagnostics").waitFor();
  await page.close();
}

async function narrow({ browser, base }) {
  // Rows wrap rather than push controls off-screen.
  const page = await newPage(browser, [], { width: 560, height: 900 });
  await page.goto(`${base}/settings`, { waitUntil: "networkidle" });
  await page.locator("[data-support-id]").waitFor();
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
  assert.ok(overflow <= 0, `no horizontal overflow at 560px (${overflow}px)`);
  await page.close();
}


async function compactHelp({ browser, base }) {
  for (const width of [1200, 560, 320]) {
    const page = await newPage(browser, [], { width, height: 900 });
    await page.goto(`${base}/settings`, { waitUntil: "networkidle" });
    const privacy = page.locator("#privacy");
    await privacy.locator("[data-support-id]").waitFor();
    const notice = privacy.getByText(/ATV Optimizer sends anonymous reports:/);
    assert.equal(await notice.isVisible(), false, "privacy explanation is collapsed by default");
    const help = privacy.getByRole("button", { name: "About Share anonymous device and error reports", exact: true });
    await help.focus();
    await help.press("Enter");
    assert.equal(await help.getAttribute("aria-expanded"), "true");
    assert.equal(await notice.isVisible(), true);
    assert.match(await privacy.innerText(), /No serial numbers, IP addresses/);
    assert.match(await privacy.innerText(), /Turning this off stops sending at once/);
    await help.press("Escape");
    assert.equal(await help.getAttribute("aria-expanded"), "false");
    assert.equal(await help.evaluate((el) => el === document.activeElement), true);
    const checkbox = privacy.getByRole("checkbox", { name: "Share anonymous device and error reports", exact: true });
    await privacy.locator('label[for="reports-toggle"]').click();
    assert.equal(await checkbox.isChecked(), false, "the label still controls its checkbox");
    assert.equal(await help.getAttribute("aria-expanded"), "false", "toggling does not open help");
    await help.click();
    assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), `help fits ${width}px`);
    await page.close();
  }
}

async function main() {
  const restore = setHarnessEnvironment();
  let server, browser;
  try {
    const { createServer } = await import("vite");
    const { chromium, webkit } = await import("playwright");
    server = await createServer({ root: V2, server: { host: "127.0.0.1", port: 0, strictPort: false, hmr: false } });
    await server.listen();
    browser = await (process.argv.includes("--webkit") ? webkit : chromium).launch();
    const base = serverURL(server);
    await reachAndReports({ browser, base });
    await sendLogs({ browser, base });
    await unavailableBuild({ browser, base });
    await noticeLandsOnSettings({ browser, base });
    await sharedPreferences({ browser, base });
    await narrow({ browser, base });
    await compactHelp({ browser, base });
    console.log(
      "Settings screen passed: reached from the header and the first-run notice, the report switch and support ID agree with the Report a bug dialog, Send logs uploads only after confirmation and shows the ID to quote (or the shared error), theme / auto-update / Stop ADB when quitting / debug logging are the same settings as elsewhere, and the screen fits 560px.",
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
