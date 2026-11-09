// The Optimize wizard is the product's headline feature, and it must actually
// recommend something: a row left on "Checking safety" forces Keep, so a plan
// whose lookups never resolve reports zero actions and running it does nothing.
//
// `optimizePlan !== plan` compares a Svelte $state deep proxy against the raw
// object returned by the API and is always true, so a superseded-load guard
// written that way drops every load and the verdicts are never written.
//
// Nothing throws and nothing logs. Only rendering the wizard catches it.
import assert from "node:assert/strict";
import { readFile, mkdir } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const V2 = join(HERE, "..");
const reviewed = JSON.parse(await readFile(join(HERE, "fixtures/reviewed-optimize-packages.json"), "utf8"));

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

async function exercise({ browser, base }) {
  const evidence = process.env.OPTIMIZE_CATALOG_EVIDENCE;
  if (evidence) await mkdir(evidence, { recursive: true });
  const page = await browser.newPage({ viewport: { width: 1400, height: 1000 }, colorScheme: "dark",
    ...(evidence ? { recordVideo: { dir: join(evidence, "raw"), size: { width: 1400, height: 1000 } } } : {}) });
  await page.addInitScript(() => localStorage.setItem("shieldopt.demo.reviewedCatalog", "1"));
  await page.goto(base, { waitUntil: "networkidle" });
  await page.evaluate(() => {
    window.optimizeReviewCalls = [];
    const invoke = window.__TAURI_INTERNALS__.invoke;
    window.__TAURI_INTERNALS__.invoke = async (command, args) => {
      window.optimizeReviewCalls.push(command);
      return invoke(command, args);
    };
  });
  if (evidence) await page.evaluate(() => {
    const label = document.createElement("div");
    label.textContent = "SIMULATED Optimize review · no TV or removal";
    label.style.cssText = "position:fixed;top:0;right:0;z-index:99999;padding:8px;background:#1a2540;color:white;font:14px sans-serif";
    document.body.appendChild(label);
  });
  await page.getByText("NVIDIA SHIELD", { exact: false }).first().click();
  await page.getByRole("tab", { name: "Optimize" }).click();
  const panel = page.locator("#tabpanel-optimize");
  // The Mode toggle's Optimize side loads the plan.
  await panel.locator(".mode-box").getByRole("button", { name: "Optimize", exact: true }).click();
  await panel.locator("table.optimize-table tbody tr").first().waitFor();
  assert.deepEqual(
    await panel.locator("table.optimize-table thead th").allInnerTexts().then((t) => t.map((x) => x.trim().toUpperCase())),
    ["DETAILS", "APP", "VERDICT & SOURCE", "RAM", "LAST USED", "ACTION"],
    "a run's result shows in the Action cell, not in a trailing column that scrolls off",
  );

  // Every safety lookup must actually resolve.
  await page.waitForFunction(
    () => {
      const cells = [...document.querySelectorAll("#tabpanel-optimize td.verdict-cell")];
      return cells.length > 0 && cells.every((c) => c.dataset.verdict !== "checking");
    },
    null,
    { timeout: 15000 },
  );

  const rows = await panel.evaluate((root) =>
    [...root.querySelectorAll("table.optimize-table tbody tr")]
      .filter((tr) => tr.querySelector("td.verdict-cell"))
      .map((tr) => ({
        pkg: tr.querySelector(".pkg-id")?.textContent?.trim() ?? "",
        verdict: tr.querySelector("td.verdict-cell")?.getAttribute("data-verdict"),
        pills: tr.querySelectorAll(".action-radio .radio-pill").length,
        armed: tr.querySelector(".action-radio .radio-pill.active .radio-label")?.textContent?.trim() ?? null,
      })),
  );
  assert.ok(rows.length > 0, "the plan has rows");
  const acted = rows.filter((r) => r.armed && r.armed !== "Keep");
  assert.ok(
    acted.length > 0,
    `the wizard must recommend something; every row defaulted to Keep: ${JSON.stringify(rows)}`,
  );

  // The Run button has to agree with the rows, not report zero over a full plan.
  const run = await panel.getByRole("button", { name: /^Run plan/ }).innerText();
  const [, stated] = run.match(/Run plan · (\d+) item/) ?? [];
  assert.equal(
    Number(stated),
    acted.length,
    `Run button says ${stated} but ${acted.length} rows are armed: ${run}`,
  );
  assert.notEqual(Number(stated), 0, `a full plan must not report zero actions: ${run}`);

  // A protected package must never be armable, however the plan is built.
  const armedProtected = rows.filter((r) => r.verdict === "never_disable" && r.pills > 0);
  assert.deepEqual(armedProtected, [], `protected rows must offer no action: ${JSON.stringify(armedProtected)}`);

  for (const entry of reviewed) {
    const row = panel.locator("tr").filter({ has: page.locator(".pkg-id", { hasText: entry.package }) });
    assert.equal(await row.locator('[data-verdict="caution"]').count(), 1, entry.package);
    assert.equal((await row.locator(".radio-pill.active .radio-label").innerText()).trim(), "Keep", entry.package);
    assert.equal((await row.locator(".review-pill").innerText()).trim(), "Review: disable if unused");
    if (entry.consequence_fragments) {
      await row.getByRole("button", { name: `Show why ${entry.name} has this verdict`, exact: true }).click();
      const detail = row.locator("xpath=following-sibling::tr[1]");
      assert.ok(await detail.isVisible(), `expanded consequences: ${entry.package}`);
      for (const fragment of entry.consequence_fragments) {
        assert.ok((await detail.innerText()).includes(fragment), `visible consequence: ${entry.package}: ${fragment}`);
      }
      await row.getByRole("button", { name: `Hide why ${entry.name} has this verdict`, exact: true }).click();
    }
  }
  // Review rows are armed one at a time by a person; nothing arms them in bulk.
  assert.equal(await panel.getByRole("button", { name: /Select all/ }).count(), 0, "Optimize has no bulk arm control");
  await panel.getByRole("button", { name: "Keep all", exact: true }).click();
  for (const entry of reviewed) {
    const row = panel.locator("tr").filter({ has: page.locator(".pkg-id", { hasText: entry.package }) });
    assert.equal((await row.locator(".radio-pill.active .radio-label").innerText()).trim(), "Keep", `Keep all: ${entry.package}`);
  }
  const focusPackage = process.env.OPTIMIZE_CATALOG_FOCUS || reviewed[0].package;
  assert.ok(reviewed.some((entry) => entry.package === focusPackage), "evidence focus must be a reviewed package");
  const first = panel.locator("tr").filter({ has: page.locator(".pkg-id", { hasText: focusPackage }) });
  if (evidence) {
    await first.scrollIntoViewIfNeeded();
    await page.mouse.move(0, 0);
    await page.waitForTimeout(600);
  }
  await first.getByRole("button", { name: "Disable", exact: true }).click();
  assert.equal((await first.locator(".radio-pill.active .radio-label").innerText()).trim(), "Disable");
  if (evidence) await page.waitForTimeout(400);
  await first.getByRole("button", { name: "Keep", exact: true }).click();
  if (evidence) {
    await first.locator(".row-caret").click();
    await first.scrollIntoViewIfNeeded();
    await page.mouse.move(0, 0);
    await page.waitForTimeout(700);
    await page.screenshot({ path: join(evidence, "desktop-optimize-reviewed.png") });
    await page.setViewportSize({ width: 720, height: 1000 });
    await first.locator(".app-name").scrollIntoViewIfNeeded();
    await page.screenshot({ path: join(evidence, "desktop-optimize-minimum.png") });
    await page.emulateMedia({ colorScheme: "light" });
    await page.addStyleTag({ content: "html{font-size:200%}" });
    await first.locator(".app-name").scrollIntoViewIfNeeded();
    await page.screenshot({ path: join(evidence, "desktop-optimize-enlarged-light.png") });
  }

  await page.setViewportSize({ width: 720, height: 1000 });
  if (!evidence) await page.addStyleTag({ content: "html{font-size:200%}" });
  const overflow = await page.evaluate(() => ({
    viewport: innerWidth, width: document.documentElement.scrollWidth,
    outside: [...document.querySelectorAll("body *")].filter((el) => !el.closest(".plan-table-scroll") && el.getBoundingClientRect().right > innerWidth + 1).slice(0, 12).map((el) => ({ tag: el.tagName, class: el.className, right: el.getBoundingClientRect().right })),
  }));
  assert.ok(overflow.width <= overflow.viewport, `the page fits; wide choices scroll inside the table region: ${JSON.stringify(overflow)}`);
  for (const entry of reviewed) {
    const row = panel.locator("tr").filter({ has: page.locator(".pkg-id", { hasText: entry.package }) });
    const name = row.locator(".app-name");
    assert.ok(await name.evaluate((el) => el.getBoundingClientRect().width <= el.parentElement.clientWidth), `readable enlarged name: ${entry.package}`);
  }
  const scroller = page.getByRole("region", { name: "Optimize app choices" });
  await scroller.focus();
  const before = await scroller.evaluate((el) => el.scrollLeft);
  await page.keyboard.press("ArrowRight");
  await page.waitForTimeout(200);
  assert.ok(await scroller.evaluate((el) => el.scrollLeft) > before, "keyboard can reach the trailing action columns");

  assert.deepEqual(await page.evaluate(() => window.optimizeReviewCalls.filter((c) => /^(disable_package|uninstall_package|enable_package|apply_)/.test(c))), []);
  console.log(
    `Optimize plan passed: ${rows.length} rows, ${acted.length} recommended, Run button agrees, no row left Checking.`,
  );
  const video = page.video();
  await page.close();
  if (evidence) await video.saveAs(join(evidence, "desktop-optimize-reviewed.webm"));
}

async function main() {
  const restore = setHarnessEnvironment();
  let server, browser;
  try {
    const { createServer } = await import("vite");
    const { chromium } = await import("playwright");
    server = await createServer({ root: V2, server: { host: "127.0.0.1", port: 0, strictPort: false, hmr: false } });
    await server.listen();
    const catalog = JSON.parse(await readFile(join(V2, "crates/core/data/app-lists/common.json"), "utf8"));
    const { recommendation, effectiveMethod, canOfferUninstall } = await server.ssrLoadModule("/src/lib/recommendation.ts");
    for (const record of reviewed) {
      const entry = catalog.find((app) => app.package === record.package);
      assert.ok(entry, record.package);
      assert.equal(entry.default_optimize, false);
      assert.equal(entry.default_restore, false);
      assert.equal(effectiveMethod(entry), "disable");
      assert.equal(canOfferUninstall(entry), true);
      const safety = { status: "ready", verdict: { kind: "caution", source: "reviewed_catalog", reason: entry.optimize_description } };
      for (const state of ["enabled", "disabled", "missing"]) {
        const result = recommendation(entry, state, safety);
        assert.equal(result.kind, state === "enabled" ? "review" : "keep", `${record.package} ${state}`);
        if (state === "enabled") {
          assert.equal(result.action, "disable");
          assert.equal(result.label, "Review: disable if unused");
        }
      }
    }
    console.log(`Reviewed catalog state checks passed: ${reviewed.length * 3} enabled/disabled/missing cases.`);
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
