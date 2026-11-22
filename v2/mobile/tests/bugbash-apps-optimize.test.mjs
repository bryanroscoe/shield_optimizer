// Optimize bug-bash regressions: a run interrupted by a lost connection (and
// its silent redial) still reports where it stopped and drops the Dashboard's
// cached count; losing Pro mid-run says so and offers Pro; the Apply
// confirmation names each app once; Keep all stays on one line, and there is
// no Select all in Optimize (desktop removed "Select all safe").
import assert from "node:assert/strict";
import { after, before, test } from "node:test";
import { chromium } from "playwright";
import { startViteServer } from "./helpers/vite-harness.mjs";

let server, origin, browser, rows;
before(async () => {
  ({ server, origin } = await startViteServer());
  rows = await server.ssrLoadModule("/src/lib/optimizeRows.ts");
  browser = await chromium.launch({ headless: true });
});
after(async () => { await browser?.close(); await server?.close(); });

const entry = (name, method = "disable") => ({
  package: `com.example.${name.toLowerCase().replaceAll(" ", "-")}`,
  name,
  method,
  risk: "safe",
  optimize_description: `${name} is optional.`,
  restore_description: "",
  default_optimize: true,
  default_restore: false,
  play_store: true,
});
const names = Array.from({ length: 10 }, (_, i) => `Bloat ${i + 1}`);

async function openOptimize(t, { viewport = { width: 384, height: 812 } } = {}) {
  const context = await browser.newContext({ viewport });
  const page = await context.newPage();
  page.setDefaultTimeout(10000);
  t.after(() => context.close());
  await page.addInitScript(({ catalog }) => {
    localStorage.clear();
    window.calls = [];
    window.handlers = {};
    window.states = Object.fromEntries(catalog.map((e) => [e.package, "enabled"]));
    window.__TAURI_INTERNALS__ = { invoke: async (command, args) => {
      window.calls.push({ command, args });
      if (window.handlers[command]) return window.handlers[command](args);
      switch (command) {
        case "get_entitlement": return "pro";
        case "wireless_connect": return { ok: true };
        case "wireless_status": return { connected: true };
        case "list_devices": return [{ id: 1, serial: "A:5555", name: "Living Room", model: "Shield", status: "device", connection: "network", device_type: "shield", tv_evidence: "tv", properties: { serial_number: "HW-OPT-1" } }];
        case "health_report": return { ram: { free_mb: 1024 }, storage: {}, display: {}, top_memory: [] };
        case "app_list_for_device": return catalog;
        case "package_states": return Object.fromEntries(args.packages.map((p) => [p, window.states[p] ?? "missing"]));
        case "prepare_optimize": return {
          mode: args.mode,
          items: catalog.map((e) => ({ entry: e, action: window.states[e.package] === "enabled" ? { kind: "disable" } : { kind: "skip", reason: "already_disabled" }, memory_mb: null })),
        };
        case "safety_info": return { kind: "safe", reason: "Reviewed and safe to remove.", source: "reviewed_catalog" };
        case "disable_package": window.states[args.package] = "disabled"; return { ok: true, message: "" };
        default: return { ok: true, message: "done" };
      }
    } };
  }, { catalog: names.map((n) => entry(n)) });
  await page.goto(origin);
  await page.evaluate(async () => {
    const { session } = await import("/src/lib/session.svelte.ts");
    const { router } = await import("/src/lib/router.svelte.ts");
    window.session = session; window.router = router;
    await session.connect("A", 5555);
    await session.loadBloat(true);
    router.reset("dashboard");
    router.navigate("optimize");
  });
  await page.getByText("10 selected", { exact: false }).waitFor();
  return page;
}

async function apply(page) {
  await page.getByRole("button", { name: /Apply optimization/ }).click();
  await page.locator(".dialog-card").getByRole("button", { name: "Apply", exact: true }).click();
}

test("a lost connection mid-run that redials silently still summarizes the run and drops the cached count", async (t) => {
  const page = await openOptimize(t);
  assert.equal(await page.evaluate(() => session.bloatLoaded), true);
  await page.evaluate(() => {
    let n = 0;
    handlers.disable_package = (args) => {
      if (++n === 6) return Promise.reject("Connection to the TV was lost: synthetic drop");
      window.states[args.package] = "disabled";
      return { ok: true, message: "" };
    };
  });
  await apply(page);
  await page.getByText("Stopped after 5 of 10. 1 result unknown. 4 left untouched.", { exact: true }).waitFor();
  await page.waitForFunction(() => session.liveness === "live");
  assert.equal(await page.evaluate(() => session.bloatLoaded), false, "the Dashboard must re-read its count");
  assert.equal(await page.evaluate(() => calls.filter((c) => c.command === "apply_performance_settings").length), 0);
  await page.evaluate(() => router.reset("dashboard"));
  await page.getByText("5 of 10 installed recommended apps are enabled.", { exact: true }).waitFor();
});

test("losing Pro mid-run says Pro is required and offers it rather than a plain stop", async (t) => {
  const page = await openOptimize(t);
  await page.evaluate(() => {
    let n = 0;
    handlers.disable_package = (args) => {
      window.states[args.package] = "disabled";
      if (++n === 3) window.session.entitlement = "free";
      return { ok: true, message: "" };
    };
  });
  await apply(page);
  await page.getByText("Stopped — Pro is required. 3 apps updated.", { exact: true }).waitFor();
  await page.locator(".paywall-card").waitFor();
  assert.equal(await page.getByText(/Stopped after/).count(), 0);
  assert.equal(await page.evaluate(() => calls.filter((c) => c.command === "disable_package").length), 3);
});

test("the Apply confirmation is sectioned by tier and names each app once", () => {
  const item = (name, method, kind, reason) => ({
    item: { entry: entry(name, method), action: { kind: method }, memory_mb: null },
    verdict: { kind, reason, source: "reviewed_catalog" },
  });
  const items = [
    item("Google Feedback", "disable", "safe", "Sends Google crash reports. Disabling stops those reports."),
    item("Play Movies", "uninstall", "safe", "Defunct service (replaced by YouTube)"),
    item("Play Music", "uninstall", "safe", "Defunct service (replaced by YouTube Music)"),
    item("Stadia", "uninstall", "safe", "Service shut down January 2023"),
    item("Nvidia Games", "disable", "caution", "Removable, but you may notice it go"),
    item("Nvidia Store", "disable", "caution", "Removable, but you may notice it go"),
    item("Mystery", "disable", "unknown", "No safety rule matched this package."),
  ];
  const { sections, message } = rows.applyConfirmation(items, "optimize");
  // Most serious first, one section per tier.
  assert.deepEqual(sections.map((s) => s.label), ["Unknown", "Caution", "Safe to remove"]);
  const [unknown, caution, safe] = sections;
  assert.deepEqual(unknown.groups, [{ names: ["Disable Mystery"], reason: "No safety rule matched this package." }]);
  // A shared consequence is said once for both apps, as a sentence.
  assert.deepEqual(caution.groups, [
    { names: ["Disable Nvidia Games", "Disable Nvidia Store"], reason: "Removable, but you may notice it go." },
  ]);
  // Safe rows already showed their description; the confirmation lists names.
  assert.deepEqual(safe.groups, [
    { names: ["Disable Google Feedback", "Uninstall Play Movies", "Uninstall Play Music", "Uninstall Stadia"], reason: "" },
  ]);
  const names = sections.flatMap((s) => s.groups.flatMap((g) => g.names));
  assert.equal(names.length, items.length);
  assert.equal(message.split("Restore can't bring it back").length - 1, 1);
  // Every uninstall here comes back the same way, so that is said once.
  assert.equal(message.split("Get it back from the Play Store").length - 1, 1);
  assert.match(message, /animation scales are set to 0\.5×\.$/);

  const restore = rows.applyConfirmation([{ item: { entry: entry("Netflix"), action: { kind: "enable" }, memory_mb: null }, verdict: null }], "restore");
  assert.deepEqual(restore, { sections: [], message: "Re-enable Netflix. Afterward, animation scales are reset to 1×." });
});

test("Keep all stays on one line at 150% text, and Optimize has no Select all", async (t) => {
  const page = await openOptimize(t, { viewport: { width: 275, height: 610 } });
  assert.equal(await page.getByRole("button", { name: /^Select all/ }).count(), 0);
  const button = page.getByRole("button", { name: "Keep all", exact: true });
  const lines = await button.evaluate((el) => {
    const range = document.createRange();
    range.selectNodeContents(el);
    return new Set([...range.getClientRects()].map((r) => Math.round(r.top))).size;
  });
  assert.equal(lines, 1);
  const box = await button.boundingBox();
  assert.ok(box.height >= 44, "keeps a 44px touch target");
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
});
