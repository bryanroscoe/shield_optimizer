// "Report a bug" is the only place the app tells on itself. Three things have
// to hold or it is worse than nothing:
//   - the bundle carries what the device actually reported, verbatim — the
//     `ro.build.characteristics` value is the whole argument in #120, and a
//     bundle that paraphrased it would settle nothing;
//   - the diagnostics for the device being looked at, not some other one;
//   - Debug logging really toggles, because a user asked to reproduce a bug
//     with it on needs it to have been on.
//
// Nothing is uploaded anywhere, which is why this can be driven entirely
// through the demo fixture layer.
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

async function newPage(browser) {
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
  await page.addInitScript(() => {
    localStorage.clear();
    window.__OPENED__ = [];
  });
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
    () => !document.querySelector("textarea")?.value.includes("Collecting…"),
  );
  const hostOnly = await bundle.inputValue();
  assert.match(hostOnly, /ATV Optimizer diagnostics/, hostOnly);
  assert.match(hostOnly, /No device selected/, hostOnly);

  // The report leaves the machine only if the user opens the issue, and the
  // dialog says so.
  assert.match(await dialog.innerText(), /never sent on its own/);

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
    () => !document.querySelector("textarea")?.value.includes("Collecting…"),
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
    "Report a bug passed (host): the bundle renders, says the report is never sent on its own, shows the log folder, toggles debug logging through the backend, and opens the bug form with Diagnostics prefilled.",
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
    () => !document.querySelector("textarea")?.value.includes("Collecting…"),
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

async function main() {
  const restore = setHarnessEnvironment();
  let server, browser;
  try {
    const { createServer } = await import("vite");
    const { chromium } = await import("playwright");
    server = await createServer({ root: V2, server: { host: "127.0.0.1", port: 0, strictPort: false, hmr: false } });
    await server.listen();
    browser = await chromium.launch();
    await exercise({ browser, base: serverURL(server) });
    await exerciseOnDevice({ browser, base: serverURL(server) });
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
