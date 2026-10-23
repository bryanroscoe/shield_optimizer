# Mobile integration of reviewed PR206

Mobile base: `d388cad1e3da9c17e0a5c0bdeec52022fd8d9970`. Incoming main: `c78176d5ea640a0e230633a3f0b0bc0720c01af4`, the squash merge of reviewed PR206 head `ef72305198af19d7457b97e02cd71cb00b00f7f5`. A normal merge applied without conflicts, preserving both branch histories. This is an integration draft, not a claim that the default mobile branch has merged it.

## Exact scope and preservation

Shared launcher production code is byte-identical to reviewed main. Its only whole-file difference from main is mobile's preexisting setup-helper rollback test. Mobile `src`, `src-tauri` and `tauri-plugin-atv-adb` trees are unchanged from the mobile base. Existing desktop/shared differences on mobile are retained; this integration does not overwrite them with main's versions. Imported PR206 test and evidence files are byte-identical to main.

The README additionally replaces the absolute assertion that missing devices must be a discovery/pairing issue: firmware must expose usable ADB, authorize the connection and provide network reachability; version alone establishes no compatibility guarantee. No workaround, settings change, new safety classification or Fire TV support removal is introduced.

Release documentation records merged PR204, its [final independent review](https://github.com/bryanroscoe/shield_optimizer/pull/204#issuecomment-6235587609) and [six passing exact-head CI jobs](https://github.com/bryanroscoe/shield_optimizer/actions/runs/39043175482). PR204's original local evidence body is retained, with its pending sentence explicitly dated to that original run and a subsequent completion receipt appended. Raw receipts, 343-test provenance and screenshots/video remain unchanged. PR202/203 completion and physical release gates remain intact.

The parent handoff confirms independent source approval of PR206's exact head; [its six CI jobs passed](https://github.com/bryanroscoe/shield_optimizer/actions/runs/39185141432), and [PR206](https://github.com/bryanroscoe/shield_optimizer/pull/206) merged to main as above. Its public automated review summary still said running when inspected, so that summary is not claimed complete. Those receipts establish PR206's source branch status, not validation of this new integration head.

## Combined local validation

Workspace formatting check and all-target Clippy with warnings denied pass (`clippy.txt`). Complete serial workspace tests pass (`workspace-tests.txt`): core 493, desktop 109 with one preexisting live test ignored, mobile Rust 26, license 2 and registry 9; doc tests pass. This includes the imported explicit-protection/unrelated-error/rollback/Home tests and existing mobile helper coverage. Test counts reflect this combined mobile tree, rather than overwriting PR206's historical counts.

The device-free message regression passes on the combined tree (`message-test.txt`), using the unchanged serialized mock result, production wrapping rule and existing cached Playwright 1.62.1 / Chromium headless shell 1223. No device command is dispatched.

Rust ran on macOS with existing offline dependencies, two jobs, incremental off, and dev/test debug info off, reusing this task's previous target cache. Local frontend type checks/builds, full browser/E2E suites and Android APK compilation were not run: this isolated checkout has no installed frontend dependencies, and the previously available Vite/Svelte cache remains offloaded (`compressed,dataless`). No installation or hydration workaround was attempted. Fresh exact-head CI is required to establish frontend/cross-platform success for this integration; prior PR204/206 CI is not substituted for it. Independent review and the new draft's checks remain the merge gate.

## Nonvisual integration rationale

No new UI markup or styles changed relative to reviewed PR206, and the mobile frontend is untouched. The shared message is already covered by [PR206's labeled simulated screenshots and short video](../../../evidence/firetv-protection-205-20261019/README.md); these are preserved as original message-fixture evidence, not recaptured or claimed as native/app acceptance. The only additional user-facing content is README text. New screenshots/video and gallery regeneration would add no new integration evidence, so they are not generated.

Reporter Fire OS/build diagnostics, real protected-package behavior, Android scaling/touch/lifecycle, native export, shared-client/updater, signing/store/release and other physical acceptance gates remain open. No device queries/mutations, installs, security changes, paid calls, releases or merge to a shared branch occurred.
