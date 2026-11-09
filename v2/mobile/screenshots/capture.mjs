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
import { createServer } from "vite";

const HERE = dirname(fileURLToPath(import.meta.url));
const PLAY_STORE = process.argv.includes("--play-store");
const FRAMES = PLAY_STORE ? join(HERE, "../store-listing/screenshots") : join(HERE, "frames");
const SHEET = join(HERE, "contact-sheet.png");
const VIEWPORT = { width: 412, height: PLAY_STORE ? 732 : 915 };
const SCALE = PLAY_STORE ? 3 : 2;

// Each step starts from a freshly connected Dashboard. `open` gets there and
// `ready` waits for the screen's data to land so no frame shows a spinner.
const SCREENS = [
  { name: "onboarding", connect: false, ready: (p) => p.getByText("192.168.1.42").first().waitFor() },
  { name: "dashboard", ready: (p) => p.getByText("NVIDIA SHIELD", { exact: false }).first().waitFor() },
  { name: "apps", open: ["apps"] },
  { name: "app-detail", open: ["apps"], then: openFirstAppSheet },
  { name: "tweaks", open: ["tweaks"] },
  { name: "remote", open: ["remote"] },
  { name: "more", open: ["more"] },
  { name: "optimize", open: ["optimize"] },
  // `?billing=free` switches the demo to a Free Play build (see demo-mock.ts).
  { name: "paywall", query: "?billing=free", open: ["optimize"], then: openPaywall },
  { name: "diagnostics", open: ["diagnostics"] },
  { name: "launcher", open: ["launcher"] },
  { name: "snapshots", open: ["snapshots"] },
  { name: "devices", open: ["devices"] },
  { name: "screenshot", then: openScreenshot },
  { name: "files", open: ["files"] },
  { name: "files-copied", open: ["files"], then: copyFile },
  { name: "files-upload", open: ["files"], then: uploadFile },
  { name: "sideload", open: ["sideload"] },
  { name: "sideload-ready", open: ["sideload"], then: pickApk },
  { name: "backups", open: ["backups"] },
  { name: "logs", open: ["logs"], then: readLogs },
  { name: "riskguide", open: ["riskguide"] },
  { name: "add-tv", open: ["addtv"] },
  { name: "catalog", open: ["catalog"], then: openCatalogProfile },
];

async function openFirstAppSheet(page) {
  await page.locator('.app-row[data-package="com.google.android.videos"]').click();
  await page.locator(".sheet").first().waitFor();
}

async function openScreenshot(page) {
  await page.locator(".action-btn").filter({ hasText: "Screenshot" }).click();
  await page.getByRole("dialog", { name: "Screenshot", exact: true }).getByRole("button", { name: "Save a copy" }).waitFor();
}

async function copyFile(page) {
  await page.locator(".file-row").filter({ hasText: "device-report.txt" }).click();
  await page.getByText("Copied this session", { exact: true }).waitFor();
  // Let the copy toast clear so the row's Save a copy / Share buttons are in the frame.
  await page.locator(".dl-list").scrollIntoViewIfNeeded();
  await page.waitForTimeout(4000);
}

async function uploadFile(page) {
  await page.getByRole("button", { name: "Upload to this folder" }).click();
  await page.getByText("Uploaded this session", { exact: true }).waitFor();
  await page.locator(".dl-list").scrollIntoViewIfNeeded();
  await page.waitForTimeout(4000);
}

async function pickApk(page) {
  await page.getByRole("button", { name: "Pick APK files" }).click();
  await page.getByText(/Ready to install/).waitFor();
}

async function readLogs(page) {
  await page.getByRole("button", { name: "Read logs", exact: true }).click();
  await page.getByLabel("Log output").waitFor();
}

async function openCatalogProfile(page) {
  await page.getByRole("searchbox", { name: "Search device profiles" }).fill("acer-ar3-r3");
  await page.getByRole("searchbox", { name: "Search package IDs" }).fill("com.realtek.leanbacklauncher.partnercustomizer");
  await page.locator('[data-profile="acer-ar3-r3"]').click();
  await page.locator('[data-package-source="com.realtek.leanbacklauncher.partnercustomizer"]').waitFor();
}

async function openPaywall(page) {
  await page.getByRole("button", { name: "Unlock Pro" }).click();
  await page.getByRole("button", { name: /Buy Pro/ }).waitFor();
}

async function settle(page) {
  await page.waitForLoadState("networkidle");
  await page.evaluate(() => document.fonts.ready);
  // Let entrance transitions and the last fixture-backed render finish.
  await page.waitForTimeout(400);
}

async function capture(browser, origin, screen, index) {
  const context = await browser.newContext({
    viewport: VIEWPORT,
    deviceScaleFactor: SCALE,
    isMobile: true,
    hasTouch: true,
    colorScheme: "dark",
  });
  const page = await context.newPage();
  page.on("pageerror", (e) => console.error(`  [${screen.name}] page error: ${e.message}`));
  await page.addInitScript(() => {
    localStorage.clear();
    // One saved TV so the launch screen shows the picker, with auto-dial off
    // so it stays on that screen instead of connecting.
    localStorage.setItem("atv.autoConnect.v1", "0");
    localStorage.setItem("atv.savedDevices.v1", JSON.stringify([{
      host: "192.168.1.42",
      connectPort: 5555,
      name: "NVIDIA SHIELD Android TV",
      deviceType: "shield",
      hardwareId: "0323220012345",
      lastUsed: "2026-10-21T20:14:00.000Z",
    }]));
  });
  await page.goto(origin + (screen.query ?? ""), { waitUntil: "networkidle" });
  await page.evaluate(async ({ connect, open }) => {
    const { session } = await import("/src/lib/session.svelte.ts");
    const { router } = await import("/src/lib/router.svelte.ts");
    if (!connect) return;
    await session.connect("192.168.1.42", 5555);
    router.reset("dashboard");
    for (const s of open) router.navigate(s);
  }, { connect: screen.connect !== false, open: screen.open ?? [] });
  if (screen.ready) await screen.ready(page);
  await settle(page);
  if (screen.then) {
    await screen.then(page);
    await settle(page);
  }
  const file = join(FRAMES, `${String(index + 1).padStart(2, "0")}-${screen.name}.png`);
  await page.screenshot({ path: file });
  await context.close();
  console.log(`  ${screen.name}`);
}

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

const only = process.argv.slice(2).filter((arg) => arg !== "--play-store");
const storeScreens = ["dashboard", "apps", "optimize", "launcher", "tweaks", "more"];
const selected = only.length ? SCREENS.filter((s) => only.includes(s.name))
  : PLAY_STORE ? SCREENS.filter((s) => storeScreens.includes(s.name)) : SCREENS;
if (only.length && selected.length !== only.length) {
  const known = SCREENS.map((s) => s.name).join(", ");
  throw new Error(`unknown screen in ${only.join(", ")}; known: ${known}`);
}

process.env.VITE_DEMO = "1";
const server = await createServer({
  root: join(HERE, ".."),
  logLevel: "error",
  server: { host: "127.0.0.1", port: 5300, strictPort: false },
});
await server.listen();
const origin = `http://127.0.0.1:${server.httpServer.address().port}`;
const browser = await chromium.launch();

try {
  if (!only.length) await rm(FRAMES, { recursive: true, force: true });
  await mkdir(FRAMES, { recursive: true });
  console.log(`Capturing ${selected.length} screens at ${VIEWPORT.width}x${VIEWPORT.height}@${SCALE}x → ${FRAMES}`);
  for (const screen of selected) await capture(browser, origin, screen, SCREENS.indexOf(screen));
  if (!PLAY_STORE) {
    await contactSheet(browser);
    console.log(`Contact sheet → ${SHEET}`);
  }
} finally {
  await browser.close();
  await server.close();
}
