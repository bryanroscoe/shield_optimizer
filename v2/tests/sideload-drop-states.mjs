// Install APK keeps working as a drop target once a watch folder is saved:
//   - a bad drop explains itself (role=alert) in every state, not only on the
//     empty drop box;
//   - a drag over the tab highlights it in every state;
//   - a successful install re-reads install state, so the row reads INSTALLED;
//   - with a list longer than the window, the drop label stays on screen.
// Tauri's drag-drop stream is stood in for at the module level, as in
// files-drop.mjs.
//
// Run via `node tests/sideload-drop-states.mjs` from v2/.
import { freePort } from "./lib/free-port.mjs";
import assert from "node:assert/strict";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const V2 = join(dirname(fileURLToPath(import.meta.url)), "..");
const SERIAL = "192.168.1.42:5555";
const FOLDER = "/simulation/apks";
const PKG = "org.fixture.player";

const WEBVIEW_STUB = `
export function getCurrentWebview() {
  return {
    onDragDropEvent(handler) {
      (window.__DROP_HANDLERS__ ??= []).push(handler);
      return Promise.resolve(() => {
        window.__DROP_HANDLERS__ = window.__DROP_HANDLERS__.filter((h) => h !== handler);
      });
    },
  };
}
`;

const fire = (page, payload) =>
  page.evaluate((p) => {
    for (const handler of window.__DROP_HANDLERS__ ?? []) handler({ event: "tauri://drag-drop", id: 0, payload: p });
  }, payload);

async function main() {
  const previous = new Map(["VITE_DEMO", "TAURI_DEV_HOST"].map((key) => [key, process.env[key]]));
  process.env.VITE_DEMO = "1";
  delete process.env.TAURI_DEV_HOST;
  let server;
  let browser;
  try {
    const { createServer } = await import("vite");
    const { chromium } = await import("playwright");
    server = await createServer({ root: V2, server: { host: "127.0.0.1", port: await freePort(), strictPort: true, hmr: false } });
    await server.listen();
    browser = await chromium.launch();
    const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
    page.setDefaultTimeout(10000);
    await page.route(/tauri-apps[_/]api[_/]webview/, (route) =>
      route.fulfill({ status: 200, contentType: "application/javascript", body: WEBVIEW_STUB }),
    );
    await page.addInitScript(({ folder, pkg }) => {
      localStorage.setItem("shieldopt.lastApkFolder", folder);
      let installed = false;
      const waitForBridge = setInterval(() => {
        const bridge = window.__TAURI_INTERNALS__;
        if (!bridge?.invoke) return;
        clearInterval(waitForBridge);
        const original = bridge.invoke.bind(bridge);
        bridge.invoke = async (command, args = {}) => {
          if (command === "list_apks_in_folder") {
            // Long enough that the pane runs past the bottom of the window.
            return [
              { path: `${folder}/player.apk`, name: "player.apk", size_bytes: 2048, package: pkg },
              ...Array.from({ length: 30 }, (_, i) => ({
                path: `${folder}/fixture-${i}.apk`,
                name: `fixture-${i}.apk`,
                size_bytes: 4096,
                package: `org.fixture.extra${i}`,
              })),
            ];
          }
          if (command === "package_states" && args.packages?.includes(pkg)) {
            return { [pkg]: installed ? "enabled" : "missing" };
          }
          if (command === "install_apk") {
            installed = true;
            return { ok: true, message: "Success", hint: null };
          }
          return original(command, args);
        };
      }, 0);
    }, { folder: FOLDER, pkg: PKG });

    await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/devices/${encodeURIComponent(SERIAL)}`, {
      waitUntil: "networkidle",
    });
    await page.locator("#tab-sideload").click();
    const panel = page.locator("#tabpanel-sideload");
    await panel.getByText("Watch folder").waitFor();
    await page.waitForFunction(() => (window.__DROP_HANDLERS__ ?? []).length > 0);
    assert.equal(await panel.locator(".sideload-empty").count(), 0, "a saved folder replaces the empty drop box");

    const at = { x: 400, y: 400 };
    await fire(page, { type: "enter", paths: ["/Users/me/notes.txt"], position: at });
    await panel.locator(".drop-overlay").waitFor();
    await fire(page, { type: "drop", paths: ["/Users/me/notes.txt"], position: at });
    await panel.locator(".drop-overlay").waitFor({ state: "detached" });
    const alert = panel.getByRole("alert").filter({ hasText: "not an .apk" });
    await alert.waitFor();

    await panel.getByRole("button", { name: "Scan folder" }).click();
    const row = panel.locator(".apk-list li").filter({ hasText: "player.apk" });
    await row.waitFor();
    assert.equal(await row.locator(".tag.installed").count(), 0, "not installed before");
    await row.getByRole("button", { name: "Install", exact: true }).click();
    await row.locator(".tag.installed").waitFor();
    await row.getByRole("button", { name: "Reinstall", exact: true }).waitFor();

    // A drag over a pane taller than the window keeps its label on screen,
    // at the top of the list and scrolled to its bottom alike.
    for (const where of ["top", "bottom"]) {
      await page.evaluate((w) => window.scrollTo(0, w === "top" ? 0 : document.documentElement.scrollHeight), where);
      await fire(page, { type: "enter", paths: ["/Users/me/player.apk"], position: at });
      const label = panel.locator(".drop-overlay .drop-label");
      await label.waitFor();
      const fits = await label.evaluate((el) => {
        const r = el.getBoundingClientRect();
        const overlay = el.parentElement.getBoundingClientRect();
        return { inView: r.top >= 0 && r.bottom <= window.innerHeight, taller: overlay.height > window.innerHeight };
      });
      assert.ok(fits.taller, "the pane is taller than the window");
      assert.ok(fits.inView, `the drop label is visible with the page scrolled to the ${where}`);
      await fire(page, { type: "leave" });
      await panel.locator(".drop-overlay").waitFor({ state: "detached" });
    }

    console.log("Sideload drop states passed: with a watch folder saved, a drag highlights the tab, a bad drop shows an alert, and an install re-reads state.");
  } finally {
    await browser?.close().catch((e) => console.error("browser cleanup failed", e));
    await server?.close().catch((e) => console.error("Vite cleanup failed", e));
    for (const [key, value] of previous) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
