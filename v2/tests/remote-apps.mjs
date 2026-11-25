// The Remote's Apps key: the TV's launchable apps by name, opened through the
// Remote's session order, and up to three pinned shortcuts filed under the
// TV's hardware id (never its address).
//
// SIMULATED: demo fixtures, no TV.
import { freePort } from "./lib/free-port.mjs";
import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";
import { chromium } from "playwright";

const V2 = join(dirname(fileURLToPath(import.meta.url)), "..");
const evidence = process.env.REMOTE_APPS_EVIDENCE;
process.env.VITE_DEMO = "1";
delete process.env.TAURI_DEV_HOST;
const KEY = "atvopt.remotePins";
const SHIELD_ID = "0323220012345";

function memoryStorage(fail = false) {
  const map = new Map();
  return {
    getItem: (k) => (map.has(k) ? map.get(k) : null),
    setItem: (k, v) => {
      if (fail) throw new Error("quota");
      map.set(k, String(v));
    },
    removeItem: (k) => map.delete(k),
  };
}

async function moduleChecks(server) {
  const m = await server.ssrLoadModule("/shared/remote-shortcuts.ts");
  assert.equal(m.appLabel({ package: "com.x.y", name: "  " }), "com.x.y", "a blank name falls back to the package");
  assert.equal(m.appLabel({ package: "com.x.y", name: "Plex" }), "Plex");
  const apps = [{ package: "com.plexapp.android", name: "Plex" }, { package: "com.example.cam", name: null }];
  assert.deepEqual(m.filterApps(apps, " PLEX ").map((a) => a.package), ["com.plexapp.android"]);
  assert.deepEqual(m.filterApps(apps, "example").map((a) => a.package), ["com.example.cam"], "matches the package id too");
  assert.equal(m.filterApps(apps, "").length, 2);

  const s = memoryStorage();
  let pins = [];
  for (const pkg of ["a.one", "b.two", "c.three"]) ({ pins } = m.togglePin(s, "TV-1", pins, pkg));
  assert.deepEqual(pins, ["a.one", "b.two", "c.three"]);
  const full = m.togglePin(s, "TV-1", pins, "d.four");
  assert.deepEqual(full.pins, pins, "a fourth pin is refused");
  ({ pins } = m.togglePin(s, "TV-1", pins, "b.two"));
  assert.deepEqual(m.pinnedApps(s, "TV-1"), ["a.one", "c.three"], "unpin is saved");
  assert.deepEqual(m.pinnedApps(s, " TV-1 "), ["a.one", "c.three"], "ids are trimmed");
  assert.deepEqual(m.pinnedApps(s, "TV-2"), [], "another TV has its own pins");
  for (const id of [null, undefined, "", "unknown"]) {
    const r = m.togglePin(s, id, [], "a.one");
    assert.deepEqual(r, { pins: ["a.one"], saved: false }, `no id (${JSON.stringify(id)}) is never saved`);
    assert.deepEqual(m.pinnedApps(s, id), []);
  }
  assert.deepEqual(m.togglePin(s, "TV-1", [], "a.one; reboot").pins, [], "only package ids can be pinned");
  s.setItem(KEY, JSON.stringify({ "TV-3": ["ok.app", "bad pkg", "ok.app", 7, "b.b", "c.c", "d.d"] }));
  assert.deepEqual(m.pinnedApps(s, "TV-3"), ["ok.app", "b.b", "c.c"], "stored pins are cleaned and capped");
  s.setItem(KEY, "{not json");
  assert.deepEqual(m.pinnedApps(s, "TV-1"), []);
  assert.equal(m.togglePin(memoryStorage(true), "TV-1", [], "a.one").saved, false, "a refused write reports false");
  console.log("Remote shortcuts module passed: labels, search, 3-pin cap, per-id storage, no-id and bad storage.");
}

async function openRemote(page, base, { pins = null, launchFails = false } = {}) {
  await page.addInitScript(({ pins, launchFails, KEY, SHIELD_ID }) => {
    if (pins) localStorage.setItem(KEY, JSON.stringify({ [SHIELD_ID]: pins }));
    window.appCalls = [];
    window.holdLaunch = false;
    window.swapId = false;
    let internals;
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true, get: () => internals,
      set(value) {
        const original = value.invoke.bind(value);
        value.invoke = async (command, args) => {
          window.appCalls.push({ command, args });
          if (command === "launch_app" && launchFails) {
            return { ok: false, transport: "shell", message: "Starting: Intent { … }\nError: Activity not started, unable to resolve Intent { … }" };
          }
          if (command === "launch_app" && window.holdLaunch) await new Promise((resolve) => (window.releaseLaunch = resolve));
          const result = await original(command, args);
          if (command === "device_profile" && window.swapId) {
            return { ...result, properties: { ...result.properties, serial_number: "SIMULATED-OTHER-TV" } };
          }
          return result;
        };
        internals = value;
      },
    });
  }, { pins, launchFails, KEY, SHIELD_ID });
  await page.goto(base, { waitUntil: "networkidle" });
  await page.getByText("NVIDIA SHIELD", { exact: false }).first().click();
  await page.getByRole("tab", { name: "Remote" }).click();
  return page.locator("#tabpanel-remote");
}

// Button labels without the icon ligature text in front.
const labels = async (locator) => (await locator.allInnerTexts()).map((t) => t.split("\n").pop().trim());
const calls = (page, command) => page.evaluate((c) => window.appCalls.filter((x) => x.command === c), command);

let server, browser;
try {
  server = await createServer({ root: V2, logLevel: "error", server: { host: "127.0.0.1", port: await freePort(), strictPort: true, hmr: false } });
  await server.listen();
  await moduleChecks(server);
  const base = `http://127.0.0.1:${server.httpServer.address().port}`;
  browser = await chromium.launch();
  if (evidence) await mkdir(evidence, { recursive: true });

  {
    const context = await browser.newContext({ viewport: { width: 1280, height: 900 },
      ...(evidence ? { recordVideo: { dir: join(evidence, "raw"), size: { width: 1280, height: 900 } } } : {}) });
    const page = await context.newPage();
    const panel = await openRemote(page, base);
    if (evidence) await page.evaluate(() => {
      const label = document.createElement("div");
      label.textContent = "SIMULATED · demo fixtures, no TV";
      label.style.cssText = "position:fixed;bottom:0;right:0;z-index:99999;padding:6px 10px;background:#1a2540;color:white;font:13px sans-serif";
      document.body.append(label);
    });
    const appsKey = panel.getByRole("button", { name: "Open app", exact: true });
    assert.equal(await appsKey.getAttribute("aria-expanded"), "false");
    assert.equal((await calls(page, "list_launchable_apps")).length, 0, "nothing is read until Open app is used");
    await appsKey.focus();
    await page.keyboard.press("Enter");
    const apps = panel.locator("#remote-apps-panel");
    await apps.locator("li.app-item").first().waitFor();
    assert.equal(await appsKey.getAttribute("aria-expanded"), "true");
    const names = await apps.locator(".app-name").allInnerTexts();
    assert.deepEqual(names.slice(0, 3), ["Amazon Prime Video", "Crunchyroll", "Netflix"], "named apps in the TV's order");
    const unnamed = apps.locator('li[data-app="com.example.homecam"]');
    assert.equal(await unnamed.innerText().then((t) => t.split("com.example.homecam").length - 1), 1, "an unnamed app shows its package once");
    if (evidence) await page.screenshot({ path: join(evidence, "apps-panel-1280-dark.png") });

    await apps.getByRole("searchbox", { name: "Search apps" }).fill("plex");
    assert.deepEqual(await apps.locator(".app-name").allInnerTexts(), ["Plex"]);
    await apps.getByRole("searchbox", { name: "Search apps" }).fill("zzz");
    await apps.getByText('No app matches "zzz".').waitFor();
    await apps.getByRole("searchbox", { name: "Search apps" }).fill("");

    await apps.getByRole("button", { name: "Open Netflix", exact: true }).click();
    await apps.getByText("Opened Netflix.", { exact: true }).waitFor();
    const launches = await calls(page, "launch_app");
    assert.equal(launches.length, 1);
    assert.equal(launches[0].args.package, "com.netflix.ninja");
    assert.match(String(launches[0].args.generation), /^[1-9]\d*$/, "launch runs in the Remote's session");

    for (const name of ["Netflix", "YouTube", "Plex"]) {
      await apps.getByRole("button", { name: `Pin ${name}`, exact: true }).click();
    }
    assert.equal(await apps.getByRole("button", { name: "Unpin Netflix", exact: true }).getAttribute("aria-pressed"), "true");
    assert.equal(await apps.getByRole("button", { name: "Pin VLC", exact: true }).isDisabled(), true, "a fourth pin is refused");
    await apps.getByText("3 of 3 pinned").waitFor();
    const shortcuts = panel.locator("[data-shortcuts]");
    assert.deepEqual(await labels(shortcuts.getByRole("button")), ["Netflix", "YouTube", "Plex", "Open app"]);
    assert.deepEqual(await page.evaluate((k) => JSON.parse(localStorage.getItem(k)), KEY),
      { [SHIELD_ID]: ["com.netflix.ninja", "com.google.android.youtube.tv", "com.plexapp.android"] },
      "pins are filed under the Shield's hardware id");

    for (const scheme of ["dark", "light"]) {
      await page.evaluate((s) => document.documentElement.setAttribute("data-theme", s), scheme);
      for (const [width, size] of [[1280, "100%"], [720, "100%"], [720, "150%"]]) {
        await page.setViewportSize({ width, height: 900 });
        await page.evaluate((s) => (document.documentElement.style.fontSize = s), size);
        for (const box of [await shortcuts.boundingBox(), await apps.boundingBox()]) {
          assert.ok(box.x >= 0 && box.x + box.width <= width + 1, `${scheme} ${width}px ${size}: fits (${JSON.stringify(box)})`);
        }
        const clipped = await panel.evaluate((root) => [...root.querySelectorAll("[data-shortcuts] button, #remote-apps-panel button, #remote-apps-panel .app-name")]
          .filter((n) => n.scrollWidth > n.clientWidth + 1).map((n) => n.textContent.trim()));
        assert.deepEqual(clipped, [], `${scheme} ${width}px ${size}: nothing is clipped`);
        if (evidence) await page.screenshot({ path: join(evidence, `remote-${width}-${size.replace("%", "")}-${scheme}.png`), fullPage: true });
      }
    }
    await page.evaluate(() => { document.documentElement.style.fontSize = ""; document.documentElement.setAttribute("data-theme", "dark"); });
    await page.setViewportSize({ width: 1280, height: 900 });

    await shortcuts.getByRole("button", { name: "YouTube", exact: true }).click();
    await page.waitForFunction(() => window.appCalls.filter((c) => c.command === "launch_app").length === 2);
    assert.equal((await calls(page, "launch_app"))[1].args.package, "com.google.android.youtube.tv", "a shortcut opens its app");

    await apps.getByRole("button", { name: "Close", exact: true }).focus();
    await page.keyboard.press("Enter");
    assert.equal(await apps.count(), 0);
    await page.waitForFunction(() => document.activeElement?.textContent?.includes("Open app"));
    await page.keyboard.press("Enter");
    await apps.getByRole("searchbox", { name: "Search apps" }).focus();
    await page.keyboard.press("Escape");
    assert.equal(await apps.count(), 0, "Escape closes the panel");
    assert.equal(await appsKey.evaluate((el) => el === document.activeElement), true, "focus returns to Open app");
    if (evidence) {
      const video = page.video();
      await context.close();
      await video.saveAs(join(evidence, "remote-apps-simulated.webm"));
    } else await context.close();
  }

  // Pins survive a reload for the same TV and read the list for their names.
  {
    const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
    const panel = await openRemote(page, base, { pins: ["com.plexapp.android", "com.example.homecam"] });
    const shortcuts = panel.locator("[data-shortcuts]");
    await shortcuts.getByRole("button", { name: "Plex", exact: true }).waitFor();
    assert.deepEqual(await labels(shortcuts.getByRole("button")), ["Plex", "com.example.homecam", "Open app"]);
    assert.equal((await calls(page, "list_launchable_apps")).length, 1, "one read names the pins");
    await page.close();
  }

  // Another TV in the demo has no pins of its own.
  {
    const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
    await page.addInitScript(({ KEY, SHIELD_ID }) => localStorage.setItem(KEY, JSON.stringify({ [SHIELD_ID]: ["com.plexapp.android"] })), { KEY, SHIELD_ID });
    await page.goto(base, { waitUntil: "networkidle" });
    await page.getByText("Living Room Box", { exact: false }).first().click();
    await page.getByRole("tab", { name: "Remote" }).click();
    const shortcuts = page.locator("#tabpanel-remote [data-shortcuts]");
    await shortcuts.waitFor();
    assert.deepEqual(await labels(shortcuts.getByRole("button")), ["Open app"], "pins never follow another TV");
    await page.close();
  }

  // A refresh at the same address that reports another TV drops a queued launch.
  {
    const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
    const panel = await openRemote(page, base);
    await panel.getByRole("button", { name: "Open app", exact: true }).click();
    await panel.getByRole("button", { name: "Open Netflix", exact: true }).waitFor();
    await page.evaluate(() => (window.holdLaunch = true));
    await panel.getByRole("button", { name: "Open Netflix", exact: true }).click();
    await page.waitForFunction(() => typeof window.releaseLaunch === "function");
    await panel.getByRole("button", { name: "Open Plex", exact: true }).click();
    const profiles = (await calls(page, "device_profile")).length;
    await page.evaluate(() => (window.swapId = true));
    await page.getByRole("button", { name: "Rename", exact: true }).click();
    await page.locator(".rename-row input").fill("Den TV");
    await page.locator(".rename-row").getByRole("button", { name: "Save", exact: true }).click();
    await page.waitForFunction((n) => window.appCalls.filter((c) => c.command === "device_profile").length > n, profiles);
    await page.evaluate(() => { window.holdLaunch = false; window.releaseLaunch(); });
    await page.waitForTimeout(500);
    assert.deepEqual((await calls(page, "launch_app")).map((c) => c.args.package), ["com.netflix.ninja"],
      "the launch queued for the first TV never reaches the second");
    await page.close();
  }

  // A launch the TV refuses says so in words.
  {
    const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
    const panel = await openRemote(page, base, { launchFails: true });
    await panel.getByRole("button", { name: "Open app", exact: true }).click();
    await panel.getByRole("button", { name: "Open Plex", exact: true }).click();
    await panel.getByText("Couldn't open Plex. Error: Activity not started, unable to resolve Intent { … }").waitFor();
    await page.close();
  }
  console.log("Remote apps passed: lazy list, named order, search, Open through the session, 3 pins per hardware id, shortcuts, reload, another TV, refusal, 1280/720/150% in both themes, keyboard.");
} finally {
  await browser?.close();
  await server?.close();
}
