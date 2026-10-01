# registry-triage

Turns a folder of user diagnostics into a candidate list for whoever reviews
the app catalog (`crates/core/data/app-lists/`). It reads and prints. It never
writes the catalog or any other file.

```
cargo run -p registry-triage -- path/to/reports            # table
cargo run -p registry-triage -- --json path/to/reports     # {truncated_exports, candidates}
```

Run it from `v2/` or `v2/src-tauri/`. Several folders may be given. Each
folder is searched up to four levels deep, and symlinks are not followed.

## What it reads

- **Mobile exports** (`*.json`): the `UnknownDiagnosticReport` that
  `mobile/src/lib/unknownDiagnostics.ts` exports, `schema_version: 1`. Each
  record is checked against the same rules the collector applies, including
  the token shape and the rejection of address-like tokens. A record that
  fails a check is counted as rejected and dropped. It is never repaired.
- **Desktop app reports** (`*.json`): what "Report this app" in the desktop
  app's detail panel saves (`src/lib/app-report.ts`). It is the same
  `schema_version: 1` shape with one record, a `user_report_*` reason, and
  extra fields this tool ignores (app name, the verdict shown, the user's
  note, and optional state). A report is one user's claim, read like any
  other sighting; it never changes the verdict printed beside it.
- **Desktop bug-report bundles** (`*.md`, `*.txt`): the Markdown from Copy
  diagnostics. By design the bundle carries no package inventory, so the only
  packages taken from it are the HOME handlers. Device scope comes from its
  "Detected type" line. The serial and device properties are never read.

Anything else is listed as skipped.

## What it prints

There is one row per package (and a separate section for unresolved process
names). Each row shows how many report files mention it, the summed sighting
count, the device scope it was seen on (`shield`, `googletv`, `android_tv`,
`unknown`), the reasons the app gave, and the verdict the shipped catalog gives
today, with that entry's `reviewed_at` when it has one.

The mobile collector caps itself (100 records, 64 KiB) and drops its oldest
records when it hits the cap, exporting `truncated: true`. The summary line
names every truncated export. A capped export cannot say which records it
dropped, so while a batch holds one, every row marks its report and sighting
counts with `*`: they are lower bounds, not totals. An export whose
`truncated` flag is missing or not a boolean is treated as truncated, since it
does not vouch that nothing was dropped. In `--json` the same files are listed
under `truncated_exports`, and each candidate carries `lower_bound`.

## Rules for using the output

- Reports are untrusted evidence. A package that shows up often is a package
  worth researching, not a package that is safe. Nothing here is a reason to
  set `risk: "safe"`.
- A catalog change is a normal reviewed PR. It edits the JSON by hand, sets
  `reviewed_at` to the review date, and lists the evidence in `sources`. The
  loader tests in `crates/core/src/commands/loader.rs` then validate the
  dates, the scopes and the Safe-needs-a-date rule.
- Run it when a batch of reports arrives, when upstream evidence changes, or
  after a classification regression. Nothing schedules it.
