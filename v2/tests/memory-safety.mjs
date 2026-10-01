// Every row of Top Memory Users sat on "Checking" forever.
//
// The cause is a Svelte 5 trap worth a permanent guard: `report` is $state, so
// assigning an object stores a deep *proxy*. A later `report !== nextReport`
// identity check therefore compares a proxy against the raw object and is
// always true — so the guard meant to drop a superseded load dropped every
// load, and the resolved verdicts were never written.
//
// Nothing threw and nothing logged; the table just never finished. Only
// rendering it catches that.
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
  const page = await browser.newPage({ viewport: { width: 1280, height: 1000 } });
  await page.goto(base, { waitUntil: "networkidle" });
  await page.getByText("NVIDIA SHIELD", { exact: false }).first().click();
  await page.getByRole("tab", { name: "Health" }).click();
  await page.getByText("Top Memory Users").waitFor();

  // Read the resolved state off the cell rather than matching words: the
  // labels are free to change, and a poll for a word that no longer exists
  // passes before anything has resolved.
  const cells = "table.mem-table tbody tr td.suggestion-cell";
  await page.waitForFunction(
    (sel) => {
      const all = [...document.querySelectorAll(sel)];
      return all.length > 0 && all.every((c) => c.dataset.verdict !== "checking");
    },
    cells,
    { timeout: 10000 },
  );

  const rows = await page.$$eval(cells, (all) =>
    all.map((c) => ({
      process: c.closest("tr")?.dataset.process ?? "",
      pkg: c.closest("tr")?.dataset.package ?? "",
      shown: c.closest("tr")?.querySelector("td.pkg")?.innerText ?? "",
      opens: c.closest("tr")?.querySelector("button.mem-open") !== null,
      suggestion: c.dataset.suggestion,
      verdict: c.dataset.verdict,
      text: c.innerText.trim(),
    })),
  );
  assert.ok(rows.length > 0, "the demo device has memory rows");

  const KINDS = new Set(["never_disable", "caution", "safe", "unknown"]);
  const resolved = rows.filter((r) => KINDS.has(r.verdict));
  // Positive: "unavailable" everywhere would satisfy "nothing is checking"
  // while meaning every lookup broke.
  assert.equal(
    resolved.length,
    rows.length,
    `every row must resolve to a real verdict, got: ${JSON.stringify(rows)}`,
  );

  // Fail closed: a name we cannot tie to an installed package is a process.
  // It never inherits a catalog verdict and never reads as removable.
  const processes = rows.filter((r) => r.suggestion === "process");
  for (const r of processes) {
    assert.notEqual(r.verdict, "safe", `unverified process ${r.process} must never be Safe: ${JSON.stringify(r)}`);
    assert.equal(r.text, "Not an app", `process ${r.process} reads as a process: ${JSON.stringify(r)}`);
    assert.equal(r.pkg, "", `process ${r.process} is not tied to a package: ${JSON.stringify(r)}`);
    assert.ok(!r.opens, `process ${r.process} offers no way into the App List: ${JSON.stringify(r)}`);
  }

  // Issue #97: rows keep the full process name and PID. A HAL is not cut at
  // its `@`, a package-shaped name nothing installed carries stays a process,
  // and a `:subprocess` of an installed app is that app's row.
  const byProcess = new Map(rows.map((r) => [r.process, r]));
  for (const name of [
    "vendor.nvidia.hardware.graphics.composer@2.0-service",
    "system",
    "surfaceflinger",
    "com.google.process.gservices",
  ]) {
    const r = byProcess.get(name);
    assert.ok(r, `${name} keeps its own row under its full name: ${JSON.stringify(rows)}`);
    assert.equal(r.suggestion, "process", `${name} is not an app: ${JSON.stringify(r)}`);
  }
  const sub = byProcess.get("com.android.vending:background");
  assert.ok(sub, `the subprocess keeps its full name: ${JSON.stringify(rows)}`);
  assert.notEqual(sub.suggestion, "process", `an installed app's subprocess is that app: ${JSON.stringify(sub)}`);
  assert.equal(sub.pkg, "com.android.vending", `the subprocess opens its owning package: ${JSON.stringify(sub)}`);
  for (const r of rows) {
    assert.ok(r.shown.includes(r.process), `row shows the full process name: ${JSON.stringify(r)}`);
    assert.match(r.shown, /pid \d+/, `row shows its PID: ${JSON.stringify(r)}`);
  }

  // An installed catalog app carries a recommendation, and the reviewed
  // catalog verdict behind it is no longer downgraded on this screen.
  const recs = rows.filter((r) => r.suggestion === "recommendation");
  assert.ok(recs.length > 0, `installed catalog apps get a recommendation: ${JSON.stringify(rows)}`);
  assert.ok(
    recs.some((r) => r.verdict === "safe"),
    `a reviewed-safe installed app keeps its Safe verdict on Health: ${JSON.stringify(recs)}`,
  );

  console.log(
    `Memory safety passed: ${rows.length} rows resolved (${recs.length} recommendations, ${processes.length} processes, no process Safe).`,
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
