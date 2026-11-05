# Physical device test script

Run this on real hardware before tagging a v2 release. `npm run e2e` (see
[`v2/e2e/README.md`](../v2/e2e/README.md)) covers the same flows against simulated devices; this
script covers what simulation and CI cannot: vendor firmware behavior, macOS permission (TCC)
prompts, installer upgrades, and the Android runtime lifecycle of the mobile app.

Step IDs are stable so other docs can cite them: **D1–D12** desktop basics, **A1–A10** upgrade,
launcher and device-type checks, **R1–R9** upgrades across the 2.5.0 rename, **N1–N18** the
built-in (native Rust) ADB engine, **M1–M8** the mobile app.

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

## Desktop (D1–D12, about 40 minutes)

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
| D10 | macOS 15+ only, bundled app opened from Finder or `open` (not `tauri dev`, which macOS credits to Terminal). Start with no Local Network decision for ATV Optimizer: a fresh install in a new macOS user account, or on macOS 27.2+ remove ATV Optimizer from System Settings → Privacy & Security → Local Network with the − button. (1) Launch. (2) Choose Don't Allow. (3) Click Open Privacy & Security, then Local Network; turn ATV Optimizer on and return to the app | (1) The Local Network alert appears at launch, before any scan, naming ATV Optimizer and showing its usage string ("…connects to your TV over your local network using ADB…"). (2) Devices shows "macOS is blocking ATV Optimizer from your local network." with Open Privacy & Security, and its ? help explains turning it off and on again (every entry, if listed more than once) and then Restart ADB. (3) The button opens System Settings at Privacy & Security (macOS 26.6 has no deeper link to Local Network; on a version that has one it lands there); within a few seconds of returning, the notice clears and Scan LAN finds the TV. Record the macOS version |
| D11 | macOS 15+, after D10 with Local Network granted: quit it, install the next build (a newer staging or release DMG) over it and launch it | No new Local Network alert; no blocked notice on Devices; Scan LAN still finds the TV |
| D12 | macOS 15+: with the previous version running and connected to a TV over the network, install the new build over it (DMG or Homebrew) and launch it, without restarting ADB by hand. Depends on the adb auto-repair in #286 (restart a cut-off adb server once and retry) | The TVs are found and connect with no manual steps; no blocked notice. Without the auto-repair, expect "No route to host" on every TV while the app itself is allowed: the connect error then suggests Restart ADB (not Local Network settings), and Restart ADB fixes it |

## Upgrade, launcher and device-type checks (A1–A10)

| Step | Action | Expected |
|------|--------|----------|
| A1 | Windows: install the previous release's MSI, then run a **staging** MSI of the candidate over it (see "Staging build") | The installer treats it as an upgrade (no "uninstall the existing version first"); the app relaunches with the same data dir, saved devices and snapshots intact |
| A2 | macOS: with the previous release's app installed from its DMG, install a staging build of the candidate | Launching it shows the same saved devices and snapshots. For the 2.5.0 rename, run R5–R8 instead |
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

## Upgrades across the 2.5.0 rename (R1–R9)

2.5.0 changes the identifier to `com.atvoptimizer.app`, the data folder to `ATVOptimizer` and the
binary to `atv-optimizer`. Every earlier 2.x used the old names, so each check starts from a
machine that has used a published release. Before upgrading, on that machine:

- **The new folders must not exist yet**, or the check proves nothing: macOS
  `~/Library/Application Support/ATVOptimizer` and `~/Library/WebKit/com.atvoptimizer.app`;
  Windows `%LOCALAPPDATA%\ATVOptimizer` and `%LOCALAPPDATA%\com.atvoptimizer.app`; Linux
  `~/.local/share/ATVOptimizer` and `~/.local/share/com.atvoptimizer.app`. If an earlier test or
  build left them, quit every copy of the app and **rename** them (for example to
  `ATVOptimizer.before-R1`; `mv` on macOS/Linux, `ren` on Windows). Don't delete them, and rename
  them back when you are done.
- Change three settings you can recognize (theme, auto-update, Stop ADB when quitting), save a
  snapshot, and note the support ID in Settings › Privacy & diagnostics (with reports on).
- Record the old data: macOS `~/Library/Application Support/ShieldOptimizer` and
  `~/Library/WebKit/com.shieldoptimizer.app`; Windows `%LOCALAPPDATA%\ShieldOptimizer` and
  `%LOCALAPPDATA%\com.shieldoptimizer.app`. A file list with sizes and modification times is
  enough (`find … -type f -exec ls -l {} +`, or `dir /s`).

Every R step then expects: the three settings, the snapshot and the support ID are as they were;
`ATVOptimizer` and the new profile folder exist, each with a `.atvoptimizer-migrated` file, and
`ATVOptimizer` also has `.webview-profile-migrated`; the recorded old folders are unchanged (the
app never writes to them again); Devices finds the TV with the adb the old release downloaded,
with no prompt to install platform-tools. The app log (`ATVOptimizer/logs/`) has one
`rename migration copied` line per folder and no `rename migration failed` line.

A staging build can't be offered by the in-app updater (it only offers published releases). Steps
that say "in-app" are simulated before the tag as described, and repeated for real right after the
release on a published 2.4.1 copy. **Stop-ship:** if the real in-app update loses settings,
snapshots or the support ID, or leaves an app that doesn't open, pull the release (restore the
previous `v2/updater/latest.json`) and cut a patch.

| Step | Action | Expected |
|------|--------|----------|
| R1 | Windows, NSIS side by side: install **2.2.0** `_x64-setup.exe`, run it once, update it in-app to 2.4.1 (or install 2.4.1 `-setup.exe` over it) so "Shield Optimizer" and "ATV Optimizer" are both in Add/Remove Programs, then install the 2.5.0 staging `-setup.exe` | Afterwards Add/Remove Programs lists only ATV Optimizer 2.5.0. `%LOCALAPPDATA%\Shield Optimizer` is gone (or holds only files the old installer did not put there), and the Start menu and desktop "Shield Optimizer" shortcuts are gone. "ATV Optimizer" Start menu and desktop shortcuts exist and open `atv-optimizer.exe`. `%LOCALAPPDATA%\com.shieldoptimizer.app` and `%LOCALAPPDATA%\ShieldOptimizer` are still there |
| R1a | Windows, update from a running 2.2.0: set up R1's side-by-side state, delete the "ATV Optimizer" Start menu and desktop shortcuts (a user who only ever used the old ones), launch **Shield Optimizer** from its shortcut, then run the staging `-setup.exe /P /UPDATE` from a terminal, which is what the in-app updater runs. After the tag, repeat for real: update in-app from the running 2.2.0 | The old program is closed without a prompt; only ATV Optimizer 2.5.0 remains. The Start menu has an "ATV Optimizer" shortcut (and the desktop one, if a desktop "Shield Optimizer" shortcut existed) that opens `atv-optimizer.exe`; no "Shield Optimizer" shortcut is left. A taskbar pin of the old Shield Optimizer survives and opens `atv-optimizer.exe`, starting in the install folder |
| R1b | Repeat R1 with the old Shield Optimizer **running** during an interactive install. Once click OK on "An earlier version … is running", once click Cancel | OK: it closes and is removed as in R1. Cancel: the installer stops without installing anything; the old install and any 2.4.1 install are unchanged. Silent and passive installs (`/S`, `/P`, in-app updates) never ask and always close it |
| R2 | Windows, NSIS: with only 2.4.1 `-setup.exe` installed and ATV Optimizer **pinned to the taskbar**, run the 2.5.0 `-setup.exe` (and, after the tag, update in-app) | One ATV Optimizer entry, version 2.5.0, in the same folder as before; Start menu and desktop shortcuts and the taskbar pin open the new `atv-optimizer.exe` with "Start in" set to the install folder, and the running app groups under that pin; `shield-optimizer-v2.exe` is gone from the install folder |
| R3 | Windows, MSI: with 2.4.1 `.msi` installed and pinned to the taskbar, install the 2.5.0 `.msi` | Treated as an upgrade (one entry, 2.5.0); Start menu and desktop shortcuts open `atv-optimizer.exe`. **Known and expected:** the taskbar pin still points at the deleted `C:\Program Files\ATV Optimizer\shield-optimizer-v2.exe` and doesn't open the app (the MSI has no hook; the CI upgrade harness shows the same). Unpinning and pinning ATV Optimizer again fixes it, as the CHANGELOG tells MSI users |
| R4 | Windows: after R1–R3, uninstall ATV Optimizer without ticking "Delete the application data" | `%LOCALAPPDATA%\com.atvoptimizer.app` and `ATVOptimizer` remain; the old folders remain |
| R4b | Windows: reinstall 2.5.0, then uninstall it **ticking** "Delete the application data", reinstall and launch | `%LOCALAPPDATA%\com.atvoptimizer.app` was deleted and comes back empty: the app starts with default settings (theme, auto-update, Stop ADB), not the 2.4.1 ones, and, with debug logging on, the log says the profile was already migrated |
| R5 | macOS, old bundle, simulated: put a 2.4.1 `Shield Optimizer.app` in a scratch folder (copy the 2.4.1 DMG's app and rename it; not `/Applications`, unless it is the copy under test and isn't Homebrew's), add it to the **Dock** and to **Login Items** (System Settings › General › Login Items), launch and quit it. Then do what the updater does: move it aside and `ditto "<staging>/ATV Optimizer.app" "<scratch>/Shield Optimizer.app"`. Launch it from the Dock, quit, launch again, quit. After the tag, repeat for real by updating a 2.4.1 `Shield Optimizer.app` in-app | The first launch shows a new "find devices on your local network" prompt for ATV Optimizer. Until you click Allow, Scan LAN finds nothing (the adb server 2.4.1 left running is cut off); after Allow, the app restarts it and the TV is found. System Settings › Privacy & Security › Local Network may now list ATV Optimizer twice; the old entry stays. After the first quit the bundle is `<scratch>/ATV Optimizer.app`; the Dock icon and the Login Item still open it (log out and in for the Login Item). If `ATV Optimizer.app` already existed beside it, nothing is renamed and the log says so |
| R6 | macOS: with `ATV Optimizer.app` 2.4.1 from the DMG, simulate the update the same way (`ditto` the staging bundle over a copy at the same path), then after the tag update in-app for real | Same bundle path; settings, snapshot and support ID survive; the Local Network prompt behaves as in R5 |
| R7 | macOS, Homebrew: with the `shield-optimizer` cask at 2.4.1, merge the tap rename and run `brew update && brew upgrade --cask` | Brew reports the rename to `atv-optimizer` and installs 2.5.0 as `/Applications/ATV Optimizer.app`; `brew list --cask` shows `atv-optimizer`; settings survive. A pre-2.3 brew install (`Shield Optimizer.app`) is replaced by brew and is never renamed by the app |
| R8 | Any OS: launch 2.5.0 a second time | Nothing is copied again: the second launch logs `AlreadyMigrated` (debug logging), `ATVOptimizer` is untouched, and no `*.migrating-*` folder is left beside it |
| R9 | Linux: with the 2.4.1 `.deb` installed and used, `sudo apt install ./ATV.Optimizer_2.5.0_amd64.deb`, then open ATV Optimizer from the app menu. Repeat with the AppImage (replace the 2.4.1 file, run the new one) | `/usr/bin/atv-optimizer` replaces `/usr/bin/shield-optimizer-v2`; the app opens from the menu; `~/.local/share/ATVOptimizer` and `~/.local/share/com.atvoptimizer.app` are filled as above. After an in-app update (post-tag) the app may not reopen by itself; reopening it from the menu works |

### Staging build (for A1, A2 and R1–R9)

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
4. Run A1 / A2 and R1–R9 against those artifacts, then `git checkout -- .` to discard the version
   edits.

If no Windows machine is available before tagging, run A1 immediately after the release workflow
finishes, on the published MSI, and treat a failure as a stop-ship: pull the release and cut a
patch.

## Built-in ADB engine (N1–N18, about 60 minutes)

The desktop app's own Rust ADB (PR #268), chosen in Settings → ADB → ADB engine. Already covered
without you, so not repeated here: the vendored transport against both Shields (Android 11, plain
TCP) and the Android TV 14 emulator (plain and TLS) with `examples/device_smoke.rs` — `suite`
28/28, `control` 17/17 (deadline/cancel, bounded shell, identity-bound services, a mismatched
hardware ID never running the command), single/split/3 MiB/corrupt/downgrade installs, first-time
approval, cancel during the prompt, PIN pairing with right and wrong codes. What only you can do is
the app itself on your LAN (its launch scan dials every TV), TVs other than the two Shields, real
Wireless debugging hardware, and Windows/Linux.

Preconditions: a build of the commit under test (normal builds start on Built-in; the PR's
"Native ADB macOS test app" artifact is a separately labeled copy); a TV that has never approved the app's
built-in key, or one where you accept clearing its approvals (TV Settings → Developer options →
Revoke USB debugging authorizations); a Google TV or other Android 11+ TV with Wireless
debugging; the devtest APKs or any small APK you are willing to install and remove.

| Step | Action | Expected |
|------|--------|----------|
| N1 | macOS: first launch of the native build from Finder | The Local Network alert appears once (the test build has its own identifier) → Allow. Settings → ADB shows "Built-in" selected; Devices says "Native Rust ADB · direct connection" |
| N2 | With TVs that approved the earlier native test app (both Shields have), launch and let the scan run | They connect with no prompt on the TV: the key moved from `ShieldOptimizer/native-adb` to `ATVOptimizer/native-adb` with the 2.5.0 rename |
| N3 | A TV that has never approved this key: Scan LAN, watch the TV, choose Allow with "Always allow" | The row says Approval requested while the TV asks; within a few seconds of Allow it becomes a normal TV card. Repeat with Cancel/Deny on another TV (or after revoking): the row stays awaiting approval with Retry approval, nothing else breaks |
| N4 | Wireless debugging TV: Pair PIN with the code and pairing port; then let it connect on the connect port. Then a wrong PIN once | Pairs, connects on the separate connect port, survives an app restart without pairing again. Wrong PIN: "The PIN didn't match…" |
| N5 | Same TV: open Files, then immediately a download, a Device Logs read and an install, back to back | Each works first time (adbd on Android 14 sometimes drops a TLS connection opened right after another; the app now retries that once) |
| N6 | Files: upload a small file to `/sdcard/Download`, download it back; upload to `/system` | Same bytes back; `/system` fails with the TV's reason, no "success" |
| N7 | Install APK: a single APK, the same with replace, a split APK set (base + split), a corrupt file | Each installs or fails with the package manager's own `Failure […]` text; nothing reports success that didn't happen. Uninstall the test package afterwards |
| N8 | Remote tab: D-pad, Home, Back, type text, hold a key | Works; note the delay per press (fast native Remote is still off, so this is shell input) |
| N9 | Screenshot, Device Logs, a Shell bookmark, the health report | All return real data |
| N10 | Optimize: disable then re-enable one harmless app; snapshot and restore | Same results as with platform-tools |
| N11 | Put a TV to sleep from its remote, wait a minute, wake it | Its card goes offline, then reconnects by itself or with Retry, without a second card for the same TV |
| N12 | Turn the Mac's Wi-Fi off for 20 seconds, then on | Cards go offline without the app hanging; they come back after a scan or Retry |
| N13 | Quit during a large download or install | The app quits within a few seconds and leaves no helper running on the TV |
| N14 | Settings → ADB engine: switch to Google platform-tools, use a TV, switch back to Built-in | Each switch takes effect without relaunching; Devices follows; Restart ADB and Stop ADB when quitting are usable only with platform-tools |
| N15 | USB: plug a TV in by USB with Built-in selected, then switch to platform-tools | Built-in is network-only and doesn't list it; platform-tools does |
| N16 | Upgrade from 2.5.0 (platform-tools) to the native build with an adb server still running | TVs connect through Built-in; any TV that never approved the built-in key asks once; the old server doesn't interfere |
| N17 | Windows, packaged build: first launch, approve one TV, scan, Files download, install | Works as on macOS. If Windows Firewall asks about ATV Optimizer (mDNS discovery listens for TV announcements), allow private networks. The key is created under `%LOCALAPPDATA%\ATVOptimizer\native-adb` and only your account can read it |
| N18 | Linux, packaged build (.deb or AppImage): same as N17 | Works as on macOS; the key is under `~/.local/share/ATVOptimizer/native-adb` with mode 700/600 |

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
