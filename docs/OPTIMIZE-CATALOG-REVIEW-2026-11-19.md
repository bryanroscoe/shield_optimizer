# Daily Optimize catalog review — 2026-11-19

**2 assessed, 0 accepted, 2 held; 1,603 active candidates remain queued.**
Base main: `d62f4a4df6c0db1c2cf9993561365c820e9661ea`. Source pin:
`8672e68e8a24c1258e4662cd11daa4ed13795dad`. Prior decisions are unchanged.

The implementation agent and exactly one independent read-only reviewer researched
the same two exact IDs: identity, purpose, dependencies, disable consequences,
recovery limits and applicability. Both agree to hold them until 2026-12-19 or
material new evidence. Neither frequency nor a source disable choice proves safety.

## Sony demo support

`com.sony.dtv.demosupport`: 33 active profiles, no commented or note profiles.
Sony's [demo-mode instructions](https://www.sony.com/electronics/support/televisions-projectors-lcd-tvs-android-/k-75s30c/articles/00050389)
describe turning off Demo and Picture reset settings through version-dependent
menus. They do not identify this package's components or prove that disabling
the package is equivalent to those controls. Exact-ID primary identity, callers,
firmware dependencies and recovery remain unresolved. The source's retail-demo
labels cannot establish those facts. Hold rather than infer optional Caution.

Original examples include [UR3 line 54](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/sony-bravia-4k-ur3/debloat.txt#L54)
(Android TV 10, EU/TW/US observations) and [VH21 line 86](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/sony-bravia-4k-vh21/debloat.txt#L86)
(Google TV 12, Australia/India plus a New Zealand observation). These are source
contexts, not app compatibility certifications or enforceable family scope.

## BRAVIA Help

`com.sony.dtv.smarthelp`: 33 active, 2 commented, 3 note profiles.
The [Sony developer listing](https://play.google.com/store/apps/details?id=com.sony.dtv.smarthelp)
identifies BRAVIA Help and describes troubleshooting, TV instructions and support
information. This establishes function beyond its display name. Sony's
[self-diagnostics instructions](https://www.sony.com/electronics/support/articles/00090165)
place diagnostics behind HELP/Help, with model/software differences, but do not
map this exact package's callers or firmware-dependent recovery.

The pinned observations include material consequences and conflicts:

- [VU1 lines 61 and 68](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/sony-bravia-vu1/debloat.txt#L61): HELP becomes inert after disabling; Android TV 11 Realtek context.
- [VU2 line 24](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/sony-bravia-vu2/debloat.txt#L24): kept with uncertainty and HELP/GLOBAL_BUTTON receiver notes; Android 11/14 builds.
- [UR2 line 54](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/sony-bravia-4k-ur2/debloat.txt#L54): commented optional, also a search provider.
- [KD-55XE8596 lines 79–80](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/sony-kd-55xe8596/debloat.txt#L79): deliberately kept as a feature on an older Android TV 9 build.
- [VH21-TW line 21](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/sony-bravia-4k-vh21-tw/debloat.txt#L21): factory-disabled state should be left alone by restore; [VH21 line 80](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/sony-bravia-4k-vh21/debloat.txt#L80) actively disables it on another unit.

Disabling would stop the Help app and may remove remote/search/diagnostic access.
The exact dependency graph and lossless recovery are unverified. Re-enabling a
present package does not prove configured callers or firmware integrations recover;
enabling a factory-disabled package is not a faithful restore. Hold pending scope
that the canonical loader can enforce.

## Validation scope

Only the research ledger and this note change. All source observations, MIT
notices, license, runtime lists, planner/safety authorities and defaults remain
unchanged. Queue tests: 4 passed; source-catalog tests: 15 passed; provenance check:
427 pinned informational profiles verified. Exact ledger delta is audited before
commit. Loader/planner/frontend/gallery evidence is unaffected; no new-entry
fixture capture or gallery regeneration applies. Hosted substantive CI remains
the merge gate. No native packaging, hardware, release or deployment is claimed.

Next batch should seek an optional installed-app gap with primary exact-ID and
dependency evidence; unchanged holds remain excluded until due or new evidence.
