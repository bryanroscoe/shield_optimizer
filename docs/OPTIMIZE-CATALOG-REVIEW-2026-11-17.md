# Daily Optimize catalog review — 2026-11-17

**2 assessed, 0 accepted, 2 held.** Base main is
`2b0ca4fb3d958fefc5a201e589026f6f6c852fae`. Prior decisions remain unchanged.
The imported source stays pinned to
`8672e68e8a24c1258e4662cd11daa4ed13795dad`.

## Health Connect backup and controller

The [AOSP Android 14 backup manifest](https://android.googlesource.com/platform/packages/modules/HealthFitness/+/refs/tags/android-14.0.0_r1/backuprestore/AndroidManifest.xml)
binds `com.android.health.connect.backuprestore` to a system-process full-backup
agent using `HEALTH_CONNECT_BACKUP_INTER_AGENT`. Disabling it may interrupt
health backup/restore work. Absence of callers or valuable data on TV firmware
has not been established, and re-enabling cannot promise recovery of missed
backup work.

The [AOSP Android 14 controller manifest](https://android.googlesource.com/platform/packages/modules/HealthFitness/+/refs/tags/android-14.0.0_r1/apk/AndroidManifest.xml)
binds `com.android.healthconnect.controller` to permission request/management,
data management, migration, Settings routes and a Safety Center receiver.
Disabling may remove access to these controls. The [developer architecture guide](https://developer.android.com/health-and-fitness/health-connect/architecture)
describes client integration and permission/data management; it does not certify
TV removal safety. Neither exact ID is interchangeable with a Google-prefixed
variant. Firmware callers and lossless recovery remain unverified.

Both are **held until 2026-12-17 or material new evidence**. No common/family
scope can currently enforce the needed absence of dependencies. Neither enters
an action list; uncertainty does not justify Caution or a default action.

## Original observations and conflicts

Backup has **31 active / 3 commented / 0 prose** profile observations;
controller has **31 active / 4 commented / 0 prose**. Frequency is triage only.
The full pinned snapshot retains the original context and all observations:

- [Acer R3:91–92](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/acer-ar3-r3/debloat.txt#L91)
  actively disables both as unneeded on that TV. Context is SCBC/Realtek R3,
  Android TV 14, India; this is an owner assessment, not dependency evidence.
- [Anker Nebula D2426:68–69](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/anker-nebula-d2426/debloat.txt#L68)
  leaves both commented. Context is Capsule III Pro, SDMC/MediaTek, Android
  TV 14, Brazil, with firmware re-enabling some unrelated packages at boot.
- [Kalley R10G:67–68](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/kalley-r10g/debloat.txt#L67)
  leaves both commented as untested across reboots. Context is MOKA/Realtek,
  Google TV 14, Latin America, with per-unit boot and firmware caveats.
- [TCL Q7C:201–202](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-q7c-google-tv/debloat.txt#L201)
  leaves both commented. Context is European G08/MediaTek Google TV 14,
  AU02 with additional per-unit firmware variations in the source notes.
- [Xiaomi TV A Pro 2026:85](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/xiaomi-tv-a-pro-2026/debloat.txt#L85)
  also leaves the controller commented (MiTV-MZTU0/MediaTek, Google TV 14).

These are source observations, not tests by this batch. No profile is matched
to a connected device and no compatibility or performance claim follows.

## Review and validation scope

Implementation research and exactly one independent read-only reviewer cover
the same two identities, primary source, original conflicts, dependency effects,
recovery limits and applicability. Both recommend holding both packages. This
is source/static review, not formal GitHub approval or hardware acceptance.

Queue tests and provenance checks validate this research-only delta; a delta
audit checks that prior decisions and all product authorities remain unchanged.
Queue movement is **1,609 → 1,607**, held **40 → 42**, curated remains **87**.
No product, demo, fixture, gallery, source pin or license changes, so prior UI
and safety evidence is unaffected; no fresh screenshots or native build apply.
Hosted checks and exact tested revision are recorded in the PR.

Next useful batch: investigate installed optional app gaps farther down the
queue; high-frequency Assistant/Sony service leads remain unreviewed and need
exact vendor dependency evidence before any action coverage.
