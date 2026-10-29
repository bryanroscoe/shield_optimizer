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
