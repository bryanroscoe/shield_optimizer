// "Update now" must show what the update actually changes before installing
// it. This app disables packages on a user's TV and can update itself
// unattended, so the notes are a consent surface, not decoration.
//
// The notes arrive as remote Markdown from the updater manifest, so the other
// half of this test is that they can never become markup.
import assert from "node:assert/strict";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { readFileSync } from "node:fs";
const CURRENT_VERSION = JSON.parse(
  readFileSync(new URL("../package.json", import.meta.url), "utf8"),
).version;


const HERE = dirname(fileURLToPath(import.meta.url));
const V2 = join(HERE, "..");

const NOTES = [
  "Safer defaults and honest pairing.",
  "",
  "### Safety",
  "",
  "- **Unknown, not Safe.** Uncatalogued apps are never pre-selected.",
  "- A failed inventory read no longer shows an app as Enabled.",
  "",
  "### Devices",
  "",
  "- Android 11+ devices are discoverable again (#88).",
  "- <img src=x onerror=alert(1)> should stay literal text.",
  "- [Release page](https://github.com/bryanroscoe/shield_optimizer/releases)",
  "- [Not a link](javascript:alert(1))",
  "",
  "---",
  "",
  "### First-run warnings",
  "",
  "- macOS Gatekeeper boilerplate that does not belong in a running app.",
].join("\n");

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

async function exercise({ browser, base }) {
  const page = await browser.newPage({ viewport: { width: 1100, height: 900 } });

  // Stand in for the Tauri plugins at the module level: patching after the app
  // has already imported them is too late, and a reload would undo it. Serving
  // replacement modules is the only seam that does not require a test-only
  // hook in production code.
  const stub = (body) => ({
    status: 200,
    contentType: "application/javascript",
    body,
  });

  await page.route(/plugin-updater/, (route) =>
    route.fulfill(
      stub(`
        export async function check() {
          return {
            version: "2.2.0",
            body: ${JSON.stringify(NOTES)},
            downloadAndInstall: async () => { window.__INSTALLED__ = true; },
          };
        }
      `),
    ),
  );
  await page.route(/plugin-opener/, (route) =>
    route.fulfill(
      stub(`
        export async function openUrl(url) { (window.__OPENED__ ??= []).push(url); }
      `),
    ),
  );
  await page.route(/plugin-process/, (route) =>
    route.fulfill(stub(`export async function relaunch() {}`)),
  );

  await page.addInitScript(() => {
    window.__INSTALLED__ = false;
    window.__OPENED__ = [];
  });

  await page.goto(base, { waitUntil: "networkidle" });

  const updateButton = page.getByRole("button", { name: /Update now/ });
  await updateButton.waitFor();

  // Nothing installs just because an update exists.
  assert.equal(await page.evaluate(() => window.__INSTALLED__), false);

  await updateButton.click();
  const dialog = page.getByRole("dialog");
  await dialog.waitFor();

  const body = await dialog.innerText();
  assert.match(body, /Unknown, not Safe/, body);
  assert.match(body, /Android 11\+ devices are discoverable again/, body);
  // innerText reflects the rendered case; headings are styled uppercase.
  assert.match(body, /Safety/i, "section headings render");

  // The workflow's install boilerplate is trimmed.
  assert.doesNotMatch(body, /First-run warnings/, body);
  assert.doesNotMatch(body, /Gatekeeper/, body);

  // Remote text never becomes markup.
  assert.equal(await dialog.locator("img").count(), 0, "an <img> in the notes must not render");
  assert.match(body, /<img src=x onerror=alert\(1\)>/, "it shows as literal text instead");

  // Only a vouched-for scheme becomes a link.
  assert.equal(await dialog.getByRole("button", { name: "Release page" }).count(), 1);
  assert.equal(await dialog.getByRole("button", { name: "Not a link" }).count(), 0);
  assert.match(body, /javascript:alert\(1\)/, "a rejected link is still shown, as text");

  // Reading the notes is not consenting to them.
  await dialog.getByRole("button", { name: "Not now" }).click();
  assert.equal(await page.getByRole("dialog").count(), 0);
  assert.equal(
    await page.evaluate(() => window.__INSTALLED__),
    false,
    "dismissing must not install",
  );

  // The version badge opens the release history.
  await page.locator("button.version").click();
  assert.deepEqual(await page.evaluate(() => window.__OPENED__), [
    "https://github.com/bryanroscoe/shield_optimizer/releases",
  ]);

  // The version shown must be the one whose notes are shown. These come from
  // two different reads (the updater manifest and a GitHub API call) and the
  // dialog must never pair one version's number with another's notes.
  await updateButton.click();
  // The section headings are also headings; the dialog's own label is #notes-title.
  const titled = await page.locator("#notes-title").innerText();
  assert.match(titled, /2\.2\.0/, `manifest version, not the API's: ${titled}`);
  await page.getByRole("dialog").getByRole("button", { name: "Not now" }).click();

  // Installing is a separate, explicit act.
  await updateButton.click();
  await page.getByRole("dialog").getByRole("button", { name: /^Install v/ }).click();
  await page.waitForFunction(() => window.__INSTALLED__ === true);

  console.log(
    "Update notes dialog passed: notes shown before installing, boilerplate trimmed, remote markup stays text, only https links are links, the version opens release history, and dismissing installs nothing.",
  );
}

/// Someone with auto-update on never sees the pre-install notes: the update
/// downloads, installs and relaunches without them ever pressing anything. The
/// first launch on the new version is the only moment they can be told.
///
/// Driven entirely through the remembered version, because that is the only
/// thing the feature actually keys off.
async function exerciseArrived({ browser, base }) {
  const stub = (body) => ({ status: 200, contentType: "application/javascript", body });
  const newPage = async (lastSeen) => {
    const page = await browser.newPage({ viewport: { width: 1100, height: 900 } });
    // No update pending: this is the path after one has already installed.
    await page.route(/plugin-updater/, (r) =>
      r.fulfill(stub(`export async function check() { return null; }`)),
    );
    await page.route(/plugin-opener/, (r) =>
      r.fulfill(stub(`export async function openUrl() {}`)),
    );
    await page.route(/plugin-process/, (r) =>
      r.fulfill(stub(`export async function relaunch() {}`)),
    );
    await page.addInitScript((seen) => {
      localStorage.clear();
      if (seen) localStorage.setItem("shieldopt.lastSeenVersion", seen);
    }, lastSeen);
    await page.goto(base, { waitUntil: "networkidle" });
    return page;
  };

  // Last launch was on an older version, so one landed in between.
  const updated = await newPage("2.0.0");
  const dialog = updated.getByRole("dialog");
  await dialog.waitFor();
  const body = await dialog.innerText();
  assert.match(body, /Updated to v/, body);
  assert.match(body, /Reliable switch away from the stock launcher/, body);
  // Nothing to install — this is a notification, not a prompt.
  assert.equal(await dialog.getByRole("button", { name: /^Install/ }).count(), 0);
  await dialog.getByRole("button", { name: "Got it" }).click();
  assert.equal(await updated.getByRole("dialog").count(), 0);
  await updated.close();

  // Same version as last launch: nothing happened, say nothing. Read the
  // version from package.json rather than naming one — this assertion is about
  // "last seen equals current", and hardcoding a number turns every release
  // into a test failure. It did: the v2-2.2.0 bump left this pinned at 2.1.0,
  // which is a *different* version, so the app correctly announced an update
  // and the test read that as a bug.
  const same = await newPage(CURRENT_VERSION);
  await same.waitForTimeout(300);
  assert.equal(
    await same.getByRole("dialog").count(),
    0,
    "no update landed, so there is nothing to announce",
  );
  await same.close();

  // A first run has nothing to compare against; greeting a new user with
  // "what's new" would be nonsense.
  const fresh = await newPage(null);
  await fresh.waitForTimeout(300);
  assert.equal(await fresh.getByRole("dialog").count(), 0, "a first run shows nothing");
  await fresh.close();

  console.log(
    "Arrival notice passed: shown when a new version has landed, and not on a first run or an unchanged one.",
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
    await exerciseArrived({ browser, base: serverURL(server) });
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
