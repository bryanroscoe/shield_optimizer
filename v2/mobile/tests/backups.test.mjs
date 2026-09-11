import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import { createServer } from "vite";

let server;
let browser;
let origin;

before(async () => {
  const port = 20_000 + (process.pid % 20_000);
  server = await createServer({
    root: fileURLToPath(new URL("../", import.meta.url)),
    logLevel: "silent",
    server: { host: "127.0.0.1", port, strictPort: true },
  });
  await server.listen();
  origin = `http://127.0.0.1:${server.httpServer.address().port}`;
  browser = await chromium.launch({ headless: true });
});

after(async () => {
  await browser?.close();
  await server?.close();
});

async function open(t) {
  const page = await browser.newPage({ viewport: { width: 384, height: 812 } });
  t.after(() => page.close());
  await page.addInitScript(() => {
    window.calls = [];
    window.handlers = {};
    window.activeHost = "A";
    window.packageRows = [
      { package: "com.catalog.video", name: "Catalog Video", system: false, enabled: true },
      { package: "com.example.sideload", name: "Sideload Player", system: false, enabled: true },
      { package: "com.android.settings", name: "Android Settings", system: true, enabled: false },
    ];
    window.device = (host) => ({
      id: 1,
      serial: `${host}:5555`,
      name: host,
      model: "Shield",
      status: "device",
      connection: "network",
      device_type: "shield",
      properties: null,
    });
    window.__TAURI_INTERNALS__ = {
      invoke: async (command, args) => {
        window.calls.push({ command, args });
        if (window.handlers[command]) return window.handlers[command](args);
        switch (command) {
          case "get_entitlement": return "pro";
          case "list_devices": return window.activeHost ? [window.device(window.activeHost)] : [];
          case "wireless_status": return { connected: true };
          case "wireless_connect":
            window.activeHost = args.host;
            return { ok: true, message: "connected" };
          case "wireless_disconnect":
            window.activeHost = "";
            return { ok: true, message: "disconnected" };
          case "wireless_cancel_connect": return;
          case "list_backups": return [
            {
              package: "com.example.complete",
              path: "/backups/complete",
              saved_at: "2026-09-05T13:00:00Z",
              size_bytes: 4096,
              apk_count: 3,
              complete: true,
            },
            {
              package: "com.example.legacy",
              path: "/backups/legacy",
              saved_at: "2026-09-05T12:00:00Z",
              size_bytes: 1024,
              apk_count: 1,
              complete: false,
            },
          ];
          case "list_installed_packages": return window.packageRows;
          case "backup_apk": return {
            package: args.package,
            path: `/backups/${args.package}`,
            saved_at: "2026-09-05T13:00:00Z",
            size_bytes: 4096,
            apk_count: 3,
            complete: true,
          };
          default: return { ok: true, message: "done" };
        }
      },
    };
  });
  await page.goto(origin);
  await page.evaluate(async () => {
    const { session } = await import("/src/lib/session.svelte.ts");
    const { router } = await import("/src/lib/router.svelte.ts");
    window.session = session;
    window.router = router;
    session.entitlement = "pro";
    await session.connect("A", 5555);
    router.reset("dashboard");
    router.navigate("backups");
  });
  await page.getByRole("heading", { name: "Backups" }).waitFor();
  return page;
}

test("installed picker hides systems by default and composes reveal with search", async (t) => {
  const page = await open(t);
  await page.getByRole("button", { name: "Choose an app" }).click();
  await page.getByRole("button", { name: /Catalog Video/ }).waitFor();

  assert.equal(await page.getByRole("button", { name: /Android Settings/ }).count(), 0);
  await page.getByRole("textbox", { name: /Search installed apps/ }).fill("SETTINGS");
  assert.equal(await page.getByText("No matching apps.").count(), 1);
  await page.getByRole("checkbox", { name: "Show system apps" }).check();
  await page.getByRole("button", { name: /Android Settings.*com\.android\.settings/ }).waitFor();
  assert.equal(await page.getByText("System app · Disabled").count(), 1);

  await page.getByRole("textbox", { name: /Search installed apps/ }).fill("com.example.side");
  assert.equal(await page.getByRole("button", { name: /Sideload Player/ }).count(), 1);
  assert.equal(await page.getByRole("button", { name: /Catalog Video/ }).count(), 0);
});

test("one chosen app makes one backup call for the expected TV and package", async (t) => {
  const page = await open(t);
  await page.getByRole("button", { name: "Choose an app" }).click();
  await page.getByRole("button", { name: /Back up Catalog Video/ }).click();
  await page.getByText(/Backed up Catalog Video.*com\.catalog\.video.*A:5555.*3 APK parts/).waitFor();

  const calls = await page.evaluate(() => window.calls.filter((call) => call.command === "backup_apk"));
  assert.deepEqual(calls, [{
    command: "backup_apk",
    args: { serial: "A:5555", package: "com.catalog.video" },
  }]);
});

test("a delayed A picker read cannot replace the newer B result", async (t) => {
  const page = await open(t);
  await page.evaluate(() => {
    window.rowsA = [{ package: "com.example.a", name: "Only on A", system: false, enabled: true }];
    window.rowsB = [{ package: "com.example.b", name: "Only on B", system: false, enabled: true }];
    window.handlers.list_installed_packages = ({ serial }) => {
      if (serial === "A:5555") {
        return new Promise((resolve) => { window.releaseA = resolve; });
      }
      return window.rowsB;
    };
  });
  await page.getByRole("button", { name: "Choose an app" }).click();
  await page.waitForFunction(() => typeof window.releaseA === "function");
  await page.evaluate(() => window.session.connect("B", 5555));
  await page.getByRole("button", { name: /Only on B/ }).waitFor();
  await page.evaluate(() => window.releaseA(window.rowsA));
  await page.evaluate(() => new Promise((resolve) => setTimeout(resolve, 30)));

  assert.equal(await page.getByRole("button", { name: /Only on A/ }).count(), 0);
  assert.equal(await page.getByRole("button", { name: /Only on B/ }).count(), 1);
  const reads = await page.evaluate(() => window.calls.filter((call) => call.command === "list_installed_packages"));
  assert.deepEqual(reads.map((call) => call.args.serial), ["A:5555", "B:5555"]);
});

test("a backup started on A stays owned by A when it finishes after a switch to B", async (t) => {
  const page = await open(t);
  await page.evaluate(() => {
    window.handlers.backup_apk = (args) => new Promise((resolve) => {
      window.releaseBackup = () => resolve({
        package: args.package,
        path: `/backups/${args.package}`,
        saved_at: "2026-09-05T14:00:00Z",
        size_bytes: 8192,
        apk_count: 4,
        complete: true,
      });
    });
  });
  await page.getByRole("button", { name: "Choose an app" }).click();
  await page.getByRole("button", { name: /Back up Catalog Video/ }).click();
  await page.waitForFunction(() => typeof window.releaseBackup === "function");
  await page.evaluate(() => window.session.connect("B", 5555));
  await page.getByRole("textbox", { name: /Search installed apps/ }).waitFor();
  await page.evaluate(() => window.releaseBackup());
  await page.getByText(/Backed up Catalog Video.*A:5555.*4 APK parts/).waitFor();

  assert.equal(await page.getByRole("textbox", { name: /Search installed apps/ }).count(), 1);
  const calls = await page.evaluate(() => window.calls.filter((call) => call.command === "backup_apk"));
  assert.deepEqual(calls.map((call) => call.args.serial), ["A:5555"]);
});

test("a stale displayed A row cannot start a backup on B", async (t) => {
  const page = await open(t);
  await page.getByRole("button", { name: "Choose an app" }).click();
  await page.getByRole("button", { name: /Back up Catalog Video/ }).waitFor();
  await page.evaluate(() => {
    const staleButton = [...document.querySelectorAll("button")]
      .find((button) => button.getAttribute("aria-label")?.includes("Catalog Video"));
    window.session.connectedDevice = window.device("B");
    window.session.liveness = "live";
    staleButton.click();
  });
  await page.getByText(/app list is stale/i).waitFor();

  assert.equal(
    await page.evaluate(() => window.calls.filter((call) => call.command === "backup_apk").length),
    0,
  );
});

test("a restore confirmation opened on A cannot restore onto B", async (t) => {
  const page = await open(t);
  await page.locator(".restore-btn").click();
  await page.getByRole("heading", { name: "Restore com.example.complete?" }).waitFor();
  await page.evaluate(() => {
    const staleConfirm = document.querySelector(".dialog-card button.primary");
    window.session.connectedDevice = window.device("B");
    window.session.liveness = "live";
    staleConfirm.click();
  });
  await page.getByText(/TV connection changed.*Choose Restore again/).waitFor();

  assert.equal(
    await page.evaluate(() => window.calls.filter((call) => call.command === "restore_apk_backup").length),
    0,
  );
});

test("picker and backup failures stay explicit and device-owned", async (t) => {
  const page = await open(t);
  await page.evaluate(() => {
    window.handlers.list_installed_packages = () => { throw new Error("package read failed"); };
  });
  await page.getByRole("button", { name: "Choose an app" }).click();
  await page.getByText(/package read failed/).waitFor();

  await page.evaluate(() => {
    delete window.handlers.list_installed_packages;
    window.handlers.backup_apk = () => { throw new Error("copy failed"); };
  });
  await page.getByRole("button", { name: "Retry" }).click();
  await page.getByRole("button", { name: /Back up Sideload Player/ }).click();
  await page.getByText(/Backup failed for Sideload Player.*com\.example\.sideload.*A:5555.*copy failed/).waitFor();
});

test("disconnected state and APK-only limits are stated plainly", async (t) => {
  const page = await open(t);
  await page.evaluate(() => window.session.disconnect());

  await page.getByText("Connect a TV to choose an installed app.").waitFor();
  assert.equal(await page.getByRole("button", { name: "Choose an app" }).isDisabled(), true);
  await page.getByText(/base APK and every installed split APK/).waitFor();
  await page.getByText(/App data, settings, and sign-in details are not included/).waitFor();
  const systemWarning = await page.locator(".system-note").textContent();
  assert.match(systemWarning, /system APK restore may[\s\S]*Android signatures[\s\S]*system dependencies/);
  assert.match(systemWarning, /not a full-TV[\s\S]*recovery backup/);
  assert.match(
    await page.locator(".drive-note").textContent(),
    /Exporting, file sharing, and cloud sync are[\s\S]*not available/,
  );
  await page.getByText("Base only").waitFor();
});
