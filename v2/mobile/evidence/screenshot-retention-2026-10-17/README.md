# Screenshot retention — 2026-10-17

Based on `mobile-parity-pass` `dccfe99a805ffcb0c11f32a4fa9f227306ba1cf3`, independently of header PR191 and RAM PR192.

The previous shared `take_screenshot` generated a filename at second resolution and used `tokio::fs::write`, so a second capture for the same serial/time replaced the earlier PNG. It now stages complete bytes in the same directory, then uses `NamedTempFile::persist_noclobber` to atomically claim the original filename or bounded numeric suffixes (2) through (1000). A racing capture retries the next candidate. There is no preflight existence-check race. The existing `tempfile` dependency moves from dev-only to production; mobile already uses the same facility for downloaded files.

The directory, first filename format, PNG validation, and `{ path, base64 }` response remain unchanged. Returned path names the actual published file; base64 still encodes the captured bytes. Existing captures are neither deleted nor migrated. Partial write, staging/publication errors and name exhaustion return errors before any success response; temporary-file ownership cleans up failed staging. Crash durability and OS filesystem failure behavior are not broader guarantees of this change.

Deterministic tests in `crates/core/src/commands/screenshot.rs` cover sequential identical timestamps, twelve concurrent fixed-time saves beside an existing capture, distinct returned paths and exact bytes, injected failure after a partial write, all 1000 names occupied, a missing destination directory and (on Unix) an existing symlink whose target must remain untouched. Existing PNG rejection tests remain intact.

Focused screenshot tests: 10/10. Core fmt, all-target Clippy with warnings denied, and 465 core tests passed on macOS. Host desktop tests passed 83 with one pre-existing ignored test; mobile Rust tests passed 26. Both host crates passed all-target Clippy with warnings denied. Full browser-suite results are reported in the handoff. Existing browser export tests cover truthful paths/base64, Close retention and failure without a new success receipt, at 384px and 320px/200% simulated text.

This is filesystem behavior with no visible UI change, so new UI screenshots/video would not demonstrate the fix. See the existing [simulated export UI evidence](../export-discoverability-2026-10-17/README.md) for the unchanged contract. No phone/TV query, Android build/install, storage permission, sharing, security setting or credential operation was performed. macOS host tests do not establish Windows/Linux/Android filesystem coverage.
