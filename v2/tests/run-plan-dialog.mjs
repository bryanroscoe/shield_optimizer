// The Optimize/Restore confirmation used to be a native alert holding every
// removal's safety, reason and full uninstall paragraph, with the Uninstall
// rule repeated per app: a wall of text past 2,000 characters. It is now an
// in-app dialog with one section per tier and one line per app.
//
// This checks the builder (every app named once, sections most serious first,
// "can't come back" lines kept) and the dialog in the demo app: Cancel and
// Escape change nothing, Run does, and the layout holds at the 720px minimum
// with larger text in both themes. SIMULATED: demo fixtures, no TV.
import { freePort } from "./lib/free-port.mjs";
import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const V2 = join(HERE, "..");
const MUTATIONS = /^(disable_package|uninstall_package|enable_package)$/;

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

function entry(pkg, extra = {}) {
  return { package: pkg, name: pkg, method: "disable", risk: "safe", play_store: false, defunct: false, ...extra };
}

async function builderChecks(server) {
  const { runConfirmation, removalConfirmation } = await server.ssrLoadModule("/src/lib/run-confirmation.ts");
  const items = [
    { package: "a.safe", name: "Safe App", action: "disable", verdict: { kind: "safe", source: "reviewed_catalog", reason: "Optional extra." }, entry: entry("a.safe", { play_store: true }) },
    { package: "b.store", name: "Store App", action: "uninstall", verdict: { kind: "safe", source: "reviewed_catalog", reason: "Optional." }, entry: entry("b.store", { play_store: true }) },
    { package: "c.gone", name: "Gone Service", action: "uninstall", verdict: { kind: "safe", source: "reviewed_catalog", reason: "Service shut down." }, entry: entry("c.gone", { defunct: true }) },
    { package: "d.caution", name: "Careful App", action: "disable", verdict: { kind: "caution", source: "reviewed_catalog", reason: "Voice search stops working" }, entry: entry("d.caution") },
    { package: "e.unknown", name: "Mystery", action: "disable", verdict: { kind: "unknown", source: "none", reason: "No safety rule matched this package." }, entry: null },
  ];
  const { sections, uninstalls } = runConfirmation(items);
  assert.deepEqual(sections.map((s) => s.kind), ["unknown", "caution", "safe"], "most serious tier first");
  const named = sections.flatMap((s) => s.apps.map((a) => a.package));
  assert.deepEqual([...named].sort(), items.map((i) => i.package).sort(), "every app is named exactly once");
  assert.equal(uninstalls, 2);
  const apps = Object.fromEntries(sections.flatMap((s) => s.apps.map((a) => [a.package, { ...a, showReason: s.showReason }])));
  assert.equal(apps["d.caution"].showReason, true, "a Caution reason is the consequence and stays visible");
  assert.equal(apps["a.safe"].showReason, false, "a Safe reason folds");
  assert.equal(apps["c.gone"].noWayBack, true);
  assert.match(apps["c.gone"].outcome, /Discontinued: no way to download it again/);
  assert.equal(apps["b.store"].noWayBack, false);
  assert.equal(apps["a.safe"].outcome, null, "Disable carries no uninstall outcome");

  const restore = runConfirmation([{ package: "x", name: "X", action: "enable", verdict: null, entry: null }]);
  assert.deepEqual(restore.sections.map((s) => s.kind), ["enable"]);

  const single = removalConfirmation("uninstall", "c.gone", "Gone Service", items[2].verdict, items[2].entry, null);
  assert.equal(single, "Uninstall Gone Service (c.gone)?\n\nSafe to remove.\n\n• Removed for this TV's user.\n• Discontinued: no way to download it again.");
  const caution = removalConfirmation("disable", "d.caution", "Careful App", items[3].verdict, null, "This TV isn't supported.");
  assert.match(caution, /^Disable Careful App \(d\.caution\)\?\n\nCaution: Voice search stops working\.\n\n• /);
  assert.match(caution, /This TV isn't supported\.$/);
  assert.doesNotMatch(caution, /Package:|Reason:|Proceed\?/);
  console.log("Run confirmation builder passed: tiers ordered, each app once, consequences kept.");
}

async function openPlan(page, base) {
  await page.addInitScript(() => localStorage.setItem("shieldopt.demo.reviewedCatalog", "1"));
  await page.goto(base, { waitUntil: "networkidle" });
  await page.evaluate(() => {
    window.runCalls = [];
    const invoke = window.__TAURI_INTERNALS__.invoke;
    window.__TAURI_INTERNALS__.invoke = async (command, args) => {
      window.runCalls.push(command);
      return invoke(command, args);
    };
  });
  await page.getByText("NVIDIA SHIELD", { exact: false }).first().click();
  await page.getByRole("tab", { name: "Optimize" }).click();
  const panel = page.locator("#tabpanel-optimize");
  await panel.locator(".mode-box").getByRole("button", { name: "Optimize", exact: true }).click();
  await panel.locator("table.optimize-table tbody tr").first().waitFor();
  await page.waitForFunction(() => {
    const cells = [...document.querySelectorAll("#tabpanel-optimize td.verdict-cell")];
    return cells.length > 0 && cells.every((c) => c.dataset.verdict !== "checking");
  }, null, { timeout: 15000 });
  return panel;
}

async function armedPackages(panel) {
  return panel.evaluate((root) =>
    [...root.querySelectorAll("table.optimize-table tbody tr")]
      .filter((tr) => {
        const label = tr.querySelector(".action-radio .radio-pill.active .radio-label")?.textContent?.trim();
        return label && label !== "Keep";
      })
      .map((tr) => tr.querySelector(".pkg-id")?.textContent?.trim() ?? ""),
  );
}

async function mutations(page) {
  return page.evaluate((re) => window.runCalls.filter((c) => new RegExp(re).test(c)), MUTATIONS.source);
}

async function exercise({ browser, base }) {
  const evidence = process.env.RUN_PLAN_EVIDENCE;
  if (evidence) await mkdir(evidence, { recursive: true });
  const page = await browser.newPage({ viewport: { width: 1400, height: 1000 }, colorScheme: "dark",
    ...(evidence ? { recordVideo: { dir: join(evidence, "raw"), size: { width: 1400, height: 1000 } } } : {}) });
  const panel = await openPlan(page, base);
  if (evidence) await page.evaluate(() => {
    const label = document.createElement("div");
    label.textContent = "SIMULATED · demo fixtures, no TV";
    label.style.cssText = "position:fixed;bottom:0;right:0;z-index:99999;padding:6px 10px;background:#1a2540;color:white;font:13px sans-serif";
    document.body.appendChild(label);
  });

  // Arm an uninstall of an app that can't come back, when the plan offers one.
  const noWayBack = panel.locator("tr").filter({ has: page.locator(".radio-label", { hasText: /^Uninstall/ }) });
  let uninstallPkg = null;
  for (let i = 0; i < await noWayBack.count(); i++) {
    const row = noWayBack.nth(i);
    await row.locator(".radio-pill", { has: page.locator(".radio-label", { hasText: /^Uninstall/ }) }).click();
    const note = (await row.locator(".uninstall-note").innerText().catch(() => "")).trim();
    if (/Discontinued|Not on the Play Store/.test(note)) {
      uninstallPkg = (await row.locator(".pkg-id").innerText()).trim();
      break;
    }
    await row.locator(".radio-pill", { has: page.locator(".radio-label", { hasText: /^Keep$/ }) }).click();
  }
  assert.ok(uninstallPkg, "the demo plan offers an uninstall that can't come back");

  const armed = await armedPackages(panel);
  assert.ok(armed.length >= 2, `the demo plan arms several rows: ${armed}`);
  const runButton = panel.getByRole("button", { name: /^Run plan/ });

  // Cancel changes nothing.
  await runButton.click();
  const dialog = page.locator("[data-run-plan-dialog]");
  await dialog.waitFor();
  assert.equal(await dialog.getAttribute("role"), "dialog");
  assert.match(await dialog.locator("h2").innerText(), new RegExp(`^Run Optimize on ${armed.length} apps?\\?$`));
  const listed = await dialog.locator("[data-run-app]").evaluateAll((els) => els.map((el) => el.getAttribute("data-run-app")));
  assert.deepEqual([...listed].sort(), [...armed].sort(), "every selected app is named exactly once");
  for (const pkg of armed) {
    assert.equal(await dialog.locator(`[data-run-app="${pkg}"]`).count(), 1, `${pkg} named once`);
  }
  const order = await dialog.locator("[data-run-section]").evaluateAll((els) => els.map((el) => el.getAttribute("data-run-section")));
  const rank = { never_disable: 0, unknown: 1, caution: 2, safe: 3, enable: 4 };
  assert.deepEqual(order, [...order].sort((a, b) => rank[a] - rank[b]), `tiers most serious first: ${order}`);
  if (uninstallPkg) {
    const outcome = dialog.locator(`[data-run-app="${uninstallPkg}"] [data-run-outcome]`);
    assert.ok(await outcome.isVisible(), "the can't-come-back line is visible without expanding anything");
    assert.match(await outcome.innerText(), /Discontinued: no way to download it again|Not on the Play Store/);
    assert.ok(await dialog.locator("[data-uninstall-rule]").isVisible(), "the Uninstall rule is stated");
    assert.equal(await dialog.locator("[data-uninstall-rule]").count(), 1, "the Uninstall rule is stated once");
  }
  const text = await dialog.innerText();
  assert.ok(text.length < 1500, `the dialog stays short by default: ${text.length} characters`);
  assert.equal(await page.evaluate(() => document.activeElement?.textContent?.trim()), "Cancel", "focus starts on Cancel");
  if (evidence) await page.screenshot({ path: join(evidence, "after-run-dialog-dark.png") });

  // Details fold the package id and routine reason.
  const firstDetails = dialog.locator("details.app-more").first();
  await firstDetails.locator("summary").click();
  assert.equal(await firstDetails.evaluate((el) => el.open), true);
  await firstDetails.locator("summary").click();

  await dialog.getByRole("button", { name: "Cancel", exact: true }).click();
  await dialog.waitFor({ state: "detached" });
  assert.deepEqual(await mutations(page), [], "Cancel changed nothing");
  await page.waitForFunction(() => document.activeElement?.textContent?.trim().startsWith("Run plan"));
  assert.equal(await panel.locator(".action-message").count(), 0, "no run summary after Cancel");
  assert.deepEqual(await armedPackages(panel), armed, "the selection is untouched");

  // Escape cancels too.
  await runButton.click();
  await dialog.waitFor();
  await page.keyboard.press("Escape");
  await dialog.waitFor({ state: "detached" });
  assert.deepEqual(await mutations(page), [], "Escape changed nothing");

  // The 720px minimum with larger text, both themes: the dialog fits and scrolls inside.
  await page.setViewportSize({ width: 720, height: 800 });
  await page.addStyleTag({ content: "html{font-size:150%}" });
  for (const scheme of ["dark", "light"]) {
    await page.evaluate((s) => document.documentElement.setAttribute("data-theme", s), scheme);
    await runButton.click();
    await dialog.waitFor();
    const box = await dialog.boundingBox();
    assert.ok(box.x >= 0 && box.x + box.width <= 720 && box.y >= 0 && box.y + box.height <= 800, `${scheme}: the dialog fits the window: ${JSON.stringify(box)}`);
    const clipped = await dialog.evaluate((el) =>
      [...el.querySelectorAll("h2, h3, li, p, button")].filter((n) => n.scrollWidth > n.clientWidth + 1).map((n) => n.textContent.trim().slice(0, 40)));
    assert.deepEqual(clipped, [], `${scheme}: nothing is clipped`);
    if (evidence) await page.screenshot({ path: join(evidence, `after-run-dialog-720-${scheme}.png`) });
    await page.keyboard.press("Escape");
    await dialog.waitFor({ state: "detached" });
  }
  await page.setViewportSize({ width: 1400, height: 1000 });

  // Run goes ahead.
  await runButton.click();
  await dialog.waitFor();
  await dialog.getByRole("button", { name: "Run Optimize", exact: true }).click();
  await dialog.waitFor({ state: "detached" });
  await page.getByText(/Optimize complete:/).waitFor({ timeout: 20000 });
  assert.notEqual(await page.evaluate(() => document.activeElement?.tagName), "BODY", "focus stays in the panel after Run");
  assert.equal((await mutations(page)).length, armed.length, "Run applies each armed row");
  if (evidence) await page.screenshot({ path: join(evidence, "after-run-complete.png") });

  const video = page.video();
  await page.close();
  if (evidence) await video.saveAs(join(evidence, "run-plan-dialog.webm"));
  console.log(`Run plan dialog passed: ${armed.length} apps named once${uninstallPkg ? ", can't-come-back line visible" : ""}, Cancel and Escape change nothing, Run applies.`);
}

async function main() {
  const restore = setHarnessEnvironment();
  let server, browser;
  try {
    const { createServer } = await import("vite");
    const { chromium } = await import("playwright");
    server = await createServer({ root: V2, server: { host: "127.0.0.1", port: await freePort(), strictPort: true, hmr: false } });
    await server.listen();
    await builderChecks(server);
    browser = await chromium.launch();
    await exercise({ browser, base: serverURL(server) });
  } finally {
    await browser?.close().catch((e) => console.error("browser cleanup failed", e));
    await server?.close().catch((e) => console.error("Vite cleanup failed", e));
    restore();
  }
}

await main();
