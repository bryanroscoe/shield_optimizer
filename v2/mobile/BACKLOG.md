# ATV Optimizer mobile backlog

The ordered queue of remaining mobile work. [`HANDOFF.md`](HANDOFF.md) describes how the app works
today; this file lists only what is left. Nothing in `v2/mobile` has been verified on a phone or TV
in its current form, so most of P0 is physical-device work. Device sessions need the owner's
go-ahead; don't infer permission from a phone being powered on.

## P0 — physical device gates

1. **Device pass of the current build** on the Pixel against a Shield and a Google TV. One session
   should cover, in order: confirm the installed build, the #105 capture (item 2), then
   [#93](https://github.com/bryanroscoe/shield_optimizer/issues/93) saved-TV discovery and
   reconnect: a legacy `:5555` Shield without mDNS shows one Connected row; a Google TV whose port
   rotates shows the other-port state with no duplicate; an address change never carries a cached
   name or identity to a different TV; reconnect results are honest. Then the stability matrix:
   - put the TV to sleep or turn off Wi-Fi mid-session: the app flips to "Reconnecting…" and then
     the banner within about 45 s instead of staying green;
   - a one-TV launch names the TV and can be cancelled; a two-TV launch waits for a choice;
   - Disconnect, kill the app, relaunch: it must not redial;
   - Optimize apply locks the tabs, and Cancel stops after the current item;
   - Emergency recovery on a TV with a few disabled apps;
   - Back from Settings → Devices → back → back reaches the Dashboard, and one more press leaves
     the app;
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
2. **The app-restart report** ([#105](https://github.com/bryanroscoe/shield_optimizer/issues/105)).
   Follow the physical gate in [`LIFECYCLE-EVIDENCE.md`](LIFECYCLE-EVIDENCE.md): name the exact
   leaving action and correlate it with the Android lifecycle before changing any persistence.
3. **Code pairing end to end** per [`PAIRING-PLAN.md`](PAIRING-PLAN.md): a host binary against the
   Pixel's own pairing service first, then a Google TV from the app. A wrong code must fail
   cleanly, and a paired TV must then connect with no "Allow debugging" prompt.
4. **Fast-remote Phase 4** per [`FAST-REMOTE-PLAN.md`](FAST-REMOTE-PLAN.md): concurrency on the
   Bedroom Shield; on both Shields hold-to-repeat, live-session background/resume before and after
   30 seconds, sleep/wake and reboot cleanup; and a fresh controlled Living Room latency sample.

## P1 — product work

1. **Android export.** Screenshots, pulled files and APK bundles land in app-private storage with
   no way out. Start with one explicitly selected screenshot (Save a copy or Share), scoped in
   [`EXPORT-DISCOVERABILITY.md`](EXPORT-DISCOVERABILITY.md); then SAF import, which also unblocks
   phone→TV upload in Files; then consider Google Drive for complete APK bundles.
2. Install APK / sideload from the phone.
3. The bug-report bundle (`collect_diagnostics`) with an explicit share step.
4. A patch-notes viewer.
5. Product decisions for the owner:
   - Let Free users see the Optimize plan read-only. Core `prepare_optimize` is Pro-gated, so the
     flagship screen is a lock card for Free users while the Dashboard already shows them the
     count.
   - `Feature::FileManager` / `Feature::BackupClone` exist in core but gate nothing; wire them or
     delete them.
   - The launcher icon reads slightly top-heavy. A 1.8 px downward optical shift is mocked in
     `evidence/settings-polish-2026-10-17/icon-optical-comparison.png`; the assets are unchanged
     pending a preference.
6. Commerce per [`BILLING-IMPLEMENTATION-PLAN.md`](BILLING-IMPLEMENTATION-PLAN.md) and bounded Free
   undo per [`FREE-SETTINGS-UNDO-DESIGN.md`](FREE-SETTINGS-UNDO-DESIGN.md). Neither is built.

## P2 — release readiness

See [`RELEASE.md`](RELEASE.md) for the full checklist. Open items:

- Create the real keystore (or enroll Play App Signing), add a CI job that builds a signed AAB,
  and measure the first signed release APK.
- Resolve the Android release runtime dependency graph and its notices
  ([`ANDROID-DEPENDENCIES.md`](ANDROID-DEPENDENCIES.md)); the declaration-level list is not enough.
- A legal and privacy review. The Cargo notices accept five unmodified MPL-2.0 crates and the
  Unicode-3.0 family. [`DATA-INVENTORY.md`](DATA-INVENTORY.md) is a draft, not a published policy.
- Choose the store or checkout that issues license keys, and back up
  `~/.atvopt/license-signing-key.prod`.
- Complete the accessibility and navigation review.
- Many installed apps are uncatalogued. Classify existing review coverage per package, model and
  version from the shared `crates/core/data/app-lists/` before asking for new device reads; unknown
  stays Unknown, and every catalog change is individually sourced and safety-tested.

Whole-app pinch zoom stays enabled by the owner's choice; no viewport restriction is planned.
