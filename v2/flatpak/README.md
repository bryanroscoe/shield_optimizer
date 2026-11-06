# Flatpak packaging

ATV Optimizer's Flatpak builds from source inside the GNOME runtime, fully offline, the way
Flathub builds apps. Issue [#199](https://github.com/bryanroscoe/shield_optimizer/issues/199).

| File | Purpose |
|------|---------|
| `io.github.bryanroscoe.ATVOptimizer.yml` | flatpak-builder manifest |
| `io.github.bryanroscoe.ATVOptimizer.desktop` | launcher entry |
| `io.github.bryanroscoe.ATVOptimizer.metainfo.xml` | store listing (description, screenshots, releases) |
| `screenshots/` | the listing's screenshots, from `npm run screenshots` (light theme, 1280 px) |
| `generate-sources.sh` | writes `generated-sources/` (gitignored): every crate and npm package with its checksum |

**App ID.** Flathub refuses IDs that end in `.app`, so the Flatpak ID is
`io.github.bryanroscoe.ATVOptimizer`, not the Tauri identifier `com.atvoptimizer.app`. Inside
the sandbox the app keeps its data in
`~/.var/app/io.github.bryanroscoe.ATVOptimizer/data/ATVOptimizer`, so a Flatpak install starts
with fresh settings and downloads its own adb, like a first launch.

**Updates.** Inside a Flatpak, `/app` is read-only, so the app never runs its self-updater
(`update_channel` returns `"flatpak"`). The header and Settings say when a newer version is out
and how to get it. A Flathub install gets it with `flatpak update` or the software center. A
downloaded `.flatpak` bundle has no remote to update from.

**Releases.** `release.sh` adds each version to the metainfo `<releases>` list, and CI fails
when the list doesn't name the version being built. Every `v2-*` tag runs
`.github/workflows/v2-flatpak.yml` from `v2-release.yml` and attaches
`ATV.Optimizer_<version>_x86_64.flatpak` to the GitHub release.

## Build locally (Linux)

```sh
flatpak install --user flathub org.gnome.Sdk//50 org.gnome.Platform//50 \
  org.freedesktop.Sdk.Extension.rust-stable//25.08 org.freedesktop.Sdk.Extension.node22//25.08
v2/flatpak/generate-sources.sh
flatpak-builder --user --install --force-clean build-dir v2/flatpak/io.github.bryanroscoe.ATVOptimizer.yml
flatpak run io.github.bryanroscoe.ATVOptimizer
```

Run `generate-sources.sh` again whenever `Cargo.lock` or `package-lock.json` changes.

## Submitting to Flathub (one-time, by the owner)

Flathub hosts each app's manifest in its own repo, `flathub/io.github.bryanroscoe.ATVOptimizer`,
created by a submission PR. Full steps: <https://docs.flathub.org/docs/for-app-authors/submission>.

1. **Licence.** The app is freeware with all rights reserved. `LICENSE` grants Flathub alone
   permission to build and distribute unmodified packages, which is what Flathub needs to host
   a proprietary app. The metainfo says `LicenseRef-proprietary`, so the listing shows
   "Proprietary".
2. Fork `flathub/flathub`, branch from `new-pr`, and add:
   - the manifest, with the `type: dir` source replaced by
     ```yaml
     - type: git
       url: https://github.com/bryanroscoe/shield_optimizer.git
       tag: v2-X.Y.Z
       commit: <full commit SHA of that tag>
     ```
     and `subdir: v2` on the module (`buildsystem: simple` runs from that subdirectory), and
     the two source paths changed to `cargo-sources.json` and `node-sources.json`;
   - `cargo-sources.json` and `node-sources.json` from `generate-sources.sh` at that tag.
3. Open the PR against `new-pr`, titled `Add io.github.bryanroscoe.ATVOptimizer`. Reviewers will
   ask about `--device=all` (adb over USB opens `/dev/bus/usb`) and `--share=network` (adb over
   Wi-Fi, LAN scanning, the platform-tools download).
4. After merge, log in to flathub.org with this GitHub account to verify the app. Each later
   release is a PR to the app's Flathub repo bumping `tag`, `commit` and both source files. Add
   [x-checker-data](https://github.com/flathub-infra/flatpak-external-data-checker) to have a
   bot open those PRs.

## Not yet checked on a real Linux desktop

CI builds the bundle and validates the metadata, but nobody has run it on hardware yet. Check:

- the window icon and dock grouping (`StartupWMClass=atv-optimizer`);
- adb over Wi-Fi and USB from the sandbox. USB also needs the host's udev rules for the device;
- Settings, Snapshots, Logs: **Open folder** (the file manager reaches the sandbox's data folder
  through the portal);
- snapshot import and export through the file chooser portal.
