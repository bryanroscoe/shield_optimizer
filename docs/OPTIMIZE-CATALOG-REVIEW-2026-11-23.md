# Daily Optimize catalog review — 2026-11-23

2 assessed, 0 accepted, 2 held; 1,595 active candidates remain queued.
Base main: `414696212993fd493cc69b27c7e8c667f0617fb3`. Source pin:
`8672e68e8a24c1258e4662cd11daa4ed13795dad`. Prior decisions are unchanged.

## Google TV Assistant held

`com.google.android.tv.assistant`: 33 active, 6 commented and 9 note profiles.
Google's [Android TV Assistant instructions](https://support.google.com/androidtv/answer/9440564)
and [Google TV feature documentation](https://support.google.com/googletv/answer/10070077)
describe remote microphone activation, search and control functionality. They do
not identify this exact package or its components and callers. The exact Play
URL could not be read; it is not evidence that an app is absent or discontinued.

Pinned observations retain meaningful conflicts:

- [Mars 3 Air lines 21–26 and 79–81](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/anker-nebula-mars-3-air/debloat.txt#L21): Google TV 14, firmware re-enables Assistant at boot; owner keeps microphone functionality. The exact ID is commented and described as the Assistant LED service, distinct from `com.google.android.katniss`.
- [TCL 55C725 line 18](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-55c725/debloat.txt#L18) and [G06 US lines 21–23](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-g06-us-google-tv/debloat.txt#L21): explicit keep contexts for Assistant LED/voice ecosystem. These cannot be averaged away using disable frequency.

Hold: exact identity, firmware callers, disable consequences and full voice/LED
recovery remain unresolved. Generic Assistant documentation cannot establish
that disabling this particular package only removes an optional feature.
Current family/common scope cannot enforce firmware applicability.

## Xiaomi statistics held

`com.xiaomi.statistic`: 33 active, 0 commented and 0 note profiles.
[Xiaomi's general privacy policy](https://privacy.mi.com/all/en_US/)
describes usage/diagnostic processing with product-specific variation;
[Mi Smart Hub's sharing statement](https://privacy.mi.com/XiaomiSmartHub-share/en_US/)
describes OneTrack SDK analytics. Neither binds this exact TV package to an
implementation, component graph or safe-disable/recovery contract. OneTrack
must not be treated as an alias for this package.

Pinned [Mi Box S 2nd gen lines 64–68](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/xiaomi-mi-box-s-2nd-gen/debloat.txt#L64)
(Google TV 11) calls it a usage stats uploader, while
[MOOQ0 lines 94–98](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/xiaomi-mitv-mooq0/debloat.txt#L94)
(Android TV 10) groups it with telemetry. [ANSP0 line 51](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/xiaomi-mitv-ansp0/debloat.txt#L51)
is a China-firmware observation. These are original user observations, not
vendor certification of telemetry-only dependencies or recovery. No primary
exact-ID source was located in the bounded search. Hold pending exact component,
caller, consequence and recovery evidence with enforceable applicability.

## Review and validation scope

The implementation agent independently researched both candidates. Exactly one
independent inherited-model reviewer independently checked the same identities, purposes,
dependencies, consequences, recovery and applicability, plus the final diff. Both agree HOLD; no actionable findings remain.
Holds are reconsidered on 2026-12-23 or material new evidence; the date never
implies acceptance.

Only research bookkeeping and this note change. No actionable JSON entries,
defaults, demo data, planner/safety authorities or UI behavior change. Galleries
and fixture captures remain unaffected; queue/source tests, source provenance
and a precise ledger-delta check provide the relevant evidence. No native build,
hardware control, release or deployment. Existing safety and identity gates,
source notices and licenses remain unchanged.

Next useful batch: investigate other unreviewed installed-app gaps with exact
primary evidence; skip these unchanged holds until due or new evidence arrives.
