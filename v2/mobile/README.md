# ATV Optimizer (mobile)

An Android phone/tablet app that drives an Android TV over **wireless ADB** — no PC in the loop.
Tauri 2 + Rust + Svelte 5, sharing the audited engine (`v2/crates/core`) with the desktop app.

**[`HANDOFF.md`](HANDOFF.md) is the authoritative document.** Read it before doing any work here;
this file is only an orientation. [`BACKLOG.md`](BACKLOG.md) is the ordered queue.

## How it talks to the TV

- **Transport is the vendored pure-Rust `adb_client`** (`v2/vendor/adb_client`, MIT), wrapped by
  `WirelessAdb` in `src-tauri/src/wireless_adb.rs`, which implements the shared `AdbDriver` trait.
  Shell, push/pull, screencap and pairing all run through it.
- **The Kotlin plugin (`tauri-plugin-atv-adb/`) is mDNS discovery only** — Android's `NsdManager`,
  no third-party native libraries.
- The GPLv3 `libadb-android` transport this app originally used was removed in `3e8bcfc`; it
  blocked selling a closed-source product and hung on older Shield adbd. See
  [`TRANSPORT-LICENSING-RESEARCH.md`](TRANSPORT-LICENSING-RESEARCH.md) for the decision record and
  [`HANDOFF.md`](HANDOFF.md) §3 for the current story.
- **Legacy network debugging (`:5555`, no code) is the proven path.** Android 11+ code pairing
  (SPAKE2) is implemented in the vendored crate but is **not yet verified against a real adbd** —
  see [`PAIRING-PLAN.md`](PAIRING-PLAN.md).

## Layout

- `src/` — Svelte 5 frontend: 14 screens in `screens/`, 8 components in `components/`, and the
  typed API / session / router / saved-device logic in `lib/`.
- `src-tauri/src/` — `lib.rs` (command registration), `wireless_adb.rs` (the transport),
  `wireless_commands.rs`, `file_commands.rs`.
- `tauri-plugin-atv-adb/` — the mDNS discovery plugin.

## Gates

```
npm run check     # svelte-check, must be 0 errors / 0 warnings
npm run build
npm test          # browser suite (99 tests) — real Svelte screens, Tauri invokes stubbed
```

CI runs all three on every push (`.github/workflows/v2-tests.yml`, `frontend-mobile` job).

## What is still unproven

Nothing in `v2/mobile` has been verified on a phone or TV since `44d2d66`. The browser suite
exercises the real screens against a mocked Tauri layer, which is worth a lot but is not a device.
The open physical gates are listed in [`BACKLOG.md`](BACKLOG.md); the largest are code pairing
against real adbd, the fast-remote device matrix ([`FAST-REMOTE-PLAN.md`](FAST-REMOTE-PLAN.md)),
and the app-restart behaviour Bryan reported ([`LIFECYCLE-EVIDENCE.md`](LIFECYCLE-EVIDENCE.md)).

Release and signing: [`RELEASE.md`](RELEASE.md). Licensing: [`LICENSING.md`](LICENSING.md).
