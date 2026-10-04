# v2 — current state

v2 is a shipping desktop app. Last **published** version: **2.3.0** (2026-09-30).

Full release pipeline live: installers built for macOS/Linux/Windows on every `v2-*` tag push via `.github/workflows/v2-release.yml`; macOS also distributed via Homebrew tap (`bryanroscoe/homebrew-shield-optimizer`).

## Unreleased on `main`

`v2-2.3.0` shipped 2026-09-30 (the ATV Optimizer rename plus #88, #119–#124).
`v2-2.3.1` is prepared in `v2/CHANGELOG.md` (Setup Wraith fixes #122/#157/#158, #97,
#98, #100, #129 and the rest listed there) and waits on the owner's go to tag.
Things the next reader needs that the code does not say:

- **The app is now ATV Optimizer.** `identifier`, the `ShieldOptimizer` data dir and
  the updater channel are unchanged on purpose; the WiX UpgradeCode is pinned in
  `tauri.conf.json` to the value the old name derived. The Homebrew tap change
  (`homebrew-shield-optimizer#2`) merged with the `v2-2.3.0` release; its `url` names
  the renamed DMG.
- **Launchers are data**: `crates/core/data/app-lists/launchers.json`, parsed once in
  `loader.rs`. The Launcher list shows HOME handlers only. An earlier `LEANBACK_LAUNCHER`
  union listed every TV app as a Home app (every TV launch activity carries that
  category) and was removed; `launcher-rows` and a Rust test guard it. The
  last-HOME-handler guard counts HOME handlers alone.
- **Stock takeover is its own step.** `set_home_any` (Launcher › Advanced) enables a
  package and asks Android to make it Home, then reports whether Android accepted it.
  It never disables stock. Disabling stock is the separate `disable_stock_launcher`
  command behind its own confirmed button. Both stay gated by
  `is_last_enabled_home_handler`, so the TV always keeps a Home screen.
- **Snapshot semantics.** `engine/snapshot.rs` receives the current launcher in
  `ApplyPlanInputs` and sets `launcher_to_set` only when the snapshot's launcher
  differs from it; `apply_snapshot` skips the launcher ladder entirely when it is
  `None`. Settings already at the snapshot's value are carried as "already set", so the
  preview's Now column is real. A restore disables the recorded apps and writes the
  settings back; it never re-enables or reinstalls anything.
- **One label source, one recommendation source.** `src/lib/safety.ts` owns every
  verdict label (Protected, Caution, Safe to remove, Unknown, Checking…, Safety
  unavailable). `src/lib/recommendation.ts` owns `effectiveMethod`, `recommendation`,
  `canOfferUninstall` and the review labels; the App List, Optimize defaults and
  Health's Suggestion column all call it. Uninstall is recommended only when the app is
  reinstallable (`play_store || defunct`), and never labelled "Remove". Don't add a
  mapping inline in a component; `recommendation-labels` checks the screens agree.
- **Demo catalog**: `src/lib/demo-apps.json` is `common.json` followed by `shield.json`,
  copied verbatim. Regenerate it whenever those change, or the demo drifts from the
  product.
- **Logs**: `<data_dir>/logs/`, daily rotation, 7 kept. Debug level is per-crate, not
  global, so adb lines are not buried under hyper. `collect_diagnostics` formats in
  the engine (pure) and reads in `src-tauri/src/commands/diagnostics.rs`.
- **TV evidence** is a three-way value (`tv | not_tv | unknown`); only `not_tv` blocks a
  row. Do not re-introduce "unknown means phone" — that was #120.
- **Not addressed here**: the mobile identity bugs #115–#118 (same rule family, other
  app).

The 2.2.0 device-verification notes below still apply to the fixes that shipped in it.

The five user-reported issues are all addressed here:

| Issue | State |
|---|---|
| [#86](https://github.com/bryanroscoe/shield_optimizer/issues/86) SmartTube backups | Fixed. Catalog searches `Documents/SmartTubeBackup`; a failed search no longer reads as "no matches". Covered by `npm run test:app-files-catalog`. |
| [#87](https://github.com/bryanroscoe/shield_optimizer/issues/87) Sony launcher default | Root-caused. On Android 8 the stock fast path registered nothing before disabling stock, because `cmd package query-activities` (9+) and `cmd role` (10+) do not exist there. Both paths now share one setter ladder. Needs the reporter to confirm. |
| [#88](https://github.com/bryanroscoe/shield_optimizer/issues/88) TCL Android 14 not found | Fixed in two parts: pairing stopped guessing `:5555`, and Scan Network now reads `adb mdns services` so a random wireless-debugging port is discoverable at all. There is no Android version gate in this codebase. |
| [#89](https://github.com/bryanroscoe/shield_optimizer/issues/89) macOS volume prompts | Root cause identified: adb subprocesses inherited the launch cwd, and the adb daemon outlives the app, so a DMG launch left a process pinning `/Volumes/...`. Every spawn is now pinned to a stable directory. Only a physical macOS run can confirm. |
| [#91](https://github.com/bryanroscoe/shield_optimizer/issues/91) Remote clipboard paste | Fixed in `2d571fd`. A focused non-editable div does receive paste with `clipboardData` (checked in Chromium and WebKit, the latter being macOS's WKWebView), so an `onpaste` handler plus a Paste button was enough — no clipboard plugin. The only one of the five confirmed working by a human. |

The first four have migrated duplicates (`#107`, `#112`, `#94`, `#92`) — close one side of each pair. #91's duplicate is `#108`.

## Roadmap

ATV Optimizer Android app plan: see **[`ATV-OPTIMIZER-ANDROID-PLAN.md`](ATV-OPTIMIZER-ANDROID-PLAN.md)**.

Feature parity gaps against aTV Tools — see **[`v2/ATVTOOLS-PARITY.md`](ATVTOOLS-PARITY.md)** for the current comparison table and prioritized plan.

## Known deferred items

- **Mobile Android scaffold** — initial ATV Optimizer mobile app lives in [`mobile/`](mobile/). It builds an unsigned aarch64 APK and still needs real phone/TV validation for mDNS, pairing, reconnect, shell, and screencap.
- **Memory-usage spike (user report)** — likely root-caused to a tab lazy-load re-fetch loop (fixed in PR #62); confirm it's gone on the next release build. Unrelated to the Health-tab verdict stall fixed in `73d481d`.
- **Screen recording** — the one aTV Tools capability still absent; see [`ATVTOOLS-PARITY.md`](ATVTOOLS-PARITY.md).
- **Code signing** — builds are unsigned on macOS and Windows. Setup notes are at the top of `.github/workflows/v2-release.yml`.

Done since this list was last written, kept here only because other docs still point at them as pending: remote-control latency (the scrcpy control channel shipped — `crates/core/src/adb/remote_input.rs`, server jar bundled at `v2/src-tauri/resources/scrcpy-server-v3.1`) and Remote clipboard paste (#91, shipped in `2d571fd` with the `test:remote-paste` harness).

## Invariants + release process

See **`CLAUDE.md`** (or `AGENTS.md` for agents) at the repo root — architecture invariants, safety-gate rules, release script usage, and MSI versioning notes are all there.
