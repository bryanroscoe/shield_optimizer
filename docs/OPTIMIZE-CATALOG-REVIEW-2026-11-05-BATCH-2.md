# Two-agent catalog review — 2026-11-05, batch 2

Bryan requested an independent second check and ongoing batches to expand the
existing Optimize flow. One independent source reviewer checked the first eight
accepted entries at `f43065a0f2bb53dc93eeda0e08bae67ddb79bdd9` and proposed the
next eight below. The implementation agent separately opened the exact primary
developer listings, checked purpose and the original upstream observations,
verified exact-ID absence from all three app lists and known names, and agreed
with the proposed treatment.

## Independent check of the first batch

No actionable findings in the reviewed source scope. Exact developer listings
support the documented purposes; all eight entries have medium risk, Disable
suggestions, review enabled and both defaults false. Protected/caution checks
take precedence, medium maps to Caution, Select all safe excludes Caution, and
missing/disabled state and Restore behavior stay with the canonical planner.

This is a source review, not a formal GitHub approval or physical test. It does
not turn the entire imported catalog into reviewed data. The original evidence
and limitations are in [the first review](OPTIMIZE-CATALOG-REVIEW-2026-11-05.md).
An exact Play listing also allows the existing explicit Uninstall option;
describe these as **Disable suggestions**, not Disable-only controls.

## Next eight accepted identities

Both source reviews approve the following exact identities for optional Caution
coverage: `risk: medium`, `method: disable`, `review: true`, and both defaults
false. The primary sources establish application identity and function. The
assessment is that stopping each application loses the named optional viewing
function, so the person must explicitly choose Disable if unused. It does not
establish a global Safe verdict, a default action or target firmware certification.

| Exact package | Independently checked developer evidence | Consequence recorded in Optimize |
| --- | --- | --- |
| `com.zdf.android.mediathek` | [ZDF](https://play.google.com/store/apps/details?id=com.zdf.android.mediathek) | Stops ZDF live/on-demand viewing through this app. |
| `fr.francetv.pluzz` | [France Télévisions](https://play.google.com/store/apps/details?id=fr.francetv.pluzz) | Stops france.tv live/replay viewing through this exact app. |
| `com.mubi` | [MUBI](https://play.google.com/store/apps/details?id=com.mubi) | Stops viewing and download access through the app; does not cancel a subscription. |
| `com.yupptv.androidtv` | [YuppTV](https://play.google.com/store/apps/details?id=com.yupptv.androidtv) | Stops YuppTV channels and programmes through this app. |
| `com.cbsnews.ott` | [CBS Interactive](https://play.google.com/store/apps/details?id=com.cbsnews.ott) | Stops CBS News streams and programmes through this app. |
| `com.bloomberg.btva` | [Bloomberg](https://play.google.com/store/apps/details?id=com.bloomberg.btva) | Stops Bloomberg TV and Bloomberg Originals viewing through this app. |
| `com.ted.android.tv` | [TED Conferences](https://play.google.com/store/apps/details?id=com.ted.android.tv) | Stops talks through TED TV. A present listing is not evidence of a defunct service. |
| `in.startv.hotstar` | [JioStar](https://play.google.com/store/apps/details?id=in.startv.hotstar) | Stops JioHotstar viewing through this exact app. Other regional/TV IDs remain separate. |

Both reviewers checked these active source locations at upstream commit
`8672e68e8a24c1258e4662cd11daa4ed13795dad`, respectively:
`dyon-2k-android-tv:43`, `freebox-player-pop:71`, `noblex-shinjuku:53`,
`sanyo-atv-r2:55`, `formovie-mprojm05:48`, `tcl-50ep680:86`,
`formovie-mprojm05:49` and `nokia-r4:57`, under `examples/*/debloat.txt`.
The app-list entries retain full pinned links. Commented-out and prose occurrences
also exist for ZDF, france.tv, TED and Hotstar; those remain original observations,
not conflicting safety votes to average. Keep is the default regardless.

These are assessments of installed exact application IDs. No source profile is
matched to a connected TV, no additional family is certified, and no package-name
aliasing is introduced. Disable is reversible only while the package remains
installed; account access, settings, regional availability and subscriptions are
not restored by re-enabling. No performance benefit or successful hardware
removal/restore is claimed. The first review's common-scope and restore limits
continue to apply.

## System leads remain held

The independent reviewer also confirmed primary evidence for four high-frequency
system leads and agreed that it is insufficient to add removal suggestions:

- [AdServices manifest](https://android.googlesource.com/platform/packages/modules/AdServices/+/593832a5beabbc740974dc6c07f3294cd8a3bc6d/adservices/apk/AndroidManifest.xml): exported platform services and permissions need caller and recovery assessment.
- [On-device personalization manifest](https://android.googlesource.com/platform/packages/modules/OnDevicePersonalization/+/4dc4c8e909a03cbfb72a14e8758e8dfeef450397/AndroidManifest.xml) and [module documentation](https://source.android.com/docs/core/ota/modular-system/ondevicepersonalization): system infrastructure, with unverified TV removal dependencies.
- [FederatedComputeManager](https://android.googlesource.com/platform/prebuilts/fullsdk/sources/+/refs/heads/main/android-35/android/federatedcompute/FederatedComputeManager.java): the Android package and Google alternative are separate identities; their names do not establish interchangeable behavior.
- [TV feedback consent manifest](https://android.googlesource.com/platform/packages/apps/TvFeedbackConsent/+/8343380fe0b1bfff0426c31832c83c17b0c70d66/AndroidManifest.xml): core/system-UID consent behavior requires further assessment.

These holds remain in the research progress ledger. The recurring process must
record holds as well as accepts and must never fill a batch quota by weakening
the evidence requirement. See [the batch process](OPTIMIZE-CATALOG-REVIEW-PROCESS.md).
