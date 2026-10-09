import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
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

async function openApps(t, { openerFails = false } = {}) {
  const context = await browser.newContext({ viewport: { width: 384, height: 812 } });
  const page = await context.newPage();
  t.after(() => context.close());
  await page.addInitScript(
    ({ fixture, openerFails }) => {
      window.calls = [];
      window.copied = [];
      Object.defineProperty(navigator, "clipboard", {
        configurable: true,
        value: { writeText: async (text) => void window.copied.push(text) },
      });
      window.__TAURI_INTERNALS__ = {
        invoke: async (command, args) => {
          window.calls.push({ command, args });
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
