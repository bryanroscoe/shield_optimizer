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
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const V2 = join(HERE, "..");

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
  const page = await browser.newPage({ viewport: { width: 1400, height: 1000 } });
  await page.goto(base, { waitUntil: "networkidle" });
  await page.getByText("NVIDIA SHIELD", { exact: false }).first().click();
  await page.getByRole("tab", { name: "Optimize" }).click();
  const panel = page.locator("#tabpanel-optimize");
  // The Mode toggle's Optimize side loads the plan.
  await panel.locator(".mode-box").getByRole("button", { name: "Optimize", exact: true }).click();
  await panel.locator("table.optimize-table tbody tr").first().waitFor();

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

  console.log(
    `Optimize plan passed: ${rows.length} rows, ${acted.length} recommended, Run button agrees, no row left Checking.`,
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
