import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { chromium } from "playwright";
import { startViteServer } from "./helpers/vite-harness.mjs";

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

const entry = (name, options = {}) => ({
  package: options.package ?? `com.example.${name.toLowerCase().replaceAll(" ", "-")}`,
  name,
  method: options.method ?? "disable",
  risk: "safe",
  optimize_description: `${name} is optional on this TV.`,
  restore_description: `Restore ${name}.`,
  default_optimize: options.defaultOptimize ?? false,
  default_restore: false,
  play_store: true,
  review: options.review,
});

const item = (name, options = {}) => ({
  entry: entry(name, options),
  action: options.action ?? { kind: options.method === "uninstall" ? "uninstall" : "disable" },
  memory_mb: options.memoryMb ?? null,
});

async function createPage(t, options = {}) {
  const page = await browser.newPage({ viewport: { width: 384, height: 812 } });
  t.after(() => page.close());
  await page.addInitScript((options) => {
    localStorage.clear();
    window.calls = [];
    window.__TAURI_INTERNALS__ = {
      invoke: async (command, args) => {
        window.calls.push({ command, args });
        switch (command) {
          case "get_entitlement": return "pro";
          case "wireless_connect": return { ok: true };
          case "wireless_status": return { connected: true };
          case "list_devices": return [{
            id: 1, serial: "A:5555", name: "Living Room", model: "Shield", status: "device",
            connection: "network", device_type: "shield", properties: null,
          }];
          case "health_report": return options.health ?? {
            ram: { free_mb: 1024 }, storage: {}, display: {}, top_memory: [],
          };
          case "app_list_for_device": return options.catalog ?? [];
          case "package_states": return options.states ?? {};
          case "list_installed_packages":
            if (options.installedFails) throw new Error("pm list packages timed out");
            return options.installed ?? [];
          case "prepare_optimize": return options.plans?.[args.mode] ?? { mode: args.mode, items: [] };
          case "safety_info":
            return options.safety?.[args.package] ?? { kind: "caution", reason: "Review whether you use this app." };
          case "process_safety_info":
            return { kind: "unknown", reason: "Not an installed package." };
          case "list_snapshots": return options.snapshots ?? [];
          case "preview_apply": return options.preview;
          case "app_memory_map":
          case "app_usage_map":
          case "app_storage_map":
            return {};
          default: return { ok: true, message: "done", transport: "channel" };
        }
      },
    };
  }, options);
  await page.goto(origin);
  await page.evaluate(async () => {
    const { session } = await import("/src/lib/session.svelte.ts");
    const { router } = await import("/src/lib/router.svelte.ts");
    window.session = session;
    window.router = router;
    await session.connect("A", 5555);
  });
  return page;
}

async function openScreen(page, ...screens) {
  await page.evaluate((screens) => {
    window.router.reset("dashboard");
    for (const screen of screens) window.router.navigate(screen);
  }, screens);
}

test("Optimize Back returns to the screen that opened it", async (t) => {
  const page = await createPage(t, {
    plans: { optimize: { mode: "optimize", items: [item("Prime Video", { defaultOptimize: true })] } },
  });
  await openScreen(page, "diagnostics", "optimize");
  await page.getByRole("heading", { name: "Optimize" }).waitFor();
  await page.getByRole("button", { name: "Back", exact: true }).click();
  assert.equal(await page.evaluate(() => window.router.current), "diagnostics");
});

test("Optimize search filters by name and package id", async (t) => {
  const prime = item("Prime Video", { defaultOptimize: true, package: "com.amazon.amazonvideo.livingroom" });
  const photos = item("Photos", { defaultOptimize: true });
  const page = await createPage(t, {
    plans: { optimize: { mode: "optimize", items: [prime, photos] } },
  });
  await openScreen(page, "optimize");
  await page.getByText("Photos", { exact: true }).waitFor();
  const search = page.getByRole("textbox", { name: "Search the plan" });
  await search.fill("amazonvideo");
  assert.equal(await page.getByText("Prime Video", { exact: true }).count(), 1);
  assert.equal(await page.getByText("Photos", { exact: true }).count(), 0);
  await search.fill("PHOTO");
  assert.equal(await page.getByText("Photos", { exact: true }).count(), 1);
  await search.fill("netflix");
  await page.getByText("No apps on this tab match “netflix”.").waitFor();
  // Selection is the plan's, not the filter's: both stay selected.
  assert.match(await page.locator(".optimize-summary-card").innerText(), /2 selected/);
  await page.getByRole("button", { name: "Clear search" }).first().click();
  assert.equal(await page.getByText("Photos", { exact: true }).count(), 1);
});

test("tapping an Optimize row opens the app sheet, not a toggle", async (t) => {
  const prime = item("Prime Video", { defaultOptimize: true });
  const page = await createPage(t, {
    plans: { optimize: { mode: "optimize", items: [prime] } },
  });
  await openScreen(page, "optimize");
  await page.getByRole("button", { name: "Details for Prime Video" }).click();
  await page.locator(".sheet .app-pkg", { hasText: prime.entry.package }).waitFor();
  assert.match(await page.locator(".optimize-summary-card").innerText(), /1 selected/);
  assert.equal(
    await page.evaluate(() => window.calls.some((c) => ["disable_package", "uninstall_package"].includes(c.command))),
    false,
  );
});

test("Safe to remove and review chips stay on one line at 384px", async (t) => {
  const safe = item("A Long Application Name That Wraps", { defaultOptimize: true, memoryMb: 512 });
  const review = item("Reviewed Player", { review: true, method: "uninstall" });
  const page = await createPage(t, {
    plans: { optimize: { mode: "optimize", items: [safe, review] } },
    safety: {
      [safe.entry.package]: { kind: "safe", reason: "Reviewed and rated safe." },
      [review.entry.package]: { kind: "caution", reason: "Review whether you use this app." },
    },
  });
  await openScreen(page, "optimize");
  const chip = page.locator(".tier-chip", { hasText: "Safe to remove" });
  await chip.waitFor();
  const box = await chip.boundingBox();
  assert.ok(box.height < 22, `chip wrapped: ${box.height}px tall`);
  await page.getByRole("button", { name: "Optional apps", exact: true }).click();
  const pill = page.locator(".tier-chip.review", { hasText: "Review: uninstall if unused" });
  await pill.waitFor();
  assert.ok((await pill.boundingBox()).height < 22);
});

test("Health suggests per row: app recommendation, process, and package verdicts", async (t) => {
  const prime = entry("Prime Video", { defaultOptimize: true, package: "com.amazon.tv" });
  const page = await createPage(t, {
    catalog: [prime],
    installed: [
      { package: "com.amazon.tv", system: false, enabled: true },
      { package: "com.other.app", system: false, enabled: true },
    ],
    safety: {
      "com.amazon.tv": { kind: "safe", reason: "Reviewed and rated safe." },
      "com.other.app": { kind: "unknown", reason: "No rule matched." },
    },
    health: {
      ram: { free_mb: 1024, total_mb: 2048 }, storage: {}, display: {},
      top_memory: [
        { process: "com.amazon.tv:remote", pid: 41, package: "com.amazon.tv", mb: 300 },
        { process: "surfaceflinger", pid: 7, package: null, mb: 200 },
        { process: "com.other.app", pid: 12, package: "com.other.app", mb: 120 },
        { process: "media.codec", pid: 9, package: "media.codec", mb: 90 },
      ],
    },
  });
  await openScreen(page, "diagnostics");
  const rows = page.locator(".consumer-row");
  await rows.first().waitFor();
  await page.waitForFunction(() => document.querySelectorAll(".risk-badge.act").length === 1);
  const text = async (i) => (await rows.nth(i).innerText()).replace(/\s+/g, " ");
  assert.match(await text(0), /com\.amazon\.tv:remote.*pid 41.*Safety: Safe to remove.*Disable/);
  assert.match(await text(1), /surfaceflinger.*Not an app/);
  assert.match(await text(2), /com\.other\.app.*Unknown/);
  // Package-shaped but not installed: a process, and the catalog is not applied.
  assert.match(await text(3), /media\.codec.*Not an app/);
  const calls = await page.evaluate(() => window.calls);
  assert.ok(calls.some((c) => c.command === "process_safety_info" && c.args.process === "media.codec"));
  assert.ok(calls.some((c) => c.command === "safety_info" && c.args.package === "com.amazon.tv"));
  assert.ok(!calls.some((c) => c.command === "safety_info" && c.args.package === "media.codec"));
  // A confirmed app opens the sheet; a process row is not a button.
  assert.equal(await page.getByRole("button", { name: "Details for surfaceflinger" }).count(), 0);
  await page.getByRole("button", { name: "Details for com.amazon.tv:remote" }).click();
  await page.locator(".sheet .app-pkg", { hasText: "com.amazon.tv" }).waitFor();
});

test("Health never calls a package-shaped name Not an app when installed apps can't be read", async (t) => {
  const page = await createPage(t, {
    installedFails: true,
    health: {
      ram: { free_mb: 1024 }, storage: {}, display: {},
      top_memory: [{ process: "com.amazon.tv", pid: 41, package: "com.amazon.tv", mb: 300 }],
    },
  });
  await openScreen(page, "diagnostics");
  await page.getByText("Couldn't read the TV's installed apps", { exact: false }).waitFor();
  const row = (await page.locator(".consumer-row").first().innerText()).replace(/\s+/g, " ");
  assert.match(row, /Not checked/);
  assert.doesNotMatch(row, /Not an app/);
});

test("Snapshots: Beta, Preview restore, and a real Now column", async (t) => {
  const snap = {
    path: "/data/snap.json", filename: "snap.json", saved_at: "2026-10-01T10:00:00Z", label: "Before",
    device_name: "Living Room", device_serial: "A:5555", device_type: "shield",
    disabled_count: 2, settings_count: 3, launcher: "com.home",
  };
  const page = await createPage(t, {
    snapshots: [snap],
    preview: {
      cross_device_warning: null,
      packages_to_disable: ["com.bloat"],
      packages_already_disabled: ["com.old"],
      packages_not_installed: [],
      launcher_to_set: null,
      current_launcher: "com.home",
      launcher_not_installed: null,
      settings_to_write: { "global:window_animation_scale": "0.5" },
      settings_to_delete: [],
      settings_already_set: ["secure:screensaver_enabled"],
      current_values: { "global:window_animation_scale": "1.0", "secure:screensaver_enabled": "0" },
    },
  });
  await openScreen(page, "snapshots");
  await page.locator(".beta-tag", { hasText: "Beta" }).waitFor();
  await page.getByText("It never re-enables anything or reinstalls apps.", { exact: false }).first().waitFor();
  await page.getByRole("button", { name: "Preview restore" }).click();
  const card = page.locator(".plan-card");
  await card.getByText('Restore "Before"?').waitFor();
  const acting = (await card.getByRole("list", { name: "Changes" }).innerText()).replace(/\s+/g, " ");
  assert.match(acting, /com\.bloat Now enabled Disable/i);
  assert.match(acting, /global:window_animation_scale Now 1\.0 Set → 0\.5/i);
  const same = (await card.getByRole("list", { name: "No change" }).innerText()).replace(/\s+/g, " ");
  assert.match(same, /Home app Now com\.home Already Home/i);
  assert.match(same, /secure:screensaver_enabled Now 0 Already set/i);
  assert.match(same, /com\.old Now disabled Already disabled/i);
  assert.match(await card.innerText(), /never re-enables anything or reinstalls apps/);
  assert.equal(
    await page.evaluate(() => window.calls.some((c) => c.command === "apply_snapshot")),
    false,
  );
});
