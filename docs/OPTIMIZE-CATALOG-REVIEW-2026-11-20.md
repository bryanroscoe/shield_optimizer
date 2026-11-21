# Daily Optimize catalog review — 2026-11-20

2 assessed, 1 accepted, 1 held; 1,601 active candidates remain queued.
Base main: `24165177fa740fbb470a5531d08f937fbec220ff`. Source pin:
`8672e68e8a24c1258e4662cd11daa4ed13795dad`. Prior decisions are unchanged.

The implementation agent and exactly one independent inherited-model read-only
reviewer researched the same identities, purposes, dependencies, consequences,
recovery limits and applicability. Both agree on the following dispositions.

## Xiaomi TV+ accepted as optional Caution

The [exact Xiaomi developer listing](https://play.google.com/store/apps/details?id=com.mitv.tvhome.mitvplus)
identifies `com.mitv.tvhome.mitvplus` and describes internet live channels, guide,
history and recommendations. [Xiaomi support](https://www.mi.com/sa-en/support/article/KA-124775/)
corroborates the optional viewing function and channel navigation. Disabling loses
these functions and may stop its dedicated launch shortcut from opening the app.
A TV+ label in remote-pairing instructions does not establish this app as the
pairing implementation; no general remote/input/tuner safety claim follows.

There are 23 active, 1 commented and 1 note profiles. Pinned observations include:

- [Mi Box S 2nd gen line 74](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/xiaomi-mi-box-s-2nd-gen/debloat.txt#L74): ad-funded channels disabled, Google TV 11 source context.
- [MOOQ0 line 141](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/xiaomi-mitv-mooq0/debloat.txt#L141): optional commented choice, croods Android TV 10/11 context.
- [MOEU0 lines 26–29 and 76](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/xiaomi-mitv-moeu0/debloat.txt#L26): active list conflicts with re-enabled-by-preference note; playback via tile and TV+ key mapping reported on Google TV 14. Preserve both observations.

The shared common list describes only this installed exact application. It adds
no family/profile matching or compatibility certification. Medium/Caution,
Disable suggestion, review enabled, both defaults false and initially Keep;
explicit Uninstall remains available under the existing third-party/store rules.
Re-enable works only while installed; app state, history, channel/service/region
availability are not guaranteed restored. No Safe or default-action promotion.

## Smart Connect held

`com.google.android.apps.nbu.smartconnect.tv`: 28 active, 1 commented, 12 note
profiles. [Pinned AOSP TV Settings](https://android.googlesource.com/platform/packages/apps/TvSettings/+/47b9cd0ba958b1de2e489a4a3690731291032cb7/Settings/res/xml/network.xml)
references its data-saver and data-alert providers. [Google's historical announcement](https://blog.google/intl/en-in/products/platforms/watch-up-to-3x-more-with-data-saver-on/)
describes data saving, alerts, hotspot setup and offline casting, without mapping
all functions to exact package components on every firmware. Network/Settings
callers, configured state recovery and enforceable scope remain unresolved.
Hold until 2026-12-20 or material new evidence; no Caution action is added.

Original observations remain distinct: [G03 line 231](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-g03-google-tv/debloat.txt#L231)
is commented by choice (includes Kuwait AR11 TCL65P735 notes);
[Hisense A4 Android 11 line 24](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/hisense-hismarttv-a4-android11/debloat.txt#L24)
says absent after update; [50EP680 line 15](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-50ep680/debloat.txt#L15)
is factory-disabled; [G06 US line 57](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-g06-us-google-tv/debloat.txt#L57)
calls it data-saver VPN. None proves universal disable/recovery safety.

## Validation scope

Shared JSON, synchronized demo catalog and substantive reviewed-entry fixtures
cover the accepted app. Existing loader/planner/safety/identity/confirmation
rules remain authoritative. Queue/provenance checks and frontend/fixture checks
are recorded in PR evidence with exact tested head. New-entry captures focus on
Xiaomi TV+ and are simulated, not hardware evidence. Unaffected evidence is reused
only for unchanged behavior. No native packaging, release or device control.

Next batch: seek further optional installed-app gaps with primary functional
evidence. Skip unchanged held decisions until due or new material evidence.
