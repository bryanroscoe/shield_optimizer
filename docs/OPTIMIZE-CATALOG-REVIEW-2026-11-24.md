# Daily Optimize catalog review — 2026-11-24

2 assessed, 0 accepted, 2 held; 1,593 active candidates remain queued.
Base main: `ed7f8d797a57d519112a267b3030e3a531fc4637`. Source pin:
`8672e68e8a24c1258e4662cd11daa4ed13795dad`. Prior decisions are unchanged.

## TCL user agreement held

`com.tcl.useragreement`: 31 active, 4 commented and 24 note profiles.
[TCL setup instructions](https://support.tcl.com/androidtv-setup-configuration/614297)
describe activation and account/network setup. Its [Smart TV data disclosure](https://www.tcl.com/eu/en/legal/data-act/user-information-on-tv)
describes initial-setup privacy agreement state and model/service variation.
Neither identifies this exact package's components, callers or disable/recovery
contract. No primary exact-ID implementation evidence was found in the bounded
search; that is an evidence gap, not proof that the package is absent.

Original observations conflict:

- [55C725 line 41](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-55c725/debloat.txt#L41): active disable, Google TV on Android TV 11.
- [65C735 line 116](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-65c735/debloat.txt#L116): commented reference, agreement screen during setup on named variants.
- [55C7K UK lines 22–26](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-55c7k-uk/debloat.txt#L22): explicit Keep, Google TV 14, MediaTek G08.
- [G06 US line 48](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-g06-us-google-tv/debloat.txt#L48): active disable on Google TV 11 US firmware.

Hold: setup/consent callers, state recovery and firmware applicability remain
unresolved. Frequency and a setup-only description cannot establish that it is
an optional removable function.

## Xiaomi PatchWall held

`com.mitv.tvhome.atv`: 30 active, 0 commented and 7 note profiles.
The [exact developer listing](https://play.google.com/store/apps/details?id=com.mitv.tvhome.atv)
identifies Xiaomi PatchWall and explicitly describes default-home use in some
countries. [Xiaomi's TV Stick documentation](https://www.mi.com/global/support/article/KA-124771/)
describes content aggregation, recommendations and universal search. This
establishes functionality, but not the complete component/caller graph or
firmware-specific disable and recovery consequences.

Pinned [Mi Box S 2nd gen line 75](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/xiaomi-mi-box-s-2nd-gen/debloat.txt#L75)
(Google TV 11) calls it PatchWall; [MOOQ0 line 102](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/xiaomi-mitv-mooq0/debloat.txt#L102)
(Android TV 10/11) names `.app.PatchWallActivity`.
[MOSR4 line 55](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/xiaomi-mitv-mosr4/debloat.txt#L55)
(Google TV 11) says it is not a HOME candidate on that build; that narrow
observation cannot override the developer's regional home-screen description.
[MSSP1 line 28](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/xiaomi-mitv-mssp1/debloat.txt#L28)
(Android TV 9 PI.5925) disables it with content add-ons after replacing Home.

[AXFR0 lines 15–16](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/xiaomi-mitv-axfr0/debloat.txt#L15)
and [AXFR1 lines 15–18](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/xiaomi-mitv-axfr1/debloat.txt#L15)
(Android TV 11, RTT0.210630.002.1986/485 respectively) report an
InputSourcePopupActivity but alternate Settings/Live TV input paths on those
builds. [AXSO2 line 14](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/xiaomi-mitv-axso2/debloat.txt#L14)
(Android TV 9 PI.6432) reports the remote PatchWall button stops working. These
are narrow source observations, not general dependency or recovery guarantees.

Hold: launcher/input callers, firmware-specific dependencies and full state
recovery remain unresolved. Re-enable while installed is not proof of complete
Home/input/configuration recovery. Existing last-HOME protection remains
mandatory, but does not establish all other callers are optional. Current
common/family scope cannot enforce the required firmware applicability.

## Review and validation scope

The implementation agent independently researched both exact candidates.
Exactly one independent inherited-model read-only reviewer independently researched
the same identities, purposes, dependencies, consequences, recovery and
applicability, then inspected the final ledger/note diff. Both agents agree HOLD;
no actionable findings remain.
Reconsider holds on 2026-12-24 or material new evidence; the date never implies
acceptance.

Only research bookkeeping and this note change. No actionable app-list entry,
default, demo fixture, planner/safety authority or UI behavior changes. Galleries
and fixture captures are unaffected; queue/source tests, provenance and an exact
ledger-delta check are the relevant validation. No native build, hardware
control, deployment, release or compatibility claim. Existing source notices,
licenses, safety and identity gates are unchanged.

Next useful batch: other unreviewed installed-app gaps with exact primary
identity, dependency and recovery evidence; skip these holds until due or new
evidence arrives.
