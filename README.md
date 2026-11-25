# ATV Optimizer

**Manage your Android TV from your phone.** ATV Optimizer for Android lets you use your phone
as a remote, manage TV apps, switch launchers, adjust settings and check device health over Wi-Fi.
No computer needed.
Works on **Nvidia Shield TV, Onn 4K Pro, Chromecast with Google TV, Google TV Streamer** and other
Android TV devices. The mobile app does not support Fire TV.

## TL;DR — get the mobile app

**[ATV Optimizer for Android](#atv-optimizer-for-android)** lets you manage your TV over Wi-Fi
from an Android phone or tablet. To join the Google Play closed test, use the **same Google
account** for all three steps:

1. **[Join the tester group](https://groups.google.com/g/atv-optimizer-testers)** — click **Join group**.
2. **[Opt in to the test](https://play.google.com/apps/testing/com.atvoptimizer.mobile)** — click **Become a tester**.
3. **[Install from Google Play](https://play.google.com/store/apps/details?id=com.atvoptimizer.mobile)** on your Android phone or tablet.

Please stay opted in for at least **14 consecutive days**. For TV setup and what to try, see the
**[tester guide](https://bryanroscoe.com/atv-optimizer/testing/)**.

Prefer a computer? The **[free desktop app for macOS, Windows and Linux](#atv-optimizer-for-desktop)**
is available too.

## ATV Optimizer for Android

<img alt="ATV Optimizer for Android walkthrough" src="v2/mobile/screenshots/gallery.gif" width="300" />

The walkthrough shows demo data, not a live device.

**Help test it.** Join using the three steps above, then follow the
**[tester guide](https://bryanroscoe.com/atv-optimizer/testing/)** to connect your TV and try the app.

- Everything the test asks for is free, apart from the optional app change: connecting, the
  remote, the app list, diagnostics, files and backups, re-enabling apps and Emergency recovery. Don't buy Pro to help test; Google Play
  charges real money even during the test.
- **The first 12 testers get Pro free.** After you join and install, email
  bryanroscoe+atv@gmail.com from the Google account you joined with, and you'll get a Google Play
  code. Redeem it, then open the app within three days, and Pro stays on that account for good,
  including after the public release. If Pro doesn't show up right away, tap **Restore purchases**
  in More. Ko-fi supporters don't use up one of the 12.

The Android app is free, with Pro (Optimize, disabling and uninstalling apps, launcher switching,
changing tweaks, snapshots and the Shell) as a one-time Google Play purchase: US $4.99. Once you own
Pro, you can leave an optional tip in the app; a tip doesn't unlock anything extra.

Developer notes are in [`v2/mobile/`](v2/mobile/README.md).

## ATV Optimizer for desktop

<img alt="ATV Optimizer walkthrough (dark)" src="v2/screenshots/gallery.gif" />

<details>
<summary>Light theme</summary>

<img alt="ATV Optimizer walkthrough (light)" src="v2/screenshots/gallery-light.gif" />

</details>

The desktop app runs on macOS, Windows and Linux. It connects to your TVs directly with its own
built-in ADB, finds them on your network, and updates itself. You don't need a terminal, Google's
`adb` or any scripting. Google platform-tools are still one click away in **Settings → ADB** if a
TV doesn't get along with the built-in engine, or for a TV connected by USB.

## Install

### macOS: Homebrew (easiest)

```sh
brew tap bryanroscoe/shield-optimizer
brew install --cask atv-optimizer
```

Homebrew clears the macOS quarantine flag for you, so the app opens with a normal double-click.
Upgrade with `brew upgrade --cask atv-optimizer`, or let the app update itself.

### Any platform: download an installer

Get an installer from the [**Releases page**](https://github.com/bryanroscoe/shield_optimizer/releases).
Choose the newest stable `v2-*` desktop release; `v0.*` releases contain the legacy PowerShell script:

| OS | File |
|----|------|
| Windows | `.msi` or `_x64-setup.exe` |
| macOS | `_universal.dmg` (Apple Silicon + Intel) |
| Linux | `.AppImage`, `.deb`, `.rpm`, or `_x86_64.flatpak` |

### First launch

The builds aren't code-signed yet, so your OS may warn you the first time you open the app. You
only have to dismiss it once:

- **macOS** (`.dmg` only, Homebrew skips this): on macOS 15+ the dialog only offers *Move to
  Trash* / *Done*. Click **Done**, then either go to System Settings → Privacy & Security →
  **Open Anyway**, or run `xattr -dr com.apple.quarantine "/Applications/ATV Optimizer.app"`.
- **Windows** (SmartScreen): **More info → Run anyway**.
- **Linux**: `chmod +x ATV.Optimizer*.AppImage` before running the AppImage.
  The Flatpak installs with `flatpak install --user ATV.Optimizer_*_x86_64.flatpak`. It doesn't
  update itself: install the next release's `.flatpak` the same way.

On macOS 15+, also click **Allow** when macOS asks whether ATV Optimizer can find devices on your
local network. Without it, the app can't reach any TV.

## Before you start

Turn on ADB on the TV. You only do this once:

1. **Settings → Device Preferences → About →** select **Build** 7 times to unlock Developer Options.
2. **Settings → Developer Options →** turn on **Network debugging**, or **Wireless debugging** if
   that's what your TV offers. Many Google TV devices only have Wireless debugging.

The TV has to allow this computer once. A TV that hasn't yet is listed as **Not allowed yet**:
click **Request approval**, then on the TV check **Always allow from this computer** and choose
**Allow**.

## Using it

1. Launch the app. It reconnects to your saved TVs and scans your network for the rest.
2. For a TV marked **Not allowed yet**, click **Request approval** and choose **Allow** on the TV.
3. Click your device. Each tab is a feature:

| Tab | What it does |
|-----|--------------|
| **Overview** | Model, Android version, build and hardware ID. |
| **Health** | Temperature, CPU and network sampling, RAM and storage, display mode and HDR, audio, and the apps using the most memory, each with a safety verdict. |
| **Playback** | What the TV is actually outputting: resolution, refresh rate, HDR and audio passthrough. |
| **Launcher** | Install a supported launcher, make it the default, and, where the firmware allows it, disable the stock launcher after you confirm. |
| **App List** | Disable, enable or uninstall any app, or install one from the Play Store. |
| **Optimize** | The debloat wizard. Review each app's recommended action and RAM use, uncheck anything you want to keep, then **Run**. Undo it later with Restore. For apps our catalog doesn't cover, it also lists what owners of your exact model turned off, from their [CleanYourTV](https://cleanyour.tv) list, under that list's name. |
| **Tweaks** | HDMI-CEC, match-content frame rate, long-press timeout, animation speed, display scaling, audio passthrough, Private DNS. |
| **Remote** | An on-screen D-pad and live typing over a low-latency channel. Paste a long URL or password instead of typing it on the TV. **Open app** lists the TV's apps by name and opens one; pin up to three as shortcuts on the remote for that TV. |
| **Files** | Browse `/sdcard`, copy files to and from your computer, and find an app's exported backups. |
| **Install APK** | Sideload an `.apk` from a folder. |
| **Snapshot** | Save a device's state and re-apply it later, to the same device (rollback) or a different one (clone). |
| **Shell** | Run an ADB shell command directly, for anything the UI doesn't cover. |

### Can't find your TV?

Use **Scan LAN**, **Add by IP** or **Pair PIN**.

**Saved TVs.** A network TV is saved the first time it connects, and the app dials it directly at
every launch, so a box that sleeps through a scan still shows up. A saved TV that **Didn't answer**
is probably asleep: wake it and click **Connect**. When the app knows the TV's MAC address,
**Connect** also sends a Wake-on-LAN packet first and keeps trying for about 30 seconds while the
TV starts; many TVs ignore it while asleep, especially on Wi-Fi. Only **Connect** sends one, never
the launch. The bookmark on a row saves or unsaves it, and an unsaved TV stays unsaved. A saved
TV only counts as connected when the device at its address reports the same hardware ID; if
another device answers there, the saved row says **Different device**.

**A TV that has never allowed this computer** is listed as **Not allowed yet**. Scanning never
shows anything on the TV. Click **Request approval** on its row: the TV asks "Allow debugging?"
(some say "Allow USB debugging?"). Check **Always allow from this computer** and choose Allow. The
app waits up to 10 minutes; **Cancel** stops waiting. TVs that already allow Google's `adb` on this
computer connect without asking. With Google platform-tools chosen in Settings, the TV asks by
itself the first time the app connects instead.

**On a Mac, if nothing is found even by IP,** macOS may be blocking the app: open System Settings →
Privacy & Security → **Local Network** and turn ATV Optimizer on. If it already looks on, turn it
off and on again. If it's listed more than once, turn every entry on. Then scan again.

With **Google platform-tools** chosen in Settings, click **Restart ADB** first instead: after an
update, macOS cuts off the ADB server the previous version left running. The app restarts it by
itself when it notices, and a manual restart does the same.

**Scan LAN** looks two ways. It sweeps your subnet on ADB's standard port (5555), which older
devices like the Shield use. It also listens for TVs that announce themselves over mDNS. That second
check matters on Android 11+, where Wireless debugging listens on a random port that changes every
time you toggle it, so a port sweep can't find it. If a TV is only advertising its pairing service,
the scan tells you and fills in the pairing address for you.

**Pair PIN** is for Android 11+ Wireless debugging. On the TV, open **Pair device with pairing
code** and enter the IP:port and code it shows. Pairing makes the TV trust your computer but doesn't
connect to it. Go back to the main Wireless debugging screen and enter the IP:port shown *there* in
**Add by IP**. It's a different port from the pairing one. If ADB connects the paired TV on its
own, it shows up when the device list refreshes.

## Safety

- **Disable, don't uninstall.** Disabled apps can usually be re-enabled, but firmware permissions
  may prevent it. Uninstalling can remove data that Restore can't bring back.
- A curated **do-not-disable list** blocks disables that would break the device, in every curated
  action: Optimize, App List, the Launcher wizard and Snapshot apply.
- **Emergency recovery** asks the TV to re-enable every disabled app and reports any it refused.
- The **Shell** (a tab on desktop, More › Shell on Android) is the exception: it runs whatever you type. Its warnings are there for a
  reason. A bad command can erase data or leave the device unusable.

**Use at your own risk.** Changing system apps always carries some risk, but the app favors safe,
reversible changes.

## Tested devices

| Device | Status |
|--------|--------|
| Nvidia Shield TV (2015/2017/2019) | Fully tested |
| Onn 4K Pro (Walmart) | Fully tested |
| Chromecast with Google TV | Should work (Google TV profile) |
| Google TV Streamer (2024) | Should work (Google TV profile) |
| TCL Google TV | Beta: limited testing so far |
| Amazon Fire TV | Not supported: Fire OS blocks the changes this app makes (see [Known limitations](#known-limitations)) |

What's available depends on each device's firmware and the permissions it grants ADB. Android
version alone doesn't decide it. Discovery and pairing need the firmware to expose a working ADB
service, authorize the connection, and be reachable on your network.

The app shows each connected TV's support status next to its name: **Fully supported**,
**Supported**, **Beta**, **Not supported**, or **Not verified** for a model nobody has checked yet.
The status is information only. It never loosens the do-not-disable list, and on an unsupported TV
the app warns before making changes but still lets you re-enable apps and run Emergency recovery.

## Privacy

The app sends **anonymous device and error reports** by default, so we can learn which TVs it runs
on and where it fails. A report has your TV's make, model and Android version, the app version and
your computer's or phone's OS, which actions worked or failed, and error messages with addresses,
names and file paths removed. It never includes serial numbers or other hardware IDs, IP addresses,
TV names, your list of installed apps, file paths, license details or snapshot contents. Reports go
to a server we run on Cloudflare and are kept for 30 days; totals per TV model, with the random
install ID used to count installs, are kept longer. The server doesn't record where a
report came from: no IP address and no location, not even your country.

Turn this off with **Share anonymous device and error reports**, in the **Report a bug** dialog on
desktop or on the **More** screen on Android. Turning it off stops sending immediately and discards anything not yet sent; turning it
back on starts over under a new random ID. Builds made without a report server configured send
nothing.

Apart from your TVs on your local network, these are the only other connections the apps make:

- **While reports are on**, the app also downloads the list of supported TVs from the same server.
- **Report a bug** (and **Send logs** on desktop) uploads what the dialog shows you to the same
  server, only when you press Send, whether reports are on or off. What you send is deleted after
  30 days.
- **Desktop updates**: the app checks GitHub for a new version when it starts and, except in the
  Flatpak build, downloads updates from this repository's releases.
- **Google platform-tools** (desktop) are downloaded from Google only if you choose them in
  **Settings → ADB**.
- **Android**: buying Pro or leaving a tip goes through Google Play. The app asks for internet,
  network and Wi-Fi state, Wi-Fi multicast (to find TVs) and Google Play billing.

Links you tap, such as GitHub issues, Ko-fi or an app's source, open in your browser.

## Known limitations

**Shield discovery on macOS.** If your Shield goes offline or is missing from **Scan LAN** on
2.4.1, update to the [latest desktop release](https://github.com/bryanroscoe/shield_optimizer/releases/latest).
ADB discovery and recovery improvements shipped in 2.5.0. If you still have trouble, see
[connection troubleshooting](#cant-find-your-tv) and the [original report](https://github.com/bryanroscoe/shield_optimizer/issues/235).

**Built-in ADB is network-only.** A TV connected by USB needs **Google platform-tools**, chosen in
**Settings → ADB → ADB engine**. Switching takes effect immediately, and you can switch back.

**Fire TV launcher changes depend on the build.** Some Fire OS builds refuse changes to Amazon's
protected stock launcher, including attempts to re-enable it. The app recognizing Projectivy or
another supported launcher doesn't mean the firmware will let you change Home or disable stock.
When a change fails, the app reports three things separately: the command error, whether its
precautionary restore worked, and which Home app the TV actually reports. It doesn't try to get
around firmware protection. [Issue #205](https://github.com/bryanroscoe/shield_optimizer/issues/205)
tracks a Toshiba Fire TV report that's still waiting on version and build details.

**Unsigned builds.** See [First launch](#first-launch) above.

**Renamed from Shield Optimizer.** It's the same app with the same update channel. From 2.5.0 it
also uses the new name for its data folder and app identifier: the first launch copies your
settings, snapshots and logs over and leaves the old copies in place.

- **Windows:** the 2.5.0 `-setup.exe` installer (and an in-app update of a setup install) removes a
  leftover "Shield Optimizer" install. The `.msi` doesn't, so remove that one from Add/Remove
  Programs yourself.
- **macOS:** `Shield Optimizer.app` renames itself to `ATV Optimizer.app` when you quit it. If you
  have both (for example from installing a DMG beside an older copy), drag `Shield Optimizer.app`
  to the Trash.
- **Homebrew:** the cask is now `atv-optimizer`; `shield-optimizer` keeps working and
  `brew upgrade` moves you over.

## Support the project

The desktop app is free. If it saved you an afternoon of fiddling, you can
[buy me a coffee on Ko-fi](https://ko-fi.com/bryanroscoe).

**Donated on Ko-fi? You get the Android app's Pro for free.** Leave a note with your donation (or
message me on Ko-fi afterward) and I'll send you a Google Play code that unlocks Pro. Redeem it now
by [joining the Android test](https://bryanroscoe.com/atv-optimizer/testing/), or once the app is
public.

## Legacy: PowerShell script (v1)

This project started as a PowerShell script, `Shield-Optimizer.ps1`. It's now **legacy**: it still
works and still gets fixes for serious bugs, but new features only go into the desktop app. If you
need a terminal tool for scripting, a headless box or Docker, see
**[docs/LEGACY-V1.md](docs/LEGACY-V1.md)** for setup and usage.

## Contributing

Building from source, architecture and release process: [`v2/README.md`](v2/README.md).

## Credits

- Debloat research from community guides including [florisse.nl/shield-debloat](https://florisse.nl/shield-debloat/)
- Per-model TV lists from [CleanYourTV](https://github.com/Gathin23/cleanyourtv) (MIT), bundled as a pinned copy: browse them in Catalog; Optimize ranks your TV's list after our own catalog
- Built with AI assistance (Gemini Pro and Claude) by an actual software engineer, tested on real devices
