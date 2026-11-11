// Files tab drag-and-drop. Tauri's drag-drop stream is the only source of real
// paths in the webview, so the test stands in for `@tauri-apps/api/webview`
// at the module level and fires that stream by hand. Three things must hold:
//   - while a drag is over the window, the listing says where the files will
//     land;
//   - a dropped file goes to `push_file` for the folder on screen, and a
//     dropped folder is refused with a message instead;
//   - a drop does nothing while Files is not the tab showing, because a
//     visited tab stays mounted, only hidden;
//   - a folder opened from the filesystem root is "/data", not "//data".
//
// Run via `npm run test:files-drop` from v2/.
import { freePort } from "./lib/free-port.mjs";
import assert from "node:assert/strict";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const V2 = join(dirname(fileURLToPath(import.meta.url)), "..");
const SERIAL = "192.168.1.42:5555";

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
    server = await createServer({
      root: V2,
      server: { host: "127.0.0.1", port: await freePort(), strictPort: true, hmr: false },
    });
    await server.listen();
    const address = server.httpServer?.address();
    if (!address || typeof address === "string") throw new Error("Vite did not expose its TCP address");
    const base = `http://127.0.0.1:${address.port}`;

    browser = await chromium.launch();
    const page = await browser.newPage({ viewport: { width: 1280, height: 860 } });
    await page.route(/tauri-apps[_/]api[_/]webview/, (route) =>
      route.fulfill({ status: 200, contentType: "application/javascript", body: WEBVIEW_STUB }),
    );

    await page.goto(`${base}/devices/${encodeURIComponent(SERIAL)}`, { waitUntil: "networkidle" });
    // Visit Install APK first so it stays mounted (hidden) with its own drop
    // listener: a drop on Files must not reach it.
    await page.locator("#tab-sideload").click();
    await page.locator("#tabpanel-sideload").waitFor();
    await page.locator("#tab-files").click();
    const panel = page.locator("#tabpanel-files");
    await panel.waitFor();
    await panel.locator(".files-table").waitFor();
    await page.waitForFunction(() => (window.__DROP_HANDLERS__ ?? []).length > 0);

    const where = await panel.locator(".crumb-current").innerText();
    const at = { x: 400, y: 400 };

    await fire(page, { type: "enter", paths: ["/Users/me/notes.txt"], position: at });
    const overlay = panel.locator(".drop-overlay");
    await overlay.waitFor();
    assert.match(await overlay.innerText(), /Drop files to upload to/);
    assert.match(await overlay.innerText(), new RegExp(where));
    await fire(page, { type: "leave" });
    await overlay.waitFor({ state: "detached" });

    await fire(page, { type: "over", position: at });
    await overlay.waitFor();
    await fire(page, {
      type: "drop",
      paths: ["/Users/me/notes.txt", "/Users/me/Photos"],
      position: at,
    });
    await overlay.waitFor({ state: "detached" });
    const message = panel.locator(".action-message");
    await page.waitForFunction(
      () => /is a folder/.test(document.querySelector("#tabpanel-files .action-message")?.textContent ?? ""),
    );
    const text = await message.innerText();
    assert.match(text, /Uploaded notes\.txt to \/sdcard/, text);
    assert.match(text, /Photos is a folder\. Drop files, not folders\./, text);

    await page.locator("#tab-sideload").click();
    assert.doesNotMatch(
      await page.locator("#tabpanel-sideload").innerText(),
      /not an \.apk|Could not read that APK/,
      "a hidden Install APK tab ignored the drop made on Files",
    );
    await page.locator("#tab-files").click();

    // Another tab showing: Files stays mounted but must not react.
    await page.locator("#tab-overview").click();
    await fire(page, { type: "over", position: at });
    await fire(page, { type: "drop", paths: ["/Users/me/other.txt"], position: at });
    await page.waitForTimeout(300);
    await page.locator("#tab-files").click();
    assert.equal(await overlay.count(), 0, "no overlay was left behind");
    assert.doesNotMatch(await message.innerText(), /other\.txt/, "a hidden Files tab ignored the drop");

    // Climb to the root (power-user paths) and open a folder from there.
    await page.evaluate(() => {
      const internals = window.__TAURI_INTERNALS__;
      const invoke = internals.invoke;
      window.__LIST_DIRS__ = [];
      internals.invoke = (cmd, args) => {
        if (cmd === "list_dir") window.__LIST_DIRS__.push(args.path);
        return invoke(cmd, args);
      };
    });
    await panel.getByLabel("Show system paths (power user)").check();
    await panel.locator(".crumbs button", { hasText: "Up" }).click();
    await panel.locator(".dir-link", { hasText: "data" }).waitFor();
    await panel.locator(".dir-link", { hasText: "data" }).click();
    await page.waitForFunction(() => window.__LIST_DIRS__.includes("/data") || window.__LIST_DIRS__.includes("//data"));
    const listed = await page.evaluate(() => window.__LIST_DIRS__);
    assert.ok(listed.includes("/"), `climbed to the root: ${listed}`);
    assert.ok(listed.includes("/data") && !listed.some((p) => p.startsWith("//")), `root children join with one slash: ${listed}`);

    console.log(
      "Files drop passed: the overlay names the folder, a dropped file uploads there, a folder is refused, a hidden Files tab ignores drops, and root folders open as /name.",
    );
    await page.close();
  } finally {
    await browser?.close().catch((error) => console.error("browser cleanup failed", error));
    await server?.close().catch((error) => console.error("Vite cleanup failed", error));
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
