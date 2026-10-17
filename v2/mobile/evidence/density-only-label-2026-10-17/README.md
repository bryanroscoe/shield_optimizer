# Density-only display summary — 2026-10-17

Based on mobile-parity-pass `9265e5fdbf4c9c73f7cea5522e50c50dbcd90fe0`. Separate from screenshot-retention PR193.

Current source and fixtures reproduce one definite label bug: physical size 3840x2160, physical density 640 and override density 300 display as `3840x2160 @ 640 · device default · Custom size / density`. The pre-fix regression failed with that exact string; see [recorded failure](reproduction-before.txt) and [before screenshot](before-simulated.png).

A three-line summary-only branch now displays `3840x2160 @ 300 · density override · Custom size / density`. When size is missing it explicitly says `Size not reported`; no resolution is invented. Actual values, parsers, preset matching, API calls, confirmations, write ordering and restoration are unchanged. A matching density-only 1920x1080/320 readback still selects 1080p, not Reset; custom 3840x2160/300 selects neither preset nor Reset. Tests also retain physical defaults, size-only overrides, missing readback and existing failed-read behavior. New fixture flows assert no scaling or setting write occurred.

[Normal summary](after-384-simulated.png) is captured at 384px. [Narrow enlarged summary](after-320-summary-text-200-simulated.png) doubles the summary’s computed text size at 320px; the card fits horizontally. This is browser simulation, not Android accessibility or physical-phone verification.

When all card text is doubled at 320px, the unchanged preset row measures 270px of content inside 240px. [Measurements](full-card-text-200-observations.json) and [simulated card](known-full-card-text-200-simulated.png) record that separate layout limitation; the long card passes behind the fixed bottom tabs in this capture. This label fix does not claim whole-card large-text acceptance.

Remaining screensaver diagnosis: `screensaverLabel` still returns raw custom components for Current and Restore previous. Existing tests deliberately use `com.google.android.backdrop/.Backdrop` and verify exact restoration; custom explanation, identity binding and ordered writes already exist. The common catalog calls that package Ambient Mode, but Tweaks currently has no component-name resolver. A separate display choice remains: use catalog package names with visible raw component details, or a generic Custom screensaver label with the raw component retained. Arbitrary vendor service identities must not be guessed. No screensaver code or write behavior changed here.

No existing density-specific branch was found in local/remote branch names or relevant commit subjects. Earlier settings PR181 remains intact. No launcher asset, catalog, device query, native storage/export permission, credential or security change was made. Existing launcher optical comparison remains available in the settings evidence directory; no icon change was attempted.

Validation: 302/302 full mobile tests, three focused evidence tests, Svelte 0 errors/0 warnings and production build passed. See [validation receipt](validation.json). Open PR title searches for density/screensaver returned no duplicate focused PR. No Rust or desktop UI source changed.
