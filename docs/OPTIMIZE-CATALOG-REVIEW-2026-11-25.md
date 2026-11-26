# Optimize catalog review — 2026-11-25

Assessed two previously unreviewed exact IDs from the pinned CleanYourTV commit
`8672e68e8a24c1258e4662cd11daa4ed13795dad`. Accepted 0; held 2.
The implementation agent and exactly one independent inherited-model read-only
reviewer separately researched identity, purpose, dependencies, disable/removal
consequences, recovery limits and enforceable applicability. Both agree HOLD.
Frequency prioritized investigation; it is not safety evidence.

## Sony Customer Support: hold

`com.sony.dtv.customersupport`: 30 active profiles, 1 commented, 0 note profiles.
[Sony's update instructions](https://www.sony.com/electronics/support/audio-video/articles/00028577)
route some Android TV update checks through Help and Status & Diagnostics.
[Sony's diagnostics instructions](https://www.sony.com/electronics/support/televisions-projectors-lcd-tvs/xbr-65x900b/articles/00090165)
describe Help-based self-diagnostics, with model/software differences.
Neither establishes the exact package's components, callers, tile-only scope
or disable/recovery contract. These feature documents do not prove that disabling
this ID blocks all updates or diagnostics; the missing dependency evidence is
the reason to hold it.

Pinned observations include an active Help tile on
[KD-43X80L:41](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/sony-kd-43x80l/debloat.txt#L41),
a commented customer-support choice on
[KD-65X82L:106](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/sony-kd-65x82l/debloat.txt#L106),
and third-unit context on
[VH21:83](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/sony-bravia-4k-vh21/debloat.txt#L83).
KD-43X80L describes an EU VH22 Android 12 set; KD-65X82L describes an India
VH22 Google TV 12 set. A family name cannot enforce these differences.
Recovery of all help, diagnostics and caller state remains unverified.

## Xiaomi TV Manager: hold

`com.xiaomi.mitv.tvmanager`: 29 active profiles, 0 commented, 2 note profiles.
[Xiaomi's FAQ](https://www.mi.com/in/mi-led-smart-tv-4a-43/faq/)
identifies TV Manager as a bundled application.
[Xiaomi's troubleshooting guide](https://cdn.cnbj1.fds.api.mi-img.com/ics-resources/articles/648fb13417f0c706451d45a6.html)
describes cleanup/uninstall and a model-limited minimal-system mode with
XiaoAI and casting configuration. These do not identify exact package
components or prove safe disable/recovery across firmware.

The pinned
[MFFU0:59–61 warning](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/xiaomi-mitv-mffu0/debloat.txt#L59-L61)
explicitly says to keep this ID: XiaoAI app-opening uses its
`com.xiaomi.aicr.action.provider`; the recorded failure is missing-provider
resolution after an earlier disable. This is an upstream observation, not
independently reproduced hardware evidence.
[ASTR0:51–53](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/xiaomi-mitv-astr0/debloat.txt#L51-L53)
still disables it by owner choice with a re-enable warning;
[MFTP0:58–59](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/redmi-mitv-mftp0/debloat.txt#L58-L59)
keeps it with voice remote. The MOOQ0:109 cleaner/boost/data-usage label does
not resolve these conflicts. Firmware-specific voice/system callers, recovery
of configured state and enforceable scope remain unresolved.

## Outcome and validation scope

Both holds are recorded until 2026-12-25 or material new primary evidence.
No app-list, safety/planner, fixture, UI, demo, gallery, license or upstream-data
changes are needed. Existing accepted entries were not re-reviewed. All imported
data remains unreviewed/non-actionable; no model/firmware compatibility claim.

Queue tests (4), source tests (15), pinned provenance (427 profiles), progress
ledger/count audit and `git diff --check` passed. The queue has 1,591 active
candidates remaining, 57 held identities and 88 already curated. Hosted CI is
required on the pushed revision before merge. No native packaging, hardware,
phone, billing, deployment or release work was performed. Next batch: other
unreviewed installed-app gaps; skip these holds until due or new evidence.
