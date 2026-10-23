# aTV Tools parity

Comparison of the ATV Optimizer desktop app against **aTV Tools** (`dev.vodik7.atvtools` by vodik7), the most popular ADB toolbox for Android TV, and the gaps that remain.

aTV Tools is a **phone/tablet companion app** (Android 8.1+/iOS 14+) controlling the TV over network ADB; freemium (~$2 lifetime Pro unlocks app management, remote, recording, gamepad). We're a free, open-source **desktop** app over the same transport — so most of its features are reachable for us, and some of its phone-centric ones (gamepad, mirroring) don't make sense here.

## Feature comparison

| Capability | aTV Tools | ATV Optimizer (desktop) |
|---|---|---|
| Curated debloat catalog + safety list | ❌ | ✅ (core strength) |
| Optimize/Restore wizard with per-app defaults | ❌ | ✅ |
| Snapshots (state backup → restore/clone across devices) | ❌ | ✅ |
| Launcher wizard (install/set/disable stock) | ❌ | ✅ |
| Dedicated tweaks UI (CEC, frame-rate, animations, scaling) | ❌ (raw shell only) | ✅ |
| Network scan / auto-discovery | partial (manual IP) | ✅ |
| Disable/enable/uninstall apps | ✅ (Pro) | ✅ |
| Sideload APK | ✅ | ✅ |
| **APK backup/extract (device → local file)** | ✅ | ✅ |
| **Install extracted APK to another device (app cloning)** | manual two-step | ✅ |
| **File manager (browse / push / pull)** | ✅ | ✅ |
| **Screenshots of the TV** | ✅ | ✅ |
| Screen recording | ✅ (Pro, no DRM content) | ❌ |
| Remote control / D-pad / mouse | ✅ (Pro) | partial (D-pad + typing, no mouse) |
| **Send text to TV (type from keyboard)** | ✅ | ✅ |
| Permissions grant/revoke | ✅ | ✅ |
| Bulk cache clear | ✅ | ✅ |
| Running apps + force-stop | ✅ | ✅ |
| Resource monitor | CPU/RAM/net/storage | ✅ CPU/RAM/net/temp/storage/display |
| Shell runner with bookmarks | ✅ | ✅ |
| Screen mirroring / gamepad / media remote | ✅ (phone-centric) | — (out of scope for desktop) |
| Open source / free | ❌ | ✅ |

**Playback report** (Playback tab) shows codec configuration, reported HDR/display modes, and audio policy. These observations do not establish runtime decoder support, acceleration, or playback quality.

**Bottom line:** ATV Optimizer leads on the *debloat/optimize/safety* core and now covers the general device utilities that complement it (APK backup and cloning, file manager, screenshots, force-stop, text entry, shell runner with bookmarks, cache trim, CPU/network sampling, permissions, low-latency remote). The desktop Shell tab is an explicit expert opt-in: its lexical gate catches obvious protected-package commands but shell expressions can bypass it, so it never carries the curated actions' safety guarantee. Mobile does not expose a shell.

## Remaining gaps

- **Screen recording** — `screenrecord` (3-minute cap, no DRM content), pull and save. Not built.
- **Mouse mode** on the Remote tab. The Remote has D-pad, hold-to-repeat and typing/paste over the scrcpy control channel ([`REMOTE-LATENCY-PLAN.md`](REMOTE-LATENCY-PLAN.md)), but no pointer.
- **Out of scope:** screen mirroring, gamepad and media remote — phone-form-factor features.

## Idea not yet built

**Snapshot ↔ Optimize convergence.** The snapshot preview is a table. A possible next step is to render it as an *editable* optimize-style plan (per-row apply/skip) so apply-snapshot and Optimize share one mental model, and potentially one component.
