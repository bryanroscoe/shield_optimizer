# TV Recommendations conservative safety correction

The v2 SHIELD catalog previously described `com.google.android.tvrecommendations`
as Safe, selected it for default optimization, and claimed custom launchers had no
consequences. It now defaults to Keep and produces Caution, with original wording
about a reported phone-remote dependency after reboot. Select all safe also leaves
it on Keep. Explicit manual choices and the existing Restore default remain.
No device action or engine behavior was added.

Evidence is an [external report pinned at 30626a8](https://github.com/Gathin23/cleanyourtv/blob/30626a8e4714f86bf837384b109b5c73ee736917/docs/TROUBLESHOOTING.md#shield-tv-phone-app-connects-but-no-key-does-anything).
It describes reboot comparisons on SHIELD darcy and foster_e devices running
Android11. This project did not reproduce that hardware behavior. Model years,
exact firmware builds and behavior on other devices are not inferred from package
identity. This conservative default applies to the SHIELD catalog because retaining
the package avoids a reported loss of remote input; it does not claim every model
has been proved affected. Restore copy also leaves recovery unverified.

Only the package identity and reported consequence were used as research evidence.
The new catalog text and tests were independently written; upstream code and prose
were not copied. The inspected upstream root license is MIT. No strict clean-room
certification is claimed because upstream material was inspected.

Local validation passed:

- Shipped-data Rust regression: non-default removal, Caution, restore default,
  pinned provenance and consequence/model limits.
- Offline browser regression: full App List and Optimize consequence display,
  default Keep, Select all safe exclusion and Restore Enable for simulated disabled
  state. No removal command was issued.
- Existing recommendation-label regressions, Svelte check (0 errors/0 warnings),
  production frontend build and regenerated 16 dark + 16 light gallery frames/GIFs.
- Workspace Rust tests: core465, desktop114 (one existing live test ignored),
  mobile26, license2 and registry9 passed. Workspace fmt and clippy passed.

Exact-head CI and independent review gate the PR. Local macOS checks do not prove
all platforms. No real phone/TV commands, disabling, installs or APK execution
occurred. Shared catalog data will affect mobile only when this change is integrated
into its branch; this PR does not cross mobile's physical/release acceptance gates.

[App List caution detail](caution-detail-simulated.png),
[Optimize Keep and consequence](optimize-keep-simulated.png) and the
[3.36-second video](caution-review-simulated.webm) are **SIMULATED**. Asset hashes
are recorded in `visual-sha256.json`. The demo target is enabled to make the
automatic-selection assertion meaningful; disabled restoration is a test-only
fixture. Demo JSON was regenerated as common followed by SHIELD, including the
existing Ambient Mode ordering correction without changing its data.

Scope is the requested v2 catalog. The separately maintained v1 PowerShell entry
still has the older default/description and needs a focused follow-up; it is not
covered by this PR's validation. PR211's experimental catalog and its enrichment
remain separate and unmerged, and PR180/PR163 gates are untouched.
