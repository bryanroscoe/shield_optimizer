// The Sideload tab remembers the APK folder you last picked. It must not read
// that folder until you ask it to: the remembered path is often on a removable
// volume or a mounted DMG, and probing it on mount is what made macOS ask for
// removable-volume access on every visit (GitHub #89).
//
// This is the half of that fix that shipped with no regression gate.
import assert from "node:assert/strict";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const V2 = join(HERE, "..");
const SAVED_FOLDER = "/Volumes/SIDELOAD/apks";

function serverURL(server) {
  const address = server.httpServer?.address();
  if (!address || typeof address === "string") {
    throw new Error("Vite did not expose its bound TCP address");
  }
  const host = address.address.includes(":") ? `[${address.address}]` : address.address;
  return `http://${host}:${address.port}`;
}

function setHarnessEnvironment() {
  const keys = ["VITE_DEMO", "TAURI_DEV_HOST"];
  const previous = new Map(keys.map((key) => [
    key,
    { present: Object.hasOwn(process.env, key), value: process.env[key] },
  ]));
  process.env.VITE_DEMO = "1";
  delete process.env.TAURI_DEV_HOST;
  return () => {
    for (const [key, prior] of previous) {
      if (prior.present) process.env[key] = prior.value;
      else delete process.env[key];
    }
  };
}

async function exerciseSideloadScan({ browser, base }) {
  const page = await browser.newPage();

  // Arrive with a folder already remembered from a previous session, and
  // record every scan the app asks for.
  await page.addInitScript((folder) => {
    localStorage.setItem("shieldopt.lastApkFolder", folder);
    window.__SCAN_CALLS__ = [];
    const waitForBridge = setInterval(() => {
      const bridge = window.__TAURI_INTERNALS__;
      if (!bridge?.invoke) return;
      clearInterval(waitForBridge);
      const original = bridge.invoke.bind(bridge);
      bridge.invoke = async (command, args = {}) => {
        if (command === "list_apks_in_folder") {
          window.__SCAN_CALLS__.push(args);
        }
        return original(command, args);
      };
    }, 0);
  }, SAVED_FOLDER);

  await page.goto(base, { waitUntil: "networkidle" });
  await page.getByText("NVIDIA SHIELD", { exact: false }).first().click();
  await page.getByRole("tab", { name: "Install APK" }).click();

  // The remembered folder is shown, so the user can see what Scan would read.
  await page.getByText(SAVED_FOLDER, { exact: true }).waitFor();
  const scanButton = page.getByRole("button", { name: "Scan folder" });
  await scanButton.waitFor();

  let scans = await page.evaluate(() => window.__SCAN_CALLS__);
  assert.deepEqual(
    scans,
    [],
    `mounting the Sideload tab must not read the saved folder, but it scanned: ${JSON.stringify(scans)}`,
  );

  // Only an explicit click reads it.
  await scanButton.click();
  await page.waitForFunction(() => window.__SCAN_CALLS__.length > 0);
  scans = await page.evaluate(() => window.__SCAN_CALLS__);
  assert.equal(scans.length, 1);
  assert.equal(scans[0].folder, SAVED_FOLDER);

  // Leaving and coming back is still not a reason to read it again.
  await page.getByRole("tab", { name: "Overview" }).click();
  await page.getByRole("tab", { name: "Install APK" }).click();
  await scanButton.waitFor();
  scans = await page.evaluate(() => window.__SCAN_CALLS__);
  assert.equal(scans.length, 1, "revisiting the tab re-scanned the saved folder");

  console.log(
    "Sideload scan passed: the saved APK folder is read only on an explicit Scan, not on mount or revisit.",
  );
}

async function main() {
  const restoreEnvironment = setHarnessEnvironment();
  let server;
  let browser;
  try {
    const { createServer } = await import("vite");
    const { chromium } = await import("playwright");
    server = await createServer({
      root: V2,
      server: { host: "127.0.0.1", port: 0, strictPort: false, hmr: false },
    });
    await server.listen();
    browser = await chromium.launch();
    await exerciseSideloadScan({ browser, base: serverURL(server) });
  } finally {
    await browser?.close().catch((e) => console.error("browser cleanup failed", e));
    await server?.close().catch((e) => console.error("Vite cleanup failed", e));
    restoreEnvironment();
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
