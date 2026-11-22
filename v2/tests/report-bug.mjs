// "Report a bug" is the only place the app tells on itself. Three things have
// to hold or it is worse than nothing:
//   - the bundle carries what the device actually reported, verbatim — the
//     `ro.build.characteristics` value is the whole argument in #120, and a
//     bundle that paraphrased it would settle nothing;
//   - the diagnostics for the device being looked at, not some other one;
//   - Debug logging really toggles, because a user asked to reproduce a bug
//     with it on needs it to have been on.
//
// Send to developer is the dialog's one upload, and the demo fixture layer
// records it instead of sending, which is why this can be driven entirely
// through that layer: the description is required, the result says which
// report it became, a refusal is shown, and a build without an endpoint
// offers GitHub only.
import { freePort } from "./lib/free-port.mjs";
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
  const page = await browser.newPage({ viewport: { width: 1100, height: 900 } });
  // The plugins have no Tauri host to talk to; serve replacements at the
  // module level, the same seam the update-notes test uses.
  await page.route(/plugin-updater/, (r) =>
    r.fulfill(stub(`export async function check() { return null; }`)),
  );
  await page.route(/plugin-opener/, (r) =>
    r.fulfill(stub(`export async function openUrl(url) { (window.__OPENED__ ??= []).push(url); }`)),
  );
  await page.route(/plugin-process/, (r) =>
    r.fulfill(stub(`export async function relaunch() {}`)),
  );
  await page.addInitScript((flags) => {
    localStorage.clear();
    for (const flag of flags) localStorage.setItem(`shieldopt.demo.${flag}`, "1");
    window.__OPENED__ = [];
  }, flags);
  return page;
}

async function exercise({ browser, base }) {
  const page = await newPage(browser);
  await page.goto(base, { waitUntil: "networkidle" });

  await page.getByRole("button", { name: "Report a bug" }).click();
  const dialog = page.getByRole("dialog");
  await dialog.waitFor();

  const bundle = dialog.getByLabel("Diagnostics");
  await bundle.waitFor();

  // With no device selected, the bundle still carries the host facts and says
  // plainly that there is no device — it does not invent one.
  await page.waitForFunction(
    () => !document.querySelector("textarea.bug-bundle")?.value.includes("Collecting…"),
  );
  const hostOnly = await bundle.inputValue();
  assert.match(hostOnly, /ATV Optimizer diagnostics/, hostOnly);
  assert.match(hostOnly, /No device selected/, hostOnly);

  // Nothing leaves without a press, and the GitHub path says it posts nothing
  // until the user submits the form there.
  assert.match(await dialog.innerText(), /Nothing is posted\s+until you submit it there/);
  assert.match(await dialog.innerText(), /your description, a summary of this computer and its TVs, and\s+the app's recent log, all redacted/);

  // Debug logging is a real switch, not a label.
  const checkbox = dialog.getByRole("checkbox", { name: "Debug logging" });
  assert.equal(await checkbox.isChecked(), false, "debug logging starts off");
  await checkbox.check();
  await page.waitForFunction(() => document.querySelector("dialog, [role=dialog]")
    ?.querySelector("input[type=checkbox]")?.checked === true);
  assert.equal(await checkbox.isChecked(), true, "turning it on sticks");
  // And the backend is what it reflects: reopening reads the live value back.
  await dialog.getByRole("button", { name: "Close" }).click();
  await page.getByRole("button", { name: "Report a bug" }).click();
  await page.getByRole("dialog").waitFor();
  assert.equal(
    await page.getByRole("dialog").getByRole("checkbox", { name: "Debug logging" }).isChecked(),
    true,
    "the checkbox reports what the backend actually has on",
  );

  // The log path is shown, so someone can find the file without the app.
  assert.match(await page.getByRole("dialog").innerText(), /Logs: .*logs/);

  // The GitHub issue opens the bug form with the Diagnostics field (id
  // `diagnostics` in .github/ISSUE_TEMPLATE/bug_report.yml) prefilled with
  // exactly the bundle shown, since it fits in a URL.
  await page.waitForFunction(
    () => !document.querySelector("textarea.bug-bundle")?.value.includes("Collecting…"),
  );
  const shown = await page.getByRole("dialog").getByLabel("Diagnostics").inputValue();
  await page.getByRole("dialog").getByRole("button", { name: /Open GitHub issue/ }).click();
  const opened = await page.evaluate(() => window.__OPENED__);
  assert.equal(opened.length, 1, JSON.stringify(opened));
  const url = new URL(opened[0]);
  assert.equal(`${url.origin}${url.pathname}`, "https://github.com/bryanroscoe/shield_optimizer/issues/new");
  assert.equal(url.searchParams.get("template"), "bug_report.yml");
  assert.equal(url.searchParams.get("diagnostics"), shown, "the form carries the bundle verbatim");
  assert.ok(opened[0].length <= 6000, `prefilled URL stays under the limit (${opened[0].length})`);

  await page.close();
  console.log(
    "Report a bug passed (host): the bundle renders, says GitHub posts nothing until the form is submitted, shows the log folder, toggles debug logging through the backend, and opens the bug form with Diagnostics prefilled.",
  );
}

/// On a device route the bundle is about *that* device, and it carries the two
/// properties the TV verdict is made of.
async function exerciseOnDevice({ browser, base }) {
  const page = await newPage(browser);
  const serial = "192.168.1.77:5555"; // the demo's unconfirmed box
  await page.goto(`${base}/devices/${encodeURIComponent(serial)}`, { waitUntil: "networkidle" });

  await page.getByRole("button", { name: "Report a bug" }).click();
  const dialog = page.getByRole("dialog");
  await dialog.waitFor();
  await page.waitForFunction(
    () => !document.querySelector("textarea.bug-bundle")?.value.includes("Collecting…"),
  );

  const bundle = await dialog.getByLabel("Diagnostics").inputValue();
  assert.match(bundle, new RegExp(serial.replace(/[.:]/g, "\\$&")), bundle);
  // The value the device reported, verbatim. This is the fact #120 turns on.
  assert.match(bundle, /ro\.build\.characteristics: `nosdcard`/, bundle);
  assert.match(bundle, /android\.software\.leanback: `\(no answer\)`/, bundle);
  assert.match(bundle, /TV evidence: unknown/, bundle);
  // And no inventory of what is installed on someone's TV.
  assert.doesNotMatch(bundle, /package:/, bundle);

  await page.close();
  console.log(
    "Report a bug passed (device): the bundle is about the open device and quotes the characteristics and leanback answers it actually gave.",
  );
}

const sentReports = (page) =>
  page.evaluate(() => (window.__TELEMETRY__ ?? []).filter((c) => c.command === "send_bug_report"));

/// Send to developer: needs a description, says which report it became, and
/// keeps GitHub as the other way out.
async function exerciseSend({ browser, base }) {
  const page = await newPage(browser);
  await page.goto(base, { waitUntil: "networkidle" });
  await page.getByRole("button", { name: "Report a bug" }).click();
  const dialog = page.getByRole("dialog");
  await dialog.waitFor();

  const send = dialog.getByRole("button", { name: "Send to developer" });
  assert.equal(await send.isDisabled(), true, "nothing to send without a description");
  const note = dialog.getByLabel("What went wrong?");
  await note.fill("   ");
  assert.equal(await send.isDisabled(), true, "whitespace is not a description");
  assert.equal(await dialog.getByText(/characters$/).count(), 0, "no counter far from the limit");
  // The live region is there before the count is, so its first change is announced.
  const counter = dialog.locator('p.bug-count[aria-live="polite"]');
  assert.equal(await counter.count(), 1);
  assert.equal((await counter.innerText()).trim(), "");
  await note.fill("x".repeat(3700));
  await dialog.getByText("3,700 / 4,000 characters").waitFor();
  assert.equal(await note.getAttribute("maxlength"), "4000");

  await note.fill("The Apps tab spins forever on my Chromecast.");
  await send.click();
  const status = dialog.getByRole("status").filter({ hasText: "Sent as report #42" });
  await status.waitFor();
  assert.match(await status.innerText(), /Support ID is 7F3A-9C21/);
  const calls = await sentReports(page);
  assert.equal(calls.length, 1, JSON.stringify(calls));
  assert.equal(calls[0].args.note, "The Apps tab spins forever on my Chromecast.");
  assert.equal(await note.inputValue(), "", "a sent description is cleared");

  // The sent report's Copy ID has its own state: the other Copy ID is untouched.
  await page.evaluate(() => {
    Object.defineProperty(navigator, "clipboard", { value: { writeText: async () => {} }, configurable: true });
  });
  const sentCopy = status.getByRole("button", { name: "Copy ID" });
  await sentCopy.click();
  await status.getByRole("button", { name: "Copied" }).waitFor();
  const supportCopy = dialog.locator(".bug-support button.bug-inline").filter({ hasText: /Copy ID|Copied/ });
  if ((await supportCopy.count()) > 0) {
    assert.match(await supportCopy.innerText(), /Copy ID/, "the support ID button didn't flip");
  }

  // GitHub is still there, and carries what the user typed into the form.
  await note.fill("Second problem");
  await page.waitForFunction(() => !document.querySelector("textarea.bug-bundle")?.value.includes("Collecting…"));
  await dialog.getByRole("button", { name: /Open GitHub issue/ }).click();
  const opened = await page.evaluate(() => window.__OPENED__);
  assert.equal(new URL(opened.at(-1)).searchParams.get("what-happened"), "Second problem");
  await page.close();

  // The server's daily cap is shown in the dialog, not swallowed.
  const limited = await newPage(browser, ["bugReportLimited"]);
  await limited.goto(base, { waitUntil: "networkidle" });
  await limited.getByRole("button", { name: "Report a bug" }).click();
  const limitedDialog = limited.getByRole("dialog");
  await limitedDialog.getByLabel("What went wrong?").fill("It broke");
  await limitedDialog.getByRole("button", { name: "Send to developer" }).click();
  const alert = limitedDialog.getByRole("alert");
  await alert.waitFor();
  assert.match(await alert.innerText(), /limit reached.*report it on GitHub instead/);
  await limited.close();

  // A build with no endpoint has nowhere to send: GitHub only, and it says so.
  const offline = await newPage(browser, ["telemetryUnavailable"]);
  await offline.goto(base, { waitUntil: "networkidle" });
  await offline.getByRole("button", { name: "Report a bug" }).click();
  const offlineDialog = offline.getByRole("dialog");
  await offlineDialog.getByText("Sending to the developer isn't available in this build.", { exact: false }).waitFor();
  assert.equal(await offlineDialog.getByRole("button", { name: "Send to developer" }).count(), 0);
  assert.equal(await offlineDialog.getByRole("button", { name: /Open GitHub issue/ }).count(), 1);
  await offline.close();

  console.log(
    "Report a bug passed (send): a description is required, the counter shows near the limit, the result names the report and support ID, the daily cap is shown, and a build without an endpoint offers GitHub only.",
  );
}

async function main() {
  const restore = setHarnessEnvironment();
  let server, browser;
  try {
    const { createServer } = await import("vite");
    const { chromium } = await import("playwright");
    server = await createServer({ root: V2, server: { host: "127.0.0.1", port: await freePort(), strictPort: true, hmr: false } });
    await server.listen();
    browser = await chromium.launch();
    await exercise({ browser, base: serverURL(server) });
    await exerciseOnDevice({ browser, base: serverURL(server) });
    await exerciseSend({ browser, base: serverURL(server) });
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
