# Optimize catalog review — 2026-11-27

Assessed two previously unreviewed exact IDs from CleanYourTV snapshot
`8672e68e8a24c1258e4662cd11daa4ed13795dad`. Outcome: accepted 0,
held 2. Frequency prioritizes research, never establishes safety.

## Sony bug reporting: hold

`com.sony.dtv.sonybugreportsys`: 27 active, 2 commented, 0 note profiles.
[Sony setup documentation](https://www.sony.com/electronics/support/televisions-projectors-lcd-tvs-android-/articles/00122502)
describes Google's diagnostic-data choice; it does not identify this Sony
package's components, callers, permissions or recovery contract. It cannot
establish a telemetry-only purpose or optional removal.

Pinned [VH2:172](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/sony-bravia-4k-vh2/debloat.txt#L172)
is active, while [UR3:71](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/sony-bravia-4k-ur3/debloat.txt#L71)
and [KD-65X82L:105](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/sony-kd-65x82l/debloat.txt#L105)
are commented choices. The latter records a different Australian unit's choice,
not universal applicability.

New material evidence after the pin: [upstream issue 3608](https://github.com/Gathin23/cleanyourtv/issues/3608)
reports USB Ethernet failure after reboot with this exact ID disabled alone on
Android 12 build `BRAVIA_VH2_M_PA-user 12 STT2.230505.001.S136 682241`.
The reporter also isolated `com.sonyericsson.idd.agent` and says both enabled
restores Ethernet. The maintainer reports changing the VH2 list to keep both
and recovery by re-enabling both. This is an upstream single-TV observation,
not independently reproduced hardware evidence or a vendor dependency contract.
It contradicts a telemetry-only assumption. General firmware/network dependency
scope and full recovery remain unresolved; no family-wide action is justified.
The pinned imported snapshot is preserved unchanged.

## TCL auto-install configuration: hold

`android.autoinstalls.config.tcl.device`: 26 active, 1 commented, 3 note profiles.
[TCL setup](https://support.tcl.com/androidtv-setup-configuration/614297) and
[Play installation instructions](https://support.tcl.com/en_CA/how-to-download-and-install-new-apps-to-your-tcl-android-tv-963)
describe user setup and app installation, but do not identify the exact config
payload, exported receivers, installer/setup callers, disable consequences or
recovery of missed setup work. These general instructions cannot certify this
ID as disposable after setup.

Pinned [32S5400A:97](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-32s5400a/debloat.txt#L97)
is a commented second-unit choice on V8-T221T01-LF1V121, separate from the older
V8-T221T05-LF1V005 unit. [55C715:30–32](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-55c715/debloat.txt#L30-L32)
records the 55C718 staying enabled because the input-driver check could not read
a permission block, despite the active line at 57. [65P615:18–20](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-65p615/debloat.txt#L18-L20)
also leaves it enabled. [UnionTV:83–86](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-uniontv-latin-america/debloat.txt#L83-L86)
distinguishes a disabled first unit from a skipped 32S6500S; line 116 mentions
an override. A missing permission block proves neither an input dependency nor
its absence. We preserve the uncertainty and do not adopt that override.

## Review and validation scope

The implementation review covers both exact IDs, original observations,
primary vendor evidence, dependencies, disable/removal consequences, recovery
and enforceable applicability. Exactly one inherited-model read-only reviewer
independently researched the same scope and agrees HOLD for both. Final ledger and batch-note diff
review completed with no actionable findings. Its separate primary sources were Sony
[Help diagnostics](https://helpguide.sony.net/tv/iusltn1/v1/en-003/08-01_01.html),
[Sony Usage & Diagnostics](https://helpguide.sony.net/tv/iaepm1/v1/en/3_6_1_device_preferences.html),
[TCL setup](https://support.tcl.com/set-up-your-tcl-google-tv-device-remote) and
[TCL settings](https://support.tcl.com/en_US/tcl-google-tv-settings-and-user-manual-guide).
These likewise do not establish the exact package contracts.
Both decisions will remain held until 2026-12-27 or new material evidence.

No app-list, safety/planner, fixture, frontend, demo or gallery behavior changes;
screenshots/video and gallery regeneration are inapplicable. Existing accepted
entries, MIT notices, source pins and proprietary license remain unchanged.
No native build, packaging, hardware control, phone, billing, deployment or release.
Queue tests (4), source tests (15), provenance (427 profiles) and ledger/diff
audit passed. The queue has 1,589 active candidates, 59 held identities and 88
already curated. Substantive hosted CI is required before merge. Next: other unreviewed
installed-app gaps, skipping unchanged holds until due or new evidence.
