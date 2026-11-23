// Repeatable screenshot capture for the ATV Optimizer phone app.
//
// Boots Vite with VITE_DEMO=1 (fixture-backed invoke(), see
// ../src/lib/demo-mock.ts — no phone or TV needed), drives a headless Chromium
// at a phone viewport through every screen, and writes one PNG per screen to
// screenshots/frames/ plus a one-page overview at screenshots/contact-sheet.png.
//
// Run via `npm run screenshots` from v2/mobile/. Pass screen names to capture
// a subset: `npm run screenshots -- apps tweaks`.

import { mkdir, readdir, rm } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";
import { SCREENS, captureScreen, startDemoServer } from "./screens.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const FRAMES = join(HERE, "frames");
const SHEET = join(HERE, "contact-sheet.png");
const VIEWPORT = { width: 412, height: 915 };
const SCALE = 2;

async function contactSheet(browser) {
  const files = (await readdir(FRAMES)).filter((f) => f.endsWith(".png")).sort();
  const page = await browser.newPage({ viewport: { width: 1800, height: 900 } });
  const cells = await Promise.all(
    files.map(async (f) => {
      const data = (await import("node:fs")).readFileSync(join(FRAMES, f)).toString("base64");
      return `<figure><img src="data:image/png;base64,${data}"><figcaption>${f.replace(/\.png$/, "")}</figcaption></figure>`;
    }),
  );
  await page.setContent(`<!doctype html><style>
    body { margin: 0; padding: 24px; background: #0b0d10; font: 14px system-ui, sans-serif; color: #9aa4b2; }
    main { display: grid; grid-template-columns: repeat(6, 1fr); gap: 20px; }
    figure { margin: 0; } img { width: 100%; border-radius: 14px; border: 1px solid #222831; display: block; }
    figcaption { margin-top: 6px; text-align: center; }
  </style><main>${cells.join("")}</main>`);
  await page.screenshot({ path: SHEET, fullPage: true });
  await page.close();
}

const only = process.argv.slice(2);
const selected = only.length ? SCREENS.filter((s) => only.includes(s.name)) : SCREENS.filter((s) => !s.extra);
if (only.length && selected.length !== only.length) {
  const known = SCREENS.map((s) => s.name).join(", ");
  throw new Error(`unknown screen in ${only.join(", ")}; known: ${known}`);
}

const server = await startDemoServer();
const browser = await chromium.launch();

try {
  if (!only.length) await rm(FRAMES, { recursive: true, force: true });
  await mkdir(FRAMES, { recursive: true });
  console.log(`Capturing ${selected.length} screens at ${VIEWPORT.width}x${VIEWPORT.height}@${SCALE}x → ${FRAMES}`);
  for (const screen of selected) {
    const index = SCREENS.indexOf(screen);
    const path = join(FRAMES, `${String(index + 1).padStart(2, "0")}-${screen.name}.png`);
    await captureScreen(browser, server.origin, screen, { viewport: VIEWPORT, scale: SCALE, path });
    console.log(`  ${screen.name}`);
  }
  await contactSheet(browser);
  console.log(`Contact sheet → ${SHEET}`);
} finally {
  await browser.close();
  await server.close();
}
