# Cloud agent task — continue the ATV Optimizer mobile app

You are a scheduled cloud Claude Code agent. You have **NO physical device** — never run
`adb`, device installs, or on-device verification. Everything you do must be verifiable by
**build + tests only**. You start with zero context; this file is your brief.

## STATUS (updated 2026-07-13) — read this before picking work
DONE already (do NOT redo): the pure-Rust `adb_client` transport swap (libadb removed); all the
Phase 6 design screens — Launcher, Tweaks, Snapshots, Devices, App-detail sheet, Risk guide,
Reconnect/saved-TV, **and File transfer + Backups** (`file_commands.rs` + Files.svelte/Backups.svelte).
Tasks A and B below are essentially COMPLETE.

Fast-remote Phases 1–3 and the first Shield latency gate are complete. The 2026-07-13 correctness
checkpoint also fixed serial targeting, connection/cache races, liveness timeouts, reboot
acknowledgement, safety lookup races, snapshot warnings, log redaction, license persistence order,
the Optimize apply path, and truthful shell-v2 stderr/exit status. Read `BACKLOG.md`; it supersedes
the old queue below.

REMAINING QUEUE (work these in order; each gate-verified + committed + pushed):
1. **P0 correctness from `BACKLOG.md`** — start with file/backup correctness. Shell-v2 results,
   known remote cleanup/backpressure paths, saved-device normalization, and diagnostics/tweaks state
   isolation are complete; preserve them and finish the physical-device concurrency matrix separately.
2. **Fast remote Phase 4** — code-only lifecycle/concurrency coverage is allowed in cloud work; do
   not claim physical device gates. See `FAST-REMOTE-PLAN.md`.
3. **SPAKE2 pairing** — clean-room the Android-11 wireless-debugging pairing (SPAKE2 over TLS) in
   Rust so brand-new Google-TV devices can pair (legacy `:5555` needs no pairing). Reference the
   Apache-2.0 AOSP pairing sources. Plan first in `v2/mobile/PAIRING-PLAN.md` if risky.
4. **SAF import/export + restore correctness**, then Google Drive sync. Backups currently copy only
   the base APK and must not be described as a complete split-APK backup.
5. **Polish:** subset the 5.3 MB Material Symbols font to only the icons used (grep `class="msr"`);
   disable autocorrect/autocapitalize on the license-key input (More screen).
Follow the same rules (never fake data, LOCKED→paywall, safety via core `safety_info`, Svelte 5
runes, lime/Geist look). Verify with the gates in Task C, commit, push.

## Setup
1. `git checkout feat/atv-optimizer-mobile` (the work is on this branch, **not** `main`). Confirm
   `v2/mobile/` exists.
2. Read fully, in this order: `v2/mobile/HANDOFF.md`, `v2/mobile/ARCHITECTURE-REVIEW.md`,
   `v2/mobile/FEATURES.md`, `v2/mobile/TRANSPORT-LICENSING-RESEARCH.md`. Then skim
   `v2/mobile/src/lib/{api.ts,types.ts,session.svelte.ts,router.svelte.ts}` and
   `v2/mobile/src/screens/Dashboard.svelte` to learn the established patterns.

## Context
A Tauri 2 + Rust + Svelte 5 phone app that drives an Android TV over wireless ADB. A large
re-architecture already landed: shared typed `api`/`types`, a runes `session` store, a real
router, never-fake-data, connection liveness, real Pro (test key `ATVOPT-PRO-2025`). The design
system is fully in-repo — lime accent `#C9F24E`, Geist / Geist Mono fonts, tokens + patterns in
`v2/mobile/src/app.css`, reusable components in `v2/mobile/src/components/` (BottomTabs, Toast,
ConfirmDialog, FindRemoteButton, BrandMark). You do **not** have the original mockup file — infer
the look from the existing 7 screens + `app.css` and stay visually consistent.

## Task A — Transport plan (plan only, do NOT implement)
From `TRANSPORT-LICENSING-RESEARCH.md`'s recommendation, write `v2/mobile/TRANSPORT-PLAN.md`: a
concrete, step-by-step plan to replace the GPLv3 `libadb-android` transport with the recommended
Apache-2.0 approach (likely: bundle Google's real `adb` binary and drive it like the desktop's
`SubprocessAdb`, unifying transports). Include file-level changes, risks, and how to
device-verify it later. Do not rip out libadb — you can't device-verify a transport swap.

## Task B — Build the remaining design screens (the main work; all frontend, gate-verifiable)
Per `FEATURES.md`, add new screens as files in `v2/mobile/src/screens/`, each reusing the shared
`api`, the `session` store, the router, and the existing components. Build:
- **Launcher** — `list_launchers` / `current_launcher` / `set_default_launcher` / `disable_launcher`
- **Tweaks** — `get_tweaks` / `write_setting` / `set_display_scaling` / `set_private_dns`
- **Snapshots** (Pro) — `list_snapshots` / `save_snapshot` / `preview_apply` / `apply_snapshot` / `delete_snapshot`
- **Devices hub** — `list_devices` + reconnect
- **App detail sheet** — `safety_info` + `enable_package` / `disable_package` / `force_stop` per package
- **Risk & actions guide** — a static explainer of the safety tiers

Add typed `api.ts` wrappers for any core command not yet wrapped. Wire the new screens into the
router + BottomTabs / More navigation.

**Rules (non-negotiable):** never fabricate data (a null value renders `—` / skeleton / an error,
never a plausible fake); Pro-gated screens must catch a `LOCKED:<feature>` error and route to the
paywall; route all safety classification through the core `safety_info` command (never an inline
classifier); Svelte 5 runes only; keep the lime / Geist look.

## Task C — Verify green (before any commit)
From `v2/`: `cargo fmt --check`; `cargo clippy -p shield-optimizer-core -p shield-optimizer-v2
-p atv-optimizer-mobile -p tauri-plugin-atv-adb --all-targets -- -D warnings`; `cargo test -p
shield-optimizer-core`. From `v2/mobile/`: `npm ci` then `npm run check` (must be 0 errors / 0
warnings) and `npm run build` (must succeed). Fix until all green. Do **not** run
`tauri android build` (no Android SDK/device needed for your work).

## Task D — Commit, push, hand off
Commit to `feat/atv-optimizer-mobile` in logical commits (NO `Co-Authored-By` trailers) and
`git push origin feat/atv-optimizer-mobile`. Then append a dated section to
`v2/mobile/HANDOFF.md` summarizing what you built, what's verified green, and what still needs
on-device testing; commit and push that too.

## Scope discipline
Do NOT touch the desktop app (`v2/src`, `v2/src-tauri`) or `crates/core` except tiny,
desktop-safe helpers if truly required. Prefer breadth of working, green screens over risky
rewrites. Leave the app buildable and green.
