// Prints the device table from GET /v1/admin/devices.
//
//   ATV_TELEMETRY_URL=https://atv-optimizer-telemetry.<subdomain>.workers.dev \
//   ATV_TELEMETRY_ADMIN_TOKEN=... \
//   npm run report -- [--days 90] [--limit 500] [--group manufacturer,model,device,profile]
//
// Needs Node 22.18+ (runs TypeScript directly). Prints JSON with --json.

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

const url = new URL("/v1/admin/devices", base);
url.searchParams.set("group", flag("group") ?? "manufacturer,model,device,profile");
const days = flag("days");
if (days) url.searchParams.set("days", days);
const limit = flag("limit");
if (limit) url.searchParams.set("limit", limit);

const res = await fetch(url, { headers: { authorization: `Bearer ${token}` } });
if (!res.ok) {
  console.error(`${res.status} ${await res.text()}`);
  process.exit(1);
}
const body = (await res.json()) as {
  group: string[];
  since: string;
  truncated: boolean;
  rows: Array<Record<string, unknown>>;
};

if (args.includes("--json")) {
  console.log(JSON.stringify(body, null, 2));
} else {
  const table = body.rows.map((r) => {
    const ok = Number(r.action_ok);
    const fail = Number(r.action_fail);
    const out: Record<string, unknown> = {};
    for (const g of body.group) out[g] = r[g] ?? "-";
    out.installs = r.installs;
    out.seen = r.seen;
    out.ok = ok;
    out.fail = fail;
    out["ok %"] = ok + fail > 0 ? ((100 * ok) / (ok + fail)).toFixed(1) : "-";
    out.errors = r.errors;
    out["top errors"] = (r.top_errors as Array<{ code: string; count: number }>)
      .slice(0, 3)
      .map((e) => `${e.code} (${e.count})`)
      .join(", ");
    out["last seen"] = String(r.last_seen ?? "").slice(0, 10);
    return out;
  });
  console.table(table);
  console.log(`Since ${body.since}.${body.truncated ? " Truncated: raise --limit or narrow --days." : ""}`);
}
