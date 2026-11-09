# Daily Optimize catalog review — 2026-11-07

This bounded batch deliberately selects two ordinary installed-app gaps from the
25-ID queue instead of promoting unresolved high-frequency system services.
One implementation agent and exactly one independent read-only reviewer
separately researched the same identities: **one accepted, one held**.
The previous 18 accepts and 31 unchanged holds were not re-researched.
Base main is `eb6279061bef5ea5c4c3b08be6fc3106b95d3744`; prior PR326 is merged.

## BrowseHere: optional Caution

Both agents opened the [exact TCL Group listing](https://play.google.com/store/apps/details?id=com.tcl.browser)
for `com.tcl.browser`. Its substantive description covers browsing, web video,
an IPTV player requiring the user's own playlist, and downloads.
[TCL support](https://support.tcl.com/en_US/us-googletv-setup-configuration/browsehere-on-tcl-google-tv)
also documents browsing, bookmarks/history and TV-remote navigation; store
installation is conditional on availability. The [official V6A manual](https://www.tcl.com/content/dam/brandsite/region/turkey/user-manual/tv/V6A-User-Manual.pdf)
maps Web to BrowseHere, without proving the mapping on every model.

The pinned source retains actual keep/use conflicts:
[55P7K keeps the Web-key target](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-55p7k/debloat.txt#L18),
[Q7C also keeps it for that shortcut](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-q7c-google-tv/debloat.txt#L144),
and a [Sony owner re-enabled it for dock browsing](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/sony-kd-65x82l/debloat.txt#L127).
Other profiles disable it, leave it alone as ambiguous, or report it absent;
these observations remain separate, not safety votes or device matches.

The functional assessment supports an explicit optional browser choice with
`risk: medium`, `method: disable`, `review: true`, both defaults false and initial
Keep. Disabling loses its browsing/video/IPTV/download functions and can break
its Web shortcut or links targeting it. Keep it if used or selected as browser.
Re-enable requires the installed package and does not recover deleted personal
browser data or reset browser/link choices. Firmware recovery is unverified.
No claim is made that other URLs, system browsing or remote keys remain usable.

Shared common lookup identifies only an installed exact ID. It does not adopt
the vendor's broad compatibility marketing or match a source profile to a TV.
The existing explicit store-app Uninstall option remains; this is a Disable
suggestion, not a Disable-only control. No WebView, launcher, input-handler,
setup/network/update, protected/caution, last-HOME, user-0, confirmation or async
identity authority changes.

## Xiaomi music: hold

`com.xiaomi.mimusic2` remains outside action lists. The exact Play page was
inaccessible; bounded exact-ID searches of Xiaomi and MiCode did not establish
the exact implementation, media dependencies or recovery contract. No search
result is proof of absence. Third-party debloat lists are not primary evidence.
The pinned source includes an [active Mi Music label](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/redmi-mitv-mftp0/debloat.txt#L87)
and a [commented occurrence](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/xiaomi-mitv-axso1/debloat.txt#L50).
Neither proves removal safety. Both agents agree to hold until material evidence
arrives or reconsideration on **2026-12-07**; that date is not acceptance.

## Scope and validation

The source remains pinned to `8672e68e8a24c1258e4662cd11daa4ed13795dad`, unreviewed
and non-actionable, with original source comments and full MIT notices intact.
The app remains proprietary. Frequency only prioritizes research. The queue
moves from 1,627 to **1,625 pending active candidates**: 78 source identities
curated, 32 held, and 236 without an active-disable observation.

Shared demo data and the substantive loader/planner/desktop/mobile fixture now
include BrowseHere, with visible consequence assertions and focus
`OPTIMIZE_CATALOG_FOCUS=com.tcl.browser`. Existing regression logic verifies
Caution, initial Keep, exclusion from Select all safe/Recommended, exact-ID
plan states and held-entry exclusion. Captures are labeled Chromium fixtures
with a synthetic Shield identity, not actual TCL software on Shield hardware.
No removal calls or hardware control occur in the capture tests.

The task receipt and PR record actual commands, exact tested head, regenerated
galleries and inspected evidence. Native Rust validation uses hosted CI with
27 GiB free locally. Historical mobile 372/372 evidence is not reused as a final
head run. Independent research/static review is not formal GitHub approval or
hardware proof. This run's explicit no-merge instruction remains its boundary.
