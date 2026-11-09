import assert from "node:assert/strict";
import { before, after, test } from "node:test";
import { chromium } from "playwright";
import { startViteServer } from "./helpers/vite-harness.mjs";

// Upload to TV (Files) and Install APK against a mocked Tauri layer (EXPORT-DISCOVERABILITY.md
// phase 2). The mock stands in for the picker and the TV, so these cover the UI's requests and
// its truthful states, not Android's picker or a real install.
let server, origin, browser;
before(async () => { ({ server, origin } = await startViteServer()); browser = await chromium.launch({ headless: true }); });
after(async () => { await browser?.close(); await server?.close(); });

async function open(t, screen, options = {}) {
  const page = await browser.newPage({ viewport: { width: options.width ?? 384, height: 812 }, ...(options.record ? { recordVideo: { dir: options.record, size: { width: 384, height: 812 } } } : {}) });
  page.setDefaultTimeout(10000); t.after(() => page.close());
  await page.addInitScript(() => {
    localStorage.clear(); window.calls = [];
    window.mock = {
      pick: { status: "picked", batch: "import-1", files: [{ name: "clip.mp4", size_bytes: 2048 }, { name: "notes.txt", size_bytes: 10 }] },
      conflicts: [],
      upload: {},
      uploadHold: null,
      apkPick: { status: "picked", batch: "import-apk", files: [{ name: "base.apk", size_bytes: 4096 }, { name: "split_config.arm64_v8a.apk", size_bytes: 2048 }] },
      inspection: { files: [{ name: "base.apk", size_bytes: 4096 }, { name: "split_config.arm64_v8a.apk", size_bytes: 2048 }], abis: ["arm64-v8a"], abis_known: true, device_abis: ["arm64-v8a", "armeabi-v7a"], abi_compatible: true },
      install: { ok: true, message: "Success", hint: null },
    };
    window.__TAURI_INTERNALS__ = { invoke: async (command, args) => {
      calls.push({ command, args });
      const reply = (value) => { if (typeof value === "string") throw value; return value; };
      switch (command) {
        case "get_entitlement": return "free";
        case "wireless_connect": return { ok: true };
        case "wireless_status": return { connected: true };
        case "list_devices": return [{ id: 1, serial: "MOCK:5555", name: "Synthetic TV", model: "Fixture", status: "device", connection: "network", device_type: "shield", tv_evidence: "tv", properties: { android_release: "11", serial_number: "SYNTHETIC-HARDWARE" } }];
        case "health_report": return { ram: {}, storage: {}, display: {}, top_memory: [] };
        case "app_list_for_device": case "list_installed_packages": case "list_backups": return [];
        case "package_states": case "app_memory_map": case "app_usage_map": case "app_storage_map": return {};
        case "list_remote_dir": return args.path === "/sdcard" ? [{ name: "Movies", is_dir: true, is_symlink: false, size_bytes: 0, modified: "" }, { name: "old.txt", is_dir: false, is_symlink: false, size_bytes: 3, modified: "" }] : [];
        case "import_pick": return reply(args.purpose === "apk" ? mock.apkPick : mock.pick);
        case "discard_import": return null;
        case "upload_conflicts": return reply(mock.conflicts);
        case "upload_imported": {
          if (mock.uploadHold) await mock.uploadHold;
          const file = (args.batch === mock.pick.batch ? mock.pick.files : [])[args.index];
          const custom = mock.upload[file?.name];
          if (custom !== undefined) return reply(custom);
          return { status: "uploaded", name: file.name, remote_path: `${args.remoteDir}/${file.name}`, size_bytes: file.size_bytes, verified: true };
        }
        case "inspect_imported_apks": return reply(mock.inspection);
        case "install_imported_apks": return reply(mock.install);
        case "plugin:opener|open_url": return null;
        default: return { ok: true, message: "done" };
      }
    } };
  });
  await page.goto(origin);
  await page.evaluate(async ({ screen, scale }) => {
    const { session } = await import("/src/lib/session.svelte.ts"); const { router } = await import("/src/lib/router.svelte.ts");
    window.router = router; await session.connect("MOCK", 5555); router.reset("dashboard");
    router.navigate(screen);
    if (scale) document.documentElement.style.fontSize = `${16 * scale}px`;
  }, { screen, scale: options.scale });
  return page;
}

const called = (page, command) => page.evaluate((command) => calls.filter(c => c.command === command).map(c => c.args), command);
const uploadButton = page => page.getByRole("button", { name: "Upload to this folder" });
const fitsWidth = page => page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth);

test("Upload sends each picked file to the open folder, lists it and frees the copies", async t => {
  const page = await open(t, "files");
  await page.locator(".file-row").filter({ hasText: "old.txt" }).waitFor();
  await uploadButton(page).click();
  await page.getByText("Uploaded this session", { exact: true }).waitFor();
  await page.getByText("Uploaded 2 files to /sdcard.").waitFor();
  assert.deepEqual(await called(page, "import_pick"), [{ purpose: "upload" }]);
  assert.deepEqual(await called(page, "upload_conflicts"), [{ serial: "MOCK:5555", batch: "import-1", remoteDir: "/sdcard" }]);
  assert.deepEqual(await called(page, "upload_imported"), [
    { serial: "MOCK:5555", batch: "import-1", index: 0, remoteDir: "/sdcard", overwrite: false },
    { serial: "MOCK:5555", batch: "import-1", index: 1, remoteDir: "/sdcard", overwrite: false },
  ]);
  assert.deepEqual(await called(page, "discard_import"), [{ batch: "import-1" }]);
  await page.locator(".dl-row").filter({ hasText: "On the TV: /sdcard/clip.mp4" }).waitFor();
  // The folder is read again so the new files show.
  assert.equal((await called(page, "list_remote_dir")).filter(a => a.path === "/sdcard").length, 2);
});

test("progress names the file being sent, and the button waits for the upload", async t => {
  const page = await open(t, "files");
  await page.evaluate(() => { mock.uploadHold = new Promise(resolve => { window.releaseUpload = resolve; }); });
  await uploadButton(page).click();
  await page.getByRole("status").filter({ hasText: "Uploading 1 of 2: clip.mp4…" }).waitFor();
  assert.equal(await uploadButton(page).isDisabled(), true);
  await page.evaluate(() => releaseUpload());
  await page.getByText("Uploaded 2 files to /sdcard.").waitFor();
  assert.equal(await uploadButton(page).isEnabled(), true);
});

test("a dismissed picker uploads nothing", async t => {
  const page = await open(t, "files");
  await page.evaluate(() => { mock.pick = { status: "canceled" }; });
  await uploadButton(page).click();
  await page.getByText("No files picked. Nothing was uploaded.").waitFor();
  assert.equal((await called(page, "upload_conflicts")).length, 0);
  assert.equal((await called(page, "upload_imported")).length, 0);
});

test("a file already on the TV is replaced only after Replace, and only that file", async t => {
  const page = await open(t, "files");
  await page.evaluate(() => { mock.conflicts = [{ name: "notes.txt", remote_path: "/sdcard/notes.txt", is_dir: false }]; });
  await uploadButton(page).click();
  const dialog = page.getByRole("dialog", { name: "Replace files on the TV?" });
  await dialog.getByText('"notes.txt" is already in /sdcard.').waitFor();
  await dialog.getByText("The TV's copies will be overwritten. This cannot be undone.").waitFor();
  await dialog.getByRole("button", { name: "Don't upload" }).click();
  await page.getByText("Nothing was uploaded. The TV's files are unchanged.").waitFor();
  assert.equal((await called(page, "upload_imported")).length, 0);
  assert.deepEqual(await called(page, "discard_import"), [{ batch: "import-1" }]);

  await uploadButton(page).click();
  await dialog.getByRole("button", { name: "Replace" }).click();
  await page.getByText("Uploaded 2 files to /sdcard.").waitFor();
  assert.deepEqual((await called(page, "upload_imported")).map(a => [a.index, a.overwrite]), [[0, false], [1, true]]);
});

test("a folder of the same name is never replaced, and a failed file doesn't stop the rest", async t => {
  const page = await open(t, "files");
  await page.evaluate(() => {
    mock.pick.files.push({ name: "Movies", size_bytes: 1 });
    mock.conflicts = [{ name: "Movies", remote_path: "/sdcard/Movies", is_dir: true }];
    mock.upload["clip.mp4"] = "Couldn't upload clip.mp4: transport error: connection reset";
  });
  await uploadButton(page).click();
  await page.getByText("Uploaded 1 of 3. See the details below.").waitFor();
  assert.equal(await page.getByRole("dialog", { name: "Replace files on the TV?" }).count(), 0);
  assert.deepEqual((await called(page, "upload_imported")).map(a => a.index), [0, 1]);
  const problems = page.locator(".upload-problems");
  await problems.getByText("A folder named Movies is already in /sdcard, so Movies was not uploaded.").waitFor();
  await problems.getByText(/Couldn't upload clip.mp4/).waitFor();
  await page.locator(".dl-row").filter({ hasText: "/sdcard/notes.txt" }).waitFor();
  assert.equal(await page.locator(".dl-row").filter({ hasText: "/sdcard/clip.mp4" }).count(), 0);
});

test("an unconfirmed size is said, and Upload is offered only under /sdcard", async t => {
  const page = await open(t, "files");
  await page.evaluate(() => { mock.pick.files = [mock.pick.files[0]]; mock.upload["clip.mp4"] = { status: "uploaded", name: "clip.mp4", remote_path: "/sdcard/clip.mp4", size_bytes: 2048, verified: false }; });
  await uploadButton(page).click();
  await page.locator(".dl-row").filter({ hasText: "size not confirmed by the TV" }).waitFor();
  await page.locator(".crumb").first().click();
  await page.getByText("This folder is empty.").waitFor();
  assert.equal(await uploadButton(page).count(), 0);
});

test("the Files help explains upload limits, replacing and what the app keeps", async t => {
  const page = await open(t, "files");
  await page.getByRole("button", { name: "About file transfers" }).click();
  const help = page.getByRole("dialog", { name: "About file transfers" });
  await help.getByText(/up to 32 files and 2 GiB at a time/).waitFor();
  await help.getByText(/you're asked before it's replaced; a folder is never replaced/).waitFor();
  await help.getByText(/doesn't keep access to the picked files/).waitFor();
  assert.equal((await called(page, "import_pick")).length, 0);
});

test("More lists Install APK", async t => {
  const page = await open(t, "more");
  await page.getByRole("button", { name: /Install APK/ }).click();
  await page.getByRole("heading", { name: "Install APK" }).waitFor();
});

test("Install APK shows what was picked and installs only on Install", async t => {
  const page = await open(t, "sideload");
  await page.getByRole("button", { name: "Pick APK files" }).click();
  const card = page.locator(".staged");
  await card.getByText("Ready to install · 2 files").waitFor();
  await card.getByText("split_config.arm64_v8a.apk").waitFor();
  await card.locator("dd").filter({ hasText: "arm64-v8a, armeabi-v7a" }).waitFor();
  await card.getByText("doesn't know where these files came from or what the app does").waitFor();
  assert.equal(await card.getByRole("alert").count(), 0);
  assert.deepEqual(await called(page, "import_pick"), [{ purpose: "apk" }]);
  assert.deepEqual(await called(page, "inspect_imported_apks"), [{ serial: "MOCK:5555", batch: "import-apk" }]);
  assert.equal((await called(page, "install_imported_apks")).length, 0);
  await card.getByRole("button", { name: "Install" }).click();
  await page.locator(".result").getByText("Installed on", { exact: false }).waitFor();
  assert.deepEqual(await called(page, "install_imported_apks"), [{ serial: "MOCK:5555", batch: "import-apk" }]);
  // The backend uses the batch up; the screen doesn't discard it again.
  assert.equal((await called(page, "discard_import")).length, 0);
});

test("a native-code mismatch warns, and an unreadable one claims nothing", async t => {
  const page = await open(t, "sideload");
  await page.evaluate(() => { mock.inspection = { ...mock.inspection, abis: ["x86_64"], abi_compatible: false }; });
  await page.getByRole("button", { name: "Pick APK files" }).click();
  await page.locator(".staged").getByRole("alert").getByText("ships native code for x86_64, and this TV reports arm64-v8a, armeabi-v7a").waitFor();
  await page.getByRole("button", { name: "Cancel" }).click();
  await page.getByText(/Canceled. Nothing was installed/).waitFor();
  assert.deepEqual(await called(page, "discard_import"), [{ batch: "import-apk" }]);

  await page.evaluate(() => { mock.inspection = { ...mock.inspection, abis: [], abis_known: false, abi_compatible: null }; });
  await page.getByRole("button", { name: "Pick APK files" }).click();
  const card = page.locator(".staged");
  await card.getByText("couldn't be read").waitFor();
  await card.getByText("Whether this app's native code matches the TV couldn't be checked.").waitFor();
  assert.equal(await card.getByRole("alert").count(), 0);
});

test("a refused install shows the reading, the hint and the package manager's reply", async t => {
  const page = await open(t, "sideload");
  await page.evaluate(() => { mock.install = { ok: false, message: "Failure [INSTALL_FAILED_VERSION_DOWNGRADE]", hint: "Installed version is newer than this APK." }; });
  await page.getByRole("button", { name: "Pick APK files" }).click();
  await page.locator(".staged").getByRole("button", { name: "Install" }).click();
  const result = page.locator(".result");
  await result.getByText("A newer version is already installed.").waitFor();
  await result.getByText("Installed version is newer than this APK.").waitFor();
  await result.locator("pre").getByText("Failure [INSTALL_FAILED_VERSION_DOWNGRADE]").waitFor();
  assert.equal(await result.getByText(/Installed on/).count(), 0);
});

test("a refused pick and a dismissed picker install nothing", async t => {
  const page = await open(t, "sideload");
  await page.evaluate(() => { mock.apkPick = "photo.png isn't an .apk file. Pick only .apk files to install. Nothing was imported."; });
  await page.getByRole("button", { name: "Pick APK files" }).click();
  await page.getByRole("status").getByText(/isn't an .apk file/).waitFor();
  await page.evaluate(() => { mock.apkPick = { status: "canceled" }; });
  await page.getByRole("button", { name: "Pick APK files" }).click();
  await page.getByText("No files picked. Nothing was installed.").waitFor();
  assert.equal((await called(page, "inspect_imported_apks")).length, 0);
  assert.equal((await called(page, "install_imported_apks")).length, 0);
});

test("leaving Install APK frees picked files that weren't installed", async t => {
  const page = await open(t, "sideload");
  await page.getByRole("button", { name: "Pick APK files" }).click();
  await page.locator(".staged").waitFor();
  await page.locator(".topline").getByRole("button", { name: "Back" }).click();
  await page.waitForFunction(() => calls.some(c => c.command === "discard_import"));
  assert.deepEqual(await called(page, "discard_import"), [{ batch: "import-apk" }]);
});

test("popular sideloads open their official pages in the browser", async t => {
  const page = await open(t, "sideload");
  await page.getByRole("button", { name: "Open the official download page for SmartTube" }).click();
  await page.waitForFunction(() => calls.some(c => c.command === "plugin:opener|open_url"));
  const opened = await called(page, "plugin:opener|open_url");
  assert.equal(opened[0].url, "https://github.com/yuliskov/SmartTube");
});

test("Install APK help explains the flow without claiming the app is safe", async t => {
  const page = await open(t, "sideload");
  await page.getByRole("button", { name: "About installing APKs" }).click();
  const help = page.getByRole("dialog", { name: "About installing APKs" });
  await help.getByText(/Nothing installs until you confirm/).waitFor();
  await help.getByText(/every part of a split app/).waitFor();
  await help.getByText(/doesn't check\s+the signature or what the app does/).waitFor();
  assert.equal(await page.getByText(/\bsafe\b/i).count(), 0);
});

for (const [width, scale] of [[384, 1], [320, 2]]) test(`upload and install fit at ${width}px / ${scale * 100}% text`, async t => {
  const page = await open(t, "files", { width, scale });
  await uploadButton(page).click();
  await page.getByText("Uploaded this session", { exact: true }).waitFor();
  assert.equal(await fitsWidth(page), true, "Files after an upload");
  await page.evaluate(() => router.navigate("sideload"));
  await page.getByRole("button", { name: "Pick APK files" }).click();
  await page.locator(".staged").waitFor();
  assert.equal(await fitsWidth(page), true, "Install APK with a staged set");
  const viewport = await page.evaluate(() => document.documentElement.clientWidth);
  for (const box of [await page.locator(".staged").boundingBox(), await page.locator(".catalog-row").first().boundingBox()]) {
    assert.ok(box.x + box.width <= viewport + 1, "card fits the viewport");
  }
});

// IMPORT_EVIDENCE=<dir> records the simulated flows for PR evidence (fixture, not a device).
if (process.env.IMPORT_EVIDENCE) test("record simulated Upload to TV / Install APK evidence", async t => {
  const dir = process.env.IMPORT_EVIDENCE; const page = await open(t, "files", { record: dir });
  const pause = () => page.waitForTimeout(900);
  await page.locator(".file-row").filter({ hasText: "old.txt" }).waitFor(); await pause();
  await page.evaluate(() => { mock.conflicts = [{ name: "notes.txt", remote_path: "/sdcard/notes.txt", is_dir: false }]; });
  await uploadButton(page).click();
  await page.getByRole("dialog", { name: "Replace files on the TV?" }).waitFor(); await pause(); await pause();
  await page.getByRole("button", { name: "Replace" }).click();
  await page.getByText("Uploaded this session", { exact: true }).waitFor();
  await page.screenshot({ path: `${dir}/files-upload-simulated.png` });
  await page.locator(".dl-list").scrollIntoViewIfNeeded(); await pause(); await pause();
  await page.evaluate(() => router.navigate("sideload")); await pause();
  await page.getByRole("button", { name: "Pick APK files" }).click();
  await page.locator(".staged").waitFor(); await pause();
  await page.screenshot({ path: `${dir}/install-apk-ready-simulated.png` }); await pause();
  await page.locator(".staged").getByRole("button", { name: "Install" }).click();
  await page.locator(".result").waitFor(); await pause();
  await page.screenshot({ path: `${dir}/install-apk-installed-simulated.png` }); await pause();
  const video = page.video(); await page.close(); await video.saveAs(`${dir}/import-simulated.webm`);
});
