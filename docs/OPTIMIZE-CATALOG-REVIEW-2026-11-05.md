# Optimize catalog review — 2026-11-05

The imported CleanYourTV catalog supplies research leads. This review accepts
eight exact package identities into the shared app lists used by desktop and
mobile Optimize, App List and safety lookup. It does not infer a removal verdict
from upstream disable counts, a profile name, or a claimed device match.

Each accepted entry has a developer-published Google Play listing confirming
its exact identity and purpose, an attributed link to the pinned upstream
observation, a review date, and a consequence written for that app. This is a
source-based functional review, not a physical removal/restore test or proof of
compatibility with any model, firmware or platform.

## Accepted entries

All eight use `risk: medium`, `method: disable`, `review: true`, and both
`default_optimize` and `default_restore` set to false. When installed and enabled,
they become optional **Review: disable if unused** suggestions in Optimize.
They start at Keep, are excluded from Select all safe, and never generate a Safe
verdict. Missing packages are skipped; already disabled packages are not disabled
again. Restore may re-enable an installed disabled package only by explicit choice.

| Exact package | Independent primary evidence | Functional assessment and consequence |
| --- | --- | --- |
| `com.aspiro.tidal` | [TIDAL developer listing](https://play.google.com/store/apps/details?id=com.aspiro.tidal) | Music client. Disabling stops TIDAL playback on the TV. Keep if used. |
| `com.cloudmosa.puffinTV` | [CloudMosa developer listing](https://play.google.com/store/apps/details?id=com.cloudmosa.puffinTV) | Android TV web browser. Disabling stops browser use and links handled by it. A separate browser may be needed. |
| `tv.wuaki.apptv` | [Rakuten TV developer listing](https://play.google.com/store/apps/details?id=tv.wuaki.apptv) | Streaming client for movies, series and live channels. Disabling removes access through this app. |
| `deezer.android.tv` | [Deezer developer listing](https://play.google.com/store/apps/details?id=deezer.android.tv) | Android TV music client. Disabling stops its music playback. |
| `tv.arte.plus7` | [ARTE developer listing](https://play.google.com/store/apps/details?id=tv.arte.plus7) | ARTE programme client. Disabling stops viewing through this app. |
| `com.viaplay.android` | [Viaplay developer listing](https://play.google.com/store/apps/details?id=com.viaplay.android) | Viaplay streaming client. Disabling stops viewing through this app. |
| `com.antena3.atresplayer.tv` | [Atresmedia developer listing](https://play.google.com/store/apps/details?id=com.antena3.atresplayer.tv) | atresplayer live/on-demand client. Disabling stops viewing through this app. |
| `com.google.android.apps.tv.dreamx` | [Google developer listing](https://play.google.com/store/apps/details?id=com.google.android.apps.tv.dreamx) | Google TV photo screensaver. Keep unless a replacement screensaver has been selected. Disabling stops this screensaver; no automatic replacement or settings changes are made. |

The catalog entries link to the original per-profile source lines at upstream
commit `8672e68e8a24c1258e4662cd11daa4ed13795dad`. Those lines retain the original
author's decision and context; the developer listings independently establish
identity and purpose. The review does not adopt the upstream decision as a safety
guarantee. The imported snapshot, full MIT notices and app's proprietary license
remain unchanged.

The entries live in `common.json` because the review is about an installed exact
package, not a match between an upstream TV profile and a connected device. The
existing loader/planner selects installed packages; this adds no new device
detection, family, support-tier or firmware assertion. Similar-looking IDs such
as regional TV clients are separate identities and are not covered.

Although each exact package has a published Play listing, availability can vary
by region, device and account. These suggestions use Disable, not Uninstall.
Re-enable works only while the package remains installed; it does not restore
subscriptions, sign-ins or settings and is not a promise of Play reinstallation.
Disabling a paid app does not cancel its subscription. No performance improvement
is claimed without measurement.

## Held out of action coverage

The source catalog has 1,971 package identities; 59 already overlapped the 103
reviewed identities at the initial checkpoint. This is a bounded acceptance of
eight leads, not acceptance of the remaining source set.

- `com.google.android.partnersetup`, `android.autoinstalls.config.nvidia`, and
  `com.realtek.leanbacklauncher.partnercustomizer` retain the unresolved
  dependency/model/firmware questions in the imported research ledger.
- TalkBack is an accessibility service, not disposable bloat. The
  [Google listing](https://play.google.com/store/apps/details?id=com.google.android.marvin.talkback)
  confirms the screen-reader role. It is not promoted into removal suggestions.
- `com.android.tv.feedbackconsent` is held pending a dependency and consent-flow
  assessment. Its [AOSP manifest](https://android.googlesource.com/platform/packages/apps/TvFeedbackConsent/+/8343380fe0b1bfff0426c31832c83c17b0c70d66/AndroidManifest.xml)
  declares a system-UID core app and a feedback-consent service. Its frequent
  appearance in upstream disable lists does not establish removal safety.
- AdServices, personalization, vendor input/remote, setup and update components
  are not promoted from package-name guesses or source frequency.

Unaccepted observations remain unreviewed and non-actionable. The existing
protected, caution, last-HOME, setup/network, user-0 and confirmation gates
continue to govern every action.
