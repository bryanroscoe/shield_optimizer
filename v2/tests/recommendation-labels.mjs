// One recommendation vocabulary across App List, Optimize and Health.
//
// src/lib/recommendation.ts is the only source: the App List uses one word for
// one action on every row, Optimize never offers Uninstall on an app the engine
// downgraded to Disable because the store cannot give it back, and Health shows
// the same verdict as the App List.
import assert from "node:assert/strict";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const V2 = join(HERE, "..");

// Catalog method "uninstall", no Play Store listing, not defunct, enabled in
// the demo and inside the Optimize plan. Uninstalling it is unrecoverable.
const NOT_REINSTALLABLE = "com.philo.philo";
// Installed catalog app in Top memory users whose recommendation is an
// action (Disable), so a match is not two screens agreeing on "nothing".
const HEALTH_APP = "com.nvidia.tegrazone3";

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

async function waitResolved(page, selector) {
  await page.waitForFunction(
    (sel) => {
      const all = [...document.querySelectorAll(sel)];
      return all.length > 0 && all.every((c) => c.dataset.verdict !== "checking");
    },
    selector,
    { timeout: 15000 },
  );
}

/// Every visible action label in a panel: buttons plus the recommendation
/// markers that render as text. The verdict cell is a button too (it opens
/// the reason), but it names a safety tier, not an action — "Safe to remove"
/// is the one place the word belongs.
async function actionLabels(panel) {
  return panel.evaluate((root) =>
    [...root.querySelectorAll("button, [data-rec], .review-pill")]
      .filter((el) => el.offsetParent !== null && !el.closest("td.verdict-cell"))
      .map((el) => el.innerText.trim())
      .filter(Boolean),
  );
}

async function exercise({ browser, base }) {
  const page = await browser.newPage({ viewport: { width: 1400, height: 1000 } });
  await page.goto(base, { waitUntil: "networkidle" });
  await page.getByText("NVIDIA SHIELD", { exact: false }).first().click();

  // Health first: its Suggestion for an installed catalog app.
  await page.getByRole("tab", { name: "Health" }).click();
  await page.getByText("Top memory users").waitFor();
  await waitResolved(page, "table.mem-table td.suggestion-cell");
  const healthRow = page.locator(`table.mem-table tr[data-package="${HEALTH_APP}"]`);
  const healthCell = healthRow.locator("td.suggestion-cell");
  // The catalog loads alongside; wait for the cell to become a recommendation.
  await page.waitForFunction(
    (pkg) =>
      document
        .querySelector(`table.mem-table tr[data-package="${pkg}"] td.suggestion-cell`)
        ?.getAttribute("data-suggestion") === "recommendation",
    HEALTH_APP,
    { timeout: 15000 },
  );
  const healthSuggestion = (await healthCell.innerText()).trim();

  // Clicking the row opens the App List on that package.
  await healthRow.click();
  const apps = page.locator("#tabpanel-apps");
  await apps.waitFor();
  await waitResolved(page, "#tabpanel-apps td.verdict-cell");
  assert.equal(
    await page.locator(".app-search").inputValue(),
    HEALTH_APP,
    "a Health row click filters the App List to that package",
  );
  const appRow = apps.locator("tr", { has: page.locator(".pkg-id", { hasText: HEALTH_APP }) }).first();
  const appRec = await appRow.locator("[data-rec]").first().getAttribute("data-rec");
  assert.equal(
    healthSuggestion,
    appRec,
    `Health suggests ${JSON.stringify(healthSuggestion)} but the App List recommends ${JSON.stringify(appRec)} for ${HEALTH_APP}`,
  );

  // Keep is the user's decision and outranks the recommendation: the row stops
  // offering its Disable, and Health says Kept too.
  await appRow.getByRole("button", { name: "Keep", exact: true }).click();
  await appRow.locator('[data-rec="Kept"]').waitFor();
  const keptRecs = await appRow.locator("[data-rec]").evaluateAll((els) => els.map((e) => e.dataset.rec));
  assert.deepEqual(keptRecs, ["Kept"], `a kept row shows only Kept: ${JSON.stringify(keptRecs)}`);
  assert.equal(
    await appRow.getByRole("button", { name: appRec, exact: true }).count(),
    0,
    `a kept row no longer offers ${appRec}`,
  );
  await page.getByRole("tab", { name: "Health" }).click();
  assert.equal((await healthCell.innerText()).trim(), "Kept", "Health reads Kept for a kept app");
  await healthRow.click();
  await appRow.getByRole("button", { name: "Change", exact: true }).click();
  await appRow.locator(`[data-rec="${appRec}"]`).waitFor();

  // Now the whole App List, unfiltered.
  await page.locator(".app-search").fill("");
  await waitResolved(page, "#tabpanel-apps td.verdict-cell");
  const verdictText = (await apps.locator("td.verdict-cell").allInnerTexts()).join("\n");
  assert.match(verdictText, /safe to remove/i, "the Safe tier reads \"Safe to remove\"");
  assert.doesNotMatch(
    verdictText,
    /rated safe to remove/i,
    "the Safe reason no longer restates the label",
  );
  const appLabels = await actionLabels(apps);
  assert.ok(appLabels.length > 0, "the App List renders actions");
  const appRemove = appLabels.filter((l) => /\bremove\b/i.test(l));
  assert.deepEqual(appRemove, [], `App List action labels must never say Remove: ${JSON.stringify(appRemove)}`);

  // Optimize.
  await page.getByRole("tab", { name: "Optimize" }).click();
  await page.getByRole("button", { name: "Optimize", exact: true }).click();
  const opt = page.locator("#tabpanel-optimize");
  await opt.locator("table.optimize-table tbody tr").first().waitFor();
  await waitResolved(page, "#tabpanel-optimize td.verdict-cell");

  const optLabels = await actionLabels(opt);
  const optRemove = optLabels.filter((l) => /\bremove\b/i.test(l));
  assert.deepEqual(optRemove, [], `Optimize action labels must never say Remove: ${JSON.stringify(optRemove)}`);
  assert.ok(
    optLabels.some((l) => l.startsWith("Run plan")),
    `the run button reads "Run plan": ${JSON.stringify(optLabels.filter((l) => l.startsWith("Run")))}`,
  );

  const trapRow = opt.locator("tr", { has: page.locator(".pkg-id", { hasText: NOT_REINSTALLABLE }) });
  assert.equal(await trapRow.count(), 1, `the demo plan carries ${NOT_REINSTALLABLE}`);
  const options = (await trapRow.locator(".action-radio .radio-label").allInnerTexts()).map((t) => t.trim());
  // Not vacuous: the row does offer a removal, just not the one it can't undo.
  assert.ok(options.includes("Disable"), `${NOT_REINSTALLABLE} still offers Disable: ${JSON.stringify(options)}`);
  assert.ok(
    !options.includes("Uninstall"),
    `${NOT_REINSTALLABLE} has no Play Store listing, so Optimize must not offer Uninstall: ${JSON.stringify(options)}`,
  );

  // "Select all safe" arms only rows rated Safe to remove. Caution and
  // Unknown rows must stay on Keep.
  await opt.getByRole("button", { name: "Keep all", exact: true }).click();
  await opt.getByRole("button", { name: "Select all safe", exact: true }).click();
  const armed = await opt.evaluate((root) =>
    [...root.querySelectorAll("table.optimize-table tbody tr")]
      .map((tr) => {
        const active = tr.querySelector(".action-radio .radio-pill.active .radio-label");
        const verdict = tr.querySelector("td.verdict-cell [data-verdict], td.verdict-cell[data-verdict]")?.getAttribute("data-verdict")
          ?? tr.querySelector("[data-verdict]")?.getAttribute("data-verdict");
        const pkg = tr.querySelector(".pkg-id")?.textContent?.trim();
        return { pkg, action: active?.textContent?.trim() ?? null, verdict };
      })
      .filter((r) => r.pkg),
  );
  const armedRows = armed.filter((r) => r.action && r.action !== "Keep");
  assert.ok(armedRows.length > 0, "Select all safe arms at least one row");
  const armedUnsafe = armedRows.filter((r) => r.verdict !== "safe");
  assert.deepEqual(armedUnsafe, [], `Select all safe must only arm Safe to remove rows: ${JSON.stringify(armedUnsafe)}`);
  const notSafeWithChoice = armed.filter((r) => r.action === "Keep" && r.verdict && r.verdict !== "safe");
  assert.ok(notSafeWithChoice.length > 0, "the demo plan has a Caution or Unknown row with a choice, so the check is not vacuous");

  console.log(
    `Recommendation labels passed: Health "${healthSuggestion}" matches App List, no Remove labels, ${NOT_REINSTALLABLE} offers ${options.join("/")}.`,
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
