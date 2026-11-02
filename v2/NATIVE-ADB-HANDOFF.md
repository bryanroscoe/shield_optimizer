# Native desktop ADB takeover — 2026-11-02

## Outcome the owner needs

Make native Rust ADB reliable and deliver one clearly identified desktop app the
owner can test. Preserve the existing Devices UI and connection flows. The owner
reported slow scans and missing TVs; passing protocol fixtures is not evidence
that those complaints are resolved. The previous agent has not completed native
acceptance and is handing this work to a fresh agent at the owner's request.

PR: https://github.com/bryanroscoe/shield_optimizer/pull/268
Branch: `feat/native-desktop-test-20261031`
Implementation and built candidate: `995fb2a9822495c7067c09400a7f3dde204d6f4a`.
This handoff adds documentation only; it does not change the candidate binary.
PR is open and draft. No native release or shipping-default change has occurred.
Normal builds still use platform-tools; `native-adb-default` selects native Rust.

## Start here

1. Read `AGENTS.md`, this file, and `HANDOFF.md`. Check branch, status and current
   PR head. Reuse the existing task worktree; preserve unrelated primary-checkout
   edits. All implementation through the SHA above was already committed/pushed.
2. Recheck desktop access and actual app processes. The last observed Mac session
   was locked, and exact-path CUA selection returned `cgWindowNotFound`. This was
   an environmental observation, not proof the app crashed or cannot be tested
   now. Do not repeat stale status indefinitely or rebuild a passing binary merely
   because GUI access is unavailable. Do not bypass the screen lock.
3. Distinguish the old running app from the newly staged candidate below. Select
   by exact path, not bundle ID: generated and delivery bundles share the native
   test ID. Do not overwrite files of the running app.
4. When GUI access is available, quit the old test app through supported app
   automation, verify its process exited, replace the delivery with the candidate,
   verify hash/signature, launch that exact path, and record the actual PID/title.
   Update the delivery receipt only after that replacement/launch succeeds.
5. Test the owner's complaint first: scan, visible unapproved/failed rows, time to
   first responder and ready TV, completion, retry, Forget, and repeat scan. Capture
   the actual current native UI. Do not substitute a browser simulation screenshot
   for native hardware evidence. Use existing diagnostics to separate subnet probe,
   mDNS, handshake and property-read delays before making another change.
6. Exercise the corrected denied download and a small successful read-only pull.
   Then decide readiness from observed behavior, fix concrete failures, and commit
   and push each completed milestone to this same PR. Avoid unrelated redesign.

## Source changes already made

- Desktop native integration shares the mobile Rust transport. Discovered,
  unapproved and failed endpoints retain their existing cards and retry/Forget
  actions. Approval guidance requires an observed public-key AUTH packet.
- Connection attempts have bounded ownership/cancellation and a total handshake
  deadline. Forget/superseding an attempt closes its socket. A successful session
  is detached from the attempt deadline. TLS rejection points to Pair PIN and the
  separate connection port rather than assuming legacy AUTH.
- Subnet responders enter bounded native connection admission as they arrive;
  discovery no longer waits for the whole sweep/mDNS stage before connecting.
  Original 250 ms probe timeout and 64-probe concurrency are retained. Debug timing
  covers subnet detection, first response, admission completion and mDNS.
- Sync download/STAT parsing handles arbitrary WRTE fragmentation and DATA/DONE/FAIL
  framing. FAIL text is an error, never file contents. Upload SEND/DATA/DONE consume
  early or mid-transfer FAIL, including replies split around transport ACKs.
  Success requires final sync OKAY after DONE; no retry/replay of uncertain writes.
- Windows native identity uses protected current-user ACLs, bounded reads,
  reparse-free held path handles and no-clobber publication. Existing identities
  are never silently rotated or overwritten. Eleven Windows identity tests pass.

Relevant code: `src-tauri/src/adb/`, shared `crates/direct-adb/`, and
`vendor/adb_client/src/message_devices/adb_session.rs`, `commands/push.rs`,
`tcp/adb_tcp_device.rs`. Frontend discovery is in `src/routes/+page.svelte` with
regressions in `tests/native-scan-progress.mjs`. Check actual paths before editing.
The latest transfer fixes changed no UI layout.

## Validation and its limits

All six CI checks passed at **995fb2a**, run:
https://github.com/bryanroscoe/shield_optimizer/actions/runs/41396639139
Linux, macOS and Windows Rust; desktop and mobile frontend; simulated-device E2E.

Local checks at that implementation SHA:

- Native desktop/core/shared library tests: **829 passed, 1 ignored**.
- Vendored adb_client library tests: **123 passed, 1 ignored**.
- Native all-target Clippy with warnings denied, workspace/vendor formatting and
  diff checks passed. Real TCP fixtures cover cancellation, deadlines, stream
  handoff and fragmented transfer/error responses, including early upload failure.
- Production frontend build passed. Frontend source was unchanged from the prior
  Svelte check with **0 errors / 0 warnings** and UI regression run.
- Native macOS candidate build and strict complete-bundle ad-hoc signature check
  passed. Nonfatal local stripping/nice warnings occurred; build exit was zero.

Representative commands from `v2/` (use existing guarded build environment):

```sh
CARGO_BUILD_JOBS=1 cargo test -p shield-optimizer-v2 -p shield-direct-adb -p shield-optimizer-core --features shield-optimizer-v2/native-adb-default --lib -- --test-threads=1
CARGO_BUILD_JOBS=1 cargo test --manifest-path vendor/adb_client/Cargo.toml --lib -- --test-threads=1
CARGO_BUILD_JOBS=1 cargo clippy -p shield-optimizer-v2 -p shield-direct-adb -p shield-optimizer-core --features shield-optimizer-v2/native-adb-default --all-targets -- -D warnings
```

Hardware evidence is from **older 69e5f03**, not the staged candidate: two approved
Shields connected; 11 endpoint rows remained after 13 address discoveries (aliases,
not 13 TVs). Forget closed the selected pending socket while both ready TVs stayed
connected; rescan rediscovered AUTH. Overview, file browsing and a finite Logs
snapshot worked. Authentication took about 8.5 seconds on first launch and 2.02
seconds warm; full scans took 6.68–9.19 seconds. New streaming timings are unverified.
A denied system-file pull exposed the old parser writing the error message as file
contents. New parser fixtures pass, but corrected physical denial and successful
small download still need current-binary verification.

No completed physical TV approval, successful TLS PIN pairing, upload/install,
Remote or sleep/wake pass is claimed. Packaged Windows/Linux hardware checks also
remain outstanding. Keep owner-test readiness distinct from shipping certification.
No release signing/notarization is established by local ad-hoc signing.

## Exact local artifacts (not portable CI paths)

Local task evidence is under
`~/Developer/.agent-tasks/shield-native-test-20261031/`.
Use its `manifest.md`, `acceptance-status.md`, `early-upload-progress.md`,
`early-upload-*-receipt.json`, `candidate-current/app-receipt.json`, and
`read-only-hardware/receipt.json`. Private LAN screenshots/logs stay local.

| Artifact | Provenance and last observed state |
| --- | --- |
| `ATV Optimizer Native Discovery Test.app` | Old delivery at 69e5f03; last PID 2949; title ends Recovery Fix. Revalidate process before acting. |
| `candidate-current/ATV Optimizer Native Test.app` | Current candidate at 995fb2a; title ends Transfer & Scan Fix; built and signed, not yet launched. |
| `app-receipt.json` | Describes old delivery accurately; do not label it current until replacement succeeds. |

Candidate executable SHA256:
`5af25eb578d9fc0a122598db6b180d1eb1c99a605f0e4953d1e5832ac274a611`

Old delivery executable SHA256:
`1d5dd8e78e1cedd72da4af49843edb5b71604c4bcd56d0cd579f85c663cfecd7`

Both are native Rust development apps using `com.shieldoptimizer.native-test`.
A generated bundle also uses that ID, making bundle-ID selection ambiguous.

Existing worktree: `~/Developer/.worktrees/shield_optimizer/native-test-20261031`
(~36 MiB before this document). Existing mixed-ownership Cargo target:
`~/Developer/shield-issue205-20261019/v2/target` (~7.3 GiB). Do not broad-clean it.
The local guarded runner/config and exact build command are recorded in task
receipts; reuse them rather than creating another target or copying dependencies.

Own frontend dependencies/output were removed after packaging; no own compiler,
watcher or test server was left running. Old delivery and current candidate are
about 73 MiB each, retained for safe replacement and acceptance. Review retention
by 2026-11-07. This date is a review trigger, not deletion authorization. Capacity
was about 28 GiB free at last measurement; remeasure before growth. Keep compiler
jobs at one and preserve agent-build-guard. Retire only owned disposable artifacts
when their purpose ends, preserving source, receipts and useful evidence.

## Communication and completion

The owner requested a new agent because repeated blocked updates were not producing
an app they could test. Take ownership of the remaining acceptance, independently
reassess the next useful action, and state exactly which binary is running. Never
call this finished based solely on CI. Keep the current PR, preserve UI flows,
commit with the personal Gmail identity, and never amend/force-push. Only publish
scrubbed evidence; binaries, credentials and private LAN data do not belong in Git.
