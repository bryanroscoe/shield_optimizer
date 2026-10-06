import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { chromium } from "playwright";
import { startViteServer } from "./helpers/vite-harness.mjs";

let server;
let browser;
let origin;
let view;

before(async () => {
  ({ server, origin } = await startViteServer());
  view = await server.ssrLoadModule("/src/lib/launcherView.ts");
  browser = await chromium.launch({ headless: true });
});

after(async () => {
  await browser?.close();
  await server?.close();
});

const STOCK = "com.google.android.apps.tv.launcherx";
const WRAITH = "com.google.android.tungsten.setupwraith";
const PROJECTIVY = "com.spocky.projengmenu";
const FLAUNCHER = "me.efesser.flauncher";

const row = (pkg, over = {}) => ({
  installed: true,
  enabled: true,
  stock: false,
  other: false,
  setup_helper: false,
  ...over,
  entry: { name: pkg, package: pkg, source_url: null, ...(over.entry ?? {}) },
});

const stock = (enabled) =>
  row(STOCK, { stock: true, enabled, entry: { name: "Google TV Home", disable_with: [WRAITH] } });
const wraith = (enabled) => row(WRAITH, { setup_helper: true, other: true, enabled, entry: { name: "Setup Wraith" } });

// ---- pure helper ----

test("Setup Wraith reads off / on / risk from the launcher rows", () => {
  assert.equal(view.setupHelperView([stock(true)]), null);
  assert.equal(view.setupHelperView([stock(true), { ...wraith(true), installed: false }]), null);
  assert.equal(view.setupHelperView([stock(true), wraith(false)]).state, "off");
  assert.equal(view.setupHelperView([stock(true), wraith(true)]).state, "on");
  // Stock off with the helper on is the state that takes the Home button.
  assert.equal(view.setupHelperView([stock(false), wraith(true)]).state, "risk");
  assert.equal(view.setupHelperView([wraith(true)]).state, "risk");
});

test("a takeover turns Setup Wraith off only when an enabled stock names it", () => {
  assert.equal(view.takeoverTurnsOffSetupHelper([stock(true), wraith(true)]), true);
  assert.equal(view.takeoverTurnsOffSetupHelper([stock(false), wraith(true)]), false);
  assert.equal(view.takeoverTurnsOffSetupHelper([stock(true), wraith(false)]), false);
  const shieldStock = row("com.google.android.tvlauncher", { stock: true });
  assert.equal(view.takeoverTurnsOffSetupHelper([shieldStock, wraith(true)]), false);
});

test("Disable stock launcher stays off until the picked app is Home or stock is in the way", () => {
  const launchers = [stock(true), wraith(true), row(PROJECTIVY)];
  const gate = (over) =>
    view.stockDisableGate({ launchers, choice: PROJECTIVY, currentPkg: STOCK, stockHoldsHomeFor: null, ...over });

  assert.equal(gate({ choice: "" }).allowed, false);
  assert.match(gate({}).reason, /Set the app as Home first/);
  assert.equal(gate({ currentPkg: PROJECTIVY }).allowed, true);
  assert.equal(gate({ stockHoldsHomeFor: PROJECTIVY }).allowed, true);
  // A stock_holds_home answer for a different app does not carry over.
  assert.equal(gate({ stockHoldsHomeFor: "com.other" }).allowed, false);
  assert.match(gate({ choice: STOCK, currentPkg: STOCK }).reason, /not the stock launcher/);
  assert.match(gate({ choice: WRAITH, currentPkg: WRAITH }).reason, /setup helper/);
  assert.match(
    view.stockDisableGate({ launchers: [stock(false), row(PROJECTIVY)], choice: PROJECTIVY, currentPkg: PROJECTIVY, stockHoldsHomeFor: null }).reason,
    /No enabled stock launcher/,
  );
});

test("the stock-disable message never claims what wasn't verified or re-read", () => {
  const ok = { ok: true, last_error: null };
  assert.match(view.stockDisableMessage(ok, PROJECTIVY, true, "off"), /Setup Wraith\) was turned off too/);
  // Predicted off, but the re-read failed: say nothing about it.
  assert.doesNotMatch(view.stockDisableMessage(ok, PROJECTIVY, true, null), /Wraith/);
  // Predicted off, re-read says it is still on.
  assert.match(view.stockDisableMessage(ok, PROJECTIVY, true, "risk"), /still reads as on/);
  assert.equal(
    view.stockDisableMessage({ ok: false, last_error: "Refusing to disable x." }, PROJECTIVY, true, "off"),
    "Refusing to disable x.",
  );
  assert.equal(view.stockDisableMessage({ ok: false, last_error: null }, PROJECTIVY, false, null), "The stock launcher was left alone.");
});

test("picker rows hide system apps, search name and package, dedupe and sort", () => {
  const rows = [
    { package: "b.user", name: "Zeta", system: false, enabled: true },
    { package: "a.user", name: null, system: false, enabled: true },
    { package: "b.user", name: "Zeta dup", system: false, enabled: true },
    { package: "c.sys", name: "Alpha Settings", system: true, enabled: false },
  ];
  assert.deepEqual(view.filterHomeCandidates(rows, "", false).map((p) => p.package), ["a.user", "b.user"]);
  assert.deepEqual(view.filterHomeCandidates(rows, "", true).map((p) => p.package), ["a.user", "c.sys", "b.user"]);
  assert.deepEqual(view.filterHomeCandidates(rows, "ZETA", false).map((p) => p.package), ["b.user"]);
  assert.deepEqual(view.filterHomeCandidates(rows, "c.sy", false), []);
});

test("source site links are offered only for http(s) URLs", () => {
  assert.equal(view.sourceSiteHost("https://www.projectivy.app/download"), "projectivy.app");
  assert.equal(view.sourceSiteHost("http://example.org"), "example.org");
  assert.equal(view.sourceSiteHost("javascript:alert(1)"), null);
  assert.equal(view.sourceSiteHost("not a url"), null);
  assert.equal(view.sourceSiteHost(null), null);
});

// ---- screen ----

async function open(t, setup = {}) {
  const page = await browser.newPage({ viewport: { width: 384, height: 812 } });
  t.after(() => page.close());
  if (setup.clock) await page.clock.install();
  await page.addInitScript((init) => {
    window.calls = [];
    window.handlers = {};
    window.copied = [];
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: async (text) => { window.copied.push(text); } },
    });
    window.launchers = init.launchers;
    window.current = init.current;
    window.packageRows = [
      { package: "com.spocky.projengmenu", name: "Projectivy", system: false, enabled: true },
      { package: "com.example.player", name: "Some Player", system: false, enabled: true },
      { package: "com.android.settings", name: "Settings", system: true, enabled: true },
    ];
    let callbackId = 0;
    window.__TAURI_INTERNALS__ = {
      // set_default_launcher passes a progress Channel.
      transformCallback: () => ++callbackId,
      invoke: async (command, args) => {
        window.calls.push({ command, args });
        if (window.handlers[command]) return window.handlers[command](args);
        switch (command) {
          case "get_entitlement": return "pro";
          case "list_devices": return [{
            id: 1, serial: "A:5555", name: "A", model: "Chromecast", status: "device",
            connection: "network", device_type: "google_tv", properties: null,
          }];
          case "wireless_status": return { connected: true };
          case "wireless_connect": return { ok: true, message: "connected" };
          case "list_launchers": return structuredClone(window.launchers);
          case "current_launcher": return { package: window.current, activity: null };
          case "channel_provider_disabled": return false;
          case "list_installed_packages": return window.packageRows;
          default: return { ok: true, message: "done" };
        }
      },
    };
  }, setup);
  await page.goto(origin);
  await page.evaluate(async () => {
    const { session } = await import("/src/lib/session.svelte.ts");
    const { router } = await import("/src/lib/router.svelte.ts");
    window.session = session;
    session.entitlement = "pro";
    await session.connect("A", 5555);
    router.reset("dashboard");
    router.navigate("launcher");
  });
  await page.getByRole("heading", { name: "Launcher" }).waitFor();
  await page.getByText("Available launchers").waitFor();
  return page;
}

const count = (page, command) =>
  page.evaluate((c) => window.calls.filter((call) => call.command === c).length, command);

test("Setup Wraith on with stock off warns and turns it off, then re-reads the TV", async (t) => {
  const page = await open(t, {
    launchers: [stock(false), wraith(true), row(PROJECTIVY, { entry: { name: "Projectivy" } })],
    current: PROJECTIVY,
  });
  const card = page.locator('[data-setup-helper="risk"]');
  await card.getByText("on while the stock launcher is off. It will likely grab the Home button").waitFor();
  const before = await count(page, "list_launchers");

  await page.evaluate((w) => {
    window.handlers.disable_setup_helper = () => {
      window.launchers = window.launchers.map((l) => (l.entry.package === w ? { ...l, enabled: false } : l));
      return { ok: true, message: "Setup Wraith is off." };
    };
  }, WRAITH);
  await card.getByRole("button", { name: "Turn it off" }).click();
  await page.locator('[data-setup-helper="off"]').getByRole("button", { name: "Re-enable Setup Wraith" }).waitFor();

  const call = await page.evaluate(() => window.calls.find((c) => c.command === "disable_setup_helper"));
  assert.deepEqual(call.args, { serial: "A:5555", package: WRAITH });
  assert.ok((await count(page, "list_launchers")) > before);
});

test("Setup Wraith off offers Re-enable through enable_package", async (t) => {
  const page = await open(t, {
    launchers: [stock(false), wraith(false), row(PROJECTIVY, { entry: { name: "Projectivy" } })],
    current: PROJECTIVY,
  });
  await page.getByText(/Setup Wraith is off\. Turn this back on if Google asks you to sign in again/).waitFor();
  await page.getByRole("button", { name: "Re-enable Setup Wraith" }).click();
  await page.getByText("Setup Wraith is back on.").waitFor();
  const call = await page.evaluate(() => window.calls.find((c) => c.command === "enable_package"));
  assert.deepEqual(call.args, { serial: "A:5555", package: WRAITH });
});

test("a not-ok default switch re-reads the TV and offers its diagnostics", async (t) => {
  const page = await open(t, {
    launchers: [stock(true), wraith(true), row(PROJECTIVY, { entry: { name: "Projectivy" } })],
    current: STOCK,
  });
  await page.evaluate(() => {
    window.handlers.set_default_launcher = () => ({
      ok: false,
      strategy: null,
      current_launcher: null,
      last_error: "The TV kept Google TV Home as Home.",
      stock_takeover_available: false,
      diagnostics: ["cmd role add-role-holder -> ok", "verify -> still stock"],
    });
  });
  const before = await count(page, "list_launchers");
  await page.locator(".launcher-row", { hasText: "Projectivy" }).getByRole("button", { name: /Set default/ }).click();
  await page.locator(".action-note").getByText("The TV kept Google TV Home as Home.").waitFor();
  assert.ok((await count(page, "list_launchers")) > before, "launcher state is re-read after a not-ok result");

  await page.getByRole("button", { name: "Copy diagnostic details" }).first().click();
  await page.getByRole("button", { name: "Copied" }).waitFor();
  assert.deepEqual(await page.evaluate(() => window.copied), [
    "cmd role add-role-holder -> ok\nverify -> still stock",
  ]);
});

test("set any app as Home reports Android's answer, then unlocks the separate stock step", async (t) => {
  const page = await open(t, {
    launchers: [stock(true), wraith(true)],
    current: STOCK,
  });
  await page.getByRole("button", { name: /Advanced: set another app as Home/ }).click();
  const disableStock = page.getByRole("button", { name: /Disable stock launcher/ });
  assert.equal(await disableStock.isDisabled(), true);

  await page.getByRole("button", { name: "Choose an app" }).click();
  await page.getByRole("button", { name: /Choose Projectivy/ }).click();
  await page.getByText(/Set the app as Home first/).waitFor();
  assert.equal(await disableStock.isDisabled(), true);

  const message =
    'Android took the request, but the stock launcher still holds Home. On this TV only disabling the stock launcher hands Home over; that is the separate "Disable stock launcher" step. Nothing was disabled. Home is still ' +
    STOCK + ".";
  await page.evaluate((m) => {
    window.handlers.set_home_any = () => ({
      ok: false,
      current_launcher: "com.google.android.apps.tv.launcherx",
      declares_home: true,
      stock_holds_home: true,
      message: m,
      diagnostics: ["pm enable -> ok"],
    });
  }, message);
  await page.getByRole("button", { name: /Set as Home/ }).click();
  const result = page.locator(".adv-result");
  await result.getByText(/stock launcher still holds Home/).waitFor();
  assert.equal(await result.evaluate((el) => el.classList.contains("ok")), false, "not-ok is never styled as success");
  const setCall = await page.evaluate(() => window.calls.find((c) => c.command === "set_home_any"));
  assert.deepEqual(setCall.args, { serial: "A:5555", package: PROJECTIVY, activity: null });
  // set_home_any must never disable anything on its own.
  assert.equal(await count(page, "disable_stock_launcher"), 0);
  assert.equal(await disableStock.isDisabled(), false);

  await page.evaluate(() => {
    window.handlers.disable_stock_launcher = () => ({
      ok: false,
      strategy: null,
      current_launcher: null,
      last_error: "Refusing to disable it: it's the only enabled launcher left on this device. Nothing was disabled.",
      stock_takeover_available: false,
      diagnostics: ["query-activities HOME -> x"],
    });
  });
  const before = await count(page, "list_launchers");
  await disableStock.click();
  const dialog = page.getByRole("alertdialog", { name: "Confirm disabling the stock launcher" });
  await dialog.getByText(/Also turns off Google TV's setup helper/).waitFor();
  await dialog.getByRole("button", { name: "Disable stock launcher" }).click();
  await result.getByText(/only enabled launcher left/).waitFor();
  const stockCall = await page.evaluate(() => window.calls.find((c) => c.command === "disable_stock_launcher"));
  assert.deepEqual(stockCall.args, { serial: "A:5555", target: PROJECTIVY });
  assert.ok((await count(page, "list_launchers")) > before);
  await page.locator(".adv").getByRole("button", { name: "Copy diagnostic details" }).waitFor();
});

test("Save snapshot first leaves stock alone when the snapshot fails", async (t) => {
  const page = await open(t, { launchers: [stock(true)], current: PROJECTIVY });
  await page.getByRole("button", { name: /Advanced: set another app as Home/ }).click();
  await page.getByRole("button", { name: "Choose an app" }).click();
  await page.getByRole("button", { name: /Choose Projectivy/ }).click();
  await page.evaluate(() => {
    window.handlers.save_snapshot = () => { throw "disk full"; };
  });
  await page.getByRole("button", { name: /Disable stock launcher/ }).click();
  await page.getByRole("button", { name: "Save snapshot first" }).click();
  await page.getByText(/The snapshot wasn't saved, so the stock launcher was left alone/).waitFor();
  assert.equal(await count(page, "disable_stock_launcher"), 0);
});

test("a locked Set as Home opens the paywall and calls nothing else", async (t) => {
  const page = await open(t, { launchers: [stock(true)], current: STOCK });
  await page.getByRole("button", { name: /Advanced: set another app as Home/ }).click();
  await page.getByRole("button", { name: "Choose an app" }).click();
  await page.getByRole("button", { name: /Choose Projectivy/ }).click();
  await page.evaluate(() => {
    window.handlers.set_home_any = () => { throw "LOCKED:launcher_takeover"; };
  });
  await page.getByRole("button", { name: /Set as Home/ }).click();
  await page.getByRole("heading", { name: "Unlock ATV Optimizer Pro" }).waitFor();
  assert.equal(await page.locator(".adv-result").count(), 0);
});

test("source site copies the launcher's page link instead of navigating the app", async (t) => {
  const page = await open(t, {
    launchers: [
      stock(true),
      row(FLAUNCHER, { installed: false, enabled: false, entry: { name: "FLauncher", source_url: "https://gitlab.com/flauncher/flauncher" } }),
    ],
    current: STOCK,
  });
  await page.getByRole("button", { name: /Source site for FLauncher \(gitlab\.com\)/ }).click();
  await page.getByText(/Copied the gitlab\.com link for FLauncher/).waitFor();
  assert.deepEqual(await page.evaluate(() => window.copied), ["https://gitlab.com/flauncher/flauncher"]);
  assert.equal(new URL(page.url()).origin, origin);
});

test("after opening the Play Store the list re-reads until the launcher is installed", async (t) => {
  const page = await open(t, {
    clock: true,
    launchers: [
      stock(true),
      row(FLAUNCHER, { installed: false, enabled: false, entry: { name: "FLauncher" } }),
    ],
    current: STOCK,
  });
  await page.locator(".launcher-row", { hasText: "FLauncher" }).getByRole("button", { name: "Install" }).click();
  await page.getByText(/Checking for FLauncher every few seconds/).waitFor();
  const call = await page.evaluate(() => window.calls.find((c) => c.command === "open_play_store"));
  assert.deepEqual(call.args, { serial: "A:5555", package: FLAUNCHER });

  await page.evaluate((pkg) => {
    window.launchers = window.launchers.map((l) => (l.entry.package === pkg ? { ...l, installed: true, enabled: true } : l));
  }, FLAUNCHER);
  await page.clock.runFor(5_100);
  await page.getByText("FLauncher is installed.").waitFor();
  assert.equal(await page.getByText(/Checking for FLauncher/).count(), 0);
});
