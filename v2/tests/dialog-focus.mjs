// Modal dialogs own the keyboard while open: focus moves in, Tab stays in,
// Escape closes, and focus goes back to the control that opened it. Covers
// the shell's Report a bug and release-history dialogs, which share
// `$lib/dialog-focus` with the snapshot-name, approval and app-report
// dialogs. Also pins that Report a bug discloses the same report contents as
// Settings (NOTICE_TEXT) next to its reports switch.
//
// Run via `node tests/dialog-focus.mjs` from v2/.
import assert from "node:assert/strict";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const V2 = join(dirname(fileURLToPath(import.meta.url)), "..");
const stub = (body) => ({ status: 200, contentType: "application/javascript", body });

const inDialog = (page) =>
  page.evaluate(() => document.activeElement?.closest("[role='dialog']") !== null);

async function checkTrap(page, opener, label) {
  await page.waitForFunction(() => document.activeElement?.closest("[role='dialog']") !== null);
  for (let i = 0; i < 25; i++) {
    await page.keyboard.press("Tab");
    assert.ok(await inDialog(page), `${label}: Tab ${i + 1} stays in the dialog`);
  }
  for (let i = 0; i < 5; i++) {
    await page.keyboard.press("Shift+Tab");
    assert.ok(await inDialog(page), `${label}: Shift+Tab stays in the dialog`);
  }
  await page.keyboard.press("Escape");
  await page.getByRole("dialog").waitFor({ state: "detached" });
  assert.ok(
    await opener.evaluate((el) => el === document.activeElement),
    `${label}: focus returns to the control that opened it`,
  );
}

async function main() {
  const previous = new Map(["VITE_DEMO", "TAURI_DEV_HOST"].map((key) => [key, process.env[key]]));
  process.env.VITE_DEMO = "1";
  delete process.env.TAURI_DEV_HOST;
  let server;
  let browser;
  try {
    const { createServer } = await import("vite");
    const { chromium } = await import("playwright");
    const { NOTICE_TEXT } = await startServer(createServer, (s) => (server = s));
    browser = await chromium.launch();
    const page = await browser.newPage({ viewport: { width: 1100, height: 900 } });
    page.setDefaultTimeout(10000);
    await page.route(/plugin-updater/, (r) => r.fulfill(stub(`export async function check() { return null; }`)));
    await page.route(/plugin-opener/, (r) => r.fulfill(stub(`export async function openUrl() {}`)));
    await page.route(/plugin-process/, (r) => r.fulfill(stub(`export async function relaunch() {}`)));
    await page.addInitScript(() => localStorage.clear());
    await page.goto(`http://127.0.0.1:${server.httpServer.address().port}`, { waitUntil: "networkidle" });

    const bugButton = page.getByRole("button", { name: "Report a bug" });
    await bugButton.focus();
    await page.keyboard.press("Enter");
    const dialog = page.getByRole("dialog", { name: "Report a bug" });
    await dialog.waitFor();
    // The disclosure is the one Settings shows, word for word.
    const help = dialog.locator("[data-bug-reports-note] details");
    await help.locator("summary").click();
    assert.ok((await help.innerText()).includes(NOTICE_TEXT), "Report a bug shows NOTICE_TEXT");
    // Escape inside the open help closes the help, not the dialog.
    await help.locator("summary").focus();
    await page.keyboard.press("Escape");
    assert.equal(await help.evaluate((d) => d.open), false, "Escape folds the help first");
    assert.equal(await dialog.count(), 1, "the dialog stays open");
    await checkTrap(page, bugButton, "Report a bug");

    const versionButton = page.locator("button.version");
    await versionButton.focus();
    await page.keyboard.press("Enter");
    await page.getByRole("dialog").waitFor();
    await checkTrap(page, versionButton, "Release history");

    console.log("Dialog focus passed: focus moves in, Tab is trapped, Escape closes and returns focus; Report a bug discloses NOTICE_TEXT.");
  } finally {
    await browser?.close().catch((e) => console.error("browser cleanup failed", e));
    await server?.close().catch((e) => console.error("Vite cleanup failed", e));
    for (const [key, value] of previous) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
  }
}

async function startServer(createServer, keep) {
  const server = await createServer({ root: V2, server: { host: "127.0.0.1", port: 0, strictPort: false, hmr: false } });
  keep(server);
  await server.listen();
  return server.ssrLoadModule(join(V2, "shared/telemetry.ts"));
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
