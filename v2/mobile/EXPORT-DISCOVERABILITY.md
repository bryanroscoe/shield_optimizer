# Screenshot and report export diagnosis

Where every file and report the app produces actually goes, and the proposed scope for Android
export. SAF export is open work (`BACKLOG.md` P1). Nothing here is implemented beyond what the
tables describe.

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
| TV screenshot | `<app-data>/screenshots/<filename_safe(adb-serial)>_<local YYYY-MM-DD_HH-MM-SS>.png` | Dashboard displays returned absolute path and base64 preview; explicitly says private phone storage and sharing/export unavailable. Close dismisses the preview, without deleting the PNG. Capture is automatic; there is no Save chooser/cancel path or screenshot history browser. Errors show a toast. |
| Files / found app backups copied to phone | `<app-data>/downloads/<remote basename>`; collision suffixes preserve existing files | Toast says copied into this app's storage. “Copied this session” lists actual returned name, path and size. Callout says not exportable to Downloads/other apps. No local Open/Share action; session list is not a persistent phone-file browser. “Open folder” on a search hit navigates to the **TV** folder. |
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
private files. The generated AndroidManifest already declares `androidx.core.content.FileProvider` at
`${applicationId}.fileprovider`, with `exported=false` and `grantUriPermissions=true`.
`gen/android/app/src/main/res/xml/file_paths.xml` maps `external-path` and `cache-path` to `.`.
A provider exists; screenshot Open/Share UI and the native export/share bridge remain unwired.
No app-level `ACTION_SEND` or SAF create-document flow was found in the inspected sources. No broad storage permission is needed for the
existing private saves.

Existing tests cover screenshot PNG/error validation and safe filename conversion in core;
file-copy feedback, stale-session suppression, collision preservation and partial-file cleanup;
unknown-diagnostic snapshot fidelity/redaction; and app-report browser/clipboard fallback.
No mobile browser test stub for native `take_screenshot` was found. Evidence-capture calls to
Playwright `page.screenshot` are not product screenshot tests. No SAF save/cancel or native Share
coverage exists because those product flows are not implemented. Green runs of unrelated tests are not export acceptance.

## Copy rules for the existing flows

The screenshot preview names the saved file and its private location, says it is absent from the
public Gallery/Downloads, and says Close only dismisses the preview; Files' "Copied this session"
uses the same destination language. Don't add an Open/Share button before the native bridge
exists, and don't send users to Files as if it listed phone screenshots. Text reports say **Copy**,
with clipboard success/error states, and never claim a file was saved. Opening an oversized app
report copies its text before the browser launches; keep that feedback.

## Broader export scope to agree before implementation

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
