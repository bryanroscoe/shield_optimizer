// "Update now" must show what the update actually changes before installing
// it. This app disables packages on a user's TV and can update itself
// unattended, so the notes are a consent surface, not decoration.
//
// The notes arrive as remote Markdown from the updater manifest, so the other
// half of this test is that they can never become markup.
import assert from "node:assert/strict";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

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
            // Reports half the download, then waits for the test to finish it,
            // so the progress the badge shows can be read mid-flight.
            downloadAndInstall: async (onEvent) => {
              onEvent({ event: "Started", data: { contentLength: 20 * 1024 * 1024 } });
              onEvent({ event: "Progress", data: { chunkLength: 10 * 1024 * 1024 } });
              await new Promise((resolve) => (window.__FINISH_INSTALL__ = resolve));
              onEvent({ event: "Finished" });
              window.__INSTALLED__ = true;
            },
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
    route.fulfill(
      stub(`
        export async function relaunch() {
          window.__RELAUNCHED__ = (window.__RELAUNCHED__ ?? 0) + 1;
          if (window.__RELAUNCH_FAILS__) throw new Error("no permission");
        }
      `),
    ),
  );

  await page.addInitScript(() => {
    window.__INSTALLED__ = false;
    window.__OPENED__ = [];
    window.__RELAUNCHED__ = 0;
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

  // There is exactly one clickable update badge, and it is the updater's.
  // The GitHub-API badge used to sit beside it offering a release page the
  // updater could not install from — two badges, one a dead end (#119).
  assert.equal(await page.getByRole("button", { name: /Update available/ }).count(), 0,
    "the release-page badge is gone; only the updater offers an update");

  // The version shown must be the one whose notes are shown. These come from
  // two different reads (the updater manifest and a GitHub API call) and the
  // dialog must never pair one version's number with another's notes.
  await updateButton.click();
  // The section headings are also headings; the dialog's own label is #notes-title.
  const titled = await page.locator("#notes-title").innerText();
  assert.match(titled, /2\.2\.0/, `manifest version, not the API's: ${titled}`);
  await page.getByRole("dialog").getByRole("button", { name: "Not now" }).click();

  // Installing is a separate, explicit act, and it shows how far it has got.
  await updateButton.click();
  await page.getByRole("dialog").getByRole("button", { name: /^Install v/ }).click();
  const progress = page.locator(".update-badge.updating");
  await progress.waitFor();
  assert.match(await progress.innerText(), /Downloading 50% of 20 MB/);
  await page.evaluate(() => window.__FINISH_INSTALL__());
  await page.waitForFunction(() => window.__INSTALLED__ === true);

  // A finished install offers the restart rather than leaving the user to
  // work out that they need one.
  const prompt = page.getByRole("dialog", { name: "Update installed" });
  await prompt.waitFor();
  assert.match(await prompt.innerText(), /v2\.2\.0 is ready/);
  await prompt.getByRole("button", { name: "Later" }).click();
  assert.equal(await page.getByRole("dialog").count(), 0, "Later closes the prompt");
  assert.equal(await page.evaluate(() => window.__RELAUNCHED__), 0, "and does not restart");

  // The badge keeps the restart reachable after Later.
  const restartBadge = page.locator(".update-badge.installed");
  await restartBadge.click();
  assert.equal(await page.evaluate(() => window.__RELAUNCHED__), 1, "Restart now relaunches");

  // A relaunch that fails says what to do instead of failing silently.
  await page.evaluate(() => (window.__RELAUNCH_FAILS__ = true));
  await restartBadge.click();
  const failed = page.getByRole("dialog", { name: "Update installed" });
  await failed.waitFor();
  assert.match(await failed.innerText(), /Quit and reopen the app to finish/);
  assert.match(await restartBadge.innerText(), /Quit and reopen the app to finish/);
  await page.keyboard.press("Escape");
  assert.equal(await page.getByRole("dialog").count(), 0, "Escape closes the prompt");

  console.log(
    "Update notes dialog passed: notes shown before installing, boilerplate trimmed, remote markup stays text, only https links are links, dismissing installs nothing, progress shows, and a finished install offers Restart now / Later with a fallback when relaunch fails.",
  );
}

/// Someone with auto-update on never sees the pre-install notes: the update
/// downloads, installs and relaunches without them ever pressing anything. The
/// first launch on the new version is the only moment they can be told.
///
/// Each "launch" is a fresh page in one browser context, so localStorage
/// carries over exactly as it does between real launches. The demo layer's
/// `check_for_update` returns no notes, like the GitHub call that timed out on
/// the owner's first 2.3.0 launch — the pop-up must not depend on it.
async function launcher(browser, seed = {}) {
  const stub = (body) => ({ status: 200, contentType: "application/javascript", body });
  const context = await browser.newContext({ viewport: { width: 1100, height: 900 } });
  // No update pending: this is the path after one has already installed.
  await context.route(/plugin-updater/, (r) =>
    r.fulfill(stub(`export async function check() { return null; }`)),
  );
  await context.route(/plugin-opener/, (r) =>
    r.fulfill(stub(`export async function openUrl(url) { (window.__OPENED__ ??= []).push(url); }`)),
  );
  await context.route(/plugin-process/, (r) =>
    r.fulfill(stub(`export async function relaunch() {}`)),
  );
  let seeded = false;
  return async (base, version) => {
    const page = await context.newPage();
    await page.addInitScript(
      ({ version, seed, first }) => {
        if (first) for (const [k, v] of Object.entries(seed)) localStorage.setItem(k, v);
        localStorage.setItem("shieldopt.demo.version", version);
      },
      { version, seed, first: !seeded },
    );
    seeded = true;
    await page.goto(base, { waitUntil: "networkidle" });
    await page.locator("button.version").waitFor();
    await page.waitForTimeout(200);
    return page;
  };
}

async function expectArrival(page, version, mustContain) {
  const dialog = page.getByRole("dialog");
  await dialog.waitFor();
  assert.equal(await page.locator("#notes-title").innerText(), `Updated to v${version}`);
  const body = await dialog.innerText();
  assert.match(body, mustContain, body);
  // Nothing to install — this is a notification, not a prompt.
  assert.equal(await dialog.getByRole("button", { name: /^Install/ }).count(), 0);
  await dialog.getByRole("button", { name: "Got it" }).click();
  assert.equal(await page.getByRole("dialog").count(), 0);
  await page.close();
}

async function expectQuiet(page, why) {
  assert.equal(await page.getByRole("dialog").count(), 0, why);
  await page.close();
}

async function exerciseArrived({ browser, base }) {
  // Three upgrades in a row, from a fresh install.
  const launch = await launcher(browser);
  await expectQuiet(await launch(base, "2.1.0"), "a first run shows nothing");
  await expectQuiet(await launch(base, "2.1.0"), "a relaunch shows nothing");
  await expectArrival(await launch(base, "2.2.0"), "2.2.0", /Playback/i);
  await expectQuiet(await launch(base, "2.2.0"), "2.2.0's notes show once");
  await expectArrival(await launch(base, "2.3.0"), "2.3.0", /Renamed to ATV Optimizer/i);
  await expectQuiet(await launch(base, "2.3.0"), "2.3.0's notes show once");
  await expectQuiet(await launch(base, "2.3.0"), "and stay shown");

  // The owner's upgrade: 2.2.0 had left its per-launch key behind, and the
  // first 2.3.0 launch got no notes from GitHub.
  const owner = await launcher(browser, { "shieldopt.lastSeenVersion": "2.2.0" });
  await expectArrival(await owner(base, "2.3.0"), "2.3.0", /now called ATV Optimizer/);
  await expectQuiet(await owner(base, "2.3.0"), "once, not on the next launch");

  // Someone the bug already skipped: the old key says 2.3.0 but nothing was
  // ever shown. They see the 2.3.0 notes on their next launch, once.
  const skipped = await launcher(browser, { "shieldopt.lastSeenVersion": "2.3.0" });
  await expectArrival(await skipped(base, "2.3.0"), "2.3.0", /Renamed to ATV Optimizer/i);
  await expectQuiet(await skipped(base, "2.3.0"), "migrated users see it once");

  // Quitting with the pop-up still open leaves it owed.
  const quit = await launcher(browser, { "shieldopt.notesSeenVersion": "2.2.0" });
  const open = await quit(base, "2.3.0");
  await open.getByRole("dialog").waitFor();
  await open.close();
  await expectArrival(await quit(base, "2.3.0"), "2.3.0", /Renamed to ATV Optimizer/i);

  console.log(
    "Arrival notice passed: shown once per upgrade (2.1.0 → 2.2.0 → 2.3.0) from the bundled notes with GitHub returning none, never on a relaunch or first run, and once for installs the old per-launch key skipped.",
  );
}

/// The notes are always reachable: the version label opens the recent
/// release history at any time, not only right after an update.
async function exerciseHistory({ browser, base }) {
  const launch = await launcher(browser, { "shieldopt.notesSeenVersion": "2.3.0" });
  const page = await launch(base, "2.3.0");
  assert.equal(await page.getByRole("dialog").count(), 0);

  const version = page.getByRole("button", { name: /v2\.3\.0/ });
  assert.equal(await version.getAttribute("data-tip"), "What's new");
  // A real button: reachable and operable from the keyboard.
  await version.focus();
  await page.keyboard.press("Enter");
  const dialog = page.getByRole("dialog");
  await dialog.waitFor();
  assert.equal(await page.locator("#notes-title").innerText(), "What's new");

  const entries = dialog.locator("details.release");
  const versions = await entries.evaluateAll((els) => els.map((e) => e.dataset.version));
  assert.ok(versions.length >= 2 && versions.length <= 5, `a few recent releases: ${versions}`);
  assert.deepEqual(versions.slice(0, 2), ["2.3.0", "2.2.0"], "newest first");
  const open = await entries.evaluateAll((els) => els.map((e) => e.open));
  assert.equal(open[0], true, "the running version is expanded");
  assert.ok(open.slice(1).every((o) => o === false), "older releases are collapsed");
  assert.match(await entries.nth(0).locator("summary").innerText(), /2026-09-30/, "with its date");
  assert.match(await entries.nth(0).innerText(), /Renamed to ATV Optimizer/i);

  // Same renderer as the pre-install notes: headings and bullets alike.
  assert.ok((await entries.nth(0).locator("h3").count()) > 1);
  assert.ok((await entries.nth(0).locator(".notes-bullet").count()) > 5);

  // An older release opens on demand.
  await entries.nth(1).locator("summary").click();
  assert.equal(await entries.nth(1).evaluate((e) => e.open), true);
  assert.match(await entries.nth(1).innerText(), /Playback/i);

  await dialog.getByRole("button", { name: /See all releases on GitHub/ }).click();
  assert.deepEqual(await page.evaluate(() => window.__OPENED__), [
    "https://github.com/bryanroscoe/shield_optimizer/releases",
  ]);

  await page.keyboard.press("Escape");
  assert.equal(await page.getByRole("dialog").count(), 0, "Escape closes it");

  // Opening it by hand is not the arrival, and closing it records nothing new.
  await version.click();
  await dialog.waitFor();
  await dialog.getByRole("button", { name: "Close" }).click();
  await page.close();

  console.log(
    "Release history passed: the version button opens the last few releases newest first, the running one expanded and the rest collapsed, with a link to all releases, and Escape closes it.",
  );
}

/// Between a tag push and `latest.json` propagating, the GitHub API knows
/// about a version the in-app updater cannot install yet. The app used to
/// offer that as a clickable "Update available" badge whose only destination
/// was a release page — a dead end presented as an action (#119).
///
/// One source of truth now drives anything clickable: the updater. The API's
/// head start is reported, inertly, as news.
async function exerciseRollingOut({ browser, base }) {
  const stub = (body) => ({ status: 200, contentType: "application/javascript", body });
  const newPage = async (updaterBody) => {
    const page = await browser.newPage({ viewport: { width: 1100, height: 900 } });
    await page.route(/plugin-updater/, (r) => r.fulfill(stub(updaterBody)));
    await page.route(/plugin-opener/, (r) =>
      r.fulfill(stub(`export async function openUrl(url) { (window.__OPENED__ ??= []).push(url); }`)),
    );
    await page.route(/plugin-process/, (r) =>
      r.fulfill(stub(`export async function relaunch() {}`)),
    );
    await page.addInitScript(() => {
      localStorage.clear();
      // The demo layer's "API ahead of manifest" case.
      localStorage.setItem("shieldopt.demo.updateAhead", "1");
    });
    await page.goto(base, { waitUntil: "networkidle" });
    return page;
  };

  // The updater has nothing yet. The pill says so and does nothing.
  const waiting = await newPage(`export async function check() { return null; }`);
  const pill = waiting.getByText(/rolling out/);
  await pill.waitFor();
  assert.match(await pill.innerText(), /v2\.9\.9 rolling out/);
  assert.equal(await waiting.getByRole("button", { name: /rolling out/ }).count(), 0,
    "a version the updater cannot install must not be clickable");
  assert.equal(await waiting.getByRole("button", { name: /Update available/ }).count(), 0,
    "and must not be dressed up as one either");
  assert.equal(
    await pill.getAttribute("data-tip"),
    "The in-app updater will offer it within a few minutes",
  );
  await waiting.close();

  // The updater has caught up part-way: it offers 2.2.0 while the API has
  // already seen 2.9.9. One clickable badge, one inert pill, no confusion
  // about which version is being installed.
  const both = await newPage(`
    export async function check() {
      return { version: "2.2.0", body: "", downloadAndInstall: async () => {} };
    }
  `);
  await both.getByRole("button", { name: /Update now/ }).waitFor();
  assert.match(
    await both.getByRole("button", { name: /Update now/ }).innerText(),
    /v2\.2\.0/,
    "the clickable badge names the version the updater will actually install",
  );
  assert.match(await both.getByText(/rolling out/).innerText(), /v2\.9\.9/);
  await both.close();

  console.log(
    "Update badge passed: only the in-app updater offers a clickable update, and a version the API has seen first is reported as rolling out rather than linked to a dead end.",
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
    await exerciseHistory({ browser, base: serverURL(server) });
    await exerciseRollingOut({ browser, base: serverURL(server) });
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
