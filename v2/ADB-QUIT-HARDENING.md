# Quit hardening after PR168

The implementation authority is [shyney7's PR168](https://github.com/bryanroscoe/shield_optimizer/pull/168), merged as `40ba983b76e37891f439d5ce19c8364acdf53698`. Its default-off setting, shared snapshot leases, tracked local children and conservative quit path remain the foundation. There is no server ownership inference. A live Remote session still causes quit to leave the server running; this work does not force session teardown.

## Focused regression scope

Installation must hold the existing AppState operation counter before any installer effects, through download, extraction and driver replacement. A canceled async caller must not release a still-running blocking extraction's lease. Closed admission must reject a new install before the injected installer runs. Tests use a bounded worker and a nonexistent replacement binary, never Google's download endpoint or the user's platform-tools directory.

Additional fixtures cover a direct client exiting while a descendant retains its output pipes, and old/replacement drivers retaining the same process-admission boundary. The pipe test tracks the direct client only; it does not claim to own or reap grandchildren. A mock quit test verifies that a snapshot of the old driver holds up drain and that the eventual stop is delegated only to the replacement mock driver.

See [the validation evidence](evidence/quit-install-admission-20261017/README.md). No UI, copy or captured screen changes, so screenshots and video are not applicable. Existing simulated preview video is historical evidence of a different proposal, not evidence of current production behavior.

## Preserved historical work

These draft PRs and their branches remain intact pending independent review of this replacement. Their controllers, policies and development app must not be merged as a stack onto PR168.

| Draft | Preserved value | Disposition after independent review |
| --- | --- | --- |
| [174](https://github.com/bryanroscoe/shield_optimizer/pull/174), head `2eb5fef` | Earlier unwired lifecycle reasoning | Superseded by PR168's production implementation |
| [175](https://github.com/bryanroscoe/shield_optimizer/pull/175), head `e5484d9` | Installer, inherited-pipe, driver replacement and process/IPC regression ideas; [audit](https://github.com/bryanroscoe/shield_optimizer/blob/e5484d9e2eb3095f0c1926d2c921f8e2b94b5763/v2/ADB-PROCESS-LIFETIMES.md) | Extract selected cases; retire competing controller/factory and broad wrappers |
| [176](https://github.com/bryanroscoe/shield_optimizer/pull/176), head `4df768c` | Installer admission-gap regression; [simulated settings video and provenance](https://github.com/bryanroscoe/shield_optimizer/tree/4df768ce1c888e6e39d066d98c9b5ac48fbf54f0/v2/evidence/exit-preview-20261016) | Preserve evidence; three-choice policy/first-quit prompt remain a separate proposal |
| [177](https://github.com/bryanroscoe/shield_optimizer/pull/177), head `10497a0` | [Cancellation/updater contract research](https://github.com/bryanroscoe/shield_optimizer/blob/10497a0cece9b884149e076a8a7621b22bdb3012/v2/EXIT-COORDINATOR-CONTRACT.md) | Historical alternative, not a second coordinator to adopt |
| [178](https://github.com/bryanroscoe/shield_optimizer/pull/178), head `af3afc9` | [Pinned framework and native storage notes](https://github.com/bryanroscoe/shield_optimizer/blob/af3afc9aa71a05dce64d9e03a95d2890483c5ef8/v2/NATIVE-EXIT-ADAPTER-GATES.md) | Preserve research; no native preference migration or new exit executor |
| [179](https://github.com/bryanroscoe/shield_optimizer/pull/179), head `1b0c555` | [Development event/mailbox experiments and limitations](https://github.com/bryanroscoe/shield_optimizer/blob/1b0c555908a5ed857232ed8ea3e9149a277b14af/v2/WIRED-NATIVE-EXIT-DEVELOPMENT.md) | Retire alternate entry point after preservation review |

## Framework boundaries checked against pinned source

Cargo.lock pins Tauri and tauri-runtime-wry 2.11.2, updater plugin 2.10.1 and process plugin 2.3.1. The cached registry sources were inspected for these versions:

- `tauri-2.11.2/src/app.rs`: `ExitRequestApi::prevent_exit` ignores restart requests. `AppHandle::exit` falls back to cleanup and `std::process::exit` if runtime delivery fails. `cleanup_before_exit` requires immediate exit without further Tauri API calls.
- `tauri-plugin-updater-2.10.1/src/updater.rs`: Windows installation calls its synchronous `on_before_exit` hook and then exits directly. An ordinary asynchronous exit listener cannot be assumed to run for that path.

These boundaries justify keeping updater integration separate. This patch does not replace updater hooks, intercept process exit, claim native-window acceptance, or infer a token from the next exit event. It does not alter PR168's restart handling. Physical macOS/Windows/Linux shared-client and updater acceptance remain unperformed here.

## Mobile boundary

[Mobile PR163](https://github.com/bryanroscoe/shield_optimizer/pull/163) at `60a1934c5e4c575a81bd124698b9a9f6203e5fe8` diverged from merged main (56 commits ahead, 16 behind when inventoried). Integrating main into that branch is a separate task. This follow-on adds only a shared operation-lease API and desktop installation/tests; it does not add mobile quit policy, change Android transport behavior, or port the historical broad command-wrapper rewrite.
