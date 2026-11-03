// Turns a support lookup (src/support.ts) into a report a person can read in
// a terminal. Pure, so the test suite covers it.

interface Device {
  manufacturer: string;
  model: string;
  android_release: string | null;
  profile: string | null;
  events: number;
}

interface Event {
  received_at: string;
  app: string;
  app_version: string;
  os: string;
  kind: string;
  device: Record<string, unknown> | null;
  action: string | null;
  ok: boolean | null;
  error_code: string | null;
  message: string | null;
  diag: unknown;
  log_excerpt: string | null;
}

interface Upload {
  id: number;
  received_at: string;
  app_version: string;
  os: string;
  reports_on: boolean | null;
  bytes: number;
}

interface Install {
  install_id: string;
  short: string;
  first_seen: string | null;
  last_seen: string | null;
  events: number;
  kinds: Record<string, number>;
  apps: string[];
  app_versions: string[];
  os: string[];
  devices: Device[];
  error_codes: Array<{ code: string; count: number }>;
  recent: Event[];
  uploads: Upload[];
}

export interface Report {
  query: string;
  matches: number;
  truncated: boolean;
  installs: Install[];
}

const when = (ts: string | null) => (ts ? ts.replace("T", " ").replace(/\.\d+Z$/, "Z") : "-");
const kb = (n: number) => (n < 1024 ? `${n} B` : `${(n / 1024).toFixed(1)} KB`);

function eventLine(e: Event): string {
  const parts = [when(e.received_at), e.kind];
  if (e.action) parts.push(e.action);
  if (e.ok !== null) parts.push(e.ok ? "ok" : "failed");
  if (e.error_code) parts.push(e.error_code);
  if (e.device) parts.push(`[${e.device.manufacturer ?? ""} ${e.device.model ?? ""}]`.replace("[ ", "["));
  parts.push(`(${e.app} ${e.app_version}, ${e.os})`);
  return parts.join("  ");
}

export function formatSupportReport(r: Report): string {
  const out: string[] = [];
  const plural = r.matches === 1 ? "install" : "installs";
  out.push(`Support ID ${r.query}: ${r.matches} ${plural} matched${r.truncated ? " (showing the 5 most recent)" : ""}`);
  if (r.matches === 0) {
    out.push("", "Nothing stored under that ID. Raw events are kept 90 days and logs 30 days; an ID changes when");
    out.push("the user turns reports off and on again.");
  }
  for (const i of r.installs) {
    out.push("", `Install ${i.install_id} (${i.short})`);
    out.push(`  Seen        ${when(i.first_seen)} .. ${when(i.last_seen)}`);
    const kinds = Object.entries(i.kinds)
      .map(([k, n]) => `${k} ${n}`)
      .join(", ");
    out.push(`  Events      ${i.events}${kinds ? ` (${kinds})` : ""}`);
    if (i.apps.length) out.push(`  App         ${i.apps.join(", ")} ${i.app_versions.join(", ")} on ${i.os.join(", ")}`);
    for (const d of i.devices) {
      out.push(
        `  TV          ${d.manufacturer} ${d.model}${d.android_release ? `, Android ${d.android_release}` : ""}${d.profile ? `, ${d.profile}` : ""} (${d.events} events)`,
      );
    }
    if (i.error_codes.length) out.push(`  Codes       ${i.error_codes.map((c) => `${c.code} x${c.count}`).join(", ")}`);
    for (const u of i.uploads) {
      const reports = u.reports_on === null ? "" : u.reports_on ? ", reports on" : ", reports off";
      out.push(`  Upload #${u.id}  ${when(u.received_at)}  ${kb(u.bytes)}  ${u.app_version} on ${u.os}${reports}`);
      out.push(`              read it: scripts/support-lookup.sh --upload ${u.id}`);
    }
    if (i.recent.length) out.push("", "  Recent events, newest first:");
    for (const e of i.recent) {
      out.push(`    ${eventLine(e)}`);
      if (e.message) out.push(`      message: ${e.message}`);
      if (e.diag) out.push(`      diag: ${JSON.stringify(e.diag)}`);
      if (e.log_excerpt) {
        out.push("      log:");
        for (const line of e.log_excerpt.split("\n")) out.push(`        | ${line}`);
      }
    }
  }
  return `${out.join("\n")}\n`;
}
