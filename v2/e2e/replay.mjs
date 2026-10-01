// `npm run e2e:replay -- <session.jsonl> [--no-profile]`
//
// Replays a recorded session (see README → "Recorded sessions"): the E2E
// server answers every adb call with what the device said at the time, and
// the session's UI breadcrumbs are driven through the real UI where they map
// to an action (route → navigate, tab → click the tab, click → click the
// button with that label). Each step is screenshotted to
// `e2e/artifacts/replay-<name>/`. At the end, every divergence — a command
// the app sent that the recording never saw, or saw in a different order —
// is printed and written to `divergences.json`, so a behaviour change between
// versions shows up as a diff.

import { writeFileSync } from "node:fs";
import { basename, join, resolve } from "node:path";
import { chromium } from "playwright";

import { buildServer, scenarioContext, startServer, startVite } from "./lib/harness.mjs";

const args = process.argv.slice(2);
const file = args.find((a) => !a.startsWith("--"));
if (!file) {
  console.error("usage: npm run e2e:replay -- <session.jsonl> [--no-profile]");
  process.exit(2);
}
const sessionPath = resolve(file);
const name = `replay-${basename(sessionPath).replace(/\.jsonl$/, "")}`;

await buildServer();
const server = await startServer();
let vite;
let browser;
let exitCode = 0;
try {
  vite = await startVite(server.url);
  browser = await chromium.launch();
  const ctx = await scenarioContext({ browser, base: vite.url, serverUrl: server.url, name });
  const loaded = await ctx.replay(sessionPath, !args.includes("--no-profile"));
  console.log(`[replay] ${loaded.adb_calls} adb calls, ${loaded.ui.length} UI breadcrumbs`);
  await ctx.open("/");
  await ctx.step("start");
  let skipped = 0;
  for (const crumb of loaded.ui) {
    const label = (crumb.label ?? "").trim();
    try {
      if (crumb.event === "route" && (crumb.path || label).startsWith("/")) {
        await ctx.open(crumb.path || label);
      } else if (crumb.event === "tab") {
        await ctx.page.getByRole("tab", { name: label, exact: true }).first().click({ timeout: 5000 });
      } else if (crumb.event === "click" && label) {
        await ctx.page.getByRole("button", { name: label, exact: true }).first().click({ timeout: 5000 });
      } else {
        continue;
      }
      await ctx.page.waitForLoadState("networkidle").catch(() => {});
      await ctx.step(`${crumb.event} ${label || crumb.path || ""}`);
    } catch (error) {
      skipped += 1;
      console.log(`    ! could not replay ${crumb.event} "${label}": ${String(error.message).split("\n")[0]}`);
    }
  }
  const report = await ctx.replayReport();
  writeFileSync(join(ctx.dir, "divergences.json"), JSON.stringify(report, null, 2));
  const divergences = report?.divergences ?? [];
  console.log(`\n[replay] ${divergences.length} divergence(s), ${report?.repeats ?? 0} repeated poll(s), ${report?.reordered ?? 0} concurrent reorder(s), ${skipped} breadcrumb(s) not replayable`);
  for (const d of divergences.slice(0, 50)) {
    console.log(`  ${d.kind}: ${d.args.join(" ")}${d.expected ? `  (expected next: ${d.expected.join(" ")})` : ""}`);
  }
  console.log(`Screenshots and divergences.json: ${ctx.dir}`);
  await ctx.close();
  // A recorded call the app no longer makes is as much a change as a new one.
  if (divergences.length) exitCode = 1;
} finally {
  await browser?.close().catch(() => {});
  await vite?.vite.close().catch(() => {});
  server.child.kill();
}
process.exit(exitCode);
