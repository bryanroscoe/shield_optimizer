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
// Catalog "uninstall if unused" store apps: one the demo TV shipped with, one
// its owner installed (demo-mock's USER_INSTALLED).
const PREINSTALLED_STORE_APP = "com.netflix.ninja";
const USER_STORE_APP = "com.plexapp.android";
// The demo Optimize plan carries Hulu but not Plex.
const USER_REVIEW_IN_PLAN = "com.hulu.livingroomplus";

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

// The "?" beside the filters: reachable by keyboard and pointer, names its
// purpose, explains the suggestion, and stays inside a narrow window.
async function checkRemovalHelp(page, root, where) {
  const trigger = root.getByRole("button", { name: "About Disable or uninstall", exact: true });
  assert.equal(await trigger.count(), 1, `${where} has one Disable-or-uninstall help control`);
  const panel = root.getByRole("region", { name: "Disable or uninstall", exact: true });
  assert.equal(await panel.isVisible(), false, `${where}: help starts closed`);
  // Reach it by keyboard so :focus-visible applies, as it does for a person.
  await trigger.focus();
  await page.keyboard.press("Shift+Tab");
  await page.keyboard.press("Tab");
  assert.equal(await trigger.evaluate((el) => document.activeElement === el), true, `${where}: Tab reaches the help control`);
  const ring = await trigger.evaluate((el) => getComputedStyle(el).outlineStyle);
  assert.notEqual(ring, "none", `${where}: the help control shows keyboard focus`);
  await page.keyboard.press("Enter");
  await panel.waitFor({ state: "visible" });
  const text = await panel.innerText();
  assert.match(text, /Uninstall is suggested for Play Store apps that were added to this TV/);
  assert.match(text, /Disable is suggested for apps that came with the TV or were sideloaded/);
  assert.match(text, /Snapshots record both/);
  await page.keyboard.press("Escape");
  await panel.waitFor({ state: "hidden" });
  assert.equal(await trigger.evaluate((el) => document.activeElement === el), true, `${where}: Escape returns focus`);
  const size = page.viewportSize();
  await page.setViewportSize({ width: 720, height: 900 });
  await trigger.scrollIntoViewIfNeeded();
  await trigger.click();
  await panel.waitFor({ state: "visible" });
  const box = await panel.boundingBox();
  assert.ok(box && box.x >= 0 && box.x + box.width <= 720, `${where}: help fits a 720px window: ${JSON.stringify(box)}`);
  await trigger.click();
  await panel.waitFor({ state: "hidden" });
  await page.setViewportSize(size);
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
  // Change undoes Keep, and is there without hovering the row: touch has no
  // hover, and a hidden control read as "Keep can't be undone" (#324).
  // Playwright's isVisible ignores opacity, so read the painted opacity.
  await page.mouse.move(0, 0);
  const change = appRow.getByRole("button", { name: "Change", exact: true });
  assert.ok(await change.isVisible(), "a kept row shows Change");
  const opacity = await change.evaluate((el) => {
    let o = 1;
    for (let n = el; n; n = n.parentElement) o *= Number(getComputedStyle(n).opacity);
    return o;
  });
  assert.ok(opacity >= 0.9, `Change is visible before hover (opacity ${opacity})`);
  await page.getByRole("tab", { name: "Health" }).click();
  assert.equal((await healthCell.innerText()).trim(), "Kept", "Health reads Kept for a kept app");
  await healthRow.click();
  const describedBy = await change.getAttribute("aria-describedby");
  assert.equal(
    (await page.locator(`[id="${describedBy}"]`).textContent()).trim(),
    `Undo keeping ${(await appRow.locator(".app-name").first().textContent()).trim()}`,
    "Change says what it undoes without a hover tooltip",
  );
  await change.click();
  await appRow.locator(`[data-rec="${appRec}"]`).waitFor();
  assert.equal(await appRow.locator('[data-rec="Kept"]').count(), 0, "Change returns the row to its recommendation");

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

  // Uninstall is suggested only for a store app this TV's user installed.
  // In the demo, Netflix came with the Shield and Plex was installed from the
  // Play Store; both are catalog "uninstall if unused" store apps.
  const reviewRec = async (root, pkg) =>
    root.locator("tr", { has: page.locator(".pkg-id", { hasText: new RegExp(`^${pkg.replaceAll(".", "\\.")}\\s*$`) }) })
      .first().locator("[data-rec]").first().getAttribute("data-rec");
  assert.equal(await reviewRec(apps, PREINSTALLED_STORE_APP), "Review: disable if unused", "a preinstalled store app suggests Disable");
  assert.equal(await reviewRec(apps, USER_STORE_APP), "Review: uninstall if unused", "a store app the user installed suggests Uninstall");
  await checkRemovalHelp(page, apps, "App List");

  // Optimize.
  await page.getByRole("tab", { name: "Optimize" }).click();
  await page.getByRole("button", { name: "Optimize", exact: true }).click();
  const opt = page.locator("#tabpanel-optimize");
  await opt.locator("table.optimize-table tbody tr").first().waitFor();
  await waitResolved(page, "#tabpanel-optimize td.verdict-cell");

  const reviewPill = async (pkg) =>
    (await opt.locator("tr", { has: page.locator(".pkg-id", { hasText: pkg }) }).first().locator(".review-pill").innerText()).trim();
  assert.equal(await reviewPill(PREINSTALLED_STORE_APP), "Review: disable if unused", "Optimize agrees for a preinstalled store app");
  assert.equal(await reviewPill(USER_REVIEW_IN_PLAN), "Review: uninstall if unused", "Optimize agrees for a user-installed store app");
  await checkRemovalHelp(page, opt, "Optimize");

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

  // There is no bulk arm. By default a review row ("remove only if unused")
  // and a Caution or Unknown row stay on Keep; only the recommended bloat is
  // armed, and Keep all disarms that too.
  assert.equal(await opt.getByRole("button", { name: /Select all/ }).count(), 0, "Optimize has no bulk arm control");
  const readRows = () => opt.evaluate((root) =>
    [...root.querySelectorAll("table.optimize-table tbody tr")]
      .map((tr) => {
        const active = tr.querySelector(".action-radio .radio-pill.active .radio-label");
        const verdict = tr.querySelector("td.verdict-cell [data-verdict], td.verdict-cell[data-verdict]")?.getAttribute("data-verdict")
          ?? tr.querySelector("[data-verdict]")?.getAttribute("data-verdict");
        const pkg = tr.querySelector(".pkg-id")?.textContent?.trim();
        const review = !!tr.querySelector(".review-pill");
        return { pkg, action: active?.textContent?.trim() ?? null, verdict, review };
      })
      .filter((r) => r.pkg),
  );
  const byDefault = await readRows();
  const reviewRows = byDefault.filter((r) => r.review);
  assert.ok(reviewRows.length > 0, "the demo plan has review rows, so the check is not vacuous");
  const armedReview = reviewRows.filter((r) => r.action !== "Keep");
  assert.deepEqual(armedReview, [], `review rows start at Keep: ${JSON.stringify(armedReview)}`);
  const armedNotSafe = byDefault.filter((r) => r.action && r.action !== "Keep" && r.verdict !== "safe" && r.verdict !== "caution");
  assert.deepEqual(armedNotSafe, [], `nothing Unknown or Protected is armed by default: ${JSON.stringify(armedNotSafe)}`);
  await opt.getByRole("button", { name: "Keep all", exact: true }).click();
  const afterKeepAll = (await readRows()).filter((r) => r.action && r.action !== "Keep");
  assert.deepEqual(afterKeepAll, [], `Keep all leaves every row on Keep: ${JSON.stringify(afterKeepAll)}`);

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
