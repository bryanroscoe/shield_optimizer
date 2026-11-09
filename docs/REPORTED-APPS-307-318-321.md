# DIRECTV, DisneyNOW and Live Channels: resolved review

Reviewed 2026-11-09 for reports #307, #318 and #321. This supersedes the initial holds in [305–308](REPORTED-APPS-305-308.md) and [318–322](REPORTED-APPS-318-322.md). Bryan approved using the publisher evidence for app-specific recommendations. No installed APK/signature audit, device write or physical recovery test is claimed.

All three exact packages now have shared catalog entries: **Caution**, review enabled, both Optimize/Restore defaults false, and initial **Keep**. They are deliberate choices, never automatic removals. The existing install-source policy remains authoritative: an ordinary live Play Store app installed by the user suggests Uninstall; preinstalled or unknown-source apps suggest Disable. Safety and last-Home checks still gate each operation. These entries do not certify an OEM, firmware, absence of all callers, or a performance gain.

## DIRECTV — `com.att.tv`

The [publisher's exact Play listing](https://play.google.com/store/apps/details?id=com.att.tv) describes the DIRECTV streaming client for live TV, on-demand programming and recorded shows. More directly, [DIRECTV's Android TV instructions](https://www.directv.com/support/article/000087389) link their download button to that exact package and explicitly allow uninstall/reinstall for an isolated app problem. This supports an optional viewing-app recommendation on ordinary Android TVs, with the loss of its viewing functions and no subscription cancellation.

[Gemini app-management restrictions](https://www.directv.com/support/article/000091427) apply broadly to preloaded apps, including third-party streaming clients. Those restrictions alone do not establish that this exact ordinary store package is essential on every Android TV. They still justify the explicit **keep if the DIRECTV Gemini/operator box relies on this app for its TV interface** warning. This is a conditional Caution choice, not an assertion that an operator device is supported or that all DIRECTV-branded packages are interchangeable. `com.att.tv.openvideo` and other operator packages are not added or aliased.

Re-enable requires the installed package. Store recovery depends on compatible device, region, account and availability; neither operation restores deleted data or guarantees sign-in/subscription access. Preinstalled/unknown source suggests Disable. A confirmed user-installed store copy follows the existing Uninstall-if-unused policy.

## DisneyNOW — `com.disney.datg.videoplatforms.android.watchdc`

[Disney's own 2017 announcement](https://d23.com/9-reasons-why-you-need-disneynow-this-very-minute/) describes streaming Disney Channel, Disney XD and Disney Junior programming and links its Android download directly to the exact reported package. This publisher-owned link resolves the historical identity concern from the initial hold. Its Android link now returns 404.

[Current DisneyNOW support](https://disneynow.com/help) states that mobile and streaming-device apps are no longer available. It describes desktop/mobile website alternatives; it does not promise Google TV browser compatibility. The role is a retired entertainment client, not a TV tuner or system service.

The entry records `defunct: true` and `play_store: false`, but uses Disable with review/default-Keep. Under the current shared policy, defunct apps never suggest Uninstall solely because they were user-installed. A person can explicitly choose Uninstall in Optimize, where the existing discontinued-service warning says there is nowhere to download it again. Re-enabling the installed client does not restore Disney's retired service or TV-provider access. This is not a claim that an archived APK is unavailable anywhere or that removing the client cancels a subscription.

## Live Channels — `com.google.android.tv`

The [exact Google publisher listing](https://play.google.com/store/apps/details?id=com.google.android.tv) describes watching built-in/IP tuners and other channel sources with a program guide. [Google's user instructions](https://support.google.com/androidtv/answer/6285282) describe selecting sources and opening the guide; they also distinguish devices with their own TV app from devices using Live Channels.

[AOSP customization documentation](https://source.android.com/docs/devices/tv/customize-tv-app) explicitly identifies Live Channels as Google's implementation of Live TV and maps the `com.google.android.tv` namespace. The [reference TV app documentation](https://source.android.com/docs/devices/tv/reference-tv-app) explains its TV Input Framework role and manufacturer customization. This establishes a known role and a specific consequence; it does not establish that every firmware can lose this app without affecting television functions.

The recommendation therefore says to **keep it for antenna/network tuners, its channel guide, or TV input/remote shortcuts that open it**. Disable/remove only when those functions are unused. Its deprecation message is not removal evidence. The initial choice remains Keep. Restoring an installed app may require tuner/guide/input setup again; deleted data is not restored. A user-installed store copy follows the shared Uninstall-if-unused policy, whereas a preinstalled or unknown-source copy suggests Disable. No change is made to TV providers, launcher/HOME, TV Input Framework services or protected system packages.

## Validation boundaries

The reviewed fixture exercises all three entries in desktop and mobile Optimize, including Caution, initial Keep and visible consequence text. Shared loader/planner tests cover exact identities, defaults, installed/disabled/missing states and recovery metadata. Specific regression assertions cover Disney's retired/no-store policy, DIRECTV's operator/subscription warning and Live Channels' tuner/input warning. The prior hold assertions are replaced, not bypassed. Final local checks, synthetic captures and hosted CI are recorded in the PR; none proves physical-device behavior.
