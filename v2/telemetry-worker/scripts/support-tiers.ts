// The support-tier file is the source of truth; this keeps the Worker in step.
//
//   node scripts/support-tiers.ts check <file> [--base <file>] [--today YYYY-MM-DD]
//     Validates the file (shape, limits, dead rules, a date that isn't in the
//     future) and, with --base, that `updated` moved forward when the rules
//     changed. Exit 1 on any error.
//
//   ATV_TELEMETRY_URL=... ATV_TELEMETRY_ADMIN_TOKEN=... \
//   node scripts/support-tiers.ts sync <file>
//     Checks the file, PUTs its rules to /v1/admin/support-tiers, reads
//     /v1/support-tiers back and fails unless the served rules are exactly
//     the file's and the served date is not older than the file's.
//
// Needs Node 22.18+ (runs TypeScript directly). No dependencies.

import { readFileSync } from "node:fs";
import { checkTierFile, rulesEqual, type TierDoc } from "./tiers-lib.ts";

const [command, file, ...rest] = process.argv.slice(2);
function flag(name: string): string | undefined {
  const i = rest.indexOf(`--${name}`);
  return i >= 0 ? rest[i + 1] : undefined;
}

function readJson(path: string): unknown {
  return JSON.parse(readFileSync(path, "utf8"));
}

function fail(message: string): never {
  console.error(message);
  process.exit(1);
}

if ((command !== "check" && command !== "sync") || !file) {
  console.error("usage: support-tiers.ts check <file> [--base <file>] [--today YYYY-MM-DD] | sync <file>");
  process.exit(2);
}

const basePath = flag("base");
const result = checkTierFile(readJson(file), {
  base: basePath ? readJson(basePath) : undefined,
  today: flag("today"),
});
for (const w of result.warnings) console.log(`warning: ${w}`);
if (!result.doc || result.errors.length > 0) {
  fail(`${file} is not ready to ship:\n${result.errors.map((e) => `  - ${e}`).join("\n")}`);
}
const doc = result.doc;
console.log(`${file}: ${doc.rules.length} rules, updated ${doc.updated}: ok`);

if (command === "sync") {
  const base = process.env.ATV_TELEMETRY_URL;
  const token = process.env.ATV_TELEMETRY_ADMIN_TOKEN;
  if (!base || !token) {
    console.error("Set ATV_TELEMETRY_URL and ATV_TELEMETRY_ADMIN_TOKEN.");
    process.exit(2);
  }
  const put = await fetch(new URL("/v1/admin/support-tiers", base), {
    method: "PUT",
    headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
    body: JSON.stringify({ rules: doc.rules }),
  });
  if (!put.ok) fail(`PUT /v1/admin/support-tiers: ${put.status} ${await put.text()}`);

  // The public route is served with a one-hour cache header; ask past any cache.
  const getUrl = new URL("/v1/support-tiers", base);
  getUrl.searchParams.set("verify", String(Date.now()));
  const get = await fetch(getUrl, { headers: { "cache-control": "no-cache" } });
  if (!get.ok) fail(`GET /v1/support-tiers: ${get.status} ${await get.text()}`);
  const live = (await get.json()) as TierDoc;

  if (!rulesEqual(doc.rules, live.rules)) {
    fail(
      "The Worker serves different rules from the file.\n" +
        `file:\n${JSON.stringify(doc.rules, null, 2)}\nserved:\n${JSON.stringify(live.rules, null, 2)}`,
    );
  }
  // The apps compare these as text and ignore a served list older than their
  // embedded copy.
  if (!(live.updated >= doc.updated)) {
    fail(`The served list is dated ${live.updated}, older than the file's ${doc.updated}; apps would ignore it.`);
  }
  console.log(`Live: ${live.rules.length} rules, updated ${live.updated}. Matches the file.`);
}
