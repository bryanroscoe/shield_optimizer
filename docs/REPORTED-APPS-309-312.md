# Reports 309–312: existing TIDAL coverage and three optional app reviews

Source review dated 2026-11-06. This separate batch is based on the preserved
seven-app candidate `ac27bed14b309a05e5d3892af2129ab596992c68`, whose main
ancestor is `c3624ecf09a6a8f32f8c7b866bf1dd3fd54ee613`. The older candidate,
its proposed PR wording and eight capture files are not expanded by this batch.
The reports describe installed/enabled packages on one Google TV / Android 14
installation. Their removal-safety assertions are untrusted.

## Coverage reconciliation

#309 `com.aspiro.tidal` already exists on current main as TIDAL, with medium risk
(Caution), Disable, review enabled and both defaults false. It already appears
in the shared reviewed-Optimize fixture and research progress file. This batch
leaves that entire entry and its evidence unchanged and adds no duplicate.
The [exact TIDAL publisher listing](https://play.google.com/store/apps/details?id=com.aspiro.tidal)
confirms the music-client role. The [pinned source occurrence](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/sony-bravia-4k-vh2/debloat.txt#L74-L83)
is a commented owner-choice entry, not independent removal proof. The report's
2.5.0 installed-build behavior and current source coverage are distinct; this
review does not prove the cause of its missing row or claim a release/update.

The other three exact IDs were absent from common, SHIELD, Google TV, known
names, research progress and the pinned source snapshot. Similar branding is
not an alias: existing `com.starz.starzplay.android` (LIONSGATE+), `com.cbs.ott`
(Paramount+), CBS News and CBS Sports entries remain unchanged. No prior Safe
verdict is inherited by a new package.

## Functional assessments

| Report / exact package | Independently opened primary evidence | App-specific consequence and dependency review |
| --- | --- | --- |
| #310 `com.bamnetworks.mobile.android.gameday.atbat` | [MLB Advanced Media publisher listing](https://play.google.com/store/apps/details?id=com.bamnetworks.mobile.android.gameday.atbat) | Baseball video/audio, scores/news, notifications, subscription/provider-linked programming and ticket-related access. Disabling stops its media, scores and alerts, including app links/integrations another person uses. Features vary by platform and entitlement; this is not proof that every listed feature exists in this TV build. No ticket deletion, entitlement continuity or alternate access is promised. Keep if used. |
| #311 `com.bydeluxe.d3.android.program.starz` | [Starz Entertainment publisher listing](https://play.google.com/store/apps/details?id=com.bydeluxe.d3.android.program.starz) | Streaming, downloads where supported, and direct-account or participating TV-provider viewing. Disabling stops streaming/provider viewing through this app; listing-wide download support is not proof that this TV build supports downloads. The listing limits service to participating partners in the US and certain US territories. Billing and provider/account eligibility are separate. Keep if used. |
| #312 `com.cbs.tve` | [CBS Interactive publisher listing](https://play.google.com/store/apps/details?id=com.cbs.tve) | Free episodes and optional cable-provider authentication for additional seasons/local live TV. Disabling stops this client's viewing, including provider-linked use. The listing limits use to the US and warns of device/content availability differences. It is not Paramount+, CBS News or CBS Sports. Keep if used. |

These verified optional content-client functions support Caution with deliberate
Disable-if-unused suggestions, not global Safe verdicts. No primary source here
identifies these exact clients as a launcher, input/accessibility stack or operator
core. Complete installed manifests, signatures, dependency graphs, firmware
integration and recovery were not obtained. Stopping the named app functions is
a functional inference, not a physical observation. Shared callers or provider
workflows may rely on them, so no claim that everything else is unaffected is made.
Unresolved operator integration for DIRECTV is not resolved by these assessments;
The initial DIRECTV hold was resolved on 2026-11-09; see [the publisher evidence review](REPORTED-APPS-307-318-321.md) for its conditional Caution recommendation.

The [MLB Android TV help page](https://www.mlb.com/live-stream-games/help-center/android-tv)
is additional platform evidence. Its existence is not a reinstall or compatibility
guarantee for the reported firmware. A separately supplied Android troubleshooting
search excerpt concerned location/blackout verification; the page failed text
retrieval, so it is not used as complete troubleshooting or recovery evidence.
A STARZ support page is titled [TV login guidance](https://support.starz.com/en_us/how-to-log-into-starz-from-an-apple-tv-android-tv-roku-samsung-tv-or-xbox-HylHuDYhj),
but its body was not text-readable; no additional behavior is inferred from it.
Support search excerpts about reinstall and device entries were supplied during
independent research; without readable page bodies they are not used as verified
recovery instructions. Historical and current supported-device claims differ;
no universal Android-version or reinstall promise is made. No official app-specific
Android TV reinstall instructions were verified for MLB or CBS. TIDAL's iOS
uninstall/download warning is not evidence for SHIELD behavior and is not used.
No shared auth, Cast, DRM or provider service is changed by this batch.

## Conservative scope and recovery

Add only the three missing exact IDs to shared `common.json`, with medium risk,
Disable, review true and both Optimize/Restore defaults false. The installed-package
planner supplies applicability; this does not match a source profile to a TV or
certify a family/firmware. Keep is the initial choice, Select all safe excludes
Caution, missing packages are skipped and disabled packages are not disabled again.
Explicit Restore may re-enable an installed disabled app, without automatic selection.

[Android package-manager documentation](https://developer.android.com/tools/adb#pm)
distinguishes enable/disable from uninstall or clear-data. Re-enable requires the
package to remain installed and does not restore sign-in, paid access, deleted
data or settings. An exact Play listing sets the existing `play_store` flag,
which permits the separately confirmed Uninstall option; these are Disable
suggestions, not disable-only controls. Device/account/region availability and
reinstallation are not guaranteed. [Android TV subscription guidance](https://support.google.com/androidtv/answer/9461288?hl=en)
explicitly says uninstall does not cancel subscriptions; disabling is likewise
not a billing-cancellation operation. MLB and STARZ consequences disclose this.
No performance gain is claimed.

Safety engine purity, protected/caution checks, last-HOME, setup/network, user-0,
confirmation guards and action code are unchanged. Only exact package JSON,
research bookkeeping, focused tests, synchronized demo data and this assessment
are involved. Pinned source bytes/provenance, research ledger, full attribution,
notices and the proprietary app license are preserved.

## Validation scope

The reviewed-Optimize fixture adds these three identities to the existing tests
for installed/disabled/missing states across Optimize/Restore and device families.
A focused loader regression checks metadata, Caution/Disable/default flags,
existing TIDAL uniqueness, distinct STARZ/CBS variants and the DIRECTV hold.
Desktop/mobile fixture tests check initial Keep, review labels, deliberate action
choices and exclusion from automatic recommendations. Captures and generated
galleries are synthetic browser fixtures, not ADB or installed-device validation.
Actual commands, exact tested revisions, capacity blockers and independent review
must be recorded separately. Earlier candidate tests do not validate these new
application inputs. No hardware action, public response or release is part of
this batch.
