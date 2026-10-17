import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { mkdir, writeFile } from "node:fs/promises";
import { after, before, test } from "node:test";
import { chromium } from "playwright";
import { startViteServer } from "./helpers/vite-harness.mjs";

const common = JSON.parse(
  readFileSync(new URL("../../crates/core/data/app-lists/common.json", import.meta.url), "utf8"),
);
const katniss = common.find((entry) => entry.package === "com.google.android.katniss");
const protectedEntry = {
  package: "com.example.launcherguard",
  name: "Launcher Guard",
  method: "disable",
  risk: "high",
  optimize_description: "Keeps the home screen alive.",
  restore_description: "",
  default_optimize: false,
  default_restore: false,
  play_store: false,
};
const goneEntry = {
  package: "com.example.gonebloat",
  name: "Gone Bloat",
  method: "uninstall",
  risk: "safe",
  optimize_description: "A promo app this TV doesn't have.",
  restore_description: "",
  default_optimize: true,
  default_restore: false,
  play_store: true,
};

const fixture = {
  catalog: [katniss, protectedEntry, goneEntry],
  states: {
    [katniss.package]: "disabled",
    [protectedEntry.package]: "enabled",
    [goneEntry.package]: "missing",
  },
  others: [
    { package: "com.example.streambox", name: "StreamBox", system: false, enabled: true },
    { package: "com.android.helper", name: "System Helper", system: true, enabled: false },
  ],
};

let server;
let browser;
let origin;

before(async () => {
  ({ server, origin } = await startViteServer());
  browser = await chromium.launch({ headless: true });
});

after(async () => {
  await browser?.close();
  await server?.close();
});

async function openApps(t, { openerFails = false, modalEvidence = false } = {}) {
  const context = await browser.newContext({ viewport: { width: 384, height: 812 }, ...(modalEvidence ? { recordVideo: { dir: "evidence/modal-handoffs-2026-10-17/raw", size: { width: 384, height: 812 } } } : {}) });
  const page = await context.newPage();
  page.setDefaultTimeout(10000);
  t.after(() => context.close());
  await page.addInitScript(
    ({ fixture, openerFails }) => {
      window.calls = [];
      window.handlers = {};
      window.copied = [];
      Object.defineProperty(navigator, "clipboard", {
        configurable: true,
        value: { writeText: async (text) => void window.copied.push(text) },
      });
      window.__TAURI_INTERNALS__ = {
        invoke: async (command, args) => {
          window.calls.push({ command, args });
          if (window.handlers[command]) return window.handlers[command](args);
          switch (command) {
            case "get_entitlement":
              return "pro";
            case "wireless_connect":
              return { ok: true, message: "" };
            case "list_devices":
              return [{
                id: 1,
                serial: "TV:5555",
                name: "TV",
                model: "Shield",
                status: "device",
                connection: "network",
                device_type: "shield",
                tv_evidence: "tv",
                properties: { android_release: "11", serial_number: "HWSERIAL123" },
              }];
            case "wireless_status":
              return { connected: true, serial: "TV:5555", host: "TV" };
            case "list_other_packages":
              return fixture.others.map((app) => ({ ...app }));
            case "app_list_for_device":
              return fixture.catalog.map((entry) => ({ ...entry }));
            case "package_states":
              return { ...fixture.states };
            case "app_memory_map":
              return { "com.google.android.katniss": 0 };
            case "app_usage_map":
              return {};
            case "app_storage_map":
              return {
                "com.google.android.katniss": { app_bytes: 12582912, data_bytes: 3145728, cache_bytes: 1048576 },
              };
            case "app_apk_size":
              return { app_bytes: 2048, data_bytes: null, cache_bytes: null };
            case "safety_info":
              if (args.package === "com.example.launcherguard") {
                return { kind: "never_disable", reason: "Required for Home.", source: "protected_list" };
              }
              if (args.package === "com.google.android.katniss") {
                return { kind: "caution", reason: "Ends all voice control.", source: "reviewed_catalog" };
              }
              if (args.package === "com.example.gonebloat") {
                return { kind: "safe", reason: "Promo app.", source: "reviewed_catalog" };
              }
              return { kind: "unknown", reason: "Not in any list.", source: "no_record" };
            case "plugin:opener|open_url":
              if (openerFails) throw new Error("plugin opener not found");
              return null;
            default:
              return { ok: true, message: "" };
          }
        },
      };
    },
    { fixture, openerFails },
  );
  await page.goto(origin);
  await page.evaluate(async () => {
    const { session } = await import("/src/lib/session.svelte.ts");
    const { router } = await import("/src/lib/router.svelte.ts");
    window.session = session; window.router = router;
    session.entitlement = "pro";
    await session.connect("TV", 5555);
    router.reset("dashboard");
    router.navigate("apps");
  });
  await page.getByRole("heading", { name: /Recognised apps/ }).waitFor();
  return page;
}

async function rowPackages(page) {
  return page.locator(".app-row").evaluateAll((rows) => rows.map((r) => r.dataset.package));
}

async function commands(page) {
  return page.evaluate(() => window.calls.map((c) => c.command));
}

test("a disabled katniss is found by package and by name, and can be re-enabled", async (t) => {
  const page = await openApps(t);
  const search = page.getByPlaceholder("Search apps or packages");
  assert.equal(await page.getByText(/finds nothing/).count(), 0);

  for (const query of ["katniss", "assistant"]) {
    await search.fill(query);
    await page.getByText("1 result", { exact: true }).waitFor();
    assert.deepEqual(await rowPackages(page), [katniss.package]);
  }
  const row = page.locator(`.app-row[data-package="${katniss.package}"]`);
  assert.equal(await row.locator(".status-badge").textContent(), "OFF");
  await row.locator(".verdict-chip", { hasText: "Caution" }).waitFor();

  await search.fill("");
  await page.getByRole("button", { name: "Disabled", exact: true }).click();
  assert.ok((await rowPackages(page)).includes(katniss.package));

  await row.click();
  await page.locator(".sheet .description", { hasText: katniss.optimize_description }).waitFor();
  await page.getByText(`Reviewed ${katniss.reviewed_at}`).waitFor();
  await page.getByText(/App 12\.0 MB · Data 3\.0 MB \(incl\. 1\.0 MB cache\)/).waitFor();
  await page.getByText(/measured at/).first().waitFor();
  const enable = page.getByRole("button", { name: "Enable", exact: true });
  await enable.click();
  await page.waitForFunction(() => window.calls.some((c) => c.command === "enable_package"));
  const call = await page.evaluate(() => window.calls.find((c) => c.command === "enable_package"));
  assert.equal(call.args.package, katniss.package);
});

test("not-installed recognised apps hide by default and can be revealed", async (t) => {
  const page = await openApps(t);
  assert.equal((await rowPackages(page)).includes(goneEntry.package), false);
  await page.getByPlaceholder("Search apps or packages").fill("gone");
  await page.getByRole("button", { name: "Show 1 not installed" }).click();
  assert.deepEqual(await rowPackages(page), [goneEntry.package]);
  await page.locator(".app-row .chip", { hasText: "Not installed" }).waitFor();
  assert.equal(await page.getByRole("switch", { name: /Hide not installed/ }).getAttribute("aria-checked"), "false");
});

test("long-press opens the action menu without opening the sheet; a tap still opens it", async (t) => {
  const page = await openApps(t);
  const row = page.locator(`.app-row[data-package="${protectedEntry.package}"]`);
  // Wait for the row's verdict so the menu knows it is protected.
  await row.locator(".verdict-chip", { hasText: "Protected" }).waitFor();
  const box = await row.boundingBox();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.waitForTimeout(650);
  await page.mouse.up();
  const menu = page.getByRole("menu");
  await menu.waitFor();
  assert.equal(await page.getByText("If you remove it").count(), 0);
  assert.equal(await menu.getByRole("menuitem", { name: "Disable" }).isDisabled(), true);

  await menu.getByRole("menuitem", { name: "Copy package name" }).click();
  await page.waitForFunction(() => window.copied.length > 0);
  assert.deepEqual(await page.evaluate(() => window.copied), [protectedEntry.package]);
  assert.equal(await page.getByRole("menu").count(), 0);

  await row.click();
  await page.getByText("This package is protected", { exact: false }).waitFor();
  assert.equal(await page.getByRole("button", { name: "Disable", exact: true }).isDisabled(), true);
  const sent = await commands(page);
  assert.equal(sent.includes("disable_package"), false);
  // No uninstall for a recognised app the store can't give back.
  assert.equal(await page.getByRole("button", { name: /Uninstall/ }).count(), 0);

  const style = await row.evaluate((el) => {
    const s = getComputedStyle(el);
    return { userSelect: s.userSelect || s.webkitUserSelect };
  });
  assert.equal(style.userSelect, "none");
});

test("menu Disable goes through the safety lookup and confirm, never straight to the TV", async (t) => {
  const page = await openApps(t);
  const row = page.locator(`.app-row[data-package="com.example.streambox"]`);
  await row.click({ button: "right" });
  await page.getByRole("menu").getByRole("menuitem", { name: "Disable" }).click();
  await page.getByRole("button", { name: "Disable", exact: true }).waitFor();
  const sent = await commands(page);
  assert.ok(sent.filter((c) => c === "safety_info").length >= 1);
  assert.equal(sent.includes("disable_package"), false);
});

test("report opens a prefilled GitHub form through the opener", async (t) => {
  const page = await openApps(t);
  await page.getByPlaceholder("Search apps or packages").fill("katniss");
  await page.locator(`.app-row[data-package="${katniss.package}"]`).click();
  await page.getByRole("button", { name: "Report this app" }).click();
  const dialog = page.getByRole("dialog", { name: "Report this app" });
  await dialog.waitFor();
  // Caution from the reviewed list: the default reason is "Wrong verdict".
  assert.equal(await dialog.getByLabel("Wrong verdict").isChecked(), true);
  await dialog.getByLabel("Note (optional)").fill("Seen at 192.168.1.20 next to HWSERIAL123");
  await dialog.getByRole("button", { name: /Show the full report/ }).click();
  const preview = await dialog.getByLabel("Report preview").inputValue();
  assert.equal(preview.includes("192.168.1.20"), false);
  assert.equal(preview.includes("HWSERIAL123"), false);
  assert.ok(preview.includes(katniss.package));

  await dialog.getByRole("button", { name: "Open GitHub issue" }).click();
  await dialog.getByText(/opened with this report filled in/).waitFor();
  const call = await page.evaluate(() => window.calls.find((c) => c.command === "plugin:opener|open_url"));
  const url = new URL(call.args.url);
  assert.equal(url.searchParams.get("template"), "app_report.yml");
  assert.equal(url.searchParams.get("package"), katniss.package);
});

test("without an opener, report falls back to copy and a link to copy", async (t) => {
  const page = await openApps(t, { openerFails: true });
  const row = page.locator(`.app-row[data-package="com.example.streambox"]`);
  await row.click({ button: "right" });
  await page.getByRole("menu").getByRole("menuitem", { name: "Report this app" }).click();
  const dialog = page.getByRole("dialog", { name: "Report this app" });
  await dialog.getByRole("button", { name: "Open GitHub issue" }).click();
  await dialog.getByText(/Couldn't open the browser/).waitFor();
  await dialog.getByRole("button", { name: "Copy link" }).click();
  await page.waitForFunction(() => window.copied.some((t) => t.startsWith("https://github.com/")));
  await dialog.getByRole("button", { name: "Copy report" }).click();
  await page.waitForFunction(() => window.copied.some((t) => t.includes('"schema_version": 1')));
});

test("rows, chips and labels fit a 384px screen", async (t) => {
  const page = await openApps(t);
  await page.getByRole("switch", { name: /Hide not installed/ }).click();
  await page.locator(".verdict-chip", { hasText: "Safe to remove" }).waitFor();
  const overflow = await page.evaluate(() => {
    const out = [];
    if (document.documentElement.scrollWidth > window.innerWidth) out.push("page");
    for (const row of document.querySelectorAll(".app-row")) {
      const r = row.getBoundingClientRect();
      for (const el of row.querySelectorAll(".chip, .rec-text, .status-badge, .app-name-text")) {
        const b = el.getBoundingClientRect();
        if (b.right > r.right + 0.5 || b.left < r.left - 0.5) out.push(`${row.dataset.package}: ${el.className}`);
      }
    }
    for (const chip of document.querySelectorAll(".filter-chip, .system-toggle")) {
      const b = chip.getBoundingClientRect();
      if (b.right > window.innerWidth) out.push(chip.textContent.trim());
    }
    return out;
  });
  assert.deepEqual(overflow, []);
});

test("long report keyboard wrap reveals its focused actions without moving the locked page", async (t) => {
  const page = await openApps(t);
  await page.setViewportSize({ width: 384, height: 420 });
  await page.getByPlaceholder('Search apps or packages').fill('katniss');
  await page.locator(`.app-row[data-package="${katniss.package}"]`).click();
  await page.getByRole('button', { name: /Report this app/ }).click();
  const report = page.getByRole('dialog', { name: 'Report this app', exact: true });
  const close = report.getByRole('button', { name: 'Close', exact: true });
  const last = report.getByRole('button', { name: 'Open GitHub issue', exact: true });
  await report.evaluate(el => Promise.all(el.getAnimations().map(a => a.finished)));
  const top = await page.evaluate(() => document.body.style.top);
  const inView = async control => control.evaluate(el => {
    const r = el.getBoundingClientRect(), s = el.closest('[role="dialog"]').getBoundingClientRect();
    return r.top >= Math.max(0, s.top) - 1 && r.bottom <= Math.min(innerHeight, s.bottom) + 1;
  });
  await close.focus();
  await page.keyboard.press('Shift+Tab');
  assert.equal(await last.evaluate(el => document.activeElement === el), true);
  assert.equal(await inView(last), true, 'last report action must be visible');
  assert.equal(await page.evaluate(() => document.body.style.top), top);
  await page.keyboard.press('Tab');
  assert.equal(await close.evaluate(el => document.activeElement === el), true);
  assert.equal(await inView(close), true, 'report Close action must be visible');
  assert.equal(await page.evaluate(() => document.body.style.top), top);
});

test("menu Report returns focus to its original app row after closing", async (t) => {
  const evidence = process.env.MODAL_HANDOFF_EVIDENCE === "1";
  const dir = "evidence/modal-handoffs-2026-10-17";
  if (evidence) await mkdir(dir, { recursive: true });
  const page = await openApps(t, { modalEvidence: evidence });
  const row = page.locator('.app-row[data-package="com.example.streambox"]');
  await row.focus();
  await row.click({ button: "right" });
  await page.getByRole("menu").getByRole("menuitem", { name: "Report this app" }).click();
  const report = page.getByRole("dialog", { name: "Report this app", exact: true });
  await report.waitFor();
  if (evidence) {
    await page.screenshot({ path: `${dir}/menu-to-report-simulated.png`, animations: "disabled" });
    await page.waitForTimeout(700);
  }
  await page.keyboard.press("Escape");
  await report.waitFor({ state: "detached" });
  assert.equal(await row.evaluate(el => document.activeElement === el), true);
  assert.notEqual(await page.evaluate(() => document.body.style.position), "fixed");
  assert.equal((await commands(page)).includes("disable_package"), false);
  if (evidence) {
    await page.screenshot({ path: `${dir}/original-app-row-restored-simulated.png`, animations: "disabled" });
    await page.waitForTimeout(700);
    const video = page.video(); await page.context().close();
    await video.saveAs(`${dir}/menu-report-close-simulated.webm`);
  }
});


async function openMeasuredReport(page, pkg = 'com.example.streambox') {
  await page.locator(`.app-row[data-package="${pkg}"]`).click();
  const details = page.getByRole('dialog', { name: /App details for/ });
  await details.locator('[data-measure="storage"]').getByText(/APK files only/).waitFor();
  return { details, ...await reportFromDetails(page, details) };
}

async function reportFromDetails(page, details = page.getByRole('dialog', { name: /App details for/ })) {
  await details.getByRole('button', { name: 'Report this app' }).click();
  const report = page.getByRole('dialog', { name: 'Report this app', exact: true });
  await report.getByLabel('Include installed and enabled state, and the measured RAM and storage', { exact: true }).check();
  await report.getByRole('button', { name: /Show the full report/ }).click();
  return { report, preview: report.getByLabel('Report preview') };
}

test('report includes the successful APK fallback visible in the detail sheet', async t => {
  const page = await openApps(t);
  const { preview } = await openMeasuredReport(page);
  assert.deepEqual(JSON.parse(await preview.inputValue()).records[0].state.storage, { source: 'apk_files', app_bytes: 2048, data_bytes: null, cache_bytes: null });
});


for (const bytes of [0, 1234567]) test(`APK report keeps exact ${bytes} bytes, unknown data/cache and identical copied JSON`, async t => {
  const page = await openApps(t);
  await page.evaluate(bytes => { handlers.app_apk_size = () => ({ app_bytes: bytes, data_bytes: 999, cache_bytes: 888 }); }, bytes);
  await page.locator('.app-row[data-package="com.example.streambox"]').click();
  const details = page.getByRole('dialog', { name: /App details for/ });
  await details.locator('[data-measure="storage"]').getByText(/APK files only/).waitFor();
  const evidence = process.env.APK_REPORT_EVIDENCE === '1' && bytes > 0;
  const dir = 'evidence/apk-report-fallback-2026-10-17';
  if (evidence) { await mkdir(dir, { recursive: true }); await details.locator('dl.measures').screenshot({ path: `${dir}/visible-apk-measurement-simulated.png` }); }
  const { report, preview } = await reportFromDetails(page, details);
  const text = await preview.inputValue(); const record = JSON.parse(text).records[0];
  assert.deepEqual(record.state.storage, { source: 'apk_files', app_bytes: bytes, data_bytes: null, cache_bytes: null });
  for (const internal of ['TV:5555', 'HWSERIAL123', 'measurementId', 'generation', 'appBytes']) assert.equal(text.includes(internal), false);
  await report.getByRole('button', { name: 'Copy report' }).click();
  assert.equal(await page.evaluate(() => copied.at(-1)), text);
  if (evidence) {
    await preview.evaluate(el => el.scrollTop = el.scrollHeight);
    await preview.screenshot({ path: `${dir}/report-preview-simulated.png` });
    await writeFile(`${dir}/storage-report-simulated.json`, text + '\n');
  }
  assert.equal((await commands(page)).some(c => ['disable_package', 'uninstall_package', 'write_setting'].includes(c)), false);
});

test('diskstats remains the report source when a complete measured row exists', async t => {
  const page = await openApps(t);
  await page.locator(`.app-row[data-package="${katniss.package}"]`).click();
  const details = page.getByRole('dialog', { name: /App details for/ });
  await details.locator('[data-measure="storage"]').getByText(/Android refreshes these sizes/).waitFor();
  const { preview } = await reportFromDetails(page, details);
  assert.deepEqual(JSON.parse(await preview.inputValue()).records[0].state.storage, { source: 'diskstats', app_bytes: 12582912, data_bytes: 3145728, cache_bytes: 1048576 });
  assert.equal((await commands(page)).includes('app_apk_size'), false);
});

for (const mode of ['pending', 'failed', 'missing', 'negative']) test(`APK report keeps storage unknown for a ${mode} fallback`, async t => {
  const page = await openApps(t);
  await page.evaluate(mode => { handlers.app_apk_size = () => {
    if (mode === 'pending') return new Promise(() => {});
    if (mode === 'failed') return Promise.reject('APK unreadable');
    return { app_bytes: mode === 'negative' ? -1 : null, data_bytes: null, cache_bytes: null };
  }; }, mode);
  await page.locator('.app-row[data-package="com.example.streambox"]').click();
  const details = page.getByRole('dialog', { name: /App details for/ });
  await page.waitForFunction(() => calls.some(c => c.command === 'app_apk_size'));
  if (mode === 'pending') await details.getByText('Reading APK size…', { exact: true }).waitFor();
  else if (mode === 'failed' || mode === 'missing') await details.getByText(/The APK size couldn't be read either/).waitFor();
  else await details.getByText(/APK files only/).waitFor();
  const { preview } = await reportFromDetails(page, details);
  assert.equal(JSON.parse(await preview.inputValue()).records[0].state.storage, null);
});

test('remeasure invalidates a successful APK receipt and ignores an older fallback reply', async t => {
  const page = await openApps(t);
  let { details, report, preview } = await openMeasuredReport(page);
  assert.equal(JSON.parse(await preview.inputValue()).records[0].state.storage.app_bytes, 2048);
  await page.evaluate(() => { window.apkReplies = []; handlers.app_apk_size = () => new Promise(resolve => apkReplies.push(resolve)); });
  // Simulate a measurement refresh while its existing report preview is open.
  await details.getByRole('button', { name: /Re-measure/ }).evaluate(el => el.click());
  await page.waitForFunction(() => apkReplies.length === 1);
  assert.equal(JSON.parse(await preview.inputValue()).records[0].state.storage, null);
  await report.getByRole('button', { name: 'Close', exact: true }).click();
  await details.getByRole('button', { name: /Re-measure/ }).click();
  await page.waitForFunction(() => apkReplies.length === 2);
  await page.evaluate(() => apkReplies[1]({ app_bytes: 8192, data_bytes: null, cache_bytes: null }));
  await details.getByText(/APK 8.0 KB/).waitFor();
  await page.evaluate(() => apkReplies[0]({ app_bytes: 4096, data_bytes: null, cache_bytes: null })); await page.waitForTimeout(50);
  ({ report, preview } = await reportFromDetails(page, details));
  assert.equal(JSON.parse(await preview.inputValue()).records[0].state.storage.app_bytes, 8192);
});

for (const host of ['TV', 'OTHER']) test(`APK fallback cannot cross a ${host === 'TV' ? 'same-endpoint reconnect' : 'TV switch'}`, async t => {
  const page = await openApps(t);
  await page.evaluate(() => { handlers.app_apk_size = () => new Promise(resolve => { window.finishOldApk = resolve; }); });
  await page.locator('.app-row[data-package="com.example.streambox"]').click(); await page.waitForFunction(() => !!window.finishOldApk);
  await page.evaluate(async host => {
    handlers.list_devices = () => [{ id: 1, serial: `${host}:5555`, status: 'device', connection: 'network', device_type: 'shield', tv_evidence: 'tv', properties: { android_release: '11', serial_number: host === 'TV' ? 'HWSERIAL123' : 'OTHER_HARDWARE' } }];
    handlers.app_apk_size = () => new Promise(resolve => { window.finishNewApk = resolve; });
    await session.connect(host, 5555);
  }, host);
  await page.getByRole('dialog', { name: /App details for/ }).waitFor({ state: 'detached' });
  await page.locator('.app-row[data-package="com.example.streambox"]').click(); await page.waitForFunction(() => !!window.finishNewApk);
  await page.evaluate(() => finishOldApk({ app_bytes: 4096, data_bytes: null, cache_bytes: null })); await page.waitForTimeout(50);
  const details = page.getByRole('dialog', { name: /App details for/ });
  await details.getByText('Reading APK size…', { exact: true }).waitFor();
  let { report, preview } = await reportFromDetails(page, details);
  assert.equal(JSON.parse(await preview.inputValue()).records[0].state.storage, null);
  await report.getByRole('button', { name: 'Close', exact: true }).click();
  await page.evaluate(() => finishNewApk({ app_bytes: 16384, data_bytes: null, cache_bytes: null }));
  await details.getByText(/APK 16.0 KB/).waitFor();
  ({ preview } = await reportFromDetails(page, details));
  assert.equal(JSON.parse(await preview.inputValue()).records[0].state.storage.app_bytes, 16384);
});


test('an APK result for a closed package cannot replace the next package measurement', async t => {
  const page = await openApps(t);
  await page.evaluate(() => { handlers.app_apk_size = args => args.package === 'com.example.streambox'
    ? new Promise(resolve => { window.finishStreamApk = resolve; })
    : { app_bytes: 32768, data_bytes: null, cache_bytes: null }; });
  await page.locator('.app-row[data-package="com.example.streambox"]').click(); await page.waitForFunction(() => !!window.finishStreamApk);
  await page.getByRole('dialog', { name: /App details for/ }).getByRole('button', { name: 'Close', exact: true }).click();
  await page.locator('.app-row[data-package="com.example.launcherguard"]').click();
  const details = page.getByRole('dialog', { name: /App details for/ });
  await details.getByText(/APK 32.0 KB/).waitFor();
  await page.evaluate(() => finishStreamApk({ app_bytes: 4096, data_bytes: null, cache_bytes: null })); await page.waitForTimeout(50);
  const { preview } = await reportFromDetails(page, details);
  const record = JSON.parse(await preview.inputValue()).records[0];
  assert.equal(record.token, 'com.example.launcherguard');
  assert.equal(record.state.storage.app_bytes, 32768);
});
