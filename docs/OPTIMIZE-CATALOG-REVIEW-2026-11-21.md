# Daily Optimize catalog review — 2026-11-21

2 assessed, 0 accepted, 2 held; 1,599 active candidates remain queued.
Base main: `32fa10c65df5467acdd3ef3c7ec5bb1fd8ccb11a`. Source pin:
`8672e68e8a24c1258e4662cd11daa4ed13795dad`. Prior decisions are unchanged.

The implementation agent and exactly one independent inherited-model read-only
reviewer independently researched the same identities, purposes, dependencies,
disable/removal consequences, recovery limits and applicability. Both agree to
hold both packages until 2026-12-21 or material new primary evidence.

## Gallery held

`com.mitv.gallery`: 22 active, 2 commented and 4 note profiles.
[Xiaomi USB playback documentation](https://www.mi.com/sg/support/article/KA-12981/)
describes HD Player; [Xiaomi Q2 support](https://www.mi.com/global/support/faq/details/KA-501304/)
describes USB photo/media playback. Neither establishes the exact package's
components, callers, recovery behavior or applicability across firmware.

Pinned original observations include:

- [MSSP1 lines 64–67](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/xiaomi-mitv-mssp1/debloat.txt#L64): Gallery doubles as Photos screensaver; keep enabled unless reassigning. Android TV 9, PI.5925 source context.
- [AXFU0 lines 18–19](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/xiaomi-mitv-axfu0/debloat.txt#L18): keep Gallery/video player for USB because Media Explorer opens through them, conflicting with active Gallery line 62. Google TV 14 source context.
- [AXSO1 line 51](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/xiaomi-mitv-axso1/debloat.txt#L51) and [MOOR2 line 166](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/xiaomi-mitv-moor2/debloat.txt#L166): commented optional choices, Android TV 9 and Google TV 11 respectively. MOOR2's optional second round uses replacement Home and a third-party file manager on a specific firmware.
- [MOOQ0 line 112](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/xiaomi-mitv-mooq0/debloat.txt#L112): MiCloud/Xiaomi market SDK observation, Android TV 10/11 context.

Media and screensaver dependencies remain unresolved. A replacement media browser
or a re-enable command does not establish restoration of selected screensaver,
media handling or app configuration. No universal USB-photo fallback is claimed.

## MiChannel held

`com.mitv.tvhome.michannel`: 23 active, 0 commented and 1 note profiles.
[Xiaomi PatchWall documentation](https://www.mi.com/global/support/article/KA-124771/)
describes content aggregation, recommendations and search. It does not map this
exact package to components or establish launcher/content-provider callers,
disable effects, configuration recovery or enforceable firmware scope.

Pinned observations include [Mi Box S 2nd-gen line 73](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/xiaomi-mi-box-s-2nd-gen/debloat.txt#L73)
(content rows, Google TV 11), [MOOQ0 line 103](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/xiaomi-mitv-mooq0/debloat.txt#L103)
(recommendation channels/Firebase/GMS), [AFMU1 line 82](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/xiaomi-mitv-afmu1/debloat.txt#L82)
(one unit only, firmware-disabled on the other, Google TV 14) and
[MSSP3 Android 9 line 52](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/xiaomi-mitv-mssp3-android9/debloat.txt#L52)
(absent on older PI.4663 Italy firmware). Counts and observations are triage
information, not safety votes or compatibility certification.

## Validation scope

Only research bookkeeping and this note change. No actionable entries, demo data,
planner/safety logic or UI behavior changes; existing galleries and fixtures are
unaffected. Queue tests, source tests, provenance verification and exact ledger
delta checks are recorded with the tested head in PR evidence. No native builds,
packaging, hardware control, release or deployment.

Next batch: investigate other optional installed-app gaps with exact primary
identity/dependency/recovery evidence; skip these unchanged holds until due or
material new evidence. Both holds preserve all existing action and identity gates.
