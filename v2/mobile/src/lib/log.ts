// Frontend half of the debug-logging feature. Every backend call routed
// through `api.ts` appends a compact record here (and mirrors to console.*),
// so the "Debug log" section in the More screen can show what the UI has been
// doing alongside the native `read_debug_log()` output.

export interface FrontendLogEntry {
  ts: string;
  command: string;
  argsSummary: string;
  ok: boolean;
  /// Error message when ok === false; short result note otherwise.
  detail?: string;
}

const CAP = 300;
const buffer: FrontendLogEntry[] = [];

/// Compact, non-sensitive one-line summary of an invoke's args. License keys
/// and long text are truncated so the log stays readable and safe to copy.
export function summarizeArgs(args: Record<string, unknown> | undefined): string {
  if (!args) return "";
  try {
    const parts: string[] = [];
    for (const [k, v] of Object.entries(args)) {
      let s: string;
      if (v == null) s = "null";
      else if (Array.isArray(v)) s = `[${v.length}]`;
      else if (typeof v === "object") s = "{…}";
      else s = String(v);
      if (k === "key" || k === "code") s = "***";
      if (s.length > 40) s = s.slice(0, 37) + "…";
      parts.push(`${k}=${s}`);
    }
    return parts.join(" ");
  } catch {
    return "";
  }
}

export function logCall(
  command: string,
  argsSummary: string,
  ok: boolean,
  detail?: string,
): void {
  const entry: FrontendLogEntry = {
    ts: new Date().toISOString().slice(11, 23),
    command,
    argsSummary,
    ok,
    detail,
  };
  buffer.push(entry);
  if (buffer.length > CAP) buffer.splice(0, buffer.length - CAP);

  const line = `[${entry.ts}] ${command}${argsSummary ? " " + argsSummary : ""}`;
  if (ok) console.log(line);
  else console.error(line, detail ?? "");
}

/// Snapshot of the ring buffer, oldest first.
export function getFrontendLog(): FrontendLogEntry[] {
  return [...buffer];
}

/// Rendered as plain text for the copy button.
export function frontendLogText(): string {
  return getFrontendLog()
    .map(
      (e) =>
        `[${e.ts}] ${e.ok ? "OK " : "ERR"} ${e.command}${
          e.argsSummary ? " " + e.argsSummary : ""
        }${e.detail ? " — " + e.detail : ""}`,
    )
    .join("\n");
}
