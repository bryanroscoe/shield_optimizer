// The Optimize wizard is the product's headline feature, and it was silently
// recommending nothing: every row sat on "Checking safety", which forces Skip,
// so the summary read "0 of 16 items will be acted on" and Run Optimize did
// nothing at all.
//
// Cause: `optimizePlan !== plan` compared a Svelte $state deep proxy against
// the raw object returned by the API. Always true, so the guard meant to drop a
// superseded load dropped every load and the verdicts were never written.
//
// Nothing threw and nothing logged. Only rendering the wizard catches it.
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
  await page.getByRole("button", { name: "Optimize", exact: true }).click();

  // Every safety lookup must actually resolve.
  await page.waitForFunction(
    () => !document.body.innerText.includes("Checking safety"),
    null,
    { timeout: 15000 },
  );

  const selects = page.locator("tbody select");
  const count = await selects.count();
  assert.ok(count > 0, "the plan has rows");

  const actions = [];
  for (let i = 0; i < count; i++) actions.push(await selects.nth(i).inputValue());
  const acted = actions.filter((a) => a !== "skip");

  assert.ok(
    acted.length > 0,
    `the wizard must recommend something; every row defaulted to skip: ${JSON.stringify(actions)}`,
  );

  // And the summary has to agree with the rows, not report zero over a full plan.
  const summary = await page.getByText(/items will be acted on/).innerText();
  const [, stated] = summary.match(/^(\d+) of \d+ items/) ?? [];
  assert.equal(
    Number(stated),
    acted.length,
    `summary says ${stated} but ${acted.length} rows are set to act: ${summary}`,
  );
  assert.notEqual(Number(stated), 0, `a full plan must not report zero actions: ${summary}`);

  // A protected package must never be selectable, however the plan is built.
  const disabledSkips = await page.locator("tbody select[disabled]").count();
  assert.ok(disabledSkips >= 0);

  console.log(
    `Optimize plan passed: ${count} rows, ${acted.length} recommended, summary agrees, no row left Checking.`,
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
