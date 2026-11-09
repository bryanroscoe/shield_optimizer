# Screenshot and report export diagnosis

Where every file and report the app produces actually goes, and the Android export scope.
**Phase 1 shipped 2026-11-08:** Save a copy and Share for one explicitly selected screenshot or
pulled file (section "Phase 1" below). It has not been run on a phone. SAF import is phase 2
(`BACKLOG.md` P1). Nothing here is implemented beyond what the tables describe.

## Current destinations and actions

`src-tauri/src/lib.rs` resolves `app.path().app_data_dir()` and passes it to `AppState`.
The installed Tauri 2.11.2 source was checked: `src/path/android.rs::app_data_dir` invokes
`getDataDir`; `mobile/android/.../PathPlugin.kt::getDataDir` returns `activity.dataDir.absolutePath`
(API 24+) or `applicationInfo.dataDir`. This is the private app data root, **not** its `files/`
subdirectory. Common Android layouts resemble `/data/user/<userId>/com.atvoptimizer.mobile`
(with `/data/data/...` aliases), but no physical runtime path was observed. The returned runtime
path is authoritative, including for another Android user/profile. Resolver failure uses the
existing host-development `ATVOptimizer` fallback; do not hard-code an Android absolute path.

| Flow | Exact source destination | Current user feedback and actions |
|---|---|---|
| TV screenshot | `<app-data>/screenshots/<filename_safe(adb-serial)>_<local YYYY-MM-DD_HH-MM-SS>.png` | Dashboard displays returned absolute path and base64 preview and says it is private phone storage. The preview offers **Save a copy** and **Share** (phase 1); each reports saved/not saved/share sheet opened/error inline. Close dismisses the preview, without deleting the PNG. Capture is automatic; there is no screenshot history browser, so a screenshot can be exported only from its preview. Errors show a toast. |
| Files / found app backups copied to phone | `<app-data>/downloads/<remote basename>`; collision suffixes preserve existing files | Toast says copied into this app's storage. “Copied this session” lists actual returned name, path and size, and each row has **Save a copy** and **Share** icon buttons (phase 1). The callout says copies are private, not in public Downloads. No local Open action; the session list is not a persistent phone-file browser, so earlier copies can't be exported after leaving Files. “Open folder” on a search hit navigates to the **TV** folder. |
| APK backup bundle | `<app-data>/backups/<package>-<UTC timestamp>/`, with APK parts and manifest | Backups has a persistent in-app list, private-storage/no-export label and existing restore/delete actions. This is not portable app-data or full-device recovery. |
| Unknown-app diagnostic JSON | WebView `localStorage`, key `atv.unknownDiagnostics.v1`; explicit snapshot serialized in memory | More previews JSON, copies explicitly, shows copied/error feedback and allows manual selection after clipboard failure. No `.json` file is written by Copy. |
| “Report this app” | Report object/JSON in memory; clipboard after Copy or the long-report GitHub-opening fallback; optional prefilled GitHub web URL | Preview equals copy/issue text. Clipboard fallback and browser-failure messages exist. Opening a GitHub form does not submit it. No saved report-file destination or native Sharesheet action. |
| Debug log | Native `<app-data>/debug.log` plus in-memory frontend call log | More reads the last approximately 500 native lines, combines with frontend log, and copies text. No exported file or native Share action. |
| Diagnostics playback/resources and health reports | Component/session memory; `report_all` returns structured values and its API wrapper has no screen consumer | Read/sample actions render results; there is no file-export/copy/share control for these reports. Do not tell users a “report file” has been saved. |
| Configuration snapshots | `<app-data>/snapshots/<safe name>_<timestamp>.json` | Separate in-app snapshot management. No Android SAF export flow. |

Trace: shared `commands/state.rs`, `commands/screenshot.rs`, `commands/snapshot.rs`;
mobile `src-tauri/src/file_commands.rs`; `Dashboard.svelte`, `Files.svelte`, `Backups.svelte`,
`More.svelte`, `AppReportSheet.svelte`, `DiagnosticReports.svelte`, `lib/unknownDiagnostics.ts`.

## Existing capability and test boundaries

The opener plugin is installed, but `capabilities/default.json` grants only HTTP(S) `open-url`,
and `api.openUrl` rejects other schemes. That is browser-link capability, not permission to open
private files; phase 1 did not change the capability file. The generated AndroidManifest declares
`androidx.core.content.FileProvider` at `${applicationId}.fileprovider`, with `exported=false` and
`grantUriPermissions=true`. Phase 1 narrowed `gen/android/app/src/main/res/xml/file_paths.xml`
from `external-path` and `cache-path` at `.` to a single `cache-path` at `exports/`, so the
provider can hand out URIs only for staged share copies. No storage permission is declared or
needed.

Existing tests cover screenshot PNG/error validation and safe filename conversion in core;
file-copy feedback, stale-session suppression, collision preservation and partial-file cleanup;
unknown-diagnostic snapshot fidelity/redaction; and app-report browser/clipboard fallback.
`tests/export-discoverability.test.mjs` and `tests/export-actions.test.mjs` stub `take_screenshot`
and `export_file` in the browser and cover the Save a copy / Share requests, saved, not-saved,
share-opened and error states, one-at-a-time locking, the help text and narrow layouts.
`src-tauri/src/export_commands.rs` unit tests cover path confinement (ADB key, license, logs,
snapshots, backups, folders, traversal and symlinks refused), the address-free screenshot name,
MIME types, staging, the size cap and pruning. Nothing exercises the Kotlin picker, the
ContentResolver copy, FileProvider or the share sheet: those need a phone. Evidence-capture calls
to Playwright `page.screenshot` are not product screenshot tests. Green runs of unrelated tests
are not export acceptance.

## Copy rules for the existing flows

The screenshot preview names the saved file and its private location, says it is absent from the
public Gallery/Downloads, and says Close only dismisses the preview; Files' "Copied this session"
uses the same destination language. Save a copy says "Saved a copy" only after the backend
reports the write finished (with the provider's display name when it gives one) and "Not saved."
when the picker is dismissed. Share says only "Share sheet opened." How each works, the
address-free name and that nothing leaves on its own sit behind the "About saving and sharing" /
"About file transfers" help. Don't add an Open button, and don't send users to Files as if it
listed phone screenshots. Text reports say **Copy**,
with clipboard success/error states, and never claim a file was saved. Opening an oversized app
report copies its text before the browser launches; keep that feedback.

## Phase 1 (shipped 2026-11-08, not device-verified)

One explicitly selected file, both actions, no export centre:

- **Bridge.** `export_file { path, mode: "save" | "share" }` (Free) in
  `src-tauri/src/export_commands.rs`. It canonicalizes `path` and accepts only a regular file
  directly inside `<app-data>/screenshots` (PNG) or `<app-data>/downloads`; the ADB key, license,
  logs, snapshots, backups, folders, traversal and symlinks out are refused. One request at a time
  (`ExportGate`). The Kotlin side lives in the existing atv-adb plugin (`AdbPlugin.saveCopy`,
  `saveCopyResult`, `shareFile`), called only from Rust; no JS-invokable plugin command and no
  capability change. The only new Gradle declaration is `androidx.activity:activity:1.10.1`, which
  the app already resolved.
- **Save a copy.** `ACTION_CREATE_DOCUMENT` with the file's MIME type and suggested name; the
  ContentResolver copy runs off the main thread and only then resolves `saved` with the provider's
  `DISPLAY_NAME`. A dismissed picker resolves `canceled`. A failed write deletes the half-written
  document and reports the error. The original is never touched. The request gives up after
  15 minutes if Android never answers (e.g. the activity was destroyed), and says to check the
  chosen folder; the next Save a copy rejects that stale request on the Kotlin side. Share's
  staging copy has no timeout, so it never overlaps another export.
- **Share.** The file is copied to `<cache>/exports/share-<n>/<name>` (512 MiB cap; larger files
  are told to use Save a copy), and `ACTION_SEND` goes through `Intent.createChooser` with
  `EXTRA_STREAM`, `ClipData` and `FLAG_GRANT_READ_URI_PERMISSION`. `FileProvider.getUriForFile`
  refuses anything outside the mapped folder. Staged copies older than an hour are removed at the
  next share and at app start. The UI says only that the sheet opened.
- **Names.** Screenshots leave as `tv-screenshot-<YYYY-MM-DD_HH-MM-SS>.png`, without the TV's
  serial/address. Pulled files keep their TV file name.
- **Privacy.** Nothing leaves the phone except through the system UI the user completes. Export
  failures are not sent in anonymous reports (`shared/telemetry.ts` quiet list).
- **Device gates still open.** Picker on a phone, cancel, a Downloads and a Drive destination, a
  provider that refuses writes, Share to a real receiver, activity recreation during the picker.

## Next: phase 2 and what stays out

- **SAF import** (open-document picker → stream into a staged app-private file), which unblocks
  phone→TV upload in Files and install APK / sideload (`BACKLOG.md` P1 #1, #2). Same rules: one
  user-picked document, no persistent grants, no storage permission.
- **Snapshot export** reuses this bridge mechanically, but a snapshot carries the TV's
  `device_serial`, name and package inventory, so it needs a privacy decision (and probably a
  redacted export format) first.
- **APK bundles and the bug-report text** as files: separate format, privacy and retention
  decisions, below.

## Original scope notes (kept for the reasoning)

A useful first native scope is one explicitly selected screenshot, rather than an entire export
center or blanket filesystem access. Choose either Save a copy or Share, then add the other later:

- **Save a copy:** Android `ACTION_CREATE_DOCUMENT`, MIME `image/png`, user-selected destination,
  and a native ContentResolver copy. Return saved/canceled/error explicitly; only claim save after
  stream completion. Report provider display name/URI honestly rather than inventing a filesystem
  path. The system picker does not require broad storage permissions. See
  [Android document creation guidance](https://developer.android.com/training/data-storage/shared/documents-files).
- **Share:** a small Android bridge, content URI, correct MIME type and temporary read grant.
  First audit/narrow the generated provider's URI exposure, or use an appropriately scoped separate
  provider for a staged export/cache directory; the generated `.` paths must not be assumed to be
  a narrowly scoped screenshot provider. Do not expose the entire app
  data root, which also contains sensitive app state. Suggest a non-identifying export filename;
  current screenshot names embed a sanitized ADB serial/endpoint. Launching a chooser is not proof
  that a receiver completed sharing. See
  [Android secure file sharing](https://developer.android.com/training/secure-file-sharing/share-file).

These require new native command/capability plumbing and, for Share, review of the existing manifest provider/path
configuration (or an appropriately scoped separate provider) and a temporary URI permission grant. They are platform changes, despite not requiring
broad external-storage runtime permissions. Keep original private files intact on cancellation or
failure; scope source reads to validated exportable files; bound staging size/lifetime and handle
Activity recreation, missing provider/receiver, concurrent requests and stale UI continuations.
No persistent directory grants, Downloads-wide access, credential changes or security-setting
changes are proposed. Physical Android/provider validation would remain a separate acceptance gate.

Do not extend screenshot export to APK backups or diagnostic bundles without separate format,
privacy and retention decisions. Existing app-report/unknown-diagnostic serialization should be
reused if a later explicit JSON-file export is approved.
