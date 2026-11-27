# Archive index

Historical docs removed from the repository, each recoverable from git history.

## Docs removed from the repository

Each of these described a moment that has passed: a dated audit, a feedback log, a completed
plan, a merge receipt, or a decision that is now simply how the code works. Their still-true facts
were moved into the live docs named in parentheses. Read any of them with the `git show` command
given.

| Path | What it was | Last version |
|---|---|---|
| `docs/RELEASE-DECISION-2026-09-09.md` | The 2026-09-09 release decision packet: candidate inventory, commit plan, draft 2.2.0-beta changelog and CI proposals, plus the physical test script (the script is now `docs/DEVICE-TEST-SCRIPT.md`) | `git show 74c659f:docs/RELEASE-DECISION-2026-09-09.md` |
| `docs/gastown-shield_optimizer-beads-export.json` | Export of the retired task tracker, closed items included. Its open items became the GitHub issues | `git show 0d2bdfd5:docs/gastown-shield_optimizer-beads-export.json` |
| `docs/WHERE-EVERYTHING-IS.md` | Cross-project index written when the old multi-agent coordination workspace was shut down (other projects, on-disk archives, state at shutdown) | `git show a015c8a:docs/WHERE-EVERYTHING-IS.md` |
| `docs/INTEGRATION-MOBILE-MAIN-2026-10-17.md` | Merge and validation receipt for integrating `main` into the mobile parity branch (PR #201) | `git show 0c78c06:docs/INTEGRATION-MOBILE-MAIN-2026-10-17.md` |
| `docs/DEVICE-LOGS-2026-10-08.md` | Design note for the desktop Logs tab (#162) (now `v2/HANDOFF.md`) | `git show ab8148b:docs/DEVICE-LOGS-2026-10-08.md` |
| `v2/ADB-QUIT-HARDENING.md` | Follow-on to PR #168 (Stop ADB when quitting): installer lease regression scope, the closed draft PRs #174–179, pinned Tauri exit limits (now `v2/HANDOFF.md`) | `git show ae0436e:v2/ADB-QUIT-HARDENING.md` |
| `v2/ATV-OPTIMIZER-ANDROID-PLAN.md` | Original M0–M6 plan for the Android app, written before the transport change (now `v2/mobile/HANDOFF.md`, `v2/mobile/BACKLOG.md`, `v2/mobile/BILLING-COMMAND-MATRIX.md`) | `git show f4e2513:v2/ATV-OPTIMIZER-ANDROID-PLAN.md` |
| `v2/mobile/ARCHITECTURE-REVIEW.md` | July 2026 mobile architecture review (F/T/B findings, all since addressed) (now `v2/mobile/HANDOFF.md`) | `git show 660ffb9:v2/mobile/ARCHITECTURE-REVIEW.md` |
| `v2/mobile/FEATURES.md` | July 2026 mobile screen ↔ command status matrix | `git show 4e054c1:v2/mobile/FEATURES.md` |
| `v2/mobile/CORRECTNESS-AUDIT-2026-09-05.md` | Nine-finding mobile correctness audit and its fix pass, all fixed | `git show 0be7312:v2/mobile/CORRECTNESS-AUDIT-2026-09-05.md` |
| `v2/mobile/FEEDBACK-UX-2026-09-07.md` | Pre-implementation UX brief for saved-TV identity, Unknown risk, app details and copy (now `v2/mobile/HANDOFF.md`) | `git show a015c8a:v2/mobile/FEEDBACK-UX-2026-09-07.md` |
| `v2/mobile/USER-FEEDBACK-2026-09-05.md` | First phone feedback list; items 1–6 shipped, item 7 became #105 (now `v2/mobile/LIFECYCLE-EVIDENCE.md`) | `git show a015c8a:v2/mobile/USER-FEEDBACK-2026-09-05.md` |
| `v2/mobile/UNKNOWN-DIAGNOSTICS-UX.md` | Pre-implementation contract for the unknown-app diagnostics collector, since shipped | `git show 0be7312:v2/mobile/UNKNOWN-DIAGNOSTICS-UX.md` |
| `v2/mobile/TRANSPORT-LICENSING-RESEARCH.md` | Research behind replacing the GPLv3 libadb-android transport with MIT `adb_client` (now `v2/mobile/HANDOFF.md` §2) | `git show a015c8a:v2/mobile/TRANSPORT-LICENSING-RESEARCH.md` |
| `v2/mobile/DEPENDENCY-REMEDIATION-20261016.md` | Receipt for the 2026-10-16 npm dependency audit and pruning (the desktop advisory is now in `v2/HANDOFF.md`) | `git show 1c3f2b1:v2/mobile/DEPENDENCY-REMEDIATION-20261016.md` |
| `v2/mobile/RELEASE-POLISH.md` | Mobile release-polish checklist with per-PR merge status (open items now in `v2/mobile/BACKLOG.md`) | `git show a9f07d7:v2/mobile/RELEASE-POLISH.md` |
| `v2/mobile/RELEASE-POLISH-RECEIPTS.md` | Per-PR review/CI/merge receipts for PRs #181–#204 and #206 | `git show a9f07d7:v2/mobile/RELEASE-POLISH-RECEIPTS.md` |
| `atv-optimizer-android-strategy.html` | Early feasibility and go-to-market page for the Android app (now `v2/mobile/HANDOFF.md`) | `git show e4a9300:atv-optimizer-android-strategy.html` |
| `testing/` | Docker harness (`Dockerfile`, `Makefile`) for running the v1 script under Linux by hand; CI runs Pester directly | `git show e4a9300:testing/Dockerfile` |
| `v2/DIRECT-ADB-PARITY.md` | Per-area acceptance inventory for the built-in ADB integration while #268 was a draft and platform-tools was still the default (current limits now `v2/DIRECT-ADB.md`) | `git show e4a9300:v2/DIRECT-ADB-PARITY.md` |
| `v2/NATIVE-DISCOVERY-ACCEPTANCE.md` | Acceptance slice for native mDNS discovery before #268 merged (now `v2/DIRECT-ADB.md`) | `git show e4a9300:v2/NATIVE-DISCOVERY-ACCEPTANCE.md` |
| `v2/NATIVE-INSTALL-ACCEPTANCE.md` | Acceptance slice for native single-APK install, written when desktop native still refused installs (now `v2/DIRECT-ADB.md`) | `git show e4a9300:v2/NATIVE-INSTALL-ACCEPTANCE.md` |
| `v2/RUST-ADB-ACCEPTANCE-GATES.md` | Fixture-tested deadlines, first-run identity and physical-acceptance scope for the Rust-primary engine (now `v2/DIRECT-ADB.md`, `docs/DEVICE-TEST-SCRIPT.md` N1–N18) | `git show e4a9300:v2/RUST-ADB-ACCEPTANCE-GATES.md` |
| `docs/evidence/` | Committed PR evidence (836 KB, 10 files): `mobile-main-integration-2026-10-17/`, `native-main-integration-20261101.md`, `v1-tvrecommendations-safety/`. PR evidence is now attached to the PR and these paths are gitignored | `git show e4a9300:docs/evidence/native-main-integration-20261101.md`; list with `git ls-tree -r --name-only e4a9300 docs/evidence` |
| `v2/evidence/` | Committed desktop PR evidence (6.6 MB, 57 files): `cleartv-208-20261022/`, `firetv-protection-205-20261019/`, `launcher-curation-2026-10-23/`, `launcher-feedback-212/`, `native-main-integration-20261101/`, `quit-install-admission-20261017/`, `tvrecommendations-safety/`. The one file a test read is now `v2/tests/fixtures/launcher-protection-result.json`; the `*_EVIDENCE` test captures still write here, gitignored | `git ls-tree -r --name-only e4a9300 v2/evidence`, then `git show e4a9300:<path>` |
| `v2/mobile/evidence/` | Committed mobile PR evidence (10 MB, 189 files): `apk-report-fallback`, `daydream-order`, `density-only-label`, `detail-state`, `diagnostics-header`, `export-discoverability`, `metric-context`, `modal-handoffs`, `modal-scroll`, `ram-accounting`, `report-readability`, `report-safety`, `scaling-preset-wrap`, `screensaver-labels`, `screenshot-retention`, `settings-polish`, `setup-helper-disable`, `sheet-identity`, `thermal-threshold-wording` (all `-2026-10-17`), `cleartv-integration-2026-10-22/`, `entitlement-preview-20261016/`, `launcher-curation-2026-10-23/`, `launcher-protection-integration-2026-10-19/`, `parity-2026-10-06/`, `reconnect-20261015/`, `standalone-fixture-build-2026-10-23/`, `feedback-ux-48cba23-source.json`. The lifecycle reproduction and its baseline moved to `v2/mobile/tests/lifecycle/`; the `*_EVIDENCE` test captures still write here, gitignored | `git ls-tree -r --name-only e4a9300 v2/mobile/evidence`, then `git show e4a9300:<path>` |

