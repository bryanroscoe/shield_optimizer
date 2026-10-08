// The Launcher tab listed every TV app as a "HOME APP": YouTube, Plex and the
// Play Store all declare LEANBACK_LAUNCHER, which is how an app gets a tile on
// the home screen, not a claim to be the home screen. The engine now lists
// HOME handlers only, and anything else can be tried from the Advanced picker,
// which never disables the stock launcher as a side effect.
//
// The same round fixed two Snapshot bugs a fresh snapshot exposed: it always
// listed the launcher as a change, and the preview's "Now" column was blank.
//
// Runs against the demo fixture layer. Plex is installed there
// (list_installed_packages) but declares no Home screen, so it must reach the
// picker and must not reach the launcher list.
import assert from "node:assert/strict";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const V2 = join(HERE, "..");
const LEANBACK_ONLY = "com.plexapp.android";

function serverURL(server) {
  const address = server.httpServer?.address();
  if (!address || typeof address === "string") throw new Error("no address");
  const host = address.address.includes(":") ? `[${address.address}]` : address.address;
  return `http://${host}:${address.port}`;
}

function setHarnessEnvironment() {
  const keys = ["VITE_DEMO", "TAURI_DEV_HOST"];
  const previous = new Map(keys.map((k) => [k, { present: Object.hasOwn(process.env, k), value: process.env[k] }]));
  process.env.VITE_DEMO = "1";
  delete process.env.TAURI_DEV_HOST;
  return () => {
    for (const [k, prior] of previous) {
      if (prior.present) process.env[k] = prior.value;
      else delete process.env[k];
    }
  };
}

async function launcherTab(page) {
  await page.getByRole("tab", { name: "Launcher" }).click();
  const rows = page.locator(".launcher-list li");
  await rows.first().waitFor({ timeout: 15000 });

  const listed = await page.locator(".launcher-list .launcher-pkg").allInnerTexts();
  assert.ok(listed.length > 0, "the launcher list has rows");
  assert.ok(
    !listed.includes(LEANBACK_ONLY),
    `a leanback-only app must not be listed as a Home app: ${JSON.stringify(listed)}`,
  );
  const homeAppRows = await rows.filter({ has: page.locator(".tag", { hasText: "HOME APP" }) }).count();
  assert.equal(homeAppRows, 0, "no ordinary app is badged HOME APP in the demo");

  const currentProjectivy = rows.filter({ has: page.locator(".launcher-pkg", { hasText: "com.spocky.projengmenu" }) });
  assert.equal(await currentProjectivy.locator(".current-default").count(), 1);
  assert.equal(
    await currentProjectivy.getByRole("button", { name: "Open the developer's page for Projectivy Launcher in your browser" }).count(),
    1,
    "the current default keeps its source link without offering to disable it",
  );

  // The Advanced picker lists every installed app, so the leanback-only app is
  // reachable there — proof the fixture has it and the list above left it out.
  await page.locator("details.home-picker summary").click();
  const picker = page.getByLabel("App to set as Home");
  await picker.waitFor();
  await page.waitForFunction(
    (pkg) => [...document.querySelectorAll("details.home-picker option")].some((o) => o.value === pkg),
    LEANBACK_ONLY,
    { timeout: 10000 },
  );
  const disableStock = page.getByRole("button", { name: "Disable stock launcher" });

  // Picking an app never implies taking over from stock.
  await picker.selectOption(LEANBACK_ONLY);
  assert.equal(await disableStock.isDisabled(), true, "Disable stock launcher is off until the pick is Home");
  await page.getByRole("button", { name: "Set as Home" }).click();
  await page.getByText("doesn't declare a Home screen", { exact: false }).waitFor({ timeout: 10000 });
  assert.equal(await disableStock.isDisabled(), true, "a refused pick still can't disable stock");

  // Once the pick is Home, the takeover is a separate, confirmed step.
  await picker.selectOption("com.spocky.projengmenu");
  assert.equal(await disableStock.isDisabled(), false, "Disable stock launcher is offered when the pick is Home");
  await disableStock.click();
  const confirm = page.getByRole("alertdialog", { name: "Confirm disabling the stock launcher" });
  await confirm.waitFor();
  assert.ok(await confirm.getByRole("button", { name: "Save snapshot first" }).isVisible());
  await confirm.getByRole("button", { name: "Cancel" }).click();
  await confirm.waitFor({ state: "detached" });

  // "Open Play Store on TV" has to say it worked, where the user is looking.
  const missing = rows.filter({ has: page.locator(".tag", { hasText: "MISSING" }) }).first();
  await missing.getByRole("button", { name: "Open Play Store on TV" }).click();
  await page
    .getByText("Opened the Play Store on the TV. Confirm the install there.")
    .waitFor({ timeout: 10000 });
  assert.ok(await missing.getByRole("button", { name: "Source site" }).count() <= 1);
}

async function snapshotTab(page) {
  await page.getByRole("tab", { name: /Snapshot/ }).click();
  assert.ok(await page.locator("#tabpanel-snapshot .beta-tag").isVisible(), "Snapshots are marked Beta");

  // The newest demo snapshot recorded the launcher that is Home right now.
  await page.getByRole("button", { name: "Preview restore" }).first().click();
  const table = page.locator("#tabpanel-snapshot .plan-table");
  await table.waitFor({ timeout: 10000 });

  const launcherRows = table.locator('tr[data-plan-item="launcher"]');
  const launcherRowCount = await launcherRows.count();
  for (let i = 0; i < launcherRowCount; i++) {
    const row = launcherRows.nth(i);
    assert.ok(
      await row.evaluate((el) => el.classList.contains("plan-noop")),
      `a fresh snapshot must not list the launcher as a change: ${await row.innerText()}`,
    );
  }

  const settingRows = table.locator("tbody tr").filter({ hasText: /^(global|secure|system)\./ });
  const settings = await settingRows.evaluateAll((trs) =>
    trs.map((tr) => [tr.cells[0].innerText.trim(), tr.cells[1].innerText.trim()]),
  );
  assert.ok(settings.length > 0, "the preview lists settings");
  const blank = settings.filter(([, now]) => now === "" || now === "—");
  assert.deepEqual(blank, [], `every setting row needs a real Now value: ${JSON.stringify(settings)}`);
  // The demo TV has HDMI-CEC on ("1" in the tweaks fixture); the row must say so.
  assert.deepEqual(
    settings.find(([key]) => key === "global.hdmi_control_enabled"),
    ["global.hdmi_control_enabled", "1"],
    `the Now column must carry the device's value: ${JSON.stringify(settings)}`,
  );
  const unchanged = await table.locator("tr.plan-noop").filter({ hasText: /^(global|secure)\./ }).count();
  assert.ok(unchanged > 0, "settings already at the snapshot's value are shown, not hidden");
}

async function exercise({ browser, base }) {
  const page = await browser.newPage({ viewport: { width: 1400, height: 1000 } });
  page.on("dialog", (d) => d.dismiss());
  const pageErrors = [];
  page.on("pageerror", (e) => pageErrors.push(String(e)));
  await page.goto(base, { waitUntil: "networkidle" });
  await page.getByText("NVIDIA SHIELD", { exact: false }).first().click();
  await launcherTab(page);
  await snapshotTab(page);
  assert.deepEqual(pageErrors, [], "the page must not throw");
  console.log(
    "Launcher rows passed: leanback-only app not listed, picker renders and gates the stock takeover, Play Store callout shows, fresh snapshot previews no launcher change with a populated Now column.",
  );
}

async function main() {
  const restore = setHarnessEnvironment();
  let server, browser;
  try {
    const { createServer } = await import("vite");
    const { chromium } = await import("playwright");
    server = await createServer({ root: V2, server: { host: "127.0.0.1", port: 0, strictPort: false, hmr: false } });
    await server.listen();
    browser = await chromium.launch();
    await exercise({ browser, base: serverURL(server) });
  } finally {
    await browser?.close().catch((e) => console.error("browser cleanup failed", e));
    await server?.close().catch((e) => console.error("Vite cleanup failed", e));
    restore();
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
