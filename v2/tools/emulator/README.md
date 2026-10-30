# Local testing against Android emulators

Two separate substitutions, with different payoffs. They are independent — you can
take either one without the other.

| Replace | With | Buys you | Costs you |
|---|---|---|---|
| The Pixel running the mobile app | A phone AVD on the Mac | Most of the mobile UI, lifecycle and reconnect work, with no wireless-ADB deploy dance | mDNS discovery, real background/doze behavior, touch ergonomics |
| The Shield TV | An Android TV AVD (`tv-emulator.sh`) | Disable/enable, launcher switching, snapshots, Unknown-safety labelling, sideload, file browsing | Vendor firmware behavior, the `tv` device class, Nvidia/Google TV bloat, remote latency |

Everything below was measured on this repo's checkout against
`system-images;android-34;android-tv;arm64-v8a` on an arm64 Mac — not inferred.

## The TV emulator

```
export ANDROID_HOME=~/Android/sdk
./tv-emulator.sh install    # emulator + system image, about 9 GB on disk
./tv-emulator.sh create
./tv-emulator.sh start      # prints the detection properties when it is up
./tv-emulator.sh catalog    # what Optimize/Apps will have to work with
```

The desktop app drives it through the ordinary `emulator-5554` serial — nothing
special to configure, because `SubprocessAdb` shells out to the same `adb` the
emulator registers with.

### What works, verified

- `pm disable-user --user 0 <pkg>`, `pm enable`, `pm list packages -d`
- `cmd shortcut get-default-launcher` and the HOME activity query the launcher
  code uses — the image ships two HOME activities (`com.google.android.tvlauncher`
  and `com.android.tv.settings/.system.FallbackHome`), so launcher switching and
  rollback are exercisable
- `dumpsys meminfo`, `screencap`
- 10 catalogued packages are present, spanning `safe`/`medium`/`high` and both
  `disable` and `uninstall`, plus about 130 uncatalogued packages — a good surface
  for Unknown-safety labelling
- None of this needs root

### Two limits that bite

**1. TV classification comes from the leanback feature, not the build string.** The image
reports `ro.build.characteristics=emulator`, with no `tv`, and
`emulator -prop ro.build.characteristics=tv` does **not** change that: init refuses to overwrite a
`ro.*` property the image already set. Its real TV signal is the package-manager feature set
(`android.hardware.type.television`, `android.software.leanback`). Detection
(`crates/core/src/engine/detection.rs`) reads `pm has-feature android.software.leanback` as well
as the characteristics, and a leanback `true` is enough to call a device a TV, so the image should
classify as a TV and get the matching app lists. That has not been re-measured on the emulator;
check the profile the app shows after `./tv-emulator.sh start`. If it comes back Unknown,
`AppListBundle::for_device` serves the `common` list only (`unknown_returns_common_only` in
`engine/app_lists.rs` pins this).

**2. `adb root` is refused.** The API 34 `android-tv` image is a `user` build
(`ro.debuggable=0`, `ro.build.type=user`). So adbd cannot be told to listen on
TCP (`service.adb.tcp.port`), there is no guest port to forward, and **the mobile
app cannot use this emulator as its TV**. `./tv-emulator.sh expose` detects this
and says so rather than half-failing. Other API levels or tags were not tested for
a userdebug build.

## The phone emulator

This is the substitution that actually answers "instead of my phone". The mobile
app runs in a phone AVD and dials a **real** TV over the LAN.

Verified: an emulator guest reaches both the host loopback alias (`10.0.2.2`) and
the Mac's LAN address, so `WirelessAdb` can connect to a TV at `192.168.x.y:5555`
exactly the way the Pixel does. There is no host validation on the connect path —
`wireless_connect` passes `host`/`port` straight through.

```
export ANDROID_HOME=~/Android/sdk
./phone-emulator.sh install   # system image (only google_apis_playstore is
./phone-emulator.sh create    #   published for arm64; root is not needed here)
./phone-emulator.sh start     # boots on emulator-5584, so a TV AVD can share
./phone-emulator.sh deploy    #   the default 5554 at the same time
./phone-emulator.sh logs      # follows the RustStdoutStderr tracing output
```

Verified end to end on this setup: `deploy` builds the aarch64 debug APK, installs
it, and launches `MainActivity`; the app runs, renders the Onboarding screen
correctly, and offers "Enter IP address manually" -- which is the path to use,
since the scan will find nothing. Unlike the Playwright browser loop, the
emulator renders real safe-area insets, so layout at the top and bottom edges is
trustworthy here.

What this does **not** cover, and still needs a real phone:

- **mDNS discovery.** QEMU user-mode networking does not carry multicast, so the
  scan flow finds nothing. Add the TV by IP:port instead. This is expected from how
  the NAT works; it was not measured here.
- Real background/doze transitions — relevant to the fast-remote 30-second
  background grace and to the app-restart report (#105). The emulator reproduces the
  *JavaScript* lifecycle faithfully, which is what
  `evidence/lifecycle/reproduce.mjs` already models; it does not reproduce Android
  killing the process under real memory pressure.
- Touch ergonomics and safe-area insets.

## What the emulator cannot settle

Vendor-firmware and host-OS behavior stays on the physical script in
`docs/DEVICE-TEST-SCRIPT.md`: Sony Android 8 launcher switching, the TCL Android 14
pairing service, and macOS removable-volume (TCC) prompts, which are a host-side issue
with nothing to do with the device.

The emulator does let you rehearse the mechanics of desktop steps D5–D9 before you
touch a Shield.
