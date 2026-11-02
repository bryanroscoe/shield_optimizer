// Formats GET /v1/admin/support/<id> JSON (stdin) for a terminal. Called by
// support-lookup.sh; the formatting lives in support-lookup-lib.ts.

import { formatSupportReport } from "./support-lookup-lib.ts";

let input = "";
process.stdin.setEncoding("utf8");
for await (const chunk of process.stdin) input += chunk;
try {
  process.stdout.write(formatSupportReport(JSON.parse(input)));
} catch (e) {
  console.error(`Could not read the lookup response: ${(e as Error).message}`);
  process.exit(1);
}
