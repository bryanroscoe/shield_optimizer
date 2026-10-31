# Reported package identities and optional content-client safety review

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

Fourteen missing packages receive descriptive metadata; Monet receives its publisher
name without changing launcher choices or HOME guards. The initial naming-only
patch retained all verdicts. The owner subsequently confirmed removability on
2026-10-30 and requested Safe classifications for the clearly identified TV
content clients. On 2026-10-31 the owner asked for the remaining reported tools to be
categorized too: SetEdit, Cx File Explorer and Aerial Views follow the existing Safe
precedent for user-installed tools (Solid Explorer, Aerial Dream), and Surfshark is
rated medium (Caution) because of always-on VPN lockdown. Launchers stay excluded.

The fourteen new catalog entries use Disable, with Optimize, Restore and review-selection
defaults false. They remain explicit user choices. Re-enable applies only while
installed and does not promise deleted downloads, remote recordings, subscriptions
or accounts can be restored. No engine, protected list or launcher guards change.
Simulation verifies the resulting rows and defaults; it does not prove physical
removal behavior. Owner confirmation is an assessment input, not our hardware test.

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
not device-removal experiments. Reporter and bot assertions alone do not supply a safety verdict. The owner
confirmation is combined with the independently verified package roles below. The decisions below apply to this PR, not a general promise
about every installation of these apps. Public source links corroborate roles;
installed signatures, firmware-specific integration and removal outcomes remain
unverified.

| Report / package | Independent role/dependency assessment | Decision and uncertainty |
| --- | --- | --- |
| 240 `by4a.setedit22` | Settings database editor; its publisher warns that edited system settings can cause problems. Removing the editor is not evidence those prior edits are reverted. | Reviewed Safe (2026-10-31): optional user-installed tool; no default selection. The description says settings it changed stay changed. |
| 241 `com.cxinventor.file.explorer` | File manager with local/network/cloud access. User workflows may rely on its file-opening or storage access. | Reviewed Safe (2026-10-31), matching Solid Explorer; no default selection. Files on the TV or network are not deleted. |
| 242 `com.espn.score_center` | ESPN sports information and streaming client. Its own viewing/download/account workflows are the evident dependency. | Reviewed Safe: optional ESPN content client; no default selection. Subscription cancellation and local-data recovery are not promised. |
| 243 `com.fox.foxone` | FOX content client; subscription/content availability is distinct from Android platform function. | Reviewed Safe: optional FOX viewing client; no default selection or subscription cancellation. |
| 244 `com.getchannels.dvr.app` | Channels viewing client connected to DVR/server functionality. | Reviewed Safe: optional viewing/DVR client; no default selection. Its viewing integration stops; server recordings are not backed up or restored. |
| 245 `com.handmark.sportscaster.androidtv` | CBS Sports TV client. | Reviewed Safe: optional sports viewing client; no default selection. |
| 246 `com.hoopladigital.android` | Library borrowing client for several media types; downloaded/borrowed content and account access matter to the user. | Reviewed Safe: optional media borrowing/playback client; no default selection. Local downloads and borrowing/account access are not restored by re-enable. |
| 247 `com.kanopy.tvapp` | Library-supported TV film client. | Reviewed Safe: optional film viewing client; no default selection. No account/library eligibility guarantee. |
| 248 `com.klevico.monet` | HOME launcher, already recognized by the existing launcher catalog. The current/default and last-enabled-HOME guards matter. | Correct Monet metadata; retain launcher-list Caution and all existing HOME guards. No new launcher promotion. |
| 249 `com.moviesanywhere.goo` | Linked movie-library playback client. Store account/library claims do not establish local download recovery. | Reviewed Safe: optional movie-library player; no default selection. No local-download or account recovery promise. |
| 250 `com.neilturner.aerialviews` | Video/photo screensaver; it can be selected as the active dream component. Publisher source: https://github.com/theothernt/AerialViews | Reviewed Safe (2026-10-31), matching Aerial Dream; no default selection. The description tells users to choose another screensaver; no fallback claim. |
| 251 `com.silicondust.view` | Viewing/DVR client for HDHomeRun network tuner equipment. The tuner/server and client are distinct components. | Reviewed Safe: optional tuner/DVR viewing client; no default selection. Client access stops; no claim about remote recording retention. |
| 252 `com.surfshark.vpnclient.android` | VPN client; network access can depend on an active VPN, always-on configuration or blocking traffic outside it. Publisher corroboration is linked above. | Reviewed medium / Caution (2026-10-31); no default selection. With always-on lockdown, disabling the VPN can cut the TV's network and this app's wireless ADB link, so the description says to turn lockdown off first. |
| 253 `com.turner.tbs.android.networkapp` | TBS content client, including provider-linked viewing. Listing existence does not establish continuing service availability. | Reviewed Safe: optional TBS content client; no default selection. No inferred shutdown, current access or reinstall guarantee. |
| 254 `com.turner.tnt.android.networkapp` | TNT content client with the same service/account distinction. | Reviewed Safe: optional TNT content client; no default selection. No inferred shutdown, current access or reinstall guarantee. |
| 255 `air.com.vudu.air.DownloaderTablet` | Exact existing package now branded Fandango: Movies & Series; store describes rentals, purchases and a digital library. | Rename only. Retain the existing reviewed Safe/method/default fields without treating this branding review as new hardware removal validation. |
| 256 `com.wbd.stream` | Exact existing streaming package now branded HBO Max; subscription/client functionality is distinct from the old HBO package. | Rename only. Retain existing reviewed Safe/method/default fields; no new compatibility or restoration guarantee. |

Review authority: Bryan's explicit 2026-10-30 owner confirmation and request to mark
clearly identified TV viewing apps Safe, combined with the exact publisher package
links recorded in the fixture. This is a manual functional/dependency review, not
physical removal validation or a classification inferred from simulated UI.

Reproduction: `cargo test --offline -p shield-optimizer-core commands::loader::tests`
and `npm run test:reported-package-names`. The latter checks actual catalog fields,
rendered Safe/Unknown/Caution labels and no automatic recommendation or default selection. Refreshed
media is explicitly simulated. Cross-platform CI and hardware acceptance are
recorded separately; no hardware actions are part of this review.

The refreshed SetEdit report also links a different fork. Its build file declares
`applicationId "io.github.muntashirakon.setedit"`, not `by4a.setedit22`:
https://github.com/MuntashirAkon/SetEdit/blob/master/app/build.gradle
That fork is not used as identity or installed-behavior evidence for the reported
4A package; the exact Play listing remains its identity source.

The VPN dependency concern is grounded in Android's always-on and lockdown VPN
behavior, not an observed Surfshark failure on this reporter's TV:
https://developer.android.com/develop/connectivity/vpn

DVR dependency cross-check, 2026-10-30: Channels describes its DVR server as a
separate computer/NAS component (https://getchannels.com/dvr-server/). HDHomeRun
lists separate network DVR storage options (https://info.hdhomerun.com/info/dvr:storage).
These describe current vendor architecture, not every installed version. HDHomeRun's
consequence therefore covers DVR features in the installation and explicitly asks
users to check local recording/storage; no recording continuity or backup is promised.
