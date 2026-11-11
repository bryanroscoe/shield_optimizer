// A failed health report must not re-request forever.
//
// The Health tab lazy-loads its report from an $effect that fires while the
// report is missing. When `health_report` fails the report stays missing and
// loading ends, which used to re-trigger the effect in a tight loop against
// the TV. The failure must be shown once, and only Refresh tries again.
//
// Run via `node tests/health-load-failure.mjs` from v2/.
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
    await page.goto(`http://127.0.0.1:${server.httpServer.address().port}`, { waitUntil: "networkidle" });
    await page.evaluate(() => {
      const original = window.__TAURI_INTERNALS__.invoke;
      window.__HEALTH_CALLS__ = 0;
      window.__TAURI_INTERNALS__.invoke = async (command, args) => {
        if (command === "health_report") {
          window.__HEALTH_CALLS__++;
          throw "fixture: dumpsys timed out";
        }
        return original(command, args);
      };
    });
    const calls = () => page.evaluate(() => window.__HEALTH_CALLS__);

    await page.getByText("NVIDIA SHIELD", { exact: false }).first().click();
    await page.locator("#tab-health").click();
    const panel = page.locator("#tabpanel-health");
    await panel.getByText("fixture: dumpsys timed out").waitFor();
    await page.waitForTimeout(1500);
    const afterOpen = await calls();
    assert.ok(afterOpen >= 1, "opening Health asked for the report");
    assert.ok(afterOpen <= 2, `a failed report is not re-requested in a loop: ${afterOpen} calls`);

    await panel.getByRole("button", { name: "Refresh" }).click();
    await page.waitForTimeout(1500);
    const afterRefresh = await calls();
    assert.equal(afterRefresh, afterOpen + 1, `Refresh tries exactly once more: ${afterOpen} -> ${afterRefresh}`);
    await panel.getByText("fixture: dumpsys timed out").waitFor();

    assert.deepEqual(errors, [], `no page errors: ${errors.join("\n")}`);
    console.log(`Health load failure passed: ${afterOpen} call(s) on open, one more per Refresh, error shown.`);
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
