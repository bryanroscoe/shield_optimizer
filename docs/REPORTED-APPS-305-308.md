# Reported apps 305–308: functional review and DIRECTV hold

Reviewed 2026-11-06. Follow-up to the separate four-app branch at
`d56049963b4d635d9741b09c5f1d017156264862`, based on main `c3624ec`.
Reports are untrusted observations from one Google TV / Android 14 installation.
Their asserted removal safety is not accepted as evidence. No APK, installed
signature, manifest, device write or physical recovery experiment was used.

## Accepted with Caution and default Keep

| Exact identity | Primary evidence | Consequence, dependencies and limits |
| --- | --- | --- |
| #305 `com.aetn.history.watch.androidtv`, HISTORY: Shows & Documentaries | [A+E publisher Play listing](https://play.google.com/store/apps/details?id=com.aetn.history.watch.androidtv) | Full episodes and clips, with additional content unlocked through a supported TV provider; the listing restricts streaming to the US. Disabling stops this client's viewing and provider-linked access. Keep if used. No provider subscription cancellation, account recovery or worldwide reinstall availability is promised. This is distinct from `com.aetnd.svod.historyvault.androidtv` (HISTORY Vault). |
| #306 `com.analiti.fastest.android`, analiti - Speed & WiFi | [Publisher Play listing](https://play.google.com/store/apps/details?id=com.analiti.fastest.android) and [analiti's product documentation](https://analiti.com/) | Speed tests, WiFi analysis, connection tracking and network diagnostic tools, including scheduled background speed tests and embedded iPerf3/UDP Echo servers. Disabling stops these app services; another device or technician may rely on its monitoring or test server. Keep if those functions are used. Disabling is not a rollback of network/router changes previously made; re-enable does not prove monitors or servers resume with the expected settings. No claim that the TV's networking is unaffected is made. |

These are app-specific functional assessments, not an inference that all branded
apps are removable. Neither primary source identifies these as the TV's launcher,
input stack, accessibility or operator core. Their optional viewing/diagnostic
roles support deliberate Disable-if-unused suggestions with Caution; they do not
support a global Safe verdict. Complete installed dependencies, firmware-specific
integration, shared callers and service continuity remain unverified.

All three entries live in the shared `common.json` exact-package catalog. This adds
coverage only when the package is present; it does not certify a TV family or
match a profile to a connected device. They use medium risk, Disable, review true,
with Optimize and Restore defaults false. Keep remains the default and Select all
safe excludes them. Explicit Restore can enable an installed disabled package;
missing packages are skipped. The standard safety and action guards remain intact.

[Android's package-manager commands](https://developer.android.com/tools/adb#pm)
distinguish enabling/disabling from uninstalling or clearing data. Re-enable is
conditional on the package remaining installed; it does not restore deleted data,
account eligibility, paid access, sign-in or app settings. Publisher Play listings
set the existing `play_store` flag, which permits the separately confirmed
Uninstall option. These are Disable suggestions, not disable-only controls;
regional/device/account reinstall availability is not guaranteed.

The pinned upstream analiti occurrence is explicitly an owner choice:
[sony-bravia-vu31 lines 104–112](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/sony-bravia-vu31/debloat.txt#L104-L112).
Its surrounding comments retain the original author's device/firmware account.
Neither that observation nor the accompanying reported reboot check is our
hardware evidence, compatibility certification or the basis for a safety verdict.
No matching HISTORY or exact-ID BritBox observation exists in the pinned source queue. Source
snapshot, provenance, notices and app license are unchanged.

## Additional intake: #308 BritBox

The exact [BritBox LLC Play listing](https://play.google.com/store/apps/details?id=com.britbox.tv)
identifies BritBox: Home of British TV, a subscription viewing client. Its expected
loss is playback and links targeting the app, not cancellation of billing.
[Android TV subscription guidance](https://support.google.com/androidtv/answer/9461288?hl=en)
explicitly distinguishes uninstalling from cancelling a subscription. Keep if
anyone uses it. Add this exact identity with the same Caution/Disable/default-Keep
treatment, without inheriting the old Safe/Uninstall classification from
`com.britbox.us`; that existing entry remains unchanged. Conditional re-enable
requires the app to remain installed; reinstall depends on regional/device/account
availability and sign-in. No local-state preservation or deleted-APK recovery
through install-existing is promised. The Play listing establishes function, not
a complete manifest/dependency graph or OEM/operator compatibility. No upstream
occurrence of this exact ID was found in the pinned snapshot. A separate research
handoff and this implementation review independently checked the publisher
listing; the normal independent final reviewer must also assess this addition.

## Held: #307 DIRECTV `com.att.tv`

The [exact DIRECTV publisher listing](https://play.google.com/store/apps/details?id=com.att.tv)
confirms live/on-demand and recorded-program viewing on Android devices. The
[vendor's Android TV troubleshooting guide](https://www.directv.com/support/article/000087389)
allows uninstall/reinstall for an isolated app problem. However,
[Gemini app management guidance](https://www.directv.com/support/article/000091427)
says preloaded and interactive DIRECTV apps cannot be uninstalled. The
[Gemini settings guide](https://www.directv.com/support/article/000091414)
also describes device functions integrated with the DIRECTV experience.

Those pages do not establish whether the exact Play package is used identically
on operator firmware, which operator services depend on it, or how a disabled
operator environment would be recovered. They also do not prove that the reported
installation is independent of operator integration. The common/family catalog
cannot enforce ordinary-user-install provenance or exclude Gemini/operator
firmware for this exact package. Therefore no actionable or descriptive-name
entry is added: DIRECTV remains Unknown. The research ledger records a hold,
not a new safety authority. Revisit when exact package/manifest role, enforceable
installation scope, dependency consequences and recovery are established.

## Validation and scope

The shared reviewed-Optimize fixture extends existing planner tests for installed,
missing and disabled states in Optimize/Restore across all device families.
Focused assertions preserve HISTORY Vault's distinct identity, exact Play links,
Caution/Disable/default flags, analiti's monitoring/server consequences and
DIRECTV's absence/Unknown verdict. Desktop/mobile synthetic UI tests check Keep,
Caution and explicit choices. Synthetic screenshots/videos are fixtures, not
installed builds, ADB behavior or physical disable/re-enable validation.
Actual tested revisions, results and independent review are recorded in PR
preparation evidence. The original #301–304 branch and its pending PR wording/evidence remain unchanged.
