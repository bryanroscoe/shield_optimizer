import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { chromium } from "playwright";
import { startViteServer } from "./helpers/vite-harness.mjs";

let server;
let browser;
let origin;
let preview;

before(async () => {
  ({ server, origin } = await startViteServer());
  preview = await server.ssrLoadModule("/src/lib/snapshotPreview.ts");
  browser = await chromium.launch({ headless: true });
});

after(async () => {
  await browser?.close();
  await server?.close();
});

const SNAP = {
  path: "/data/snapshots/before-tweaks.json", filename: "before-tweaks.json", saved_at: "2026-10-01T10:00:00Z",
  label: "Before tweaks", device_name: "Living Room", device_serial: "A:5555", device_type: "shield",
  disabled_count: 1, settings_count: 1, launcher: "com.spocky.projengmenu",
};

const plan = (over = {}) => ({
  cross_device_warning: null,
  packages_to_disable: ["com.bloat"],
  packages_already_disabled: [],
  packages_not_installed: [],
  launcher_to_set: null,
  current_launcher: "com.google.android.tvlauncher",
  launcher_not_installed: null,
  settings_to_write: {},
  settings_to_delete: [],
  settings_already_set: [],
  current_values: {},
  ...over,
});

async function open(t, screen, { width = 384, setup } = {}) {
  const page = await browser.newPage({ viewport: { width, height: 812 } });
  page.setDefaultTimeout(10000);
  t.after(() => page.close());
  await page.addInitScript((setup) => {
    localStorage.clear();
    window.calls = [];
    window.handlers = {};
    window.activeHost = "A";
    window.snapshots = [];
    window.backups = [];
    window.packageRows = [];
    window.__TAURI_INTERNALS__ = {
      transformCallback: () => 1,
      invoke: async (command, args) => {
        window.calls.push({ command, args });
        if (window.handlers[command]) return window.handlers[command](args);
        switch (command) {
          case "get_entitlement": return "pro";
          case "wireless_connect": window.activeHost = args.host; return { ok: true };
          case "wireless_status": return { connected: true };
          case "list_devices": return window.activeHost ? [{
            id: 1, serial: `${window.activeHost}:5555`, name: window.activeHost, model: "Shield",
            status: "device", connection: "network", device_type: "shield", properties: null,
          }] : [];
          case "list_snapshots": return structuredClone(window.snapshots);
          case "list_backups": return structuredClone(window.backups);
          case "list_installed_packages": return window.packageRows;
          case "list_remote_dir": return [];
          case "remote_warm": return { transport: "channel", message: "" };
          case "send_key": return { ok: true, transport: "channel", message: "" };
          case "send_text": return { ok: true, transport: "channel", message: "" };
          default: return { ok: true, message: "done" };
        }
      },
    };
    if (setup) new Function(setup)();
  }, setup ? `(${setup})()` : "");
  await page.goto(origin);
  await page.evaluate(async (screen) => {
    const { session } = await import("/src/lib/session.svelte.ts");
    const { router } = await import("/src/lib/router.svelte.ts");
    window.session = session;
    window.router = router;
    session.entitlement = "pro";
    await session.connect("A", 5555);
    router.reset("dashboard");
    router.navigate(screen);
  }, screen);
  return page;
}

const count = (page, command) =>
  page.evaluate((c) => window.calls.filter((call) => call.command === c).length, command);

// ---- Snapshots ----

test("restore outcome lists every skipped or failed item with the backend's reason", () => {
  const res = {
    packages_disabled: [], launcher_set: false, settings_written: [], settings_deleted: [],
    packages_failed: ["com.stock (turned back on: no other launcher took Home)", "com.plain"],
    settings_failed: ["global.x: SecurityException"],
    launcher_message: "com.spocky.projengmenu is disabled; enable it first, then restore again.",
    summary: "",
  };
  assert.deepEqual(preview.restoreOutcome(res, plan({ launcher_to_set: null })), [
    { item: "com.stock", reason: "turned back on: no other launcher took Home" },
    { item: "com.plain", reason: "Couldn't disable it." },
    { item: "Home app", reason: "com.spocky.projengmenu is disabled; enable it first, then restore again." },
    { item: "global.x", reason: "SecurityException" },
  ]);
  // A planned switch that didn't land, with no message, is still reported.
  assert.deepEqual(
    preview.restoreOutcome({ ...res, packages_failed: [], settings_failed: [], launcher_message: null }, plan({ launcher_to_set: "com.home" })),
    [{ item: "Home app", reason: "com.home wasn't set as Home." }],
  );
  // A successful switch's note is not a failure.
  assert.deepEqual(
    preview.restoreOutcome({ ...res, packages_failed: [], settings_failed: [], launcher_set: true, launcher_message: "com.home via role" }),
    [],
  );
});

test("a disabled target launcher and kept Home apps are named with the plan's reasons", () => {
  const skipped = plan({
    launcher_disabled: "com.spocky.projengmenu",
    packages_kept: [{ package: "com.google.android.tvlauncher", reason: "only Home app" }],
  });
  assert.match(preview.launcherSkipNote(skipped), /^com\.spocky\.projengmenu is disabled on this TV, and a restore never re-enables anything\./);
  const { unchanged } = preview.previewRows(skipped, SNAP.launcher);
  assert.deepEqual(unchanged.slice(0, 2), [
    { item: "Home app", now: "com.google.android.tvlauncher", result: "com.spocky.projengmenu is disabled; skipped", kind: "launcher" },
    { item: "com.google.android.tvlauncher", now: "enabled", result: "Kept on: only Home app", kind: "disable" },
  ]);
  assert.deepEqual(preview.keptPackages(skipped), [{ package: "com.google.android.tvlauncher", reason: "only Home app" }]);
  assert.match(preview.previewSummary(skipped), / · 1 kept on for Home$/);
  // Older cores send neither field.
  assert.deepEqual(preview.keptPackages(plan()), []);
  assert.doesNotMatch(preview.previewSummary(plan()), /kept/);
  assert.equal(preview.launcherSkipNote(plan({ launcher_not_installed: "com.gone" })), "com.gone isn't installed on this TV.");
  assert.equal(preview.launcherSkipNote(plan({ launcher_to_set: "com.home" })), null);
  assert.equal(preview.launcherSkipNote(plan()), null);
});

test("snapshot delete names the snapshot and keeps the TV-unaffected sentence", async (t) => {
  const page = await open(t, "snapshots", {
    setup: () => { window.snapshots = [{
      path: "/data/snapshots/before-tweaks.json", filename: "before-tweaks.json", saved_at: "2026-10-01T10:00:00Z",
      label: "Before tweaks", device_name: "Living Room", device_serial: "A:5555", device_type: "shield",
      disabled_count: 1, settings_count: 1, launcher: null,
    }]; },
  });
  await page.getByRole("button", { name: "Delete snapshot Before tweaks", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: 'Delete snapshot "Before tweaks"?' });
  await dialog.waitFor();
  const text = await dialog.innerText();
  assert.match(text, /before-tweaks\.json/);
  assert.match(text, /Saved .*Oct/);
  assert.match(text, /Deleting a local snapshot does not change the TV\./);
  await page.evaluate(() => { window.snapshots = []; });
  await dialog.getByRole("button", { name: "Delete", exact: true }).click();
  await page.getByText('Deleted snapshot "Before tweaks".', { exact: true }).waitFor();
  const call = await page.evaluate(() => window.calls.find((c) => c.command === "delete_snapshot"));
  assert.deepEqual(call.args, { snapshotPath: "/data/snapshots/before-tweaks.json" });
});

for (const change of ["same-address redial", "lost connection"]) test(`restore preview is discarded after a ${change}; nothing is applied`, async (t) => {
  const page = await open(t, "snapshots", {
    setup: () => {
      window.snapshots = [{
        path: "/data/s.json", filename: "s.json", saved_at: "2026-10-01T10:00:00Z", label: "Old TV",
        device_name: "A", device_serial: "A:5555", device_type: "shield", disabled_count: 1, settings_count: 0, launcher: null,
      }];
      window.handlers.preview_apply = () => ({
        cross_device_warning: null, packages_to_disable: ["com.bloat"], packages_already_disabled: [],
        packages_not_installed: [], launcher_to_set: null, current_launcher: null, launcher_not_installed: null,
        settings_to_write: {}, settings_to_delete: [], settings_already_set: [], current_values: {},
      });
    },
  });
  await page.getByRole("button", { name: "Preview restore" }).click();
  const restore = page.locator(".plan-card").getByRole("button", { name: "Restore", exact: true });
  await restore.waitFor();
  if (change === "lost connection") await page.evaluate(() => { window.session.liveness = "lost"; });
  else await page.evaluate(async () => { await window.session.connect("A", 5555); });
  await page.getByText(/The TV connection changed since this preview was built, so nothing was applied/).waitFor();
  assert.equal(await page.locator(".plan-card").count(), 0);
  assert.equal(await count(page, "apply_snapshot"), 0);
});

test("a restore preview that resolves after a redial is never shown", async (t) => {
  const page = await open(t, "snapshots", {
    setup: () => {
      window.snapshots = [{
        path: "/data/s.json", filename: "s.json", saved_at: "2026-10-01T10:00:00Z", label: "Old TV",
        device_name: "A", device_serial: "A:5555", device_type: "shield", disabled_count: 1, settings_count: 0, launcher: null,
      }];
      window.handlers.preview_apply = () => new Promise((resolve) => { window.finishPreview = resolve; });
    },
  });
  await page.getByRole("button", { name: "Preview restore" }).click();
  await page.waitForFunction(() => !!window.finishPreview);
  await page.evaluate(async () => {
    await window.session.connect("A", 5555);
    window.finishPreview({
      cross_device_warning: null, packages_to_disable: ["com.bloat"], packages_already_disabled: [],
      packages_not_installed: [], launcher_to_set: null, current_launcher: null, launcher_not_installed: null,
      settings_to_write: {}, settings_to_delete: [], settings_already_set: [], current_values: {},
    });
  });
  await page.getByText("The TV connection changed while the plan was loading. Preview again.", { exact: true }).waitFor();
  assert.equal(await page.locator(".plan-card").count(), 0);
});

test("restore preview shows a skipped Home app beside Restore and the result lists what didn't restore", async (t) => {
  const page = await open(t, "snapshots", {
    setup: () => {
      window.snapshots = [{
        path: "/data/s.json", filename: "s.json", saved_at: "2026-10-01T10:00:00Z", label: "Projectivy setup",
        device_name: "A", device_serial: "A:5555", device_type: "shield", disabled_count: 1, settings_count: 0,
        launcher: "com.spocky.projengmenu",
      }];
      window.handlers.preview_apply = () => ({
        cross_device_warning: null, packages_to_disable: ["com.bloat"], packages_already_disabled: [],
        packages_not_installed: [], launcher_to_set: null, current_launcher: "com.google.android.tvlauncher",
        launcher_not_installed: null, launcher_disabled: "com.spocky.projengmenu",
        packages_kept: [{ package: "com.google.android.tvlauncher", reason: "only Home app" }],
        settings_to_write: {}, settings_to_delete: [], settings_already_set: [], current_values: {},
      });
      window.handlers.apply_snapshot = () => ({
        packages_disabled: ["com.bloat"], packages_failed: ["com.google.android.tvlauncher (kept: only Home app)"],
        launcher_set: false, launcher_message: "com.spocky.projengmenu is disabled; enable it first, then restore",
        settings_written: [], settings_deleted: [], settings_failed: [],
        summary: "Disabled 1 packages (1 failed). Launcher: skipped (com.spocky.projengmenu is disabled; enable it first, then restore). 0 settings written, 0 reset (0 failed). Kept on so the TV keeps a Home screen: com.google.android.tvlauncher.",
      });
    },
  });
  await page.getByRole("button", { name: "Preview restore" }).click();
  const card = page.locator(".plan-card");
  await card.getByText(/The Home app won't change: com\.spocky\.projengmenu is disabled on this TV, and a restore never re-enables anything/).waitFor();
  await card.getByText("Kept on so the TV keeps a Home screen: com.google.android.tvlauncher (only Home app).").waitFor();
  assert.match(await card.innerText(), /never re-enables anything or/);
  await card.getByRole("button", { name: "Restore", exact: true }).click();
  const list = page.getByRole("list", { name: "Not restored" });
  await list.waitFor();
  const text = (await list.innerText()).replace(/\s+/g, " ");
  assert.match(text, /com\.google\.android\.tvlauncher kept: only Home app/);
  assert.match(text, /Home app com\.spocky\.projengmenu is disabled; enable it first, then restore/);
  await page.getByText("2 items didn't restore", { exact: true }).waitFor();
  assert.equal(await count(page, "enable_package"), 0);
});

// ---- Remote ----

test("keyboard and accessibility activation moves the D-pad once; a tap still sends once", async (t) => {
  const page = await open(t, "remote");
  const up = page.getByRole("button", { name: "Up", exact: true });
  await up.waitFor();
  const keys = () => page.evaluate(() => window.calls.filter((c) => c.command === "send_key").map((c) => c.args.key));
  await up.focus();
  await page.keyboard.press("Enter");
  await page.waitForFunction(() => window.calls.filter((c) => c.command === "send_key").length === 1);
  await page.getByRole("button", { name: "Left", exact: true }).focus();
  await page.keyboard.press("Space");
  await page.waitForFunction(() => window.calls.filter((c) => c.command === "send_key").length === 2);
  // TalkBack / Switch Access: a click with no pointer press.
  await page.getByRole("button", { name: "Right", exact: true }).evaluate((b) => b.click());
  await page.waitForFunction(() => window.calls.filter((c) => c.command === "send_key").length === 3);
  // A pointer tap sends on pointerdown and its click must not send again.
  await page.getByRole("button", { name: "Down", exact: true }).click();
  await page.waitForTimeout(150);
  assert.deepEqual(await keys(), ["up", "left", "right", "down"]);
});

test("Send Text is a real dialog: focus moves in, stays in, Escape closes and returns focus", async (t) => {
  const page = await open(t, "remote");
  const opener = page.getByRole("button", { name: "Keyboard input", exact: true });
  await opener.focus();
  await page.keyboard.press("Enter");
  const dialog = page.getByRole("dialog", { name: "Send Text to TV" });
  await dialog.waitFor();
  const input = dialog.getByRole("textbox", { name: "Text to send" });
  await page.waitForFunction(() => document.activeElement?.classList.contains("modal-input"));
  for (let i = 0; i < 6; i++) {
    await page.keyboard.press("Tab");
    assert.equal(await dialog.evaluate((d) => d.contains(document.activeElement)), true, "Tab stays inside the dialog");
  }
  await page.keyboard.press("Shift+Tab");
  assert.equal(await dialog.evaluate((d) => d.contains(document.activeElement)), true);
  await page.keyboard.press("Escape");
  await dialog.waitFor({ state: "detached" });
  await page.waitForFunction(() => document.activeElement?.getAttribute("aria-label") === "Keyboard input");

  await opener.click();
  await dialog.waitFor();
  await page.waitForFunction(() => document.activeElement?.classList.contains("modal-input"));
  await input.fill("hello");
  await dialog.getByRole("button", { name: "Close Send Text", exact: true }).click();
  await dialog.waitFor({ state: "detached" });
  assert.equal(await count(page, "send_text"), 0);
});

// ---- Backups ----

test("backup rows show a known app label, keep the package id and wrap at 320px", async (t) => {
  const page = await open(t, "backups", {
    width: 320,
    setup: () => {
      window.backups = [{
        package: "com.teamsmart.videomanager.tv", path: "/backups/smarttube", saved_at: "2026-09-05T13:00:00Z",
        size_bytes: 4096, apk_count: 3, complete: true,
      }];
      window.packageRows = [
        { package: "com.teamsmart.videomanager.tv", name: "SmartTube", system: false, enabled: true },
      ];
    },
  });
  const row = page.locator(".backup-row");
  await row.waitFor();
  // Unknown label: the package id is the name and is never cut to "com…".
  const pkgName = row.locator(".b-name");
  assert.equal(await pkgName.innerText(), "com.teamsmart.videomanager.tv");
  assert.equal(await pkgName.evaluate((el) => el.scrollWidth <= el.clientWidth), true);
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);

  await page.getByRole("button", { name: "Choose an app" }).click();
  await page.getByRole("button", { name: /Back up SmartTube/ }).waitFor();
  await row.locator(".b-name", { hasText: "SmartTube" }).waitFor();
  assert.equal(await row.locator(".b-pkg").innerText(), "com.teamsmart.videomanager.tv");
  await row.getByRole("button", { name: "Delete the SmartTube (com.teamsmart.videomanager.tv) backup" }).waitFor();
  const restore = row.getByRole("button", { name: /Restore/ });
  const box = await restore.boundingBox();
  const rowBox = await row.boundingBox();
  assert.ok(box.x + box.width <= rowBox.x + rowBox.width, "actions stay inside the row");
});

test("backup intro keeps limits and the restore warning visible, routine steps in help", async (t) => {
  const page = await open(t, "backups");
  await page.getByText("App data, settings, and sign-in details are not included.", { exact: true }).waitFor();
  const warning = page.locator(".system-note");
  assert.equal(await warning.evaluate((el) => !!el.closest("details")), false);
  assert.match(await warning.innerText(), /not a full-TV recovery backup/);
  const help = page.locator("details.help-disclosure", { has: page.locator("summary", { hasText: "About APK backups" }) });
  assert.equal(await help.getAttribute("open"), null);
  const routine = help.getByText(/base APK and every installed split APK/);
  assert.equal(await routine.isVisible(), false);
  await help.locator("summary").focus();
  await page.keyboard.press("Enter");
  await routine.waitFor();
});

// ---- Files ----

test("Files keeps the private-storage consequence visible and the routine explainer in help", async (t) => {
  const page = await open(t, "files");
  const visible = page.getByText(/not the phone's public\s+Downloads/);
  await visible.waitFor();
  assert.equal(await visible.evaluate((el) => !!el.closest("details")), false);
  const routine = page.getByText(/Files transfer over your LAN/);
  assert.equal(await routine.isVisible(), false);
  await page.getByText("About file transfers", { exact: true }).click();
  await routine.waitFor();
  await page.getByRole("button", { name: /Find app backups/ }).click();
  await page.getByText(/Then find it here and copy it off the TV\./).waitFor();
  assert.equal(await page.getByText(/this computer/).count(), 0);
});

// ---- Fitted dialogs: content scrolls, actions stay on screen ----

const inViewport = (locator) => locator.evaluate((el) => {
  const r = el.getBoundingClientRect();
  return r.top >= 0 && r.bottom <= innerHeight && r.left >= 0 && r.right <= innerWidth;
});

test("Send Text keeps Send and Cancel on screen in a short, narrow viewport", async (t) => {
  const page = await open(t, "remote", { width: 275 });
  await page.setViewportSize({ width: 275, height: 230 });
  await page.getByRole("button", { name: "Keyboard input", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "Send Text to TV" });
  await dialog.waitFor();
  assert.equal(await inViewport(dialog.getByRole("button", { name: "Send Text", exact: true })), true);
  assert.equal(await inViewport(dialog.getByRole("button", { name: "Cancel", exact: true })), true);
  assert.equal(await page.locator(".modal-actions.divided").count(), 1, "a divider marks the cut-off content");
});

test("restore preview pins Cancel and Restore while a long plan scrolls inside the card", async (t) => {
  const page = await open(t, "snapshots", {
    width: 275,
    setup: () => {
      window.snapshots = [{
        path: "/data/s.json", filename: "s.json", saved_at: "2026-10-01T10:00:00Z", label: "Big",
        device_name: "A", device_serial: "A:5555", device_type: "shield", disabled_count: 30, settings_count: 0, launcher: null,
      }];
      window.handlers.preview_apply = () => ({
        cross_device_warning: null, packages_to_disable: Array.from({ length: 30 }, (_, i) => `com.example.app${i}`),
        packages_already_disabled: [], packages_not_installed: [], launcher_to_set: null, current_launcher: null,
        launcher_not_installed: null, settings_to_write: {}, settings_to_delete: [], settings_already_set: [], current_values: {},
      });
    },
  });
  await page.setViewportSize({ width: 275, height: 610 });
  await page.getByRole("button", { name: "Preview restore" }).click();
  const dialog = page.getByRole("dialog", { name: 'Restore "Big"?' });
  await dialog.waitFor();
  const restore = dialog.getByRole("button", { name: "Restore", exact: true });
  assert.equal(await inViewport(restore), true);
  assert.equal(await inViewport(dialog.getByRole("button", { name: "Cancel", exact: true })), true);
  assert.equal(await page.locator(".plan-body").evaluate((el) => el.scrollHeight > el.clientHeight), true);
  // A proper dialog: Escape closes it without applying.
  await page.keyboard.press("Escape");
  await dialog.waitFor({ state: "detached" });
  assert.equal(await count(page, "apply_snapshot"), 0);
});
