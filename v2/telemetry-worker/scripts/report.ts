// Prints the device table from GET /v1/admin/devices.
//
//   ATV_TELEMETRY_URL=https://atv-optimizer-telemetry.<subdomain>.workers.dev \
//   ATV_TELEMETRY_ADMIN_TOKEN=... \
//   npm run report -- [--days 90] [--limit 500] [--group manufacturer,model,device,profile]
//
// Needs Node 22.18+ (runs TypeScript directly). Prints JSON with --json.
//
// --issue prints the weekly "Device support report" issue body instead: all
// time and last-30-day windows, the tier each TV gets from --rules (default:
// the repo's device-support.json) and a suggested tier. Criteria live in
// report-lib.ts and docs/DEVICE-SUPPORT.md.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { analyze, consoleRows, fetchReport, issueMarkdown } from "./report-lib.ts";
import { checkTierFile } from "./tiers-lib.ts";

const args = process.argv.slice(2);
function flag(name: string): string | undefined {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? args[i + 1] : undefined;
}

const base = process.env.ATV_TELEMETRY_URL;
const token = process.env.ATV_TELEMETRY_ADMIN_TOKEN;
if (!base || !token) {
  console.error("Set ATV_TELEMETRY_URL and ATV_TELEMETRY_ADMIN_TOKEN.");
  process.exit(2);
}

try {
  if (args.includes("--issue")) {
    const rulesPath = flag("rules") ?? fileURLToPath(new URL("../../crates/core/data/device-support.json", import.meta.url));
    // Only the rules' shape matters here; a date check would fail a report for nothing.
    const checked = checkTierFile(JSON.parse(readFileSync(rulesPath, "utf8")), { today: "9999-12-31" });
    if (!checked.doc) throw new Error(`${rulesPath}: ${checked.errors.join("; ")}`);
    const group = "manufacturer,model,device";
    const [all, recent] = await Promise.all([
      fetchReport(base, token, { group, days: 3650, limit: 5000 }),
      fetchReport(base, token, { group, days: 30, limit: 5000 }),
    ]);
    const rows = analyze(checked.doc.rules, all, recent);
    const server = process.env.GITHUB_SERVER_URL ?? "https://github.com";
    const repo = process.env.GITHUB_REPOSITORY ?? "bryanroscoe/shield_optimizer";
    const runId = process.env.GITHUB_RUN_ID;
    process.stdout.write(
      issueMarkdown(rows, all, recent, {
        generatedAt: new Date(),
        rulesUpdated: checked.doc.updated,
        docsUrl: `${server}/${repo}/blob/main/docs/DEVICE-SUPPORT.md`,
        runUrl: runId ? `${server}/${repo}/actions/runs/${runId}` : undefined,
      }),
    );
  } else {
    const body = await fetchReport(base, token, {
      group: flag("group") ?? "manufacturer,model,device,profile",
      days: flag("days"),
      limit: flag("limit"),
    });
    if (args.includes("--json")) {
      console.log(JSON.stringify(body, null, 2));
    } else {
      console.table(consoleRows(body));
      console.log(`Since ${body.since}.${body.truncated ? " Truncated: raise --limit or narrow --days." : ""}`);
    }
  }
} catch (e) {
  console.error(e instanceof Error ? e.message : String(e));
  process.exit(1);
}
