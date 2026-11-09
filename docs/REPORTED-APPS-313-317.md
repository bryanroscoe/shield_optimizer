# Reports 313–317: existing CBS News coverage and four optional clients

Reviewed 2026-11-06. This separate batch is stacked on the preserved three-app
candidate `1b272da078fe1b354ed93e88591ad3d475e9a219`, whose main ancestor is
`c3624ecf09a6a8f32f8c7b866bf1dd3fd54ee613`. Earlier branches, proposed wording,
review evidence and capture files remain separate and unchanged. These reports
are untrusted sightings from optimizer 2.5.0 on Google TV / Android 14, not
successful disable/recovery evidence. No APK/signature or installed manifest audit
was obtained, and no device writes were performed.

## Existing coverage and exact identities

#313 `com.cbsnews.ott` already has medium risk (Caution), Disable, review enabled
and both defaults false on current main and the stacked base. It is unique and
unchanged. A shipped-version mismatch is plausible but not demonstrated; this
review does not establish why the reporter's installed build showed Unknown or
claim that a particular update already fixes it.

The four new exact IDs were absent from common, SHIELD, Google TV, launcher,
known-name, demo and progress data before this batch. Existing
`com.discovery.discoveryplus.androidtv` is a separate record, preserved without
copying its older verdict. Neither `.tv`, `.android` nor `.mobile` is evidence of
suitability, incompatibility or an alias. discovery+ and Discovery GO are distinct
services/clients, not interchangeable viewing or entitlement paths.

## Primary-source functional review

| Report / exact package | Primary evidence opened during review | Consequence and dependency limits |
| --- | --- | --- |
| #314 `com.clearchannel.iheartradio.tv` | [iHeartMedia publisher listing](https://play.google.com/store/apps/details?id=com.clearchannel.iheartradio.tv), [Android TV installation/account activation help](https://help.iheart.com/hc/en-us/articles/360028981511-Adding-iHeartRadio-to-Android-TV) | Radio, music and podcast listening through this TV client stop when disabled. Keep if used. Mobile/premium/offline features in listing-wide text are not promises about this TV build. This is not the separate `.controller` mobile package. No Android TV remote integration is inferred from help articles listing other TV platforms. |
| #315 `com.cw.fullepisodes.android` | [The CW Network exact publisher listing](https://play.google.com/store/apps/details?id=com.cw.fullepisodes.android) | Episodes, movies, sports/channels, profile/watch-list and continue-watching access through the app stop. The listing says no cable subscription is required. Profiles, other app callers or links may still matter to the household; Keep if used. No guessed TV package alias. |
| #316 `com.discovery.discoveryplus.mobile` | [Discovery Communications exact publisher listing](https://play.google.com/store/apps/details?id=com.discovery.discoveryplus.mobile), [official app-update guidance](https://help.discoveryplus.com/hc/en-us/articles/32717632355607-discovery-app-update) | Subscription viewing and local app controls, including casting initiated/controlled through it, stop or may be affected. Keep if used for playback or casting. Profiles, lists, plans, languages and availability vary by region; no universal TV download or premium capability is asserted. Billing is separate. The migration guide describes black/blue logos on Android TV without mapping them to package IDs; no exact-ID migration, alias or suffix-based rejection follows. |
| #317 `com.discovery.dsctve` | [Discovery Communications exact publisher listing](https://play.google.com/store/apps/details?id=com.discovery.dsctve) | Provider-authenticated live/on-demand Discovery-family viewing, guide/list and continue-watching access through this client stop. Keep if used; do not change provider/auth services or assume entitlements transfer to discovery+. The listing's incongruous iOS/tvOS requirements are not Android firmware or installed-build certification. |

Optional listening/viewing roles support explicit Disable-if-unused suggestions
with Caution, not global Safe verdicts or proof that every OEM/operator dependency
is absent. Shared callers, launcher rows, casting and account/provider workflows
may rely on these app functions. Their interruption is a functional assessment,
not an observed physical outcome. No source identifies these as operator core,
HOME, input or accessibility infrastructure, but complete dependency graphs and
firmware-specific behavior remain unverified. The separate DIRECTV operator-scope
hold was resolved on 2026-11-09 with explicit operator-device caveats; see [the follow-up review](REPORTED-APPS-307-318-321.md).

## Source observations and preservation

iHeart has an [active owner-choice observation on Hisense](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/hisense-smarttv-4k-ffm/debloat.txt#L78-L87)
and a [commented opt-in entry on Bell](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/bell-fibe-tv-vip7802/debloat.txt#L83-L99).
The latter source comment reports an app recommendations-refresh receiver. Those
are attributed upstream observations, not our firmware validation or removal
proof. Recommendations from this app may be relied on by a launcher; no claim of
unaffected Home content is made. No exact occurrence of the other three new IDs
was found in the pinned source snapshot. Counts and profile names are not safety
votes or automatic device matches.

## Scope, defaults and conditional recovery

All four additions use shared `common.json` exact-package coverage, medium risk,
Disable, review true and both Optimize/Restore defaults false. This adds optional
choices only for installed packages, without certifying a family/model/firmware.
Initial Keep and exclusion from Select all safe remain; missing packages are
skipped, already-disabled packages are not disabled again, and explicit Restore
may enable an installed disabled app without automatic selection.

[Android package commands](https://developer.android.com/tools/adb#pm) distinguish
enable/disable from uninstall or clear-data. Re-enable requires the package to
remain installed; it does not restore sign-in, provider authorization, paid access,
settings or deleted data. Exact Play listings set the existing `play_store` flag,
so the existing separately confirmed Uninstall option remains available. These
are Disable suggestions, not disable-only controls. Compatible store, account,
region and installation availability constrain reinstall; no deleted-APK recovery
or platform-wide availability guarantee is made. [Google's subscription guidance](https://support.google.com/googleplay/answer/7018481)
distinguishes uninstalling from cancelling billing. Disabling is not cancellation
of discovery+ or TV-provider subscriptions. No performance improvement is claimed.

The pure safety engine, protected/caution, last-HOME, setup/network, user-0 and
confirmation authorities remain intact. No transport, shared auth/Cast/DRM/provider
service or entitlement logic changes. Existing app records, CBS News, Discovery+
Android TV and all held decisions remain unchanged. Imported source bytes,
provenance, research ledger, full attribution/notices and the app license remain
unchanged; separate progress bookkeeping records the four new assessments.

## Validation scope

The shared reviewed-Optimize fixture adds four identities to the existing planner
coverage of installed/disabled/missing states across Optimize/Restore and device
families. A focused loader regression checks exact evidence, Caution/Disable and
both default flags, CBS News uniqueness, distinct discovery+ IDs and DIRECTV's hold.
Desktop Optimize tests also exercise the actual frontend recommendation module
for enabled/disabled/missing states, default Keep, Disable suggestions and separate
Uninstall eligibility across the reviewed fixture. Desktop/mobile captures use
synthetic browser fixtures, never ADB or a physical TV. Exact tested source/head, completed checks, resource gates, gallery
refresh and independent final review are recorded in PR preparation evidence.
Prior-candidate results alone cannot validate these new app inputs.
