// Apps screen bug-bash regressions: the row's ⋮ is a real menu button, rows
// stay readable at large text, section failures read like every other error,
// the search field shows focus, Free sees the PRO marker on Disable, the
// report sheet keeps privacy visible and how-it-works on demand, reports use
// the page's live state, removals drop shared caches even after a redial, and
// sideloaded apps survive the unknown-package log's trimming.
import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { chromium } from "playwright";
import { startViteServer } from "./helpers/vite-harness.mjs";

let server, origin, browser;
before(async () => { ({ server, origin } = await startViteServer()); browser = await chromium.launch({ headless: true }); });
after(async () => { await browser?.close(); await server?.close(); });

const catalogEntry = {
  package: "com.google.android.feedback",
  name: "Google Feedback",
  method: "disable",
  risk: "safe",
  optimize_description: "Sends crash reports.",
  restore_description: "",
  default_optimize: true,
  default_restore: false,
  play_store: false,
};
const others = [
  { package: "com.teamsmart.videomanager.tv", name: "SmartTube", system: false, enabled: true },
  { package: "com.android.helper", name: "System Helper", system: true, enabled: true },
];

async function openApps(t, { entitlement = "pro", viewport = { width: 384, height: 812 }, catalogFails = false, otherPackages = others } = {}) {
  const context = await browser.newContext({ viewport });
  const page = await context.newPage();
  page.setDefaultTimeout(10000);
  t.after(() => context.close());
  await page.addInitScript(({ entitlement, catalogFails, catalogEntry, otherPackages }) => {
    localStorage.clear();
    window.calls = [];
    window.handlers = {};
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText: async () => {} } });
    window.__TAURI_INTERNALS__ = { invoke: async (command, args) => {
      window.calls.push({ command, args });
      if (window.handlers[command]) return window.handlers[command](args);
      switch (command) {
        case "get_entitlement": return entitlement;
        case "wireless_connect": return { ok: true };
        case "wireless_status": return { connected: true };
        case "list_devices": return [{ id: 1, serial: "TV:5555", name: "TV", model: "Shield", status: "device", connection: "network", device_type: "shield", tv_evidence: "tv", properties: { android_release: "11", serial_number: "HW-APPS-1" } }];
        case "health_report": return { ram: {}, storage: {}, display: {}, top_memory: [] };
        case "list_other_packages": return otherPackages.map((app) => ({ ...app }));
        case "app_list_for_device":
          if (catalogFails) throw "adb: error: closed (os error 104)";
          return [{ ...catalogEntry }];
        case "package_states": return Object.fromEntries(args.packages.map((p) => [p, "enabled"]));
        case "app_memory_map": case "app_usage_map": case "app_storage_map": return {};
        case "app_apk_size": return { app_bytes: 2048, data_bytes: null, cache_bytes: null };
        case "safety_info": return args.package === catalogEntry.package
          ? { kind: "safe", reason: "Sends Google crash reports.", source: "reviewed_catalog" }
          : { kind: "unknown", reason: "Not in any list.", source: "no_record" };
        default: return { ok: true, message: "" };
      }
    } };
  }, { entitlement, catalogFails, catalogEntry, otherPackages });
  await page.goto(origin);
  await page.evaluate(async (entitlement) => {
    const { session } = await import("/src/lib/session.svelte.ts");
    const { router } = await import("/src/lib/router.svelte.ts");
    window.session = session; window.router = router;
    session.entitlement = entitlement;
    await session.connect("TV", 5555);
    router.reset("dashboard");
    router.navigate("apps");
  }, entitlement);
  await page.locator(".app-row").first().waitFor();
  return page;
}

test("the row's ⋮ is a named button that opens the action menu; the row body still opens Details", async (t) => {
  const page = await openApps(t);
  const more = page.getByRole("button", { name: "More actions for SmartTube", exact: true });
  await more.click();
  const menu = page.getByRole("menu", { name: "Actions for SmartTube" });
  await menu.waitFor();
  assert.equal(await page.getByRole("dialog", { name: /App details for/ }).count(), 0);
  await page.keyboard.press("Escape");
  await menu.waitFor({ state: "detached" });
  assert.equal(await more.evaluate((el) => document.activeElement === el), true, "focus returns to the ⋮ button");

  await page.locator('.app-row[data-package="com.teamsmart.videomanager.tv"]').click();
  await page.getByRole("dialog", { name: "App details for SmartTube" }).waitFor();
  assert.equal(await page.getByRole("menu").count(), 0);
});

test("at 150% text (275px) names wrap instead of truncating and chips never collide", async (t) => {
  const page = await openApps(t, { viewport: { width: 275, height: 610 } });
  await page.locator(".verdict-chip", { hasText: "Safe to remove" }).waitFor();
  const problems = await page.evaluate(() => {
    const out = [];
    if (document.documentElement.scrollWidth > innerWidth) out.push("page scrolls sideways");
    for (const card of document.querySelectorAll(".app-card")) {
      const c = card.getBoundingClientRect();
      const name = card.querySelector(".app-name-text");
      if (name.scrollWidth > name.clientWidth + 1) out.push(`${name.textContent}: name clipped`);
      if (name.getBoundingClientRect().width < c.width * 0.45) out.push(`${name.textContent}: name squeezed`);
      const parts = [...card.querySelectorAll(".status-badge, .chip, .rec-text, .app-name-text")].map((el) => [el, el.getBoundingClientRect()]);
      for (const [el, b] of parts) {
        if (b.left < c.left - 0.5 || b.right > c.right + 0.5) out.push(`${el.className}: outside the row`);
      }
      for (let i = 0; i < parts.length; i++) {
        for (let j = i + 1; j < parts.length; j++) {
          const [a, ra] = parts[i];
          const [b, rb] = parts[j];
          const overlap = Math.min(ra.right, rb.right) - Math.max(ra.left, rb.left) > 0.5
            && Math.min(ra.bottom, rb.bottom) - Math.max(ra.top, rb.top) > 0.5;
          if (overlap) out.push(`${a.className} overlaps ${b.className}`);
        }
      }
    }
    return out;
  });
  assert.deepEqual(problems, []);
  assert.equal(await page.locator(".app-name-text", { hasText: "Google Feedback" }).innerText(), "Google Feedback");
});

test("the search field shows a visible focus ring", async (t) => {
  const page = await openApps(t);
  const box = page.locator(".search-box");
  const before = await box.evaluate((el) => getComputedStyle(el).outlineStyle);
  await page.getByPlaceholder("Search apps or packages").focus();
  const ring = await box.evaluate((el) => {
    const s = getComputedStyle(el);
    return { style: s.outlineStyle, width: s.outlineWidth };
  });
  assert.equal(before, "none");
  assert.deepEqual(ring, { style: "solid", width: "2px" });
});

test("a failed section read is an error notice with a title, code and Retry", async (t) => {
  const page = await openApps(t, { catalogFails: true });
  const notice = page.locator(".section-error .error-notice");
  await notice.waitFor();
  assert.match(await notice.innerText(), /Couldn't load catalog apps/);
  assert.ok(await notice.getAttribute("data-code"));
  await notice.getByRole("button", { name: "Copy details" }).waitFor();
  await page.locator(".section-error").getByRole("button", { name: "Retry", exact: true }).click();
  await page.waitForFunction(() => calls.filter((c) => c.command === "app_list_for_device").length >= 2);
});

test("the long-press Disable carries the PRO marker on Free only", async (t) => {
  const page = await openApps(t, { entitlement: "free" });
  const more = page.getByRole("button", { name: "More actions for SmartTube", exact: true });
  await more.click();
  const disable = page.getByRole("menu").getByRole("menuitem", { name: /Disable/ });
  await disable.locator(".pro-badge", { hasText: "PRO" }).waitFor();
  assert.equal(await page.getByRole("menu").getByRole("menuitem", { name: /Copy package name/ }).locator(".pro-badge").count(), 0);
  await page.keyboard.press("Escape");
  await page.evaluate(() => { session.entitlement = "pro"; });
  await more.click();
  assert.equal(await page.getByRole("menu").getByRole("menuitem", { name: /Disable/ }).locator(".pro-badge").count(), 0);
});

test("the report sheet keeps privacy visible and explains copying on demand", async (t) => {
  const page = await openApps(t);
  await page.getByRole("button", { name: "More actions for SmartTube", exact: true }).click();
  await page.getByRole("menu").getByRole("menuitem", { name: "Report this app" }).click();
  const report = page.getByRole("dialog", { name: "Report this app", exact: true });
  await report.getByText(/Nothing is sent automatically/).waitFor();
  assert.ok(await report.getByText(/It includes no serial number, IP address or other installed apps/).isVisible());
  const howTo = page.getByRole("dialog", { name: "How reporting works" }).getByText(/Copy puts the report on your clipboard and doesn't save a file/);
  assert.equal(await howTo.count(), 0, "how-to is on demand");
  // The sheet moves focus inside itself on the next frame; wait for that.
  await page.waitForFunction(() => document.querySelector('[aria-labelledby="app-report-title"]').contains(document.activeElement));
  const summary = report.getByRole("button", { name: "How reporting works", exact: true });
  await summary.focus();
  await page.keyboard.press("Enter");
  await howTo.waitFor();
  // Done returns to the report sheet, which stays open beneath.
  await page.getByRole("button", { name: "Done", exact: true }).click();
  await howTo.waitFor({ state: "detached" });
  assert.equal(await summary.evaluate((el) => el === document.activeElement), true);
  assert.ok(await report.isVisible());
});

test("a recognised row's report carries the page's current state, and none while a run is changing it", async (t) => {
  const page = await openApps(t);
  const row = page.locator(`.app-row[data-package="${catalogEntry.package}"]`);
  await row.click();
  const sheet = page.getByRole("dialog", { name: "App details for Google Feedback" });
  await sheet.getByRole("button", { name: "Report this app" }).click();
  const report = page.getByRole("dialog", { name: "Report this app", exact: true });
  await report.getByRole("checkbox").check();
  await report.getByRole("button", { name: "Show the full report" }).click();
  const state = () => page.evaluate(() => JSON.parse(document.querySelector('[aria-label="Report preview"]').value).records[0].state);
  assert.equal((await state()).enabled, true);
  await page.evaluate(() => { session.applyInProgress = true; });
  await page.waitForFunction(() => JSON.parse(document.querySelector('[aria-label="Report preview"]').value).records[0].state.enabled === null);
  await page.evaluate(() => { session.applyInProgress = false; });
});

test("a removal whose reply lands after a silent redial still drops the shared caches", async (t) => {
  const page = await openApps(t);
  await page.evaluate(() => session.loadBloat(true));
  assert.equal(await page.evaluate(() => session.bloatLoaded), true);
  await page.evaluate(() => { handlers.disable_package = () => new Promise((resolve) => { window.finishDisable = resolve; }); });
  await page.getByRole("button", { name: "More actions for SmartTube", exact: true }).click();
  await page.getByRole("menu").getByRole("menuitem", { name: /Disable/ }).click();
  await page.locator(".dialog-card").getByRole("button", { name: "Disable", exact: true }).click();
  await page.waitForFunction(() => !!window.finishDisable);
  assert.equal(await page.evaluate(() => session.recoverOrMarkLost()), true);
  assert.equal(await page.evaluate(() => session.bloatLoaded), true, "the redial alone keeps the cache");
  await page.evaluate(() => finishDisable({ ok: true, message: "" }));
  await page.waitForFunction(() => session.bloatLoaded === false && session.healthLoaded === false);
});

test("sideloaded apps survive the unknown-package log's 100-record trim on a TV with many system packages", async (t) => {
  // list_other_packages puts third-party packages first, so oldest-first
  // trimming alone would drop exactly these.
  const sideloaded = ["com.teamsmart.videomanager.tv", "org.fdroid.fdroid", "com.spocky.projengmenu"];
  const otherPackages = [
    ...sideloaded.map((pkg, i) => ({ package: pkg, name: `Sideloaded ${i}`, system: false, enabled: true })),
    ...Array.from({ length: 140 }, (_, i) => ({ package: `com.vendor.system${i}`, name: null, system: true, enabled: true })),
  ];
  const page = await openApps(t, { otherPackages });
  await page.waitForFunction(() => !!localStorage.getItem("atv.unknownDiagnostics.v1"));
  const stored = await page.evaluate(() => JSON.parse(localStorage.getItem("atv.unknownDiagnostics.v1")));
  const tokens = stored.records.map((r) => r.token);
  assert.equal(stored.records.length, 100);
  assert.equal(stored.truncated, true);
  for (const pkg of sideloaded) assert.ok(tokens.includes(pkg), `${pkg} kept`);
  assert.ok(stored.records.every((r) => !("system" in r)), "the priority hint is never stored");
});

test("an uninstall confirm shows the tier chip and two consequence bullets, with the how-to behind an expander", async (t) => {
  // 275 CSS px is how Android WebView lays out a 412dp phone at 150% text.
  for (const viewport of [{ width: 384, height: 812 }, { width: 275, height: 610 }]) {
    const page = await openApps(t, { viewport });
    await page.locator('.app-row[data-package="com.teamsmart.videomanager.tv"]').click();
    const sheet = page.getByRole("dialog", { name: "App details for SmartTube" });
    await sheet.getByRole("button", { name: /^Uninstall/ }).click();
    const dialog = page.getByRole("dialog", { name: "Uninstall SmartTube?" });
    await dialog.waitFor();
    assert.equal(await dialog.getByTestId("removal-tier").evaluate((el) => el.lastChild.textContent), "Unknown");
    assert.match(await dialog.getByTestId("removal-tier").textContent(), /Safety: Unknown$/);
    const points = dialog.getByRole("list", { name: "What happens" }).getByRole("listitem");
    assert.equal(await points.count(), 2);
    assert.equal(await points.nth(0).innerText(), "Removed for this TV's user");
    assert.match(await points.nth(1).innerText(), /Play Store/);
    await dialog.getByText("Not in any list.").waitFor();
    // No sentence-per-verdict paragraph is left in the prompt.
    assert.equal(await dialog.getByText(/The removal impact is Unknown/).count(), 0);

    const toggle = dialog.getByRole("button", { name: "Getting it back" });
    const panel = dialog.locator(".unfurl-panel");
    assert.equal(await toggle.getAttribute("aria-expanded"), "false");
    assert.equal(await panel.isVisible(), false);
    await toggle.click();
    assert.equal(await toggle.getAttribute("aria-expanded"), "true");
    await panel.getByText(/Reinstall in its app sheet can bring it back while its APK is still on the TV/).waitFor();
    await toggle.focus();
    await page.keyboard.press("Enter");
    assert.equal(await toggle.getAttribute("aria-expanded"), "false");
    await page.keyboard.press("Space");
    assert.equal(await toggle.getAttribute("aria-expanded"), "true");
    // The consequences stay on screen whichever way the expander is set.
    assert.equal(await points.nth(0).isVisible(), true);
    assert.equal(await points.nth(1).isVisible(), true);
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
    assert.equal(await dialog.locator(".dialog-body").evaluate((el) => el.scrollWidth <= el.clientWidth + 1), true);

    await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
    assert.equal(await page.evaluate(() => calls.some((c) => c.command === "uninstall_package")), false);
  }
});

test("a disable confirm shows the tier chip and says Disable is reversible", async (t) => {
  const page = await openApps(t);
  await page.getByRole("button", { name: "More actions for SmartTube", exact: true }).click();
  await page.getByRole("menu").getByRole("menuitem", { name: /Disable/ }).click();
  const dialog = page.getByRole("dialog", { name: "Disable SmartTube?" });
  await dialog.getByTestId("removal-tier").waitFor();
  const points = dialog.getByRole("list", { name: "What happens" }).getByRole("listitem");
  assert.deepEqual(await points.allInnerTexts(), ["Disable is reversible with Enable"]);
  assert.equal(await dialog.getByRole("button", { name: "Getting it back" }).count(), 0);
});
