# v2 — current state

v2 is a shipping desktop app. Last **published** version: **2.1.0** (2026-06-22).

Full release pipeline live: installers built for macOS/Linux/Windows on every `v2-*` tag push via `.github/workflows/v2-release.yml`; macOS also distributed via Homebrew tap (`bryanroscoe/homebrew-shield-optimizer`).

## Unreleased on `main`

Everything past the `v2-2.1.0` tag is unreleased. `v2/CHANGELOG.md` has a drafted
`v2-2.2.0` section covering it; `docs/RELEASE-DECISION-2026-09-09.md` has the
step-by-step physical device script, which is the remaining gate.

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
