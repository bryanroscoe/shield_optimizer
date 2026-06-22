# Shield Optimizer — repo notes for agents

Two products live in this tree:

- **v1** — PowerShell debloater (`Shield-Optimizer.ps1` at the root). Released by running `release.sh` at the repo root, which tags the commit and calls `gh release create` directly. No release workflow — only `tests.yml` (CI tests). Still maintained.
- **v2** — Tauri 2 + Rust + Svelte 5 desktop app (`v2/`). Released on `desktop-*` tags (legacy `v2-*` still accepted) via `.github/workflows/v2-release.yml`. The `desktop-` prefix is the release-track namespace; everything human-facing (release title, Homebrew cask, changelog headings) uses the bare semver after it.

The two release tracks are intentionally separate. Don't mix tag namespaces and don't edit the v2 workflow when shipping v1 (or vice versa).

Before touching v2, skim `v2/HANDOFF.md` — it carries the current roadmap, the priority list, and any in-flight scope that isn't yet in the code.

## v2 architecture invariants

These are load-bearing — break them and the safety story falls over.

- **`v2/src-tauri/src/engine/` is pure.** No I/O, no ADB calls, no filesystem. Pure functions and pure types only. All I/O lives in `commands/` and `adb/`. The engine is the audited safety layer; keeping it pure is what makes the unit tests trustworthy.
- **One ADB wrapper.** Everything subprocess-y goes through `SubprocessAdb` in `adb/driver.rs`. Don't add a second wrapper — extend the `AdbDriver` trait if you need a new capability. The active driver lives behind `RwLock<Arc<dyn AdbDriver>>` so `install_adb` can hot-swap it without a restart.
- **One detection function.** Device profiling has a single canonical implementation. Don't fork.
- **App lists live in JSON, not in code.** Add/edit packages in `v2/data/app-lists/{common,shield,googletv}.json`. They are embedded at compile time via `include_str!` in `commands/loader.rs` — editing the JSON requires a rebuild. Never hard-code packages in Rust.
- **Snapshots are versioned.** `schema_version == 0` is rejected. `schema_version > current` is rejected. Bump the constant in `engine/snapshot.rs` when the schema changes and write an explicit migration.
- **Snapshot reads are path-confined** to `snapshot_dir` via `canonicalize` + `starts_with`. Keep that check on any new read path that takes a user-supplied snapshot location — zip-slip / traversal protection is the same pattern in `adb/install.rs`.
- **The do-not-disable list is mandatory.** `engine::safety::classify` / `is_never_disable` must gate every disable code path (apply-snapshot, optimize wizard, memory-table Disable button, stock-launcher wizard, panic-recovery's inverse, …). Bypassing it bricks devices.
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
cd v2 && npm run screenshots   # captures all 12 screens (demo data, dark theme) + rebuilds the GIF
```

It runs offline against the demo fixture layer (`src/lib/demo-mock.ts`, gated behind `VITE_DEMO=1`) — no device needed. Most changes (CSS, layout, copy, rows) flow through with no tooling edits. Two cases need a touch-up:

- **New screen/tab** → add a visit + capture step in `screenshots/capture.mjs`.
- **New `invoke` command that loads on a screen** → add a fixture case in `src/lib/demo-mock.ts`, or that screen renders empty.

The release workflow also regenerates the gallery on every release tag (the `refresh-screenshots` job) and commits it to the default branch, so a release always ships current screenshots even if a manual regen was missed. Requires `ffmpeg` + `npx playwright install chromium` locally. Full pipeline docs: `v2/screenshots/README.md`.

## Cutting a v2 release

- Run `v2/release.sh` from `v2/`. Flags: `--patch` (default), `--minor`, `--major`, plus `--beta` / `--rc` / `--alpha` / `--preview`, or `--set X.Y.Z[-tag]` for an explicit version; add `--yes`/`-y` for a non-interactive run. The script bumps `tauri.conf.json`, `Cargo.toml`, `Cargo.lock`, and `package.json`, commits as `Release desktop-X.Y.Z[-tag]`, creates an annotated tag (prefix from `TAG_PREFIX` in the script), and pushes (with confirmation). The tag prefix in `release.sh` must stay in sync with the workflow trigger.
- Release notes come from `v2/CHANGELOG.md`'s `## X.Y.Z[-tag]` section (bare semver, no track prefix) — add a new section above the existing ones for every release. The workflow also still matches an old prefixed `## v2-X.Y.Z` heading for re-runs. If no matching section exists, it falls back to `git log` between the previous and current tag.
- Pre-release suffixes (`-alpha`, `-beta`, `-rc`, `-preview`, `-pre`, with optional `.N`) are auto-detected by the workflow and flag the GitHub Release as a prerelease.
- Builds are unsigned. Users hit Gatekeeper (macOS) and SmartScreen (Windows) on first launch; the workflow appends the dismissal instructions to every release body. Signing setup is documented at the top of `.github/workflows/v2-release.yml` for when we add it.
- The tag push fires the workflow. Builds take ~15-25 min across the three OS bundlers and produce: `.deb / .AppImage / .rpm` (Linux), universal `.dmg` + `.app.tar.gz` (macOS), `.msi` + `.exe` (Windows).
- After a successful build the `refresh-screenshots` job regenerates `v2/screenshots/gallery.gif` from the released UI and commits it to the default branch (Linux-rendered, so it can look slightly different from a local macOS regen — accepted trade-off). See "Keep the screenshots in sync" above.
- **Windows MSI versioning gotcha** (two parts):
  - WiX rejects non-numeric pre-release identifiers — `0.1.0-beta` fails with "optional pre-release identifier in app version must be numeric-only…". So `release.sh` sets a numeric-only `bundle.windows.wix.version` (the human version stays as-is for Linux/macOS/NSIS).
  - **Windows Installer ignores the 4th version field** for upgrade detection — it compares only `major.minor.build`. An earlier scheme put the pre-release counter in the 4th field (`0.1.0.N`), so every beta read as `0.1.0` and Windows refused in-place upgrades ("uninstall the existing version first"). `release.sh` now encodes the counter into the **3rd (build)** field so each release strictly increases in semver order: `build3 = patch*1000 + typeBase + n` with type bands alpha 0 / beta 300 / rc 600 / stable 999 (e.g. `0.1.0-beta.3` → `0.1.303`, `0.1.0-rc.1` → `0.1.601`, `0.1.0` → `0.1.999`). If you hand-bump, set `bundle.windows.wix.version` with the same scheme.
  - The MSI **UpgradeCode** is auto-derived by Tauri from `productName`/`identifier` and must stay stable for upgrades to work — **don't rename the product or change the identifier** without understanding it resets the UpgradeCode and orphans existing installs.
- **Automating `release.sh`:** for a non-interactive run (CI, agents), pass `--yes` / `-y` — it skips every confirmation gate. Don't pipe `yes` into the script (the auto-mode classifier blocks that, correctly); use the flag instead, or run it interactively, or do the steps by hand.

### Homebrew tap

The macOS distribution channel is a Homebrew tap at [`bryanroscoe/homebrew-shield-optimizer`](https://github.com/bryanroscoe/homebrew-shield-optimizer). One cask, `shield-optimizer`, pointing at the universal `.dmg` from the latest desktop-app release.

- The cask strips `com.apple.quarantine` in a `postflight` block. This is what lets users skip the macOS 15+ Gatekeeper dance after `brew install --cask`. Don't remove that block unless we start signing the build.
- The `bump-tap` job in `.github/workflows/v2-release.yml` updates the cask on every release: downloads the universal DMG, computes the SHA256, rewrites `version` and `sha256` in `Casks/shield-optimizer.rb`, and pushes to the tap. It needs the `HOMEBREW_TAP_TOKEN` secret — a fine-grained PAT scoped to the tap repo with `contents:write`. If the secret is missing the job logs that and skips (rather than failing the whole release).

## Conventions

- **Commits**: always create a new commit. Never `--amend` unless explicitly asked. No `Co-Authored-By` trailers on commits.
- **PRs**: short summary, no checklists or boilerplate. Don't add test plans to the body.
- **Comments**: only when the *why* is non-obvious. Don't add docstrings/comments to code you didn't change. No banner / section-divider comments.
- **Spelling**: the company is "Truemed" — silently correct other casings (TrueMed, TRUEMED, truemed) in writing, except in verbatim quotes, URLs, and code identifiers.
