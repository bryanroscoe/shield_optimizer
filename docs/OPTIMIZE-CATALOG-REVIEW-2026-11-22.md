# Daily Optimize catalog review — 2026-11-22

2 assessed, 0 accepted, 2 held; 1,597 active candidates remain queued.
Base main: `60b078b27c65bfcd351e2e892c5b313645633284`. Source pin:
`8672e68e8a24c1258e4662cd11daa4ed13795dad`. Prior decisions are unchanged.

The implementation agent and exactly one independent inherited-model read-only
reviewer independently checked both exact identities, purposes, dependencies,
disable/removal consequences, recovery limits and applicability. Both agree to
hold until 2026-12-22 or material new primary evidence.

## Wallpaper backup held

`com.android.wallpaperbackup`: 27 active, 3 commented and 2 note profiles.
The pinned [AOSP agent source](https://android.googlesource.com/platform/frameworks/base/+/f8f72c3389edd05347da9bf210fb0c142e1e76ef/packages/WallpaperBackup/src/com/android/wallpaperbackup/WallpaperBackupAgent.java)
implements wallpaper and metadata backup/restore through WallpaperManager,
including system/lock images and live-wallpaper component restoration. This is
backup infrastructure. Disabling can interrupt that functionality; re-enabling
does not prove recovery of missed backups, lost metadata or interrupted restore.
Target-TV dependencies and harmless-disable applicability remain unverified.

Pinned original observations, all under `examples/*/debloat.txt`:

- [SHIELD 2019 lines 103–110](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/nvidia-shield-tv-2019/debloat.txt#L103): active choice under idle stubs, with a no-RAM-savings/reboot observation; Android TV 11. It is not backup/restore acceptance.
- [MOOR2 lines 147–150](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/xiaomi-mitv-moor2/debloat.txt#L147): commented opt-in; source says none runs at idle and there is little to gain, Google TV 11.
- [RCA Lasalle line 64](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/rca-2k-android-tv-lasalle/debloat.txt#L64) and [Q7C lines 111 and 205](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-q7c-google-tv/debloat.txt#L111): commented choices on Android TV 11 and Google TV 14 respectively.
- [TCL 75P8K lines 35–36](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-75p8k/debloat.txt#L35) and [Tornado lines 64–68](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tornado-65ua1400e/debloat.txt#L64): tool input-driver inspection could not read a permission block; Tornado subsequently used an override after HDMI/Live TV checks. Those observations cannot replace package dependency or backup recovery evidence; Google TV 12 and Android TV 11 respectively.

## Dynamic System Updates held

`com.android.dynsystem`: 19 active, 4 commented and 0 note profiles.
The pinned [AOSP manifest](https://android.googlesource.com/platform/frameworks/base/+/fed109a5eeab5efafc42800d7a7fae8c5bdab438/packages/DynamicSystemInstallationService/AndroidManifest.xml)
identifies system-UID code with DSU-management/reboot permissions, a boot receiver,
and installation-permission-gated service and verification activity.
[Official DSU documentation](https://source.android.com/docs/core/ota/dynamic-system-updates)
names the exact verification activity and describes OEM-app/system-API frontends.
Disabling can prevent those DSU entry points from working. OEM callers and
recovery of in-flight image state remain unresolved for target TV firmware.
This evidence does not establish an ordinary optional app or OTA-removal safety.

Pinned observations include active [SHIELD 2019 line 104](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/nvidia-shield-tv-2019/debloat.txt#L104)
under the same idle-stub note, versus commented [Panasonic 3MUP line 49](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/panasonic-4ktv-3mup/debloat.txt#L49)
(Google TV 14), [Q7C line 103](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-q7c-google-tv/debloat.txt#L103)
(Google TV 14, fifth EU/UK unit OTA from Android 12), [MOOR2 line 148](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/xiaomi-mitv-moor2/debloat.txt#L148)
(Google TV 11, opt-in/no idle benefit), and [A Pro 2026 line 86](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/xiaomi-tv-a-pro-2026/debloat.txt#L86)
(Google TV 14). Frequency and model labels establish neither safety nor compatibility.

## Validation scope

Only research bookkeeping and this note change. No actionable JSON entries,
defaults, demo data, planner/safety authorities or UI behavior change; existing
galleries and regression fixtures are unaffected. Queue tests, source tests,
provenance verification and exact ledger-delta checks are recorded in PR evidence.
No native build, packaging, hardware control, release or deployment.

Next batch: other optional installed-app gaps with exact primary evidence;
skip these unchanged holds until due or material new evidence. Existing protected,
last-HOME, setup/network, user-0, confirmation and identity gates remain unchanged.
