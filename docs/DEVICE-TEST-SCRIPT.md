# Physical device test script

Run this on real hardware before tagging a v2 release. `npm run e2e` (see
[`v2/e2e/README.md`](../v2/e2e/README.md)) covers the same flows against simulated devices; this
script covers what simulation and CI cannot: vendor firmware behavior, macOS permission (TCC)
prompts, installer upgrades, and the Android runtime lifecycle of the mobile app.

Step IDs are stable so other docs can cite them: **D1–D9** desktop basics, **A1–A10** upgrade,
launcher and device-type checks, **M1–M8** the mobile app.

Abort rule for every section: if a step leaves the TV without a launcher or with an unexpected
disabled app, use Restore / Emergency Recovery (desktop) or More → Emergency recovery (mobile),
stop, and record the step.

## Preconditions

- An Nvidia Shield TV (or another supported TV) and the computer on the same LAN.
- On the TV: Settings → Device Preferences → Developer options → Network debugging on.
- Note the current default launcher, and pick one app you are willing to disable and re-enable
  (a game, for example).
- Desktop: a build of the commit you intend to tag (`cd v2 && npm run tauri dev`, or
  `npm run tauri build`). For the upgrade checks (A1, A2) use the staging build below.
- Mobile: the debug APK sideloaded on an Android phone on the same LAN (the app runs on the phone
  and drives the TV; it is not installed on the TV). See `v2/mobile/HANDOFF.md` for the build.

## Desktop (D1–D9, about 25 minutes)

| Step | Action | Expected |
|------|--------|----------|
| D1 | Launch the app from the DMG with a USB drive mounted, do not click anything | Devices page loads; the boot-time scan may run; no macOS removable-volume permission prompt appears, and none recurs after quitting |
| D2 | Click Scan LAN | The Shield appears with a [NET] tag; a single scan message |
| D3 | On the TV enable Wireless debugging → Pair device with pairing code; in the app click Pair PIN, enter that IP:port and PIN | Message says paired; the PIN field clears; the app does not report a device on port 5555. Connect IP is pre-filled with the paired host plus a trailing colon and takes focus, with a one-line prompt for the separate port |
| D4 | Enter the port from the main Wireless debugging screen in Connect IP | Device becomes `device`, row clickable |
| D5 | Open the device → Apps. Find an app that is not in the catalog (a sideloaded APK) and a known bloat entry | The sideloaded app shows Unknown with a reason; catalog bloat shows Caution; nothing Unknown is pre-ticked in Optimize |
| D6 | Optimize tab: leave defaults, click Apply, read the confirmation | Confirmation lists only Caution items; Unknown items are absent unless you ticked them; apply completes; disabled apps re-enable from the Restore path |
| D7 | Launchers: switch the default launcher to another installed launcher, then switch back | Each attempt reports its stage; on any failure the previous launcher is restored and the report says so; Home on the TV matches the app's report |
| D8 | Sideload: pick a folder with APKs, quit and relaunch the app | The folder is remembered; nothing scans until Scan folder is clicked; on macOS no repeated volume prompt |
| D9 | Files → App file backups for SmartTube (if installed) after SmartTube's Settings → Backup/Restore → Local backup | The `.zip` under `Documents/SmartTubeBackup` is found |

## Upgrade, launcher and device-type checks (A1–A10)

| Step | Action | Expected |
|------|--------|----------|
| A1 | Windows: install the previous release's MSI, then run a **staging** MSI of the candidate over it (see "Staging build") | The installer treats it as an upgrade (no "uninstall the existing version first"); the app relaunches with the same data dir, saved devices and snapshots intact |
| A2 | macOS: with the previous release's app installed from its DMG, install a staging build of the candidate | Launching it shows the same saved devices and snapshots (shared data dir). Upgrading from a pre-2.3.0 `Shield Optimizer.app` installs `ATV Optimizer.app` alongside it; the old app is safe to trash afterward |
| A3 | Launcher tab → Advanced → Set another app as Home… with an installed custom launcher (e.g. Projectivy) | The picker lists installed apps; choosing one asks Android to make it Home and reports honestly. On a Shield or Google TV the stock launcher usually keeps Home: expect `ok = false` with a message that stock is in the way. Stock is never disabled by this step |
| A3b | Same, picking an app that declares no Home screen (e.g. YouTube) | Refused honestly ("doesn't declare a Home screen"); nothing is disabled; Disable stock launcher stays unavailable |
| A4 | **Precondition: A3 reported stock is in the way.** Launcher tab → Advanced → Disable stock launcher, taking the offered snapshot first | The confirmation names the risk. The backend disables stock, verifies the target now holds Home, and reports success; pressing Home on the remote opens the target. The TV has a Home screen throughout |
| A4b | Repeat A4 with a target that cannot actually take Home, keeping ADB connected throughout (the rollback is itself an ADB command, so a disconnected TV cannot be restored until it reconnects) | The takeover reports failure and **re-enables the stock launcher**; pressing Home opens stock. Picking TV Settings as the target is refused outright. If a real disconnect ever leaves stock disabled, reconnect and use Emergency Recovery on the Overview tab |
| A5 | A readable device whose TV evidence is **unknown** (reports neither `tv` nor a phone/tablet class, e.g. the Xiaomi TV Box S from #120) | The row carries an amber **UNCONFIRMED TV** tag and opens directly, with no Open anyway prompt. Unknown is never treated as "not a TV" |
| A6 | A device that **positively reports** it is not a TV (a phone or tablet on the same adb) | The row shows **NOT AN ANDROID TV** and is not a link; Open anyway is offered behind a short confirm. Confirming opens the tools; after reconnecting later, the row opens directly with a muted NOT A TV tag (remembered by hardware id, never by address) |
| A6b | An **unauthorized** device (accept the prompt later) | Neither tag appears; the row explains the authorization prompt. Nothing is claimed about device type |
| A7 | Wireless debugging → Pair device with pairing code; in the app use Pair PIN | On success, Connect IP is pre-filled with the paired host plus a trailing colon and takes focus, with a one-line prompt for the separate port from the Wireless debugging screen; entering that port connects |
| A8 | Tweaks → Screensaver | Reads the device's current `secure.screensaver_components`; offers AOSP Basic Daydream; Restore puts back whatever was configured when the tab loaded, without needing to know a vendor screensaver's component name |
| A9 | Snapshots: take a snapshot without changing the launcher, then open Preview restore | Preview proposes no launcher change (the Now column reads the TV's actual current Home); restoring does not touch the launcher |
| A10 | Header → Report a bug | Opens the GitHub bug-report issue form; the diagnostics field is pre-filled when the bundle fits in the link, otherwise the dialog says to paste it; nothing is sent automatically |

### Staging build (for A1 and A2)

Tagging a `v2-*` release (including `-rc`) publishes to every installed copy through the updater
manifest and bumps the Homebrew cask, so the upgrade test must use a local build that is never
tagged or committed:

1. On the target OS, check out the commit you intend to tag.
2. Set the version in the working tree only, in every file `release.sh` edits: `version` in
   `v2/src-tauri/tauri.conf.json` and `v2/package.json`; the `[package]` version in
   `v2/src-tauri/Cargo.toml` and `v2/crates/core/Cargo.toml` (the running app reports
   `CARGO_PKG_VERSION`, so skipping these builds an installer that reports the old version); and
   `bundle.windows.wix.version` using the band scheme in `CLAUDE.md` (a stable `X.Y.Z` is
   `X.Y.(Z*1000+999)`). The build refreshes `Cargo.lock` itself.
3. `cd v2 && npm ci && npm run tauri build`. The MSI lands under
   `v2/target/release/bundle/msi/`, the DMG under `.../bundle/dmg/`.
4. Run A1 / A2 against those artifacts, then `git checkout -- .` to discard the version edits.

If no Windows machine is available before tagging, run A1 immediately after the release workflow
finishes, on the published MSI, and treat a failure as a stop-ship: pull the release and cut a
patch.

## Mobile (M1–M8, about 20 minutes, phone on the same LAN)

| Step | Action | Expected |
|------|--------|----------|
| M1 | Fresh install, connect a new device | Discovery shows the Shield as Found (no Saved label yet) |
| M2 | Connect and open the Dashboard, then disconnect and rescan | The Shield shows as Saved with its name; exactly one saved row |
| M3 | Force a new DHCP lease (or change the TV's static IP), rescan | The same single saved row moves to the new address; no duplicate |
| M4 | Dashboard | "X of Y installed recommended apps are enabled" with real numbers; no score ring; Review app choices opens Optimize |
| M5 | Optimize → Recommended / Optional apps | Optional rows default to Keep; a sideloaded app shows UNKNOWN with a reason; no Select all on Optional |
| M6 | Choose Disable on one optional app, switch tabs, Apply | The summary shows 1 optional; the confirmation names the app and the 0.5× animation post-pass; Cancel leaves the TV untouched; Apply disables only that app |
| M7 | Apps → system filter; Backups | System apps hidden by default and toggle on; Backups lists every installed app, not only the catalog |
| M8 | Diagnostics → unknown-app log | Entries exist for the Unknown apps seen in M5 |

The mobile app's own open physical gates (saved-TV reconnect #93, the restart report #105, code
pairing, fast-remote Phase 4) are listed in [`v2/mobile/BACKLOG.md`](../v2/mobile/BACKLOG.md).
