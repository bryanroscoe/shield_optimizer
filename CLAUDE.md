# Shield Optimizer — repo notes for agents

Two products live in this tree:

- **v1** — PowerShell debloater (`Shield-Optimizer.ps1` at the root). Released on `v0.x.x` tags via `.github/workflows/release.yml`. Still maintained.
- **v2** — Tauri 2 + Rust + Svelte 5 desktop app (`v2/`). Released on `v2-*` tags via `.github/workflows/v2-release.yml`.

The two release tracks are intentionally separate. Don't mix tag namespaces and don't edit the v2 workflow when shipping v1 (or vice versa).

Before touching v2, skim `v2/HANDOFF.md` — it carries the current roadmap, the priority list, and any in-flight scope that isn't yet in the code.

## v2 architecture invariants

These are load-bearing — break them and the safety story falls over.

- **`v2/src-tauri/src/engine/` is pure.** No I/O, no ADB calls, no filesystem. Pure functions and pure types only. All I/O lives in `commands/` and `adb/`. The engine is the audited safety layer; keeping it pure is what makes the unit tests trustworthy.
- **One ADB wrapper.** Everything subprocess-y goes through `SubprocessAdb` in `adb/driver.rs`. Don't add a second wrapper — extend the `AdbDriver` trait if you need a new capability. The active driver lives behind `RwLock<Arc<dyn AdbDriver>>` so `install_adb` can hot-swap it without a restart.
- **One detection function.** Device profiling has a single canonical implementation. Don't fork.
- **App lists are runtime data, not code.** Add/edit packages in `v2/data/app-lists/{common,shield,googletv}.json`. `engine/app_lists.rs` loads them. Never hard-code packages in Rust.
- **Snapshots are versioned.** `schema_version == 0` is rejected. `schema_version > current` is rejected. Bump the constant in `engine/snapshot.rs` when the schema changes and write an explicit migration.
- **Snapshot reads are path-confined** to `snapshot_dir` via `canonicalize` + `starts_with`. Keep that check on any new read path that takes a user-supplied snapshot location — zip-slip / traversal protection is the same pattern in `adb/install.rs`.
- **The do-not-disable list is mandatory.** `engine::safety::is_safe_to_disable` must gate every disable code path (apply-snapshot, optimize wizard, memory-table Disable button, stock-launcher wizard, panic-recovery's inverse, …). Bypassing it bricks devices.
- **Tauri commands return `Result<T, String>`.** The error type is serialized to the frontend. Convert with `.map_err(|e| e.to_string())`.
- **Frontend is Svelte 5 runes** (`$state`, `$derived`, `$effect`, `$props`) on SvelteKit in SPA mode (`adapter-static`). No legacy stores.

## Local validation before claiming done

CI runs all of this across ubuntu-22.04 / macos-latest / windows-latest plus a frontend type-check job. Don't claim green based on one OS.

From `v2/src-tauri/`:

```
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

From `v2/`:

```
npm run check    # svelte-check must be 0 errors / 0 warnings
npm run build    # vite build must succeed
```

The Linux runner needs `libwebkit2gtk-4.1-dev libssl-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev patchelf` — the workflow installs these; if you're reproducing CI failures locally on Linux, install them too.

## Cutting a v2 release

- Run `v2/release.sh` from `v2/`. Flags: `--patch` (default), `--minor`, `--major`, plus `--beta` / `--rc` / `--alpha` / `--preview`, or `--set X.Y.Z[-tag]` for an explicit version. The script bumps `tauri.conf.json`, `Cargo.toml`, `Cargo.lock`, and `package.json`, commits as `Release v2-X.Y.Z[-tag]`, creates an annotated tag, and pushes (with confirmation).
- Release notes come from `v2/CHANGELOG.md`'s `## v2-X.Y.Z[-tag]` section — add a new section above the existing ones for every release. If no matching section exists, the workflow falls back to `git log` between the previous and current tag.
- Pre-release suffixes (`-alpha`, `-beta`, `-rc`, `-preview`, `-pre`, with optional `.N`) are auto-detected by the workflow and flag the GitHub Release as a prerelease.
- Builds are unsigned. Users hit Gatekeeper (macOS) and SmartScreen (Windows) on first launch; the workflow appends the dismissal instructions to every release body. Signing setup is documented at the top of `.github/workflows/v2-release.yml` for when we add it.
- The tag push fires the workflow. Builds take ~15-25 min across the three OS bundlers and produce: `.deb / .AppImage / .rpm` (Linux), universal `.dmg` + `.app.tar.gz` (macOS), `.msi` + `.exe` (Windows).
- **Windows MSI versioning gotcha**: WiX rejects non-numeric pre-release identifiers — `0.1.0-beta` and `0.1.0-rc1` both fail with "optional pre-release identifier in app version must be numeric-only and cannot be greater than 65535 for msi target". `release.sh` works around this by setting `bundle.windows.wix.version` to a derived `major.minor.patch.N` (where N is the trailing digits of the pre-release tag, or 0). The main version stays human-readable for Linux/macOS/NSIS. If you hand-bump versions, set `bundle.windows.wix.version` too.
- **Don't pipe `yes` into `release.sh`.** The auto-mode classifier blocks it (correctly) — the script's interactive gates are the safety net. Run it interactively, or do the steps by hand.

## Conventions

- **Commits**: always create a new commit. Never `--amend` unless explicitly asked. No `Co-Authored-By` trailers on commits.
- **PRs**: short summary, no checklists or boilerplate. Don't add test plans to the body.
- **Comments**: only when the *why* is non-obvious. Don't add docstrings/comments to code you didn't change. No banner / section-divider comments.
- **Spelling**: the company is "Truemed" — silently correct other casings (TrueMed, TRUEMED, truemed) in writing, except in verbatim quotes, URLs, and code identifiers.
