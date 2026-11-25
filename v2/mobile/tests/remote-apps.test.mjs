// The Remote's Apps key on the phone: a sheet of the TV's launchable apps by
// name, Open through the Remote's queue, and up to three pinned shortcuts on
// the remote, filed under the TV's hardware id (never its address).
// SIMULATED: demo fixtures, no phone or TV. The phone app has one (dark) theme.
import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
import { after, before, test } from "node:test";
import { chromium } from "playwright";
import { startViteServer } from "./helpers/vite-harness.mjs";

process.env.VITE_DEMO = "1";
const evidence = process.env.MOBILE_REMOTE_APPS_EVIDENCE;
const KEY = "atvopt.remotePins";
let server, browser, origin;
before(async () => {
  ({ server, origin } = await startViteServer());
  browser = await chromium.launch();
  if (evidence) await mkdir(evidence, { recursive: true });
});
after(async () => { await browser?.close(); await server?.close(); });

async function open(t, { width = 412, scale = 1, pins = null, hardwareId = "default", launchFails = false, launchRejects = false, video = false } = {}) {
  const context = await browser.newContext({ viewport: { width, height: 860 }, isMobile: true, hasTouch: true,
    ...(video && evidence ? { recordVideo: { dir: `${evidence}/raw`, size: { width, height: 860 } } } : {}) });
  const page = await context.newPage();
  page.setDefaultTimeout(10000);
  t.after(() => context.close());
  await page.addInitScript(({ pins, KEY }) => {
    localStorage.clear();
    localStorage.setItem("atv.autoConnect.v1", "0");
    if (pins) localStorage.setItem(KEY, JSON.stringify(pins));
  }, { pins, KEY });
  await page.goto(origin, { waitUntil: "networkidle" });
  const id = await page.evaluate(async ({ scale, hardwareId, launchFails, launchRejects }) => {
    const { session } = await import("/src/lib/session.svelte.ts");
    const { router } = await import("/src/lib/router.svelte.ts");
    window.session = session;
    const invoke = window.__TAURI_INTERNALS__.invoke;
    window.appCalls = [];
    window.__TAURI_INTERNALS__.invoke = async (command, args) => {
      window.appCalls.push({ command, args });
      if (command === "launch_app" && launchRejects) throw new Error("SIMULATED transport dropped");
      if (command === "launch_app" && launchFails) {
        return { ok: false, transport: "shell", message: "Starting: Intent { … }\nError: Activity not started, unable to resolve Intent { … }" };
      }
      const result = await invoke(command, args);
      if (command === "list_devices" && hardwareId !== "default") {
        return result.map((d) => ({ ...d, properties: { ...(d.properties ?? {}), serial_number: hardwareId ?? undefined } }));
      }
      return result;
    };
    await session.connect("192.168.1.42", 5555);
    router.reset("remote");
    if (scale > 1) document.documentElement.style.fontSize = `${16 * scale}px`;
    const label = document.createElement("div");
    label.textContent = "SIMULATION · demo fixtures · no phone or TV";
    label.style.cssText = "position:fixed;inset:0 0 auto;z-index:99999;background:#182238;color:white;padding:4px;font:10px sans-serif;text-align:center";
    document.body.append(label);
    return session.connectedDevice?.properties?.serial_number ?? null;
  }, { scale, hardwareId, launchFails, launchRejects });
  await page.getByRole("heading", { name: "Remote", exact: true }).waitFor();
  return { page, id };
}

const calls = (page, command) => page.evaluate((c) => window.appCalls.filter((x) => x.command === c), command);
const labels = async (locator) => (await locator.allInnerTexts()).map((t) => t.split("\n").pop().trim());

for (const [width, scale] of [[412, 1], [320, 1], [320, 2]]) {
  test(`Apps sheet opens, launches and pins at ${width}px / ${scale * 100}% text`, async (t) => {
    const { page, id } = await open(t, { width, scale, video: width === 412 && scale === 1 });
    assert.ok(id, "the demo TV reports a hardware id");
    const appsKey = page.getByRole("button", { name: "Open app", exact: true });
    assert.ok((await appsKey.boundingBox()).height >= 44);
    assert.equal((await calls(page, "list_launchable_apps")).length, 0, "nothing is read until Open app is used");
    await appsKey.tap();
    const sheet = page.getByRole("dialog", { name: "Apps on this TV" });
    await sheet.locator("li.app-item").first().waitFor();
    const names = await sheet.locator(".app-name").allInnerTexts();
    assert.deepEqual(names.slice(0, 3), ["Amazon Prime Video", "Crunchyroll", "Netflix"]);
    const unnamed = await sheet.locator('li[data-app="com.example.homecam"]').innerText();
    assert.equal(unnamed.split("com.example.homecam").length - 1, 1, "an unnamed app shows its package once");
    for (const b of await sheet.locator("button").all()) {
      if (await b.isVisible()) assert.ok((await b.boundingBox()).height >= 44, "touch targets are at least 44px");
    }
    const box = await sheet.boundingBox();
    assert.ok(box.x >= 0 && box.x + box.width <= width + 1, "the sheet fits the screen");
    const clipped = await sheet.evaluate((el) => [...el.querySelectorAll("button, .app-name, h3")]
      .filter((n) => n.scrollWidth > n.clientWidth + 1).map((n) => n.textContent.trim()));
    assert.deepEqual(clipped, [], "nothing is clipped");
    if (evidence) await page.screenshot({ path: `${evidence}/sheet-${width}-${scale * 100}.png` });

    await sheet.getByRole("searchbox", { name: "Search apps" }).fill("you");
    assert.deepEqual(await sheet.locator(".app-name").allInnerTexts(), ["YouTube"]);
    await sheet.getByRole("searchbox", { name: "Search apps" }).fill("");

    await sheet.getByRole("button", { name: "Open Netflix", exact: true }).tap();
    await sheet.getByText("Opened Netflix.", { exact: true }).waitFor();
    assert.equal((await calls(page, "launch_app"))[0].args.package, "com.netflix.ninja");

    for (const name of ["Netflix", "YouTube", "Plex"]) await sheet.getByRole("button", { name: `Pin ${name}`, exact: true }).tap();
    assert.equal(await sheet.getByRole("button", { name: "Pin VLC", exact: true }).isDisabled(), true, "a fourth pin is refused");
    await sheet.getByText("3 of 3 pinned").waitFor();
    assert.deepEqual(await page.evaluate((k) => JSON.parse(localStorage.getItem(k)), KEY),
      { [id]: ["com.netflix.ninja", "com.google.android.youtube.tv", "com.plexapp.android"] }, "filed under the hardware id");
    await sheet.getByRole("button", { name: "Done", exact: true }).tap();
    await sheet.waitFor({ state: "detached" });
    assert.equal(await appsKey.evaluate((el) => el === document.activeElement), true, "focus returns to Open app");

    const shortcuts = page.locator("[data-shortcuts]");
    assert.deepEqual(await labels(shortcuts.getByRole("button")), ["Netflix", "YouTube", "Plex", "Open app"]);
    for (const b of await shortcuts.getByRole("button").all()) {
      const r = await b.boundingBox();
      assert.ok(r.x >= 0 && r.x + r.width <= width + 1 && r.height >= 44, "shortcuts fit and are tappable");
    }
    const split = await shortcuts.evaluate((row) => [...row.querySelectorAll("button")].filter((b) => {
      // A name is split mid-word when one word's text spans more than one line box.
      const range = document.createRange();
      return [...b.childNodes].filter((n) => n.nodeType === 3).some((n) => {
        let at = 0;
        for (const word of n.textContent.split(/(\s+)/)) {
          if (word.trim()) {
            range.setStart(n, at);
            range.setEnd(n, at + word.length);
            if (range.getClientRects().length > 1) return true;
          }
          at += word.length;
        }
        return false;
      });
    }).map((b) => b.textContent.trim()));
    assert.deepEqual(split, [], "no app name breaks mid-word");
    if (evidence) {
      await shortcuts.scrollIntoViewIfNeeded();
      await page.screenshot({ path: `${evidence}/remote-${width}-${scale * 100}.png` });
    }
    await shortcuts.getByRole("button", { name: "Open Plex", exact: true }).tap();
    await page.waitForFunction(() => window.appCalls.filter((c) => c.command === "launch_app").length === 2);
    assert.equal((await calls(page, "launch_app"))[1].args.package, "com.plexapp.android");
    if (evidence && width === 412 && scale === 1) {
      const video = page.video();
      await page.context().close();
      await video.saveAs(`${evidence}/remote-apps-mobile-simulated.webm`);
    }
  });
}

test("pins are per TV, survive a reload and are named from one read", async (t) => {
  const { page, id } = await open(t, { pins: null });
  const store = { [id]: ["com.plexapp.android", "com.example.homecam"], "OTHER-TV": ["com.netflix.ninja"] };
  const again = await open(t, { pins: store });
  const shortcuts = again.page.locator("[data-shortcuts]");
  await shortcuts.getByRole("button", { name: "Open Plex", exact: true }).waitFor();
  assert.deepEqual(await labels(shortcuts.getByRole("button")), ["Plex", "com.example.homecam", "Open app"]);
  assert.equal((await calls(again.page, "list_launchable_apps")).length, 1);
  void page;

  const other = await open(t, { pins: { [id]: ["com.plexapp.android"] }, hardwareId: "ANOTHER-TV" });
  await other.page.getByRole("button", { name: "Open app", exact: true }).waitFor();
  assert.deepEqual(await labels(other.page.locator("[data-shortcuts]").getByRole("button")), ["Open app"], "pins never follow another TV");
});

test("a TV with no hardware id keeps pins for the open screen only", async (t) => {
  const { page, id } = await open(t, { hardwareId: null });
  assert.equal(id, null);
  await page.getByRole("button", { name: "Open app", exact: true }).tap();
  const sheet = page.getByRole("dialog", { name: "Apps on this TV" });
  await sheet.getByRole("button", { name: "Pin Plex", exact: true }).tap();
  await sheet.getByRole("button", { name: "Done", exact: true }).tap();
  await page.getByRole("button", { name: "Open Plex", exact: true }).waitFor();
  assert.equal(await page.evaluate((k) => localStorage.getItem(k), KEY), null, "nothing stored without an id");
});

test("a launch the TV refuses says so in words", async (t) => {
  const { page } = await open(t, { launchFails: true });
  await page.getByRole("button", { name: "Open app", exact: true }).tap();
  const sheet = page.getByRole("dialog", { name: "Apps on this TV" });
  await sheet.getByRole("button", { name: "Open Plex", exact: true }).tap();
  await sheet.getByText("Couldn't open Plex. Error: Activity not started, unable to resolve Intent { … }").waitFor();
});

test("a launch that fails in transit says so in the sheet, and on the Remote from a shortcut", async (t) => {
  const { page, id } = await open(t, { launchRejects: true });
  await page.getByRole("button", { name: "Open app", exact: true }).tap();
  const sheet = page.getByRole("dialog", { name: "Apps on this TV" });
  await sheet.getByRole("button", { name: "Open Plex", exact: true }).tap();
  await sheet.getByText("Couldn't open Plex. Error: SIMULATED transport dropped").waitFor();
  await sheet.getByRole("button", { name: "Pin Plex", exact: true }).tap();
  await sheet.getByRole("button", { name: "Done", exact: true }).tap();
  await page.getByRole("button", { name: "Open Plex", exact: true }).tap();
  await page.locator(".remote-message").getByText("Couldn't open Plex. Error: SIMULATED transport dropped").waitFor();
  void id;
});
