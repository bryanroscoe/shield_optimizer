// Display scaling on the Tweaks tab. `get_display_scaling` rejects when the
// TV's size and density can't be read; that must show as a readable error,
// not blank values or an unhandled rejection. `set_display_scaling`'s success
// message is the read-back size and density, which the UI never parses.
//
// Run via `node tests/display-scaling.mjs` from v2/.
import { freePort } from "./lib/free-port.mjs";
import assert from "node:assert/strict";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const V2 = join(dirname(fileURLToPath(import.meta.url)), "..");

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
    const page = await browser.newPage({ viewport: { width: 1280, height: 1000 } });
    page.setDefaultTimeout(10000);
    const errors = [];
    page.on("pageerror", (error) => errors.push(error.message));
    page.on("dialog", (dialog) => dialog.accept());
    await page.goto(`http://127.0.0.1:${server.httpServer.address().port}`, { waitUntil: "networkidle" });
    await page.evaluate(() => {
      const original = window.__TAURI_INTERNALS__.invoke;
      window.__SCALING__ = { readable: false, sets: [] };
      window.__TAURI_INTERNALS__.invoke = async (command, args) => {
        if (command === "get_display_scaling") {
          if (!window.__SCALING__.readable) throw "wm: the TV reported no display size or density";
          return { size: "Physical size: 3840x2160\nOverride size: 1920x1080", density: "Physical density: 640\nOverride density: 320" };
        }
        if (command === "set_display_scaling") {
          window.__SCALING__.sets.push(args.preset);
          window.__SCALING__.readable = true;
          return { ok: true, message: "Physical size: 3840x2160\nOverride size: 1920x1080\nPhysical density: 640\nOverride density: 320" };
        }
        return original(command, args);
      };
    });

    await page.getByText("NVIDIA SHIELD", { exact: false }).first().click();
    await page.getByRole("tab", { name: "Tweaks", exact: true }).click();
    const panel = page.locator("#tabpanel-tweaks");
    await panel.getByText("Couldn't read the current size and density: wm: the TV reported no display size or density").waitFor();
    assert.equal(await panel.locator(".current-scaling").count(), 0, "no blank current values on a failed read");

    await panel.getByRole("button", { name: /^1080p/ }).click();
    await panel.getByText("Applied.", { exact: true }).waitFor();
    await panel.locator(".current-scaling").getByText("Override size: 1920x1080").waitFor();
    assert.equal(await panel.locator(".scaling-read-error").count(), 0, "the read error clears once the TV answers");
    assert.deepEqual(await page.evaluate(() => window.__SCALING__.sets), ["fhd_1080p"]);

    assert.deepEqual(errors, [], `no page errors: ${errors.join("\n")}`);
    console.log("Display scaling passed: a failed read shows the error, and a confirmed change shows the TV's read-back values.");
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
