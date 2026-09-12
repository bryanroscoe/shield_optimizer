# Handoff — backlog and release state (2026-09-12)

The backlog for this repo used to live in an external tracker that is being retired. Everything
has been moved here. This file is the entry point: what the project is, where the release stands,
where the work items went, and what to pick up first.

## What Shield Optimizer is

Two products share this tree, with deliberately separate release tracks.

- **v1 — `Shield-Optimizer.ps1`** (repo root). A PowerShell debloater for Android TV devices driven
  from a Windows PC over ADB. Still maintained. Released by `release.sh` at the repo root, which
  tags and calls `gh release create`. CI is `tests.yml`.
- **v2 — `v2/`** A Tauri 2 + Rust + Svelte 5 desktop app (`v2/src-tauri` + `v2/src`) built on an
  audited pure engine (`v2/crates/core`). Released on `v2-*` tags via
  `.github/workflows/v2-release.yml`. Latest published release is **v2-2.1.0** (2026-06-22).
- **v2 mobile companion — `v2/mobile/`** A phone app (same stack, `tauri-plugin-atv-adb` +
  a vendored pure-Rust `adb_client`) that drives an Android TV over wireless ADB with no PC.
  Unreleased; no store channel and no release workflow yet. Its own handoff is
  `v2/mobile/HANDOFF.md`, which is the authoritative document for mobile work.

`v2/CLAUDE.md`-level architecture invariants worth knowing before touching v2 are in the repo's
`CLAUDE.md`: the engine stays pure (no I/O), there is exactly one ADB wrapper and one detection
function, app lists live in JSON and not in code, snapshots are versioned and path-confined, and
the do-not-disable safety list gates every disable path.

## Where the release stands

- The last **code** change on `main` is **`12c9859`** (commits after it are documentation only).
  The two large integration commits before it are `36b561c` (core + desktop) and `d02de57`
  (mobile).
- Everything on `main` past the v2-2.1.0 tag is **unreleased and unverified on physical hardware.**
  Local gates on one macOS host are green — core `cargo fmt`/`clippy`/tests, `src-tauri` tests,
  desktop and mobile `svelte-check` 0/0 and builds, mobile tests 78/78 — but **no Linux, no
  Windows, and no device run** has been done on this candidate.
- **Outstanding before a release:**
  1. **Physical device test.** A ~45-minute script for a Shield TV (desktop pairing, launcher
     switch and rollback, Unknown-safety labels, sideload folder scan, mobile reconnect) was
     written and never run. It is the gate on the reported issues below, three of which
     (#87 launcher, #88 pairing, #89 macOS volume prompts) were fixed without reproducing the
     reporter's hardware.
  2. **Tag and publish.** The proposal was desktop `v2-2.2.0-beta.1` first
     (`cd v2 && ./release.sh --minor --beta`), promoted to `v2-2.2.0` once the reporters confirm;
     mobile bumps to `0.2.0` as an internal debug APK only. A drafted `v2-2.2.0-beta.1`
     CHANGELOG section exists in the decision packet below — `v2/CHANGELOG.md` still needs it,
     since release notes are read from that file.
  3. **Cross-OS CI.** Mobile and desktop harness jobs were proposed for `v2-tests.yml` and never
     added, so the mobile suite has never run on Linux or Windows.
- **The full decision packet is preserved in this repo at
  [`docs/RELEASE-DECISION-2026-09-09.md`](RELEASE-DECISION-2026-09-09.md).** It has the per-file
  provenance, the commit plan, the drafted `v2-2.2.0-beta.1` changelog section, the exact
  step-by-step device script (desktop D1–D8 and mobile M1–M8, with an abort rule), the cross-OS CI
  gap list, and an appendix on what is still unknown about #87 and #89. Read it before tagging.

## Where the work items went

- **The live backlog is the GitHub issues labeled [`gastown`](https://github.com/bryanroscoe/shield_optimizer/issues?q=is%3Aissue+is%3Aopen+label%3Agastown).**
  One issue per work item, titled `[<id>] <title>`, carrying the original description, acceptance
  criteria, sub-items, dependencies and recent comment history, labeled `gastown` plus a priority
  (`p0`–`p4`) and a type.
- **Some migrated issues duplicate reports that were already filed here.** `#88` (TCL Android 14
  pairing), `#89` (macOS removable-volume prompts) and `#91` (Remote clipboard paste) are the
  original public reports; the migrated items covering the same work are separate issues. Close
  one side of each pair as a duplicate rather than working both.
- **Engineering records that used to live outside this repo are now in it:**
  [`docs/RELEASE-DECISION-2026-09-09.md`](RELEASE-DECISION-2026-09-09.md) (release and device-test
  packet), [`v2/mobile/UNKNOWN-DIAGNOSTICS-UX.md`](../v2/mobile/UNKNOWN-DIAGNOSTICS-UX.md) (the UX
  design for the unknown-app diagnostics log), and
  [`v2/mobile/evidence/`](../v2/mobile/evidence/) — the lifecycle acceptance evidence for the
  app-restart investigation, including `lifecycle/reproduce.mjs`, a runnable Playwright harness
  that regenerates `baseline-48cba23.json` against a mocked Tauri layer with no device.
  Not preserved: a 10 MB working directory of per-candidate manifests and patches, which was
  vite build output, screenshots, and diffs of code that is already on `main`.
- **Full history, including closed items, is `docs/gastown-shield_optimizer-beads-export.json`**
  (39 records). That file is the archive — closed items explain why decisions were made and are
  not part of the live backlog. Four records in it are retired-tracker scaffolding
  (`so-6kf`, `so-b8a`, `so-k0e`, `so-rig-shield_optimizer`) with no product content; they were
  closed rather than migrated.

## What to pick up first

1. **Run the physical device test and ship the beta.** This is the highest-value item by a wide
   margin. Four user-reported issues are fixed on `main` and none of them have reached a user,
   including #86 (SmartTube backups), whose fix has been sitting unreleased since before v2-2.1.0.
   Nothing else in the backlog unblocks as much.
2. **Add the mobile and desktop harness jobs to cross-OS CI.** Cheap, and it stops the "green on
   one macOS host" problem from repeating.
3. **`[so-ncj]` transient mobile browser-suite failures.** The Playwright suite has intermittent
   failures under a full run (apps-filter timeout, backups dynamic-import fetch) that were never
   root-caused. Flaky tests will undermine every item above.
4. Then work the `gastown` P1 items by label. `[so-fb3.2]` (process/memory-consumer identity) and
   `[so-fb3.3]` (unified app details and measurements) are the largest remaining product gaps.

A note on the mobile app specifically: its safety story depends on never claiming more identity
certainty than it has — saved TVs are matched on verified hardware id and never on IP address
alone, and uncatalogued packages are labeled Unknown rather than Safe. Several closed items in the
export exist only to enforce that. Please preserve it.
