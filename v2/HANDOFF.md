# v2 — current state

v2 is a shipping desktop app. Current version: **2.0.0-beta.11**.

58 Tauri commands registered (`v2/src-tauri/src/lib.rs`). Full release pipeline live: installers built for macOS/Linux/Windows on every `v2-*` tag push via `.github/workflows/v2-release.yml`; macOS also distributed via Homebrew tap (`bryanroscoe/homebrew-shield-optimizer`).

## Roadmap

Feature parity gaps against aTV Tools — see **[`v2/ATVTOOLS-PARITY.md`](ATVTOOLS-PARITY.md)** for the current comparison table and prioritized plan.

## Known deferred items

- **Remote-control latency** — investigated and planned: see [`REMOTE-LATENCY-PLAN.md`](REMOTE-LATENCY-PLAN.md) (scrcpy-server control channel, with a benchmark gate). Current `send_text` / `send_key` commands cover the common case.
- **Memory-usage spike (user report)** — likely root-caused to a tab lazy-load re-fetch loop (fixed in PR #62); confirm it's gone on the next release build.

## Invariants + release process

See **`CLAUDE.md`** (or `AGENTS.md` for agents) at the repo root — architecture invariants, safety-gate rules, release script usage, and MSI versioning notes are all there.
