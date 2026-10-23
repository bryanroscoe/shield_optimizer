# Archive index

Two kinds of material live outside the current tree: historical docs that were removed from the
repository (recoverable from git history, listed first), and bulk working material copied to a
local directory that is not in git at all.

## Docs removed from the repository

Each of these described a moment that has passed: a dated audit, a feedback log, a completed
plan, a merge receipt, or a decision that is now simply how the code works. Their still-true facts
were moved into the live docs named in parentheses. Read any of them with the `git show` command
given.

| Path | What it was | Last version |
|---|---|---|
| `docs/RELEASE-DECISION-2026-09-09.md` | The 2026-09-09 release decision packet: candidate inventory, commit plan, draft 2.2.0-beta changelog and CI proposals, plus the physical test script (the script is now `docs/DEVICE-TEST-SCRIPT.md`) | `git show 74c659f:docs/RELEASE-DECISION-2026-09-09.md` |
| `docs/WHERE-EVERYTHING-IS.md` | Cross-project index written when the Gas Town coordination workspace was shut down (other projects, on-disk archives, state at shutdown) | `git show a015c8a:docs/WHERE-EVERYTHING-IS.md` |
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

## Bulk material outside the repository

Some working material from the retired coordination tree was too large, too build-output-heavy, or
too internal to belong in the repository, but is worth keeping. It was **copied** (never moved) to a
local directory on Bryan's machine:

```
/Users/bryanroscoe/Developer/shield_optimizer-gastown-archive/
```

That directory is **not** in version control and is **not** backed up by anything in this repo. If
the machine is wiped, it is gone. Nothing in this repo depends on it — treat it as a reference of
last resort when you need to know *why* a merged change looks the way it does.

`MANIFEST-sha256.txt` at the archive root lists every file with its SHA-256
(665 entries, manifest sha256 `5155bf2996e453fca39ff5bc7f9985d01c9a85ceaf9582acccc4b63fcc00967b`).
Verify with `cd <archive> && shasum -a 256 -c MANIFEST-sha256.txt`.

### What is in it

- **`mayor-artifacts/`** — per-task briefs, acceptance contracts, source manifests and patches for
  the changes that were merged into `main` in September 2026. Useful if you need the acceptance
  criteria a specific change was written against.
- **`acceptance-evidence/`** — per-candidate evidence directories: review outputs, test artifacts,
  screenshots and applied patches.

Both are heavy with internal process vocabulary from the retired tracker. The durable, readable
versions of this material are the release decision packet listed above,
[`gastown-shield_optimizer-beads-export.json`](gastown-shield_optimizer-beads-export.json) and
[`../v2/mobile/evidence/`](../v2/mobile/evidence/).

| Directory | Size | Files |
|---|---|---|
| `acceptance-evidence/so-2en-97_k0vje` |  16K | 2 files |
| `acceptance-evidence/so-7iw-pending-harness-pzxumlm6` | 4.4M | 2 files |
| `acceptance-evidence/so-fb3-source-integration-kzhhjrj3` |  24K | 2 files |
| `acceptance-evidence/so-fb3.1.1-accepted-v2vec08_` |  72K | 5 files |
| `acceptance-evidence/so-fb3.2.1-technical-b6wrf4ne` | 168K | 20 files |
| `acceptance-evidence/so-vtm.3-d_91juzj` |  24K | 2 files |
| `acceptance-evidence/so-vtm.4-accepted-nd78ubph` |  20K | 2 files |
| `acceptance-evidence/so-vtm.4-appfilter06-20260905-214805` | 268K | 4 files |
| `acceptance-evidence/so-vtm.4-navigator-final-status-20260906T0255.md` | 8.0K | 1 files |
| `acceptance-evidence/so-vtm.4-navigator-k90yz3qm` | 1.9M | 77 files |
| `acceptance-evidence/so-vtm.6-accepted-p9ssvjk5` |  44K | 2 files |
| `acceptance-evidence/so-vtm.6-mechanic-probe-zfudcjny` | 516K | 10 files |
| `acceptance-evidence/so-vtm.6-navigator-review` | 1.9M | 73 files |
| `acceptance-evidence/so-vtm.7-` | 364K | 4 files |
| `acceptance-evidence/so-vtm.7-integrated` | 388K | 5 files |
| `acceptance-evidence/so-vtm.7-navigator-design` | 8.0K | 1 files |
| `acceptance-evidence/so-vtm.7-runtime-held-_k7ffcul` |  20K | 2 files |
| `acceptance-evidence/so-vtm.8-3mgugq2g` |  96K | 8 files |
| `mayor-artifacts/shield-dolt-hq-wisp-ayddd` |  48K | 12 files |
| `mayor-artifacts/shield-github-issue-handoffs-2026-09-08.md` | 8.0K | 1 files |
| `mayor-artifacts/shield-release-decision-20260909.md` |  20K | 1 files |
| `mayor-artifacts/shield-so-2en-local-integration` |  24K | 4 files |
| `mayor-artifacts/shield-so-2en-saved-folder-intent` | 140K | 20 files |
| `mayor-artifacts/shield-so-3ty-release-readiness` |  28K | 7 files |
| `mayor-artifacts/shield-so-7iw-harness-correction` | 148K | 18 files |
| `mayor-artifacts/shield-so-fb3.1-correction` |  84K | 10 files |
| `mayor-artifacts/shield-so-fb3.1-final-checks-y05pkrtn` |  68K | 8 files |
| `mayor-artifacts/shield-so-fb3.1-final-verified-q_bzvm8e` |  68K | 8 files |
| `mayor-artifacts/shield-so-fb3.1-review-sav5u3bx` |  60K | 7 files |
| `mayor-artifacts/shield-so-fb3.1-source` | 168K | 14 files |
| `mayor-artifacts/shield-so-fb3.2-contract` | 804K | 60 files |
| `mayor-artifacts/shield-so-fb3.2-desktop-consumers` | 580K | 35 files |
| `mayor-artifacts/shield-so-fb3.2-desktop-review` |  36K | 3 files |
| `mayor-artifacts/shield-so-fb3.2-integration-record.txt` | 4.0K | 1 files |
| `mayor-artifacts/shield-so-fb3.2-mobile-consumers` | 592K | 48 files |
| `mayor-artifacts/shield-so-fb3.2-mobile-review` | 960K | 99 files |
| `mayor-artifacts/shield-so-fb3.2-review` | 180K | 24 files |
| `mayor-artifacts/shield-so-fb3.2-verified` | 128K | 16 files |
| `mayor-artifacts/shield-so-vtm.9-integration` |  12K | 1 files |
| `mayor-artifacts/shield-so-x02-launcher-correction` | 384K | 26 files |
| `mayor-artifacts/shield-so-x02-launcher-failures` | 296K | 19 files |

### Deliberately excluded from the copy

- Cargo `target/` trees and `node_modules/` (build output and dependency caches; ~26 MB, the bulk of
  it one `rust-fixture` build directory).
- Vite build output and `.timestamp` files inside evidence directories.

All of it is reproducible from source and none of it is evidence of anything.
