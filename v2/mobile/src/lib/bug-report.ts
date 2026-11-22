// More › Report a bug. The report itself is built and redacted in Rust
// (`preview_bug_report` / `send_bug_report`); this only gathers what the
// frontend alone knows, the recent connection steps, and builds the GitHub
// fallback link.

import { getFrontendLog } from "./log";

export const BUG_NOTE_MAX = 4000;
/// The counter appears this close to the limit.
export const BUG_NOTE_WARN = BUG_NOTE_MAX - 400;

export interface ConnectionStep {
  command: string;
  ok: boolean;
}

export interface BugReportPreview {
  /// The description as it will be sent, after redaction. Null when empty.
  note: string | null;
  text: string;
  bytes: number;
}

export interface BugReportResult {
  support_id: string;
  bytes: number;
  report_id: number | null;
}

/// The same steps the public report lists. Rust drops anything else.
const CONNECTION_COMMANDS = new Set(["wireless_discover", "wireless_pair", "wireless_connect", "wireless_status"]);

/// The last eight connection steps: the command and whether it worked,
/// never its arguments or message.
export function connectionSteps(): ConnectionStep[] {
  return getFrontendLog()
    .filter((entry) => CONNECTION_COMMANDS.has(entry.command))
    .slice(-8)
    .map((entry) => ({ command: entry.command, ok: entry.ok }));
}

/// The text Send uploads, as the preview shows it.
export function previewText(preview: BugReportPreview): string {
  const note = preview.note ? `### Description\n\n${preview.note}\n\n` : "";
  return `${note}${preview.text}`;
}

const ISSUE_BASE =
  "https://github.com/bryanroscoe/shield_optimizer/issues/new" +
  `?template=bug_report.yml&title=${encodeURIComponent("Bug: ")}`;
/// Well under the ~8 KB GitHub accepts in a URL.
const PREFILL_LIMIT = 6000;

/// The GitHub bug form with the description in "What happened" and the
/// public report in "Diagnostics", dropping whatever doesn't fit. The form
/// still has to be submitted by the user.
export function bugIssueUrl(note: string, diagnostics: string): string {
  const what = note.trim() ? `&what-happened=${encodeURIComponent(note.trim())}` : "";
  const diag = diagnostics.trim() ? `&diagnostics=${encodeURIComponent(diagnostics)}` : "";
  for (const url of [`${ISSUE_BASE}${what}${diag}`, `${ISSUE_BASE}${what}`, `${ISSUE_BASE}${diag}`]) {
    if (url.length <= PREFILL_LIMIT) return url;
  }
  return ISSUE_BASE;
}
