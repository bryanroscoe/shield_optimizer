# Changelog — Shield Optimizer v2

This file is the authoring surface for v2 release notes. The release workflow
(`.github/workflows/v2-release.yml`) looks for a section matching the tag
being released (e.g. `## v2-0.1.0-beta`) and uses its contents as the body of
the GitHub Release. If no section is found, the workflow falls back to
auto-generated notes from `git log`.

Tag conventions:

- `v2-0.1.0` — stable release.
- `v2-0.1.0-beta`, `v2-0.1.0-beta.2`, `v2-0.1.0-rc1`, `v2-0.1.0-alpha`,
  `v2-0.1.0-preview`, `v2-0.1.0-pre` — auto-flagged as **prerelease** by the
  workflow.

When you add a new section, put it at the top; older releases go below.

---

## v2-0.1.0-beta.1

Re-cut of the first public beta to fix a Windows MSI bundling failure.

The original `v2-0.1.0-beta` build failed on the Windows runner with
`optional pre-release identifier in app version must be numeric-only and
cannot be greater than 65535 for msi target` — WiX requires a numeric
pre-release identifier. Fixed by setting `bundle.windows.wix.version` to
`0.1.0.1` (MSI-only override) while keeping `0.1.0-beta.1` as the human
version everywhere else. Linux and macOS builds were unaffected.

No other changes since `v2-0.1.0-beta` — feature set identical.

---

## v2-0.1.0-beta

First public beta of the Tauri rewrite. Feature parity with v1's PowerShell
script across every section of `docs/FEATURES.md` (see HANDOFF §2A for the
row-by-row match-up). Installable on macOS, Windows, and Linux — no
PowerShell, no terminal, no scripting required.

### What's new vs. v1

- **Native desktop app** built on Tauri 2 + Rust + Svelte 5. Same engine
  driving the same `adb` commands, but a GUI you click instead of a TUI
  you type into.
- **Optimize / Restore wizard** as a real interactive tab — review every
  app's recommended action with its current state and RAM usage, untick
  anything you want to keep, then Run with live per-row progress.
- **Cross-device snapshot apply** with a global Snapshots page. Save a
  device's state, then apply that snapshot to a different device from
  the same UI.
- **Real safety guardrails**: a curated do-not-disable list (system UI,
  shell, package installer, GMS, etc.) prevents brick-tier disables from
  every code path that could issue one — including the apply-snapshot
  flow, the stock-launcher wizard, and the memory-table Disable button.
- **APK auto-discovery** — point at a folder once and every `.apk`
  inside shows up with a one-click Install.
- **Live Health Report** with parsed display mode, HDR support, audio
  device, RAM, storage, and the top 20 memory consumers with risk tags.
- **Per-launcher actions**: install via Play Store, enable, disable, set
  as default. Multi-strategy promotion (role API → set-home-activity →
  HOME intent kick) handles Shield's customized Android 11 quirks.
- **Auto-installs ADB** on first launch if `adb` isn't on PATH — no
  manual platform-tools download required.
- **Network scan on boot**, PIN pairing for Android 11+, Restart ADB,
  Report All, Emergency Recovery (re-enable every disabled package), all
  ported from v1.

### Known limitations in this beta

- Unsigned binaries — first launch shows the Gatekeeper / SmartScreen
  warning. See dismissal steps below.
- No auto-updater yet. Future releases will need a manual download.
- Flatpak not built; use the `.AppImage` on Linux.

### What v1 users should know

The PowerShell script (`Shield-Optimizer.ps1`) still works and still gets
maintained on the `v0.x.x` tag track. v2 doesn't replace it — it sits
beside it. If you'd rather stay on the TUI, nothing changes for you.

---

<!-- Add new sections above this line. Older releases go below. -->
