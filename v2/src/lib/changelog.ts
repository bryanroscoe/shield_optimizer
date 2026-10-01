/// Release history bundled into the app at build time.
///
/// The notes for the version a user is running ship inside that version, so
/// "what changed?" never depends on a network call. It used to: the
/// after-update pop-up read the running release's notes from the GitHub API,
/// and a launch where that call failed or timed out showed nothing — while
/// still recording the version as seen, so the notes were lost for good.
///
/// Pure: takes the CHANGELOG.md text and returns data. Bodies go through
/// `parseReleaseNotes` like any other notes, so this and the pre-install
/// dialog render identically.

export interface ChangelogEntry {
  /// Without the `v2-` tag prefix, e.g. `2.3.0`.
  version: string;
  /// `YYYY-MM-DD` from the heading (`## v2-2.3.0 — 2026-09-30`), when present.
  date: string | null;
  body: string;
}

const HEADING = /^##\s+v2-(\S+?)(?:\s+[—–-]\s+(\d{4}-\d{2}-\d{2}))?\s*$/;

export function parseChangelog(markdown: string): ChangelogEntry[] {
  const entries: ChangelogEntry[] = [];
  let current: ChangelogEntry | null = null;
  let lines: string[] = [];

  const flush = () => {
    if (current) {
      current.body = lines.join("\n").trim();
      entries.push(current);
    }
    current = null;
    lines = [];
  };

  for (const line of markdown.replace(/\r\n/g, "\n").split("\n")) {
    if (/^##\s/.test(line)) {
      flush();
      const m = HEADING.exec(line.trim());
      // `## Unreleased` and any other non-release heading end the previous
      // section but start none of their own.
      if (m) current = { version: m[1], date: m[2] ?? null, body: "" };
      continue;
    }
    if (current) lines.push(line);
  }
  flush();
  return entries;
}

const isPrerelease = (version: string) => version.includes("-");

/// The `limit` releases a reader on `running` would care about, newest first:
/// the running version and the ones before it. Pre-releases are left out for
/// someone on a stable build — a list of betas they never ran is noise.
///
/// CHANGELOG.md is newest-first by convention, so file order is release order.
/// A build whose version has no section (a dev build) gets the newest entries.
export function recentReleases(
  entries: ChangelogEntry[],
  running: string,
  limit = 5,
): ChangelogEntry[] {
  const at = entries.findIndex((e) => e.version === running);
  const stable = !isPrerelease(running);
  return entries
    .slice(at === -1 ? 0 : at)
    .filter((e, i) => (i === 0 && at !== -1) || !stable || !isPrerelease(e.version))
    .slice(0, limit);
}

export function notesFor(entries: ChangelogEntry[], version: string): string | null {
  const entry = entries.find((e) => e.version === version);
  return entry && entry.body ? entry.body : null;
}
