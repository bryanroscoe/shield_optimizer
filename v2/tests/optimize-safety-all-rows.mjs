// The Optimize wizard queries safety for every row that is actually on the
// device, not only the rows it proposes to act on.
//
// The rows it proposes to act on exclude every row the backend marked skip —
// including a catalog package that is ALREADY DISABLED. `com.android.providers.tv`
// is on the Caution list, and a disabled copy of it must still show its real
// verdict rather than "SAFETY UNAVAILABLE". Only rendering the wizard proves
// it, because an absent entry is coerced to "unavailable" at the row and looks
// exactly like a lookup that failed.
import assert from "node:assert/strict";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const V2 = join(HERE, "..");

/// The fixture row: a Caution-list package the demo device reports as already
/// disabled (src/lib/demo-mock.ts holds both halves).
const CAUTION_PKG = "com.android.providers.tv";

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

  const panel = page.locator("#tabpanel-optimize");
  const rows = panel.locator("table.optimize-table tbody tr");
  await rows.first().waitFor();

  // Every lookup resolves, or the assertions below read a half-loaded table.
  // Read the state, not the words: the label is free to change, and a poll
  // for a string that no longer exists passes before anything has resolved.
  await page.waitForFunction(
    () => {
      const cells = [...document.querySelectorAll("#tabpanel-optimize td.verdict-cell")];
      return cells.length > 0 && cells.every((c) => c.dataset.verdict !== "checking");
    },
    null,
    { timeout: 15000 },
  );

  const verdicts = await panel.locator("td.verdict-cell").allInnerTexts();
  assert.ok(verdicts.length > 0, "the plan has rows");
  assert.ok(
    !verdicts.some((v) => v.includes("SAFETY UNAVAILABLE")),
    `no row may be left unqueried, got: ${JSON.stringify(verdicts)}`,
  );

  // The row this whole test exists for: disabled, on the Caution list, and no
  // action proposed. Its verdict still has to be the real one.
  const cautionRow = rows.filter({
    has: page.locator(".pkg-id", { hasText: CAUTION_PKG }),
  });
  assert.equal(
    await cautionRow.count(),
    1,
    `the demo plan must carry exactly one ${CAUTION_PKG} row`,
  );
  const verdict = await cautionRow.locator("td.verdict-cell").innerText();
  assert.ok(
    verdict.includes("CAUTION"),
    `${CAUTION_PKG} must read CAUTION, got: ${JSON.stringify(verdict)}`,
  );
  assert.ok(
    !verdict.includes("UNAVAILABLE"),
    `${CAUTION_PKG} must not read unavailable, got: ${JSON.stringify(verdict)}`,
  );

  // And querying it must not have turned an untouchable row into an offer.
  const action = await cautionRow.locator("td").nth(5).innerText();
  assert.ok(
    action.includes("Already disabled"),
    `an already-disabled row keeps its reason, got: ${JSON.stringify(action)}`,
  );

  console.log(
    `Optimize safety coverage passed: ${verdicts.length} rows queried, ${CAUTION_PKG} reads CAUTION.`,
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
