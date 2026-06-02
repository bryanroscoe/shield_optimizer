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

## Keep the screenshots in sync

The README walkthrough (`v2/screenshots/gallery.gif`) is generated, not hand-captured. **When you change v2 UI in a way that alters any captured screen, regenerate it and commit the result** — don't let the gallery drift from the real app:

```
cd v2 && npm run screenshots   # captures all 10 screens (demo data, dark theme) + rebuilds the GIF
```

It runs offline against the demo fixture layer (`src/lib/demo-mock.ts`, gated behind `VITE_DEMO=1`) — no device needed. Most changes (CSS, layout, copy, rows) flow through with no tooling edits. Two cases need a touch-up:

- **New screen/tab** → add a visit + capture step in `screenshots/capture.mjs`.
- **New `invoke` command that loads on a screen** → add a fixture case in `src/lib/demo-mock.ts`, or that screen renders empty.

The release workflow also regenerates the gallery on every `v2-*` tag (the `refresh-screenshots` job) and commits it to the default branch, so a release always ships current screenshots even if a manual regen was missed. Requires `ffmpeg` + `npx playwright install chromium` locally. Full pipeline docs: `v2/screenshots/README.md`.

## Cutting a v2 release

- Run `v2/release.sh` from `v2/`. Flags: `--patch` (default), `--minor`, `--major`, plus `--beta` / `--rc` / `--alpha` / `--preview`, or `--set X.Y.Z[-tag]` for an explicit version. The script bumps `tauri.conf.json`, `Cargo.toml`, `Cargo.lock`, and `package.json`, commits as `Release v2-X.Y.Z[-tag]`, creates an annotated tag, and pushes (with confirmation).
- Release notes come from `v2/CHANGELOG.md`'s `## v2-X.Y.Z[-tag]` section — add a new section above the existing ones for every release. If no matching section exists, the workflow falls back to `git log` between the previous and current tag.
- Pre-release suffixes (`-alpha`, `-beta`, `-rc`, `-preview`, `-pre`, with optional `.N`) are auto-detected by the workflow and flag the GitHub Release as a prerelease.
- Builds are unsigned. Users hit Gatekeeper (macOS) and SmartScreen (Windows) on first launch; the workflow appends the dismissal instructions to every release body. Signing setup is documented at the top of `.github/workflows/v2-release.yml` for when we add it.
- The tag push fires the workflow. Builds take ~15-25 min across the three OS bundlers and produce: `.deb / .AppImage / .rpm` (Linux), universal `.dmg` + `.app.tar.gz` (macOS), `.msi` + `.exe` (Windows).
- After a successful build the `refresh-screenshots` job regenerates `v2/screenshots/gallery.gif` from the released UI and commits it to the default branch (Linux-rendered, so it can look slightly different from a local macOS regen — accepted trade-off). See "Keep the screenshots in sync" above.
- **Windows MSI versioning gotcha**: WiX rejects non-numeric pre-release identifiers — `0.1.0-beta` and `0.1.0-rc1` both fail with "optional pre-release identifier in app version must be numeric-only and cannot be greater than 65535 for msi target". `release.sh` works around this by setting `bundle.windows.wix.version` to a derived `major.minor.patch.N` (where N is the trailing digits of the pre-release tag, or 0). The main version stays human-readable for Linux/macOS/NSIS. If you hand-bump versions, set `bundle.windows.wix.version` too.
- **Don't pipe `yes` into `release.sh`.** The auto-mode classifier blocks it (correctly) — the script's interactive gates are the safety net. Run it interactively, or do the steps by hand.

### Homebrew tap

The macOS distribution channel is a Homebrew tap at [`bryanroscoe/homebrew-shield-optimizer`](https://github.com/bryanroscoe/homebrew-shield-optimizer). One cask, `shield-optimizer`, pointing at the universal `.dmg` from the latest `v2-*` release.

- The cask strips `com.apple.quarantine` in a `postflight` block. This is what lets users skip the macOS 15+ Gatekeeper dance after `brew install --cask`. Don't remove that block unless we start signing the build.
- The `bump-tap` job in `.github/workflows/v2-release.yml` updates the cask on every release: downloads the universal DMG, computes the SHA256, rewrites `version` and `sha256` in `Casks/shield-optimizer.rb`, and pushes to the tap. It needs the `HOMEBREW_TAP_TOKEN` secret — a fine-grained PAT scoped to the tap repo with `contents:write`. If the secret is missing the job logs that and skips (rather than failing the whole release).

## Conventions

- **Commits**: always create a new commit. Never `--amend` unless explicitly asked. No `Co-Authored-By` trailers on commits.
- **PRs**: short summary, no checklists or boilerplate. Don't add test plans to the body.
- **Comments**: only when the *why* is non-obvious. Don't add docstrings/comments to code you didn't change. No banner / section-divider comments.
- **Spelling**: the company is "Truemed" — silently correct other casings (TrueMed, TRUEMED, truemed) in writing, except in verbatim quotes, URLs, and code identifiers.
