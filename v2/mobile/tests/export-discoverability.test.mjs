import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
import { before, after, test } from "node:test";
import { chromium } from "playwright";
import { startViteServer } from "./helpers/vite-harness.mjs";

let server, origin, browser;
before(async () => { ({ server, origin } = await startViteServer()); browser = await chromium.launch({ headless: true }); });
after(async () => { await browser?.close(); await server?.close(); });
const dir = "evidence/export-discoverability-2026-10-17";
const path = "/data/user/0/com.atvoptimizer.mobile/screenshots/synthetic-tv_2026-10-17_12-00-00.png";
async function open(t, screen = "dashboard", options = {}) {
  const page = await browser.newPage({ viewport: { width: options.width ?? 384, height: 812 }, ...(options.record ? { recordVideo: { dir: `${dir}/raw`, size: { width: 384, height: 812 } } } : {}) });
  page.setDefaultTimeout(10000); t.after(() => page.close());
  await page.addInitScript(({ path }) => {
    localStorage.clear(); window.calls = []; window.copied = []; window.savedFiles = {}; window.failShot = false; window.failPull = false; window.failCopy = false;
    const canvas = document.createElement("canvas"); canvas.width = 480; canvas.height = 270;
    const ctx = canvas.getContext("2d"); ctx.fillStyle = "#16191e"; ctx.fillRect(0, 0, 480, 270);
    ctx.fillStyle = "#c9f24e"; ctx.font = "bold 25px sans-serif"; ctx.fillText("SIMULATED TV SCREEN", 35, 110);
    ctx.fillStyle = "#c7cdd6"; ctx.font = "18px sans-serif"; ctx.fillText("Fixture only · no device connected", 35, 150);
    window.shot = { path, base64: canvas.toDataURL("image/png").split(",")[1] };
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText: async text => { if (failCopy) throw Error("Clipboard unavailable"); copied.push(text); } } });
    document.execCommand = () => false;
    window.__TAURI_INTERNALS__ = { invoke: async (command, args) => {
      calls.push({ command, args });
      switch (command) {
        case "get_entitlement": return "pro";
        case "wireless_connect": return { ok: true };
        case "wireless_status": return { connected: true };
        case "list_devices": return [{ id: 1, serial: "MOCK:5555", name: "Synthetic TV", model: "Fixture", status: "device", connection: "network", device_type: "shield", tv_evidence: "tv", properties: { android_release: "11", serial_number: "SYNTHETIC-HARDWARE" } }];
        case "health_report": return { ram: {}, storage: {}, display: {}, top_memory: [] };
        case "app_list_for_device": case "list_installed_packages": return [];
        case "package_states": return {};
        case "list_other_packages": return [{ package: "com.example.fixture", name: "Fixture app", enabled: true, system: false }];
        case "app_memory_map": case "app_usage_map": case "app_storage_map": return {};
        case "safety_info": return { kind: "unknown", reason: "Synthetic fixture", source: "no_record" };
        case "take_screenshot": if (failShot) throw "Screenshot failed (simulated)"; savedFiles[shot.path] = shot.base64; return { ...shot };
        case "list_remote_dir": return [{ name: "report.txt", is_dir: false, is_symlink: false, size_bytes: 10, modified: "2026-10-17 12:00" }];
        case "pull_file": if (failPull) throw "Copy failed (simulated)"; return { name: "report (2).txt", path: "/data/user/0/com.atvoptimizer.mobile/downloads/report (2).txt", size_bytes: 10 };
        case "read_debug_log": return "Synthetic log text";
        case "plugin:opener|open_url": return null;
        default: return { ok: true, message: "done" };
      }
    } };
  }, { path });
  await page.goto(origin);
  await page.evaluate(async ({ screen, scale }) => {
    const { session } = await import("/src/lib/session.svelte.ts"); const { router } = await import("/src/lib/router.svelte.ts");
    window.session = session; window.router = router; session.entitlement = "pro"; await session.connect("MOCK", 5555); router.reset("dashboard");
    if (screen !== "dashboard") router.navigate(screen);
    if (scale) document.documentElement.style.fontSize = `${16 * scale}px`;
  }, { screen, scale: options.scale });
  await page.getByRole("button", { name: /Screenshot/ }).waitFor({ state: screen === "dashboard" ? "visible" : "detached" });
  return page;
}
async function capture(page) { await page.locator(".action-btn").filter({ hasText: "Screenshot" }).click(); return page.getByRole("dialog", { name: "Screenshot", exact: true }); }

for (const [width, scale] of [[384, 1], [320, 2]]) test(`screenshot destination, bytes and Close are truthful at ${width}px / ${scale * 100}% text`, async t => {
  const page = await open(t, "dashboard", { width, scale }); const preview = await capture(page); await preview.waitFor();
  await preview.getByText(`Private app location: ${path}`, { exact: true }).waitFor();
  await preview.getByText(/not in Gallery or Downloads/).waitFor();
  await preview.getByText("Close screenshot preview hides this view. The saved image stays in the app.", { exact: true }).waitFor();
  assert.equal(await preview.getByRole("img").getAttribute("src"), await page.evaluate(() => `data:image/png;base64,${shot.base64}`));
  assert.equal(await preview.getByRole("button", { name: /Share|Open|Files|Save/ }).count(), 0);
  assert.equal(await preview.evaluate(el => el.scrollWidth <= el.clientWidth), true);
  if (process.env.EXPORT_COPY_EVIDENCE === "1") {
    await mkdir(dir, { recursive: true });
    await page.screenshot({ path: `${dir}/preview-${width}-${scale * 100}-simulated.png`, animations: "disabled" });
    if (scale > 1) {
      await preview.evaluate(el => { el.scrollTop = el.scrollHeight; });
      await page.screenshot({ path: `${dir}/preview-${width}-${scale * 100}-tail-simulated.png`, animations: "disabled" });
    }
  }
  await preview.getByRole("button", { name: "Close screenshot preview", exact: true }).click(); await preview.waitFor({ state: "detached" });
  assert.equal(await page.evaluate(() => savedFiles[shot.path] === shot.base64), true);
  assert.deepEqual(await page.evaluate(() => calls.filter(c => c.command === "take_screenshot")), [{ command: "take_screenshot", args: { serial: "MOCK:5555" } }]);
  assert.equal(await page.evaluate(() => calls.some(c => /delete|export|share|open_url/.test(c.command))), false);
});

test("screenshot failure creates no preview or false saved result and preserves earlier saved bytes", async t => {
  const page = await open(t); await page.evaluate(() => { failShot = true; });
  await page.locator(".action-btn").filter({ hasText: "Screenshot" }).click(); await page.getByText("Screenshot failed (simulated)", { exact: true }).waitFor();
  assert.equal(await page.getByRole("dialog", { name: "Screenshot" }).count(), 0); assert.deepEqual(await page.evaluate(() => savedFiles), {});
  await page.evaluate(() => { failShot = false; }); const preview = await capture(page); await preview.waitFor();
  await preview.getByRole("button", { name: "Close screenshot preview" }).click();
  await page.evaluate(() => { failShot = true; }); await page.locator(".action-btn").filter({ hasText: "Screenshot" }).click();
  await page.getByText("Screenshot failed (simulated)", { exact: true }).waitFor();
  assert.equal(await page.evaluate(() => savedFiles[shot.path] === shot.base64), true);
});

test("file-copy success identifies the actual private destination and failure adds no copied row", async t => {
  const page = await open(t, "files"); const file = page.locator(".file-row").filter({ hasText: "report.txt" }); await file.waitFor();
  await page.getByText(/not the phone's public Downloads/).waitFor();
  await page.evaluate(() => { failPull = true; }); await file.click(); await page.getByText("Copy failed (simulated)", { exact: true }).waitFor();
  assert.equal(await page.getByText("Copied this session", { exact: true }).count(), 0);
  await page.evaluate(() => { failPull = false; }); await file.click();
  await page.getByText("Copied report (2).txt into this app's private storage.", { exact: true }).waitFor();
  await page.getByText("10 B · Private app location: /data/user/0/com.atvoptimizer.mobile/downloads/report (2).txt", { exact: true }).waitFor();
  assert.equal(await page.locator(".dl-list").getByRole("button").count(), 0);
});

async function openReport(page) {
  await page.evaluate(() => router.navigate("apps")); const row = page.locator('.app-row[data-package="com.example.fixture"]'); await row.waitFor();
  await row.click({ button: "right" }); await page.getByRole("menuitem", { name: "Report this app" }).click();
  const report = page.getByRole("dialog", { name: "Report this app", exact: true }); await report.waitFor(); return report;
}
test("report copy and oversized GitHub fallback use preview text and never claim a saved file", async t => {
  const page = await open(t); const report = await openReport(page);
  await report.getByText(/Copy puts report text on your clipboard; it does not save a file/).waitFor();
  await report.getByRole("button", { name: "Show the full report" }).click();
  const text = await report.getByRole("textbox", { name: "Report preview" }).inputValue();
  await report.getByRole("button", { name: "Copy report" }).click(); await page.getByText("Report copied.", { exact: true }).waitFor();
  assert.equal(await page.evaluate(() => copied[0]), text);
  await report.getByPlaceholder("What you expected, or where you saw it").fill("🧪".repeat(500));
  const longText = await report.getByRole("textbox", { name: "Report preview" }).inputValue();
  await report.getByRole("button", { name: "Open GitHub issue" }).click(); await report.getByText(/This report is too long to fill in for you, so it's on your clipboard/).waitFor();
  assert.equal(await page.evaluate(() => copied.at(-1)), longText);
  assert.equal(await page.evaluate(() => calls.filter(c => c.command === "plugin:opener|open_url").length), 1);
});
test("report clipboard failure exposes the selectable preview without false copy success", async t => {
  const page = await open(t); const report = await openReport(page); await page.evaluate(() => { failCopy = true; });
  await report.getByRole("button", { name: "Copy report" }).click();
  await report.getByText(/Couldn't reach the clipboard/).waitFor(); await report.getByRole("textbox", { name: "Report preview" }).waitFor();
  assert.equal(await page.getByText("Report copied.", { exact: true }).count(), 0); assert.deepEqual(await page.evaluate(() => copied), []);
});

test("diagnostic JSON and debug log Copy actions state clipboard-only output and report failures", async t => {
  const page = await open(t);
  await page.evaluate(async () => {
    const { recordUnknownDiagnostic } = await import("/src/lib/unknownDiagnostics.ts");
    recordUnknownDiagnostic({ kind: "installed_package", token: "com.example.fixture", reason: "uncatalogued_package", appVersion: "0.1.0", deviceFamily: "shield" });
  });
  await page.waitForFunction(() => !!localStorage.getItem("atv.unknownDiagnostics.v1"));
  await page.evaluate(() => router.navigate("more"));
  await page.getByRole("button", { name: "Review records" }).click();
  await page.getByText(/Copy diagnostic JSON puts text on your clipboard; it does not save a file/).waitFor();
  for (const width of [320, 384]) {
    await page.setViewportSize({ width, height: 812 });
    assert.equal(await page.locator(".diagnostics-review .log-toolbar").evaluate(el => el.scrollWidth <= el.clientWidth), true);
  }
  const json = await page.locator(".diagnostics-json").innerText(); await page.getByRole("button", { name: "Copy diagnostic JSON" }).click();
  await page.getByText("Diagnostic JSON copied.", { exact: true }).waitFor(); assert.equal(await page.evaluate(() => copied.at(-1)), json);
  await page.evaluate(() => { failCopy = true; }); await page.getByRole("button", { name: "Copy diagnostic JSON" }).click();
  await page.getByText("Couldn't copy. You can select and copy the text below.", { exact: true }).waitFor();
  await page.getByText(/Copy puts log text on your clipboard; it does not save a file/).waitFor();
  await page.getByRole("button", { name: "Show debug log" }).click();
  await page.locator(".more-card").filter({ has: page.getByText("Debug log", { exact: true }) }).getByRole("button", { name: "Copy" }).click(); await page.getByText("Couldn't copy to clipboard.", { exact: true }).waitFor();
  await page.evaluate(() => { failCopy = false; }); await page.locator(".more-card").filter({ has: page.getByText("Debug log", { exact: true }) }).getByRole("button", { name: "Copy" }).click();
  await page.getByText("Debug log copied.", { exact: true }).waitFor(); assert.match(await page.evaluate(() => copied.at(-1)), /Synthetic log text/);
});

if (process.env.EXPORT_COPY_EVIDENCE === "1") test("capture simulated private-save and copy-only clarification", async t => {
  await mkdir(dir, { recursive: true }); const page = await open(t, "dashboard", { record: true });
  const preview = await capture(page); await preview.waitFor(); await page.screenshot({ path: `${dir}/screenshot-private-preview-simulated.png`, animations: "disabled" }); await page.waitForTimeout(700);
  await preview.getByRole("button", { name: "Close screenshot preview" }).click();
  await page.evaluate(() => { failShot = true; }); await page.locator(".action-btn").filter({ hasText: "Screenshot" }).click(); await page.getByText("Screenshot failed (simulated)", { exact: true }).waitFor();
  await page.screenshot({ path: `${dir}/screenshot-error-simulated.png`, animations: "disabled" }); await page.waitForTimeout(700);
  await page.evaluate(() => router.navigate("files")); const file = page.locator(".file-row").filter({ hasText: "report.txt" }); await file.waitFor(); await file.click();
  await page.getByText("Copied this session", { exact: true }).waitFor(); await page.screenshot({ path: `${dir}/private-file-copy-simulated.png`, animations: "disabled" }); await page.waitForTimeout(700);
  const report = await openReport(page); await report.getByRole("button", { name: "Copy report" }).scrollIntoViewIfNeeded();
  await page.screenshot({ path: `${dir}/copy-only-report-simulated.png`, animations: "disabled" }); await page.waitForTimeout(700);
  await report.getByRole("button", { name: "Close", exact: true }).click();
  await page.evaluate(async () => {
    const { recordUnknownDiagnostic } = await import("/src/lib/unknownDiagnostics.ts");
    recordUnknownDiagnostic({ kind: "installed_package", token: "com.example.fixture", reason: "uncatalogued_package", appVersion: "0.1.0" });
  });
  await page.waitForFunction(() => !!localStorage.getItem("atv.unknownDiagnostics.v1"));
  await page.evaluate(() => router.navigate("more"));
  await page.getByRole("button", { name: "Review records" }).click();
  const diagnostics = page.locator(".more-card").filter({ has: page.getByText("Unknown app diagnostics", { exact: true }) });
  await diagnostics.evaluate(el => el.scrollIntoView({ block: "start", behavior: "instant" }));
  await page.screenshot({ path: `${dir}/copy-only-diagnostic-json-simulated.png`, animations: "disabled" }); await page.waitForTimeout(700);
  await page.getByRole("button", { name: "Show debug log" }).click();
  const log = page.locator(".more-card").filter({ has: page.getByText("Debug log", { exact: true }) });
  await log.screenshot({ path: `${dir}/copy-only-debug-log-simulated.png`, animations: "disabled" }); await page.waitForTimeout(700);
  const video = page.video(); await page.close(); await video.saveAs(`${dir}/private-save-copy-clarification-simulated.webm`);
});
