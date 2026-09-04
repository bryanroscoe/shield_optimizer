# Cloud agent task — continue the ATV Optimizer mobile app

You are a scheduled cloud coding agent with **no physical device**. Never run `adb`, install an APK,
or claim on-device verification. Work only on tasks that can be proven by builds and automated tests.

## Current status (updated 2026-09-04)

Work on branch **`feat/atv-optimizer-mobile`**. The expected handoff head includes the 2026-09-04
"stability reset" commits (bounded transport reads, batched core reads, saved-TV picker, global
connection banner, Emergency recovery, one safety vocabulary).
Read these files before choosing work:

1. `v2/mobile/HANDOFF.md` — authoritative architecture and operations handoff.
2. `v2/mobile/BACKLOG.md` — current ordered queue.
3. `v2/mobile/FAST-REMOTE-PLAN.md` — exact physical evidence and remaining Phase 4 gates.

Do not redo completed work: the pure-Rust `adb_client` transport, all 14 screens, Optimize apply,
shell-v2 results, complete split-APK backup/restore, saved-device/state isolation, fast-remote Phases
1–3, Android background lifecycle code, connection-prompt guidance, previous-TV switcher, durable
cached TV names, and used-RAM visualization are implemented and pushed.

The 2026-09-04 sweep was exercised at 384×812 with Playwright and Tauri invoke stubs (scan, saved-TV
picker and auto-dial rules, connect failure guidance, lost-connection recovery, Emergency recovery,
back stack). The physical APK predates it; only a local/on-device agent can close that spot-check.
Likewise, the remaining fast-remote matrix is physical work and must not be marked done by a cloud
agent.

## Work queue

Take the first task that is both still open in `BACKLOG.md` and device-less:

1. Add code-only lifecycle/concurrency regression coverage that materially reduces risk in the
   remaining fast-remote Phase 4 paths. Do not substitute it for the physical matrix.
2. Code pairing (SPAKE2) is implemented but unverified on a device; do not rewrite it. Only add
   host-only tests or fix defects you can prove against `PAIRING-PLAN.md`'s cited AOSP sources.
3. Add Android SAF import/export and user-selected push destinations for the complete,
   manifest-backed APK bundles; Drive sync comes afterward.
4. Continue release-readiness work that is device-less: accessibility/navigation review, a signed
   AAB CI job (see `RELEASE.md`), or the product decisions listed in `BACKLOG.md`.

Do not touch desktop branding or release identity. Desktop rebranding is separate because changing
the Windows product identity can orphan installed MSI packages.

## Architecture rules

- Keep `v2/crates/core/src/engine/` pure.
- Keep all mobile ADB behavior behind the existing `WirelessAdb`/`AdbDriver` seam.
- Use the canonical safety classifier for every destructive path; never invent safety in the UI.
- App package lists belong in JSON, not Rust.
- Tauri commands return `Result<T, String>`.
- Frontend code uses Svelte 5 runes and never fabricates device data.
- Preserve unrelated untracked root files; they are user-owned.

## Validation

Before committing, run from `v2/`:

```sh
cargo fmt --check
cargo clippy -p shield-optimizer-core -p shield-optimizer-v2 -p atv-optimizer-mobile -p tauri-plugin-atv-adb --all-targets -- -D warnings
cargo test -p shield-optimizer-core
```

From `v2/mobile/`:

```sh
npm run check
npm run build
```

Run focused tests for every crate or module changed. Do not run `tauri android build` unless the
cloud environment is explicitly provisioned for it, and never interpret a cross-compile as a
physical device result.

## Commit and handoff

Create logical commits with no `Co-Authored-By` trailers and push
`feat/atv-optimizer-mobile`. Append a dated summary to `HANDOFF.md` stating what changed, exact
validation results, and what still requires a phone/TV. Never mark a physical gate complete from
reasoning, mocks, or host-only tests.
