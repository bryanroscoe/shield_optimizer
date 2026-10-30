# Display-only package identities from reports 240–254

Reviewed 2026-10-29 against main 8702632. All 15 exact package IDs were checked
against their publisher's Google Play listing; the reproducible URL/name mapping
is `v2/tests/fixtures/reported-package-names.json`. Descriptions are original,
concise functional summaries, not copied store prose. No APKs were downloaded.

Surfshark's plain Play URL timed out during inspection. Its localized Play
listing and the publisher's explicit package link independently confirm identity:
https://play.google.com/store/apps/details?hl=en_US&id=com.surfshark.vpnclient.android
https://surfshark.com/bounty-policy
https://support.surfshark.com/hc/en-us/articles/38068330464018-What-is-Surfshark-s-beta-tester-program-and-how-to-join-it

The reports are observations from one Google TV Android 14 installation, not
compatibility or removal-safety validation. Store identity does not prove the
installed binary's signature, firmware behavior, reinstall availability in every
region, or that an app is safe to disable. Watch TBS/TNT listing availability does
not establish continuing service availability. No service-longevity claim is made.

Fourteen missing packages get display-only names/descriptions. Monet gets its
publisher title and a functional description in the same name registry. Its
existing curated launcher entry and launcher-list Caution remain unchanged.
No safe labels, Optimize defaults, protected lists, uninstall recommendations,
launcher promotion or system-package overrides are added.

The UI evidence injects installed-package and verdict fixtures. It verifies
presentation, not hardware behavior or classification. Existing Rust safety
regressions compare every known name against an unnamed state; the separate
custom-launcher regression verifies launcher Caution. Captured gallery screens
use their existing demo packages and are unaffected by these registry additions.

## Existing streaming-app brand corrections, reports 255–256

The exact Google Play package URLs were checked on 2026-10-29:
https://play.google.com/store/apps/details?id=air.com.vudu.air.DownloaderTablet
https://play.google.com/store/apps/details?id=com.wbd.stream

The first listing currently says Fandango: Movies & Series, published by Fandango
Media; it is the reported Vudu package, not an inferred match to a ticketing app.
The second says HBO Max: Stream TV & Movies, published by WarnerMedia Global
Digital Services, LLC. The concise catalog name is HBO Max. Existing package IDs,
risk/method/default/review/source fields are preserved exactly; only names and
brand references in consequence/restoration descriptions change. Their previous
reviewed safety classifications are not reassessed or broadened by store metadata.
The demo catalog is regenerated from common plus shield. The rename fixture
records every preserved field and the UI test checks both rendered names.

## Independent functional assessment, 2026-10-30

The exact Play listings linked by the fixtures are identity/function evidence,
not device-removal experiments. No reporter or bot recommendation supplies a
safety verdict here. The decisions below apply to this PR, not a general promise
about every installation of these apps. Public source links corroborate roles;
installed signatures, firmware-specific integration and removal outcomes remain
unverified. Unknown is retained where that evidence does not justify a reviewed
removal rule. It does not mean the package was identified as a system dependency.

| Report / package | Independent role/dependency assessment | Decision and uncertainty |
| --- | --- | --- |
| 240 `by4a.setedit22` | Settings database editor; its publisher warns that edited system settings can cause problems. Removing the editor is not evidence those prior edits are reverted. | Identify SetEdit; retain Unknown. Do not conflate removing an editor with restoring settings. |
| 241 `com.cxinventor.file.explorer` | File manager with local/network/cloud access. User workflows may rely on its file-opening or storage access. | Identify Cx File Explorer; retain Unknown. No verified replacement/file-provider behavior after removal. |
| 242 `com.espn.score_center` | ESPN sports information and streaming client. Its own viewing/download/account workflows are the evident dependency. | Identify ESPN; retain Unknown. No firmware or offline-data removal test. |
| 243 `com.fox.foxone` | FOX content client; subscription/content availability is distinct from Android platform function. | Identify FOX One; retain Unknown. Do not promise subscription/data recovery. |
| 244 `com.getchannels.dvr.app` | Channels viewing client connected to DVR/server functionality. | Identify Channels; retain Unknown. Do not equate removing the client with preserving every server/client integration. |
| 245 `com.handmark.sportscaster.androidtv` | CBS Sports TV client. | Identify CBS Sports; retain Unknown. No cross-firmware removal evidence. |
| 246 `com.hoopladigital.android` | Library borrowing client for several media types; downloaded/borrowed content and account access matter to the user. | Identify Hoopla; retain Unknown. Do not assert borrowed/downloaded data survives uninstall. |
| 247 `com.kanopy.tvapp` | Library-supported TV film client. | Identify Kanopy; retain Unknown. Library eligibility and content access are not removal-safety evidence. |
| 248 `com.klevico.monet` | HOME launcher, already recognized by the existing launcher catalog. The current/default and last-enabled-HOME guards matter. | Correct Monet metadata; retain launcher-list Caution and all existing HOME guards. No new launcher promotion. |
| 249 `com.moviesanywhere.goo` | Linked movie-library playback client. Store account/library claims do not establish local download recovery. | Identify Movies Anywhere; retain Unknown. No blanket reinstall/data guarantee. |
| 250 `com.neilturner.aerialviews` | Video/photo screensaver; it can be selected as the active dream component. Publisher source: https://github.com/theothernt/AerialViews | Identify Aerial Views; retain Unknown. No claim that disabling an active screensaver restores a previous component. |
| 251 `com.silicondust.view` | Viewing/DVR client for HDHomeRun network tuner equipment. The tuner/server and client are distinct components. | Identify HDHomeRun; retain Unknown. No tested tuner/client dependency or recording-retention outcome. |
| 252 `com.surfshark.vpnclient.android` | VPN client; network access can depend on an active VPN, always-on configuration or blocking traffic outside it. Publisher corroboration is linked above. | Identify Surfshark; retain Unknown. Do not adopt the report's unconditional removal claim or change VPN/security settings. |
| 253 `com.turner.tbs.android.networkapp` | TBS content client, including provider-linked viewing. Listing existence does not establish continuing service availability. | Identify Watch TBS; retain Unknown. Do not infer defunct/removable status from age or branding. |
| 254 `com.turner.tnt.android.networkapp` | TNT content client with the same service/account distinction. | Identify Watch TNT; retain Unknown. No inferred shutdown or blanket removal claim. |
| 255 `air.com.vudu.air.DownloaderTablet` | Exact existing package now branded Fandango: Movies & Series; store describes rentals, purchases and a digital library. | Rename only. Retain the existing reviewed Safe/method/default fields without treating this branding review as new hardware removal validation. |
| 256 `com.wbd.stream` | Exact existing streaming package now branded HBO Max; subscription/client functionality is distinct from the old HBO package. | Rename only. Retain existing reviewed Safe/method/default fields; no new compatibility or restoration guarantee. |

No additional safety-classification change is supported strongly enough by this
review to include in the approved naming scope. In particular, an optional-looking
consumer app, a Play Store listing, or a successful simulated row does not by
itself establish safe removal on a reporter's firmware. A later safety review
would need bounded consequence descriptions, installed/system/user-scope evidence
and targeted dependency/recovery tests rather than inheriting these assertions.

Reproduction: `cargo test --offline -p shield-optimizer-core commands::loader::tests`
and `npm run test:reported-package-names`. Captures use the actual catalog JSON
at de5e015; later assessment-only documentation does not change captured UI.

The refreshed SetEdit report also links a different fork. Its build file declares
`applicationId "io.github.muntashirakon.setedit"`, not `by4a.setedit22`:
https://github.com/MuntashirAkon/SetEdit/blob/master/app/build.gradle
That fork is not used as identity or installed-behavior evidence for the reported
4A package; the exact Play listing remains its identity source.

The VPN dependency concern is grounded in Android's always-on and lockdown VPN
behavior, not an observed Surfshark failure on this reporter's TV:
https://developer.android.com/develop/connectivity/vpn
