// Screen definitions and capture helpers shared by screenshots/capture.mjs
// (README gallery) and scripts/store-assets.mjs (Google Play listing).
//
// Every capture boots the app against the VITE_DEMO=1 fixture layer
// (../src/lib/demo-mock.ts): simulated data rendered by the real UI.

import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));

// Each step starts from a freshly connected Dashboard. `open` gets there and
// `ready` waits for the screen's data to land so no frame shows a spinner.
export const SCREENS = [
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
  { name: "diagnostics", open: ["diagnostics"], then: turnOnLive },
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
  // Variants for the store listing. `npm run screenshots` skips them unless named.
  { name: "diagnostics-playback", open: ["diagnostics"], then: readPlaybackReport, extra: true },
  { name: "remote-text", open: ["remote"], then: typeRemoteText, extra: true },
];

export function screenByName(name) {
  const screen = SCREENS.find((s) => s.name === name);
  if (!screen) throw new Error(`unknown screen "${name}"; known: ${SCREENS.map((s) => s.name).join(", ")}`);
  return screen;
}

// Live mode re-polls every 3 s; the capture lands well inside the first tick.
async function turnOnLive(page) {
  await page.locator(".live-refresh-btn").click();
  await page.locator('.live-refresh-btn[aria-pressed="true"]').waitFor();
}

async function readPlaybackReport(page) {
  const card = page.getByRole("region", { name: "Playback report" });
  await card.getByRole("button", { name: "Read playback report" }).click();
  await card.getByRole("region", { name: "Video decoders" }).waitFor();
  await card.evaluate((el) => el.scrollIntoView({ block: "start" }));
}

async function typeRemoteText(page) {
  await page.getByRole("button", { name: "Keyboard input" }).click();
  await page.getByRole("dialog", { name: "Send text to TV" }).waitFor();
  await page.getByLabel("Text to send").fill("The Bear");
}

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
  await page.getByRole("searchbox", { name: "Search package names" }).fill("com.realtek.leanbacklauncher.partnercustomizer");
  await page.locator('[data-profile="acer-ar3-r3"]').click();
  await page.locator('[data-package-source="com.realtek.leanbacklauncher.partnercustomizer"]').waitFor();
}

async function openPaywall(page) {
  await page.getByRole("button", { name: "Unlock Pro" }).click();
  await page.getByRole("button", { name: /Buy Pro/ }).waitFor();
}

export async function settle(page) {
  await page.waitForLoadState("networkidle");
  await page.evaluate(() => document.fonts.ready);
  // Let entrance transitions and the last fixture-backed render finish.
  await page.waitForTimeout(400);
}

// Boots Vite in demo mode. Returns the origin and a close function.
export async function startDemoServer() {
  const { createServer } = await import("vite");
  process.env.VITE_DEMO = "1";
  const server = await createServer({
    root: join(HERE, ".."),
    logLevel: "error",
    server: { host: "127.0.0.1", port: 5300, strictPort: false },
  });
  await server.listen();
  return { origin: `http://127.0.0.1:${server.httpServer.address().port}`, close: () => server.close() };
}

// Opens `screen` in a fresh phone-sized context and writes a PNG to `path`.
// `prepare` runs after the screen has settled (for example to scroll it).
// `fixedTime` pins the page clock (with `timezoneId`) so read-at timestamps
// in the UI come out the same on every run.
export async function captureScreen(browser, origin, screen, { viewport, scale, path, prepare, fixedTime, timezoneId }) {
  const context = await browser.newContext({
    viewport,
    ...(timezoneId ? { timezoneId, locale: "en-US" } : {}),
    deviceScaleFactor: scale,
    isMobile: true,
    hasTouch: true,
    colorScheme: "dark",
  });
  try {
    const page = await context.newPage();
    if (fixedTime) await page.clock.setFixedTime(fixedTime);
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
    if (prepare) {
      await prepare(page);
      await settle(page);
    }
    await page.screenshot({ path });
  } finally {
    await context.close();
  }
}
