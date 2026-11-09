import assert from "node:assert/strict";
import { before, after, test } from "node:test";
import { chromium } from "playwright";
import { startViteServer } from "./helpers/vite-harness.mjs";

// Save a copy / Share (EXPORT-DISCOVERABILITY.md phase 1) against a mocked Tauri layer. The
// mock stands in for export_file, so these cover the UI's requests and its truthful states,
// not Android's picker or share sheet.
let server, origin, browser;
before(async () => { ({ server, origin } = await startViteServer()); browser = await chromium.launch({ headless: true }); });
after(async () => { await browser?.close(); await server?.close(); });

const shotPath = "/data/user/0/com.atvoptimizer.mobile/screenshots/192-168-1-20-5555_2026-11-08_12-30-05.png";
const filePath = "/data/user/0/com.atvoptimizer.mobile/downloads/report.txt";

async function open(t, screen = "dashboard", options = {}) {
  const page = await browser.newPage({ viewport: { width: options.width ?? 384, height: 812 }, ...(options.record ? { recordVideo: { dir: options.record, size: { width: 384, height: 812 } } } : {}) });
  page.setDefaultTimeout(10000); t.after(() => page.close());
  await page.addInitScript(({ shotPath, filePath }) => {
    localStorage.clear(); window.calls = []; window.exportReply = { status: "saved", name: "tv-screenshot-2026-11-08_12-30-05.png" }; window.exportHold = null;
    window.__TAURI_INTERNALS__ = { invoke: async (command, args) => {
      calls.push({ command, args });
      switch (command) {
        case "get_entitlement": return "pro";
        case "wireless_connect": return { ok: true };
        case "wireless_status": return { connected: true };
        case "list_devices": return [{ id: 1, serial: "MOCK:5555", name: "Synthetic TV", model: "Fixture", status: "device", connection: "network", device_type: "shield", tv_evidence: "tv", properties: { android_release: "11", serial_number: "SYNTHETIC-HARDWARE" } }];
        case "health_report": return { ram: {}, storage: {}, display: {}, top_memory: [] };
        case "app_list_for_device": case "list_installed_packages": return [];
        case "package_states": case "app_memory_map": case "app_usage_map": case "app_storage_map": return {};
        case "take_screenshot": return { path: shotPath, base64: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==" };
        case "list_remote_dir": return [{ name: "report.txt", is_dir: false, is_symlink: false, size_bytes: 10, modified: "2026-11-08 12:00" }];
        case "pull_file": return { name: "report.txt", path: filePath, size_bytes: 10 };
        case "export_file": {
          if (window.exportHold) await window.exportHold;
          const reply = window.exportReply;
          if (typeof reply === "string") throw reply;
          return reply;
        }
        default: return { ok: true, message: "done" };
      }
    } };
  }, { shotPath, filePath });
  await page.goto(origin);
  await page.evaluate(async ({ screen, scale }) => {
    const { session } = await import("/src/lib/session.svelte.ts"); const { router } = await import("/src/lib/router.svelte.ts");
    window.router = router; session.entitlement = "pro"; await session.connect("MOCK", 5555); router.reset("dashboard");
    if (screen !== "dashboard") router.navigate(screen);
    if (scale) document.documentElement.style.fontSize = `${16 * scale}px`;
  }, { screen, scale: options.scale });
  return page;
}

async function preview(page) {
  await page.locator(".action-btn").filter({ hasText: "Screenshot" }).click();
  const sheet = page.getByRole("dialog", { name: "Screenshot", exact: true }); await sheet.waitFor(); return sheet;
}
const exports = page => page.evaluate(() => calls.filter(c => c.command === "export_file").map(c => c.args));

test("screenshot Save a copy reports the provider's name only after the backend says saved", async t => {
  const page = await open(t); const sheet = await preview(page);
  await sheet.getByRole("button", { name: "Save a copy", exact: true }).click();
  await sheet.getByRole("status").getByText("Saved a copy as tv-screenshot-2026-11-08_12-30-05.png.", { exact: true }).waitFor();
  assert.deepEqual(await exports(page), [{ path: shotPath, mode: "save" }]);
});

test("backing out of the picker says nothing was saved", async t => {
  const page = await open(t); await page.evaluate(() => { exportReply = { status: "canceled" }; });
  const sheet = await preview(page); await sheet.getByRole("button", { name: "Save a copy", exact: true }).click();
  await sheet.getByRole("status").getByText("Not saved.", { exact: true }).waitFor();
  assert.equal(await sheet.getByText(/Saved a copy/).count(), 0);
});

test("Share claims only that the share sheet opened", async t => {
  const page = await open(t); await page.evaluate(() => { exportReply = { status: "share_opened" }; });
  const sheet = await preview(page); await sheet.getByRole("button", { name: "Share", exact: true }).click();
  await sheet.getByRole("status").getByText("Share sheet opened.", { exact: true }).waitFor();
  assert.deepEqual(await exports(page), [{ path: shotPath, mode: "share" }]);
});

test("an export error is shown and the buttons come back", async t => {
  const page = await open(t); await page.evaluate(() => { exportReply = "Only TV screenshots and files copied from the TV can be saved or shared."; });
  const sheet = await preview(page); await sheet.getByRole("button", { name: "Share", exact: true }).click();
  await sheet.getByRole("status").getByText(/Only TV screenshots/).waitFor();
  assert.equal(await sheet.getByRole("button", { name: "Share", exact: true }).isEnabled(), true);
  assert.equal(await sheet.getByText(/Share sheet opened|Saved a copy/).count(), 0);
});

test("one export at a time: both buttons wait while the picker is open", async t => {
  const page = await open(t);
  await page.evaluate(() => { window.exportHold = new Promise(resolve => { window.releaseExport = resolve; }); });
  const sheet = await preview(page); await sheet.getByRole("button", { name: "Save a copy", exact: true }).click();
  await sheet.getByRole("button", { name: "Opening…" }).waitFor();
  for (const button of await sheet.locator(".export-btn").all()) assert.equal(await button.isDisabled(), true);
  await sheet.locator(".export-btn").last().click({ force: true });
  assert.equal((await exports(page)).length, 1);
  await page.evaluate(() => releaseExport());
  await sheet.getByRole("status").getByText(/Saved a copy as/).waitFor();
  for (const button of await sheet.locator(".export-btn").all()) assert.equal(await button.isEnabled(), true);
});

test("the help explains where copies go and that nothing leaves on its own", async t => {
  const page = await open(t); const sheet = await preview(page);
  await sheet.getByRole("button", { name: "About saving and sharing" }).click();
  const help = page.getByRole("dialog", { name: "About saving and sharing" });
  await help.getByText(/Nothing leaves the phone unless you choose Save a copy or Share/).waitFor();
  await help.getByText(/without the TV's address/).waitFor();
  await help.getByText(/Android doesn't tell ATV Optimizer whether that app finished/).waitFor();
  await help.getByRole("button", { name: "Done" }).click(); await help.waitFor({ state: "detached" });
  assert.equal((await exports(page)).length, 0);
});

for (const [width, scale] of [[384, 1], [320, 2]]) test(`a copied file's row offers Save a copy and Share at ${width}px / ${scale * 100}% text`, async t => {
  const page = await open(t, "files", { width, scale });
  await page.locator(".file-row").filter({ hasText: "report.txt" }).click();
  const row = page.locator(".dl-row").filter({ hasText: "report.txt" }); await row.waitFor();
  assert.equal((await exports(page)).length, 0);
  await page.evaluate(() => { exportReply = { status: "saved", name: "report.txt" }; });
  await row.getByRole("button", { name: "Save a copy of report.txt" }).click();
  await row.getByRole("status").getByText("Saved a copy as report.txt.", { exact: true }).waitFor();
  await page.evaluate(() => { exportReply = { status: "share_opened" }; });
  await row.getByRole("button", { name: "Share report.txt" }).click();
  await row.getByRole("status").getByText("Share sheet opened.", { exact: true }).waitFor();
  assert.deepEqual(await exports(page), [{ path: filePath, mode: "save" }, { path: filePath, mode: "share" }]);
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth), true);
  const box = await row.boundingBox(); const page_width = await page.evaluate(() => document.documentElement.clientWidth);
  assert.ok(box.x + box.width <= page_width + 1, "row fits the viewport");
});

// EXPORT_ACTIONS_EVIDENCE=<dir> records the simulated flow for PR evidence (fixture, not a device).
if (process.env.EXPORT_ACTIONS_EVIDENCE) test("record simulated Save a copy / Share evidence", async t => {
  const dir = process.env.EXPORT_ACTIONS_EVIDENCE; const page = await open(t, "dashboard", { record: dir });
  const pause = () => page.waitForTimeout(900);
  const sheet = await preview(page); await pause();
  await sheet.getByRole("button", { name: "Save a copy", exact: true }).click();
  await sheet.getByRole("status").getByText(/Saved a copy as/).waitFor(); await pause();
  await page.evaluate(() => { exportReply = { status: "canceled" }; });
  await sheet.getByRole("button", { name: "Save a copy", exact: true }).click();
  await sheet.getByRole("status").getByText("Not saved.").waitFor(); await pause();
  await page.evaluate(() => { exportReply = { status: "share_opened" }; });
  await sheet.getByRole("button", { name: "Share", exact: true }).click();
  await sheet.getByRole("status").getByText("Share sheet opened.").waitFor(); await pause();
  await sheet.getByRole("button", { name: "About saving and sharing" }).click(); await pause(); await pause();
  await page.getByRole("dialog", { name: "About saving and sharing" }).getByRole("button", { name: "Done" }).click();
  await sheet.getByRole("button", { name: "Close screenshot preview" }).click();
  await page.evaluate(() => router.navigate("files"));
  await page.locator(".file-row").filter({ hasText: "report.txt" }).click();
  const row = page.locator(".dl-row").filter({ hasText: "report.txt" }); await row.scrollIntoViewIfNeeded(); await pause();
  await page.evaluate(() => { exportReply = { status: "saved", name: "report.txt" }; });
  await row.getByRole("button", { name: "Save a copy of report.txt" }).click();
  await row.getByRole("status").getByText(/Saved a copy as/).waitFor(); await row.scrollIntoViewIfNeeded(); await pause(); await pause();
  const video = page.video(); await page.close(); await video.saveAs(`${dir}/export-actions-simulated.webm`);
});
