# Daily Optimize catalog review — 2026-11-16

Two previously unreviewed exact IDs assessed: **2 assessed, 0 accepted, 2 held**.
The previous 77 decisions (39 accepts, 38 holds) remain unchanged. This batch
changes research bookkeeping only; neither package enters an action list.
Base main: `6893dd6`. The imported source pin remains
`8672e68e8a24c1258e4662cd11daa4ed13795dad`.

## Calendar storage provider

The [AOSP Android 14 manifest](https://android.googlesource.com/platform/packages/providers/CalendarProvider/+/refs/tags/android-14.0.0_r1/AndroidManifest.xml)
binds `com.android.providers.calendar` to an exported provider under the
`com.android.calendar` authority, gated by calendar read/write permissions.
It also declares boot, reminder and database-upgrade receivers. The
[Android developer guide](https://developer.android.com/identity/providers/calendar-provider)
describes shared calendar/event/reminder storage used by applications and sync
adapters. It is not merely the calendar app's visible interface.

Disabling the provider would make its data interface unavailable to callers.
Which callers a particular TV firmware or installed app needs, and how those
callers recover, remains unverified. No absence of callers, absence of local
data, lossless recovery or Android TV compatibility is inferred from AOSP.
Hold pending device/firmware caller and recovery evidence; reconsider
**2026-12-16**, or when material primary evidence arrives.

The pinned snapshot has **33 active / 3 commented / 0 prose** profile observations.
[OnePlus Dosa:26–27](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/oneplus-tv-dosa/debloat.txt#L26)
disables sync and storage separately on Android TV 11, India,
`RTM5.220609.299`. In contrast,
[TCL Q7C:126–129](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-q7c-google-tv/debloat.txt#L126)
leaves the provider stack commented out for EU builds. Neither observation
establishes a generally unused provider or a recovery guarantee.

## TCL HOME Dashboard

The [exact developer Play listing](https://play.google.com/store/apps/details?id=com.tcl.dashboard)
identifies TCL Group's shortcut panel for opening apps, managing smart homes
and viewing news. This establishes more than a display-name guess, but does
not establish that firmware callers or remote/settings entry points can lose
it safely. TCL's [settings guide](https://support.tcl.com/tcl-google-tv-walkthrough-settings?kb_language=en_US)
explicitly varies by model, region and software; it does not bind those
settings to this exact package or document removal and recovery.

The pinned snapshot has **34 active / 5 commented / 20 prose** profile
observations, including conflicting keep decisions:

- [TCL 55C725:51](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-55c725/debloat.txt#L51)
  calls it the stock quick-settings panel and requires a remapped gear key.
  That source describes BeyondTV4/Realtek, Android TV 11, Italian build
  `V8-R51MT05-LF1V620`.
- [TCL 32S5400A:23–38,111–112](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-32s5400a/debloat.txt#L23)
  retains it as a precaution for the TCL HOME tile, while reporting working
  remote buttons without it on that unit. This is C06/MediaTek, Android TV 11,
  with distinct AR2101/V005 and V121 firmware observations.
- [TCL Q7C:139–140,187–190](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-q7c-google-tv/debloat.txt#L139)
  keeps the gear-button app on stock-home units, while another unit disabled
  it and retained its separate Input panel. This is G08/MediaTek, Google TV 14,
  European AU02 context with additional per-unit variations.

These are upstream reports, not physical tests by either reviewer. In
particular, a working Input panel does not prove a working settings/gear path.
Hold pending exact firmware dependency, alternative settings access and
recovery evidence that the loader can enforce. Reconsider **2026-12-16**, or
when material primary evidence arrives. Do not generalize to all TCL TVs.

## Review and validation

The implementation research and exactly one independent read-only review
cover both exact identities, original conflicts, dependency effects, recovery
limits and applicability. Neither reviewer controls hardware or supplies a
formal GitHub approval. Independent review agreed with both holds and found
no actionable issues; exact-head CI is recorded in the PR before merge.

Queue tests, provenance checks and an audit of unchanged prior decisions
validate this non-visual delta. Verified queue movement is **1,611 → 1,609**
active candidates and **38 → 40** held; curated coverage remains **87**.
No action catalog, planner, safety authority, demo, fixture, screenshot,
gallery, source pin or license changes. No native build is needed locally.
Hosted CI remains required for the pushed revision.

Next queue leads are `com.google.android.tv.assistant` and
`com.sony.dtv.demosupport`; neither is accepted by this note.

## Windows CI recovery

Windows Clippy 1.100 rejected a pre-existing redundant boolean guard in
`usable_adb` after compiling out its Unix-only branch. The helper now returns
the non-empty regular-file predicate directly on non-Unix systems, and also
requires an executable bit on Unix. The checks and installer behavior are
preserved; no lint suppression or skipped test is introduced. Replacement-head
cross-platform CI must pass, including the Windows tests the failed lint step
prevented from running.

## Automation recovery

PR295 and batches through PR359 were already merged. The old daily job's
manual checkout was removed; its November 12–16 attempts failed before agent
startup. On November 16, the existing daily schedule was rebound through T3
to the current thread and its existing managed checkout, preserving the daily
cadence and two-agent review requirement. No deleted checkout was recreated.
The task manifest records ownership, exact paths and retention review on
**2026-11-23**. Future runs must inspect current main and PR state rather than
continue the merged integration PR.
