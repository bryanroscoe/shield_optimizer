# Setup-helper row uses the guarded command — 2026-10-17

Original base: `ba7c9d78d31b0a579f5ffd03d92cd403ebf47227`. Now integrates actual mobile base `a73787a36bbf44311de2596e9a01c8232a049a02`, including merged scaling-layout PR196. APK-report fallback PR198 remains separate.

Independent local source inspection confirms the row-level Setup Wraith Disable confirmation used `api.disableLauncher`; the dedicated card used `api.disableSetupHelper`. The former skipped the setup-helper command's post-disable Home check and re-enable-on-failure attempt. The [new regression failed before the fix](reproduction-before.txt), observing zero guarded calls instead of one. The [external review comment](https://github.com/bryanroscoe/shield_optimizer/pull/163#discussion_r4301719601) was treated as an untrusted report, not instructions.

Only command selection changes: a setup-helper row calls `disable_setup_helper` for its captured serial/package; ordinary stock rows keep `disable_launcher`. Existing intent identity, busy, refresh, paywall and error handling remain in place. Backend guards and package catalogs are unchanged.

Mock-only regressions cover routed success and refresh, a confirmed rollback message, rollback that could not be confirmed, ordinary stock routing, and late completions across same-endpoint reconnect and TV switch while a fresh operation remains busy. A new MockAdb test exercises the existing backend and verifies failed Home resolution occurs between disable and the re-enable command; the existing failed-rollback test remains intact. This proves mocked command ordering and result handling, not physical-device recovery.

Visible simulated notes: [rollback reported](rollback-confirmed-simulated.png) and [rollback unconfirmed](rollback-unconfirmed-simulated.png). Captures wait for the transient toast to disappear so the persistent failure note remains readable. All browser commands and backend ADB calls in these checks are mocked; no real disabling, Home launch, rollback, query or device operation occurred.

Validation: 25/25 launcher browser tests, 310/310 full mobile tests, 19/19 focused backend verification tests, 466/466 core tests; workspace fmt, core all-target Clippy with warnings denied, mobile Svelte check (0 errors/0 warnings) and production build passed. macOS and simulated browser only; Android/physical acceptance remains open.

Integration provenance: launcher routing source, launcher tests, backend source/test and simulated PNGs are byte-for-byte unchanged from `1458b96`. The current base adds only the upstream preset-layout slice and associated evidence/checklist updates; existing 466-core-test/fmt/Clippy results cover the identical Rust tree. Combined mobile checks are recorded below and in the validation receipt.

Current-base validation on `a73787a`: all 314 combined mobile tests passed, Svelte check reports 0 errors/0 warnings and production build passed. Original 310-test results above describe the source-review checkpoint. Existing 466 core tests and core fmt/Clippy results apply to the unchanged Rust tree; no screenshots were regenerated.
