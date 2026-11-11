# ATV Optimizer mobile backlog

The ordered queue of remaining mobile work. [`HANDOFF.md`](HANDOFF.md) describes how the app works
today; this file lists only what is left. Nothing in `v2/mobile` has been verified on a phone or TV
in its current form, so most of P0 is physical-device work. Device sessions need the owner's
go-ahead; don't infer permission from a phone being powered on.

## P0 — physical device gates

1. **Device pass of the current build** on the Pixel against a Shield and a Google TV. One session
   should cover, in order: confirm the installed build, the #105 capture (item 3), then
   [#93](https://github.com/bryanroscoe/shield_optimizer/issues/93) saved-TV discovery and
   reconnect: a legacy `:5555` Shield without mDNS shows one Connected row; a Google TV whose port
   rotates shows the other-port state with no duplicate; an address change never carries a cached
   name or identity to a different TV; reconnect results are honest. Then the stability matrix:
   - put the TV to sleep mid-session: the app flips to "Reconnecting…" and then the banner within
     about 45 s instead of staying green; turn off the phone's Wi-Fi or switch it to another network:
     the same happens within about 15 s at most (the network-change probe), and a switch to a
     network that reaches the TV reconnects silently;
   - discovery on a phone below Android 14 lists every TV and service type on the network (the
     serial resolve queue), and on Android 12 or older with the multicast lock;
   - a one-TV launch names the TV and can be cancelled; a two-TV launch waits for a choice;
   - Disconnect, kill the app, relaunch: it must not redial;
   - Optimize apply locks the tabs, and Cancel stops after the current item;
   - Emergency recovery on a TV with a few disabled apps;
   - Back from Settings → Devices → back → back reaches the Dashboard, and one more press leaves
     the app; Back with a dialog or sheet open (Dashboard Reboot, an Apps sheet, a help sheet)
     closes only that, and is refused while an apply runs (browser-tested only);
   - the screenshot preview renders; Tweaks shows "Unset" rather than OFF for a never-written
     setting;
   - Settings › Licensing shows "Debug build test key" after activating the dev key, and
     Settings › About opens the third-party notices.

   And the UI checks that browser emulation cannot settle: system bars, cutout, rotation, gesture
   and three-button navigation, and the keyboard opening and closing (no overlap, no doubled
   padding); Android font scaling and pinch zoom on the Diagnostics header, display-scaling
   presets and reports; touch scrolling and focus in app sheets and nested dialogs; settings
   readback on real vendor firmware (Private DNS, background-process cap, screensaver names,
   density-only display overrides); RAM, storage and thermal readings against the TV's own
   figures; the setup-helper disable rollback; and stock-launcher protection on a Fire TV build
   ([#205](https://github.com/bryanroscoe/shield_optimizer/issues/205)).
2. **Rest of the device pass of the 2026-11-05 parity and bug-bash branch** (PR #300). On
   2026-11-05 a Pixel 3 (Android 12) against the Living Room Shield confirmed: Cast names in scan
   rows; Back from Add a TV keeps the live TV; Cancel during the approval wait; a failed Retry with
   Wi-Fi off keeps the banner and name, and Wi-Fi back reconnects silently; Dashboard figures match
   the TV; Optimize confirm, apply and undo; app sheets with Report/Research; snapshot save and a
   named delete; notices as text; all tabs at font scale 1.5; no backup allowed; on `56fe49e`
   More/Add a TV → Back → Back reach Dashboard and a font size change keeps the session. Still
   open: two-TV menu, a sleep-triggered banner (the Shield's adb stays up while asleep), Optimize
   cancel and tab lock with several items, Remote D-pad/hold-to-repeat, Launcher, Backups, Files,
   Emergency recovery, a disabled-launcher snapshot restore, the support warning on a Disable
   confirm, and a Google TV.
3. **The app-restart report** ([#105](https://github.com/bryanroscoe/shield_optimizer/issues/105)).
   Follow the physical gate in [`LIFECYCLE-EVIDENCE.md`](LIFECYCLE-EVIDENCE.md): name the exact
   leaving action and correlate it with the Android lifecycle before changing any persistence.
4. **Code pairing end to end** per [`PAIRING-PLAN.md`](PAIRING-PLAN.md). The host binary
   (`vendor/adb_client/examples/device_smoke.rs pair`) passed against the Android TV 14 emulator's
   real adbd on 2026-11-04; still needed: the Pixel's own pairing service, then a Google TV from the app. A wrong code must fail
   cleanly, and a paired TV must then connect with no "Allow debugging" prompt.
5. **Fast-remote Phase 4** per [`FAST-REMOTE-PLAN.md`](FAST-REMOTE-PLAN.md): concurrency on the
   Bedroom Shield; on both Shields hold-to-repeat, live-session background/resume before and after
   30 seconds, sleep/wake and reboot cleanup; and a fresh controlled Living Room latency sample.

## P1 — product work

1. **Android export.** Phase 1 done 2026-11-08: Save a copy (create-document picker) and Share
   (share sheet) for one screenshot or one pulled file, per
   [`EXPORT-DISCOVERABILITY.md`](EXPORT-DISCOVERABILITY.md). Not yet run on a phone: the picker,
   a cancel, a Drive/Downloads destination and a share to a real receiver are the device gates.
   Phase 2 done 2026-11-09: SAF import, which powers Files › Upload to this folder and item 2. Not
   yet run on a phone: picking one and several files, from Downloads and Drive; a dismissed
   picker; a file over 2 GiB; activity recreation while the picker is open; a replace prompt and a
   same-named folder on the TV; an upload to a TV folder that refuses writes. Next: snapshot and
   APK-bundle export once their privacy and format decisions are made; then consider Google Drive
   for complete APK bundles.
2. ~~Install APK / sideload from the phone.~~ Done 2026-11-09: More › Install APK (single or
   split `.apk` sets, native-code ABI check, popular sideloads, install result with the decoded
   hint). Not yet run on a phone: a single APK, a split set, a version downgrade refusal and an
   ABI mismatch on a real TV. It can't show the APK's package id or whether it is already
   installed (needs the manifest decoder desktop uses, a new Rust dependency here).
3. ~~The bug-report bundle (`collect_diagnostics`).~~ Done 2026-11-08: More › Bug report, reviewed
   on screen and copied to the clipboard. A share-sheet step for its text can now reuse item 1's
   bridge (not wired yet); upload stays desktop-only (owner decision).
4. A patch-notes viewer (deferred).
5. ~~**Logs tab.**~~ Done 2026-11-08: More › Device logs, through core `read_device_logs`
   (`WirelessAdb::shell_bounded`, same bounds as desktop). Not yet run on a phone.
6. **Connect-lock narrowing** in `wireless_adb.rs` (deferred from the bug bash). A cancelled
   handshake can block the next connect for up to 30 s, because the whole handshake runs under the
   lifecycle lock. Patch idea: run the handshake outside the lock, then publish the connection
   under it only if the request id is still the latest.
7. **Device findings left from the 2026-11-05 pass** (P2): the TV menu's Disconnect has no
   confirmation; Cancel during the approval wait leaves the socket open until the 30 s deadline,
   so the TV queues one Allow prompt per cancelled try (pairs with the connect-lock item above); a phone
   advertising wireless debugging is listed under "new TVs"; the Send Text dialog sits under the
   keyboard; Apps rows reach the accessibility tree only after the first tap. Android 11 Shields
   can hold a stale "Allow debugging" prompt (`adbd_auth: prompt currently pending, skipping`) and
   never show the phone's; the timeout copy already advises restarting the TV.
8. ~~**Phone-side storage reclaim.**~~ Done 2026-11-08: `file_commands::reclaim_partial_storage`
   runs at startup, before any transfer, and removes staged `.tmp*` downloads and bundle-named
   backup directories with no manifest file. A directory with any manifest is kept.
9. Product decisions for the owner:
   - Let Free users see the Optimize plan read-only. Core `prepare_optimize` is Pro-gated, so the
     flagship screen is a lock card for Free users while the Dashboard already shows them the
     count.
   - `Feature::FileManager` / `Feature::BackupClone` exist in core but gate nothing; wire them or
     delete them.
   - The launcher icon reads slightly top-heavy. A 1.8 px downward optical shift was mocked in
     `icon-optical-comparison.png` (archived;
     `git show e4a9300:v2/mobile/evidence/settings-polish-2026-10-17/icon-optical-comparison.png`);
     the assets are unchanged pending a preference.
10. Commerce: Play billing v1 (one-time `atvopt_pro` plus two same-Pro support tiers, on-device verification) is **built, not yet
   tested with a real Play account**; see [`BILLING-IMPLEMENTATION-PLAN.md`](BILLING-IMPLEMENTATION-PLAN.md)
   "Implemented v1". Remaining: license-tester purchases on a Play track (buy, pending, cancel,
   restore on a second phone, refund, promo code), and later a server verifier/RTDN. Bounded Free
   undo per [`FREE-SETTINGS-UNDO-DESIGN.md`](FREE-SETTINGS-UNDO-DESIGN.md) is not built, so copy
   promises only free app re-enable and emergency recovery.
11. **Disable-vs-Uninstall help on Apps and Optimize.** The suggestion rule (Uninstall only for a
   store app the user installed) and the snapshot's absent apps match desktop as of 2026-11-09,
   but the explanation lives only in More › Risk guide. Desktop has a "?" beside its App List and
   Optimize filters; the phone has no equivalent on those screens yet.

## P2 — release readiness

See [`RELEASE.md`](RELEASE.md) for the full checklist. Open items:

- The upload keystore exists and is backed up in the owner's vault. Still open: Play App Signing
  enrollment, a verified signed AAB/APK and a measured release size. Release automation exists
  (`mobile-play-release.yml`, RELEASE.md "Automated release (CI)") but needs its one-time setup.
- Compare the final Play candidate against the resolved Android runtime baseline and shipped
  notices ([`ANDROID-DEPENDENCIES.md`](ANDROID-DEPENDENCIES.md)); 94 modules and upstream archive
  notices were captured on 2026-11-03. Re-resolve after dependency changes.
- A legal and privacy review. The Cargo notices accept five unmodified MPL-2.0 crates and the
  Unicode-3.0 family. [`DATA-INVENTORY.md`](DATA-INVENTORY.md) is a draft, not a published policy.
- Google Play is the chosen mobile launch channel (owner instruction 2026-11-03), with the
  existing one-time $4.99 US Pro product. Finish Console setup and real Play purchase tests.
- Privacy-policy serving and the in-app link/Support ID are implemented; verify the deployed
  policy and complete Data safety from the final artifact. Listing copy, feature graphic and
  six Play-sized screenshots are reproducible with `npm run store-assets` (`store-listing/`).
- Complete the accessibility and navigation review.
- Many installed apps are uncatalogued. Classify existing review coverage per package, model and
  version from the shared `crates/core/data/app-lists/` before asking for new device reads; unknown
  stays Unknown, and every catalog change is individually sourced and safety-tested.

Whole-app pinch zoom stays enabled by the owner's choice; no viewport restriction is planned.
