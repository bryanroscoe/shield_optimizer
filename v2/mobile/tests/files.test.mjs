import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { after, before, test } from "node:test";
import { chromium } from "playwright";
import { startViteServer } from "./helpers/vite-harness.mjs";

let server;
let browser;
let origin;
let appFiles;

const SMARTTUBE_DIRS = [
  "/sdcard/Documents/SmartTubeBackup",
  "/sdcard/SmartTubeBackup",
  "/sdcard/Android/data/com.teamsmart.videomanager.tv",
];
const HIT = "/sdcard/Documents/SmartTubeBackup/org.smarttube.stable_20260908.zip";

before(async () => {
  ({ server, origin } = await startViteServer());
  appFiles = await server.ssrLoadModule("/src/lib/appFiles.ts");
  browser = await chromium.launch({ headless: true });
});

after(async () => {
  await browser?.close();
  await server?.close();
});

test("the catalog is desktop's file, not a copy", () => {
  const desktop = JSON.parse(
    readFileSync(new URL("../../src/lib/app-files-catalog.json", import.meta.url), "utf8"),
  );
  assert.deepEqual(appFiles.appFilesCatalog, desktop);
  const smartTube = appFiles.appFilesCatalog.find((entry) => entry.id === "smarttube");
  assert.deepEqual(smartTube.search_dirs, SMARTTUBE_DIRS);
  assert.equal(smartTube.pattern, "*.zip");
});

test("a search that could not run never summarizes as no matches", () => {
  assert.deepEqual(appFiles.summarizeFind({ hits: [], unsearched: [] }), { kind: "none" });
  assert.deepEqual(appFiles.summarizeFind({ hits: [], unsearched: ["/sdcard/x"] }), {
    kind: "unsearched",
    unsearched: ["/sdcard/x"],
  });
  assert.deepEqual(appFiles.summarizeFind({ hits: [HIT], unsearched: ["/sdcard/x"] }), {
    kind: "found",
    hits: [HIT],
    unsearched: ["/sdcard/x"],
  });
});

test("delete is only offered strictly inside /sdcard", () => {
  for (const ok of ["/sdcard/old.zip", "/sdcard/Download/a b", "/sdcard/Documents"]) {
    assert.equal(appFiles.canDeleteOnTv(ok), true, ok);
  }
  for (const bad of [
    "/sdcard",
    "/sdcard/",
    "/sdcardX/a",
    "/storage/emulated/0/old.zip",
    "/data/local/tmp/a",
    "/sdcard/../data",
    "/sdcard/a\nb",
    "relative",
  ]) {
    assert.equal(appFiles.canDeleteOnTv(bad), false, JSON.stringify(bad));
  }
  assert.equal(appFiles.parentDir(HIT), "/sdcard/Documents/SmartTubeBackup");
  assert.equal(appFiles.parentDir("/sdcard"), "/");
});

async function open(t) {
  const page = await browser.newPage({ viewport: { width: 384, height: 812 } });
  t.after(() => page.close());
  await page.addInitScript(() => {
    window.calls = [];
    window.handlers = {};
    window.activeHost = "A";
    window.listing = {
      "/sdcard": [
        { name: "Download", is_dir: true, is_symlink: false, size_bytes: 0, modified: "" },
        { name: "old.zip", is_dir: false, is_symlink: false, size_bytes: 2048, modified: "" },
      ],
      "/": [
        { name: "sdcard", is_dir: true, is_symlink: true, size_bytes: 0, modified: "" },
        { name: "system", is_dir: true, is_symlink: false, size_bytes: 0, modified: "" },
      ],
      "/sdcard/Documents/SmartTubeBackup": [
        { name: "org.smarttube.stable_20260908.zip", is_dir: false, is_symlink: false, size_bytes: 10, modified: "" },
      ],
    };
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
          case "list_remote_dir": return window.listing[args.path] ?? [];
          case "pull_file": {
            const name = args.remotePath.slice(args.remotePath.lastIndexOf("/") + 1);
            return { name, path: `/app/downloads/${name}`, size_bytes: 10 };
          }
          case "delete_path": return { ok: true, message: `Deleted ${args.path}.` };
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
    router.navigate("files");
  });
  await page.getByRole("heading", { name: "Files" }).waitFor();
  await page.getByText("old.zip").waitFor();
  return page;
}

const callsOf = (page, command) =>
  page.evaluate((c) => window.calls.filter((call) => call.command === c), command);

test("Find SmartTube backups searches every catalog dir and copies a hit to the phone", async (t) => {
  const page = await open(t);
  await page.evaluate((hit) => {
    window.handlers.find_files = () => ({ hits: [hit], unsearched: [] });
  }, HIT);
  await page.getByRole("button", { name: "Find app backups" }).click();
  await page.getByRole("button", { name: "Find SmartTube backups", exact: true }).click();
  await page.getByText(HIT).waitFor();

  assert.deepEqual(await callsOf(page, "find_files"), [
    { command: "find_files", args: { serial: "A:5555", dirs: SMARTTUBE_DIRS, pattern: "*.zip" } },
  ]);

  await page.getByRole("button", { name: "Copy org.smarttube.stable_20260908.zip to this phone" }).click();
  await page.getByText(/Copied org\.smarttube\.stable_20260908\.zip into this app/).waitFor();
  assert.deepEqual(
    (await callsOf(page, "pull_file")).map((call) => call.args),
    [{ serial: "A:5555", remotePath: HIT }],
  );

  await page.getByRole("button", { name: "Open folder" }).click();
  await page.waitForFunction(() =>
    window.calls.some((call) => call.command === "list_remote_dir"
      && call.args.path === "/sdcard/Documents/SmartTubeBackup"));
});

test("an unsearched directory reads as a failed search, never as no matches (#86)", async (t) => {
  const page = await open(t);
  await page.evaluate((dirs) => {
    window.handlers.find_files = () => ({ hits: [], unsearched: dirs });
  }, SMARTTUBE_DIRS);
  await page.getByRole("button", { name: "Find app backups" }).click();
  await page.getByRole("button", { name: "Find SmartTube backups", exact: true }).click();
  await page.getByText(/Couldn't search \/sdcard\/Documents\/SmartTubeBackup/).waitFor();
  assert.equal(await page.getByText(/No matches/).count(), 0);

  await page.evaluate(() => {
    window.handlers.find_files = () => ({ hits: [], unsearched: [] });
  });
  await page.getByRole("button", { name: "Find SmartTube backups", exact: true }).click();
  await page.getByText(/No matches — export from the app first/).waitFor();
  assert.equal(await page.getByText(/Couldn't search/).count(), 0);
});

test("a rejected search drops the older answer instead of showing it as current", async (t) => {
  const page = await open(t);
  await page.evaluate(() => {
    window.handlers.find_files = () => ({ hits: [], unsearched: [] });
  });
  await page.getByRole("button", { name: "Find app backups" }).click();
  await page.getByRole("button", { name: "Find SmartTube backups", exact: true }).click();
  await page.getByText(/No matches/).waitFor();
  await page.evaluate(() => {
    window.handlers.find_files = () => Promise.reject("Connection to the TV was lost");
  });
  await page.getByRole("button", { name: "Find SmartTube backups", exact: true }).click();
  await page.waitForFunction(() => !document.body.innerText.includes("No matches"));
});

test("delete asks first, sends the exact /sdcard path, and reloads the folder", async (t) => {
  const page = await open(t);
  await page.getByRole("button", { name: "Delete old.zip from the TV" }).click();
  await page.getByText("Delete file from the TV?").waitFor();
  await page.getByRole("button", { name: "Cancel" }).click();
  assert.equal((await callsOf(page, "delete_path")).length, 0);

  const listsBefore = (await callsOf(page, "list_remote_dir")).length;
  await page.getByRole("button", { name: "Delete old.zip from the TV" }).click();
  await page.getByRole("button", { name: "Delete", exact: true }).click();
  await page.getByText("Deleted /sdcard/old.zip.").waitFor();
  assert.deepEqual(
    (await callsOf(page, "delete_path")).map((call) => call.args),
    [{ serial: "A:5555", path: "/sdcard/old.zip" }],
  );
  await page.waitForFunction(
    (n) => window.calls.filter((call) => call.command === "list_remote_dir").length > n,
    listsBefore,
  );

  await page.getByRole("button", { name: "Delete Download from the TV" }).click();
  await page.getByText("Delete folder from the TV?").waitFor();
});

test("no delete control outside /sdcard", async (t) => {
  const page = await open(t);
  await page.locator(".crumbs .crumb").first().click();
  await page.getByText("system").waitFor();
  assert.equal(await page.getByRole("button", { name: /from the TV$/ }).count(), 0);
});

test("a delete confirmed after a TV switch is not sent to the new TV", async (t) => {
  const page = await open(t);
  await page.getByRole("button", { name: "Delete old.zip from the TV" }).click();
  await page.getByText("Delete file from the TV?").waitFor();
  await page.evaluate(() => {
    window.session.connectedDevice = window.device("B");
  });
  // The switch clears the pending dialog; if it is still up, confirming it
  // must not delete anything.
  const dialogButton = page.getByRole("button", { name: "Delete", exact: true });
  if (await dialogButton.count()) await dialogButton.click();
  await page.evaluate(() => new Promise((resolve) => setTimeout(resolve, 50)));
  assert.equal((await callsOf(page, "delete_path")).length, 0);
});
