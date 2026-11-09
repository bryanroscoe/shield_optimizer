// `npm run e2e [-- <scenario-name-filter>…]`
//
// Builds and starts the E2E server (real Rust command layer + SimulatedAdb),
// starts Vite in VITE_E2E mode on a free port, and runs every scenario in
// `e2e/scenarios/` through headless Chromium. Each step's full-page
// screenshot lands in `e2e/artifacts/<scenario>/NN-step.png`.

import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { chromium } from "playwright";

import { ARTIFACTS, E2E, buildServer, control, scenarioContext, startServer, startVite } from "./lib/harness.mjs";

const filters = process.argv.slice(2).filter((a) => !a.startsWith("--"));
const offline = process.argv.includes("--offline");
const selection = JSON.parse(readFileSync(join(E2E, "offline-selection.json"), "utf8"));
const availableFiles = readdirSync(join(E2E, "scenarios"))
  .filter((f) => f.endsWith(".mjs"))
  .sort();
const files = offline ? selection.files : availableFiles;
if (offline) {
  const unclassified = availableFiles.filter(f => !selection.files.includes(f) && !Object.hasOwn(selection.excluded_files, f));
  if (unclassified.length) throw new Error(`Offline review missing for: ${unclassified.join(", ")}`);
}
for (const file of files) if (!availableFiles.includes(file)) throw new Error(`Missing offline scenario file: ${file}`);

const scenarios = [];
for (const file of files) {
  const mod = await import(pathToFileURL(join(E2E, "scenarios", file)).href);
  for (const s of mod.scenarios ?? []) {
    if (filters.length && !filters.some((f) => s.name.includes(f))) continue;
    scenarios.push(s);
  }
}
if (!scenarios.length) {
  console.error(`no scenarios match ${filters.join(", ")}`);
  process.exit(2);
}

await buildServer();
const server = await startServer();
let vite;
let browser;
const results = [];
let policy;
try {
  policy = await control(server.url).offlinePolicy();
  if (policy.driver !== "SimulatedAdb" || !policy.fixture_sweep_installed || !policy.command_allowlist_enforced || policy.fixture_subnet !== "192.0.2") throw new Error("Offline fixture policy is not active; refusing test execution.");
  vite = await startVite(server.url);
  browser = await chromium.launch();
  console.log(`[e2e] server ${server.url}  ui ${vite.url}`);
  for (const scenario of scenarios) {
    const started = Date.now();
    console.log(`\n▶ ${scenario.name}`);
    const ctx = await scenarioContext({
      browser,
      base: vite.url,
      serverUrl: server.url,
      name: scenario.name,
      colorScheme: scenario.colorScheme,
    });
    try {
      await scenario.run(ctx);
      const state = await ctx.state();
      if (ctx.networkDenied.length) throw new Error(`Browser external network refused: ${ctx.networkDenied.join(", ")}`);
      const denied = (await ctx.invokes()).filter(i => i.offline_denied);
      if (denied.length) throw new Error(`Offline dispatch refused: ${denied.map(i => i.cmd).join(", ")}`);
      if (state.gaps.length && !scenario.allowGaps) {
        // A flow that reached an unsimulated command was never really tested.
        throw new Error(`unsimulated commands: ${[...new Set(state.gaps)].join(" | ")}`);
      }
      results.push({ name: scenario.name, ok: true, ms: Date.now() - started, gaps: state.gaps });
      console.log(`  ✓ ${scenario.name} (${((Date.now() - started) / 1000).toFixed(1)}s)`);
    } catch (error) {
      await ctx.step("FAILED").catch(() => {});
      const state = await ctx.state().catch(() => null);
      writeFileSync(join(ctx.dir, "failure.json"), JSON.stringify({ error: String(error?.stack ?? error), consoleErrors: ctx.consoleErrors, state, invokes: await ctx.invokes().catch(() => null) }, null, 2));
      results.push({ name: scenario.name, ok: false, ms: Date.now() - started, error: String(error?.message ?? error) });
      console.log(`  ✗ ${scenario.name}: ${error?.stack ?? error}`);
    } finally {
      await ctx.close();
    }
  }
} finally {
  await browser?.close().catch(() => {});
  await vite?.vite.close().catch(() => {});
  server.child.kill();
}

writeFileSync(join(ARTIFACTS, "summary.json"), JSON.stringify(results, null, 2));
if (offline) writeFileSync(join(ARTIFACTS, "offline-summary.json"), JSON.stringify({ policy, results, excluded_files: selection.excluded_files, hardware_acceptance_not_run: selection.hardware_acceptance_not_run }, null, 2));
const failed = results.filter((r) => !r.ok);
console.log(`\n${results.length - failed.length}/${results.length} scenarios passed. Screenshots: ${ARTIFACTS}`);
for (const f of failed) console.log(`  ✗ ${f.name}: ${f.error}`);
process.exit(failed.length ? 1 : 0);
