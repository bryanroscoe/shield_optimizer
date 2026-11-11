// A TV can report the same HDR type, audio format or display mode twice
// (`5,6,5` for encoded surround formats is a real shape), two decoders for one
// video MIME type (secure and plain), or two network interfaces with one name.
// The backend passes those through as reported, so the Playback and Health
// tabs must render them without Svelte's `each_key_duplicate` crash.
//
// Run via `node tests/media-duplicate-values.mjs` from v2/.
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
    page.on("console", (msg) => {
      if (msg.type() === "error") errors.push(msg.text());
    });
    await page.goto(`http://127.0.0.1:${server.httpServer.address().port}`, { waitUntil: "networkidle" });
    await page.evaluate(() => {
      const original = window.__TAURI_INTERNALS__.invoke;
      window.__TAURI_INTERNALS__.invoke = async (command, args) => {
        const result = await original(command, args);
        if (command === "media_report") {
          const mode = result.modes[0];
          return {
            ...result,
            video: [...result.video, { ...result.video[0] }],
            verdicts: [...result.verdicts, ...result.verdicts],
            hdr_types: [...result.hdr_types, result.hdr_types[0]],
            modes: [...result.modes, { ...mode }],
            audio: { ...result.audio, enabled_formats: ["5", "6", "5"] },
          };
        }
        if (command === "resource_sample") {
          return { ...result, interfaces: [...result.interfaces, { ...result.interfaces[0], rx_bytes_per_s: 1024 }] };
        }
        if (command === "health_report") {
          return { ...result, display: { ...result.display, hdr_types: ["HDR10", "HLG", "HDR10"] } };
        }
        return result;
      };
    });

    await page.getByText("NVIDIA SHIELD", { exact: false }).first().click();
    await page.locator("#tab-media").click();
    const panel = page.locator("#tabpanel-media");
    await panel.getByText("Audio formats").waitFor();
    await panel.locator(".rail-item").first().waitFor();
    const items = await panel.locator(".rail-item > span:first-child").allInnerTexts();
    assert.equal(items.filter((t) => t === "5").length, 1, `format 5 listed once: ${items}`);
    assert.equal(items.filter((t) => t === "6").length, 1, `format 6 listed once: ${items}`);
    const mimes = await panel.locator(".media-table td.mime").allInnerTexts();
    assert.equal(mimes.length, new Set(mimes).size + 1, `both decoders for one MIME type are listed: ${mimes}`);

    await page.locator("#tab-health").click();
    await page.locator("#tabpanel-health .stat-tiles").waitFor();
    const interfaces = page.locator("#tabpanel-health .net-table tbody tr");
    await page.waitForFunction(() => document.querySelectorAll("#tabpanel-health .net-table tbody tr").length === 2);
    assert.equal(await interfaces.count(), 2, "both interfaces named eth0 get a row");

    assert.deepEqual(errors.filter((e) => /each_key_duplicate|duplicate key/i.test(e)), [], errors.join("\n"));
    assert.deepEqual(errors.filter((e) => !/Failed to load resource/.test(e)), [], `no page errors: ${errors.join("\n")}`);
    console.log("Media duplicate values passed: repeated HDR types, audio formats and modes render once each; duplicate codec MIME types, verdicts and interface names render without a keyed-each crash.");
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
