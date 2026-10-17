# Screensaver display names — 2026-10-17

Base: mobile-parity-pass `b095f697b2a24c3d2fb8f56cef7056586b6355ef`. Separate from pending density summary PR194 and the scaling preset-row layout slice.

Screensaver Current and Restore previous now use the embedded catalog's name for the exact owner package of a single well-formed Android component. For the existing checked-in `com.google.android.backdrop/.Backdrop` fixture, the common catalog supplies Ambient Mode. Basic Daydream retains its existing exact-component label. Unknown packages, lookalike package prefixes, malformed strings and multiple-component lists remain Custom screensaver; names are not guessed from class/package suffixes. Off and missing values retain their existing display states.

A native keyboard-accessible Component details disclosure keeps both raw Current and Previous values visible on request, including the configured component while Off. Missing null is shown as Not reported; an explicit empty value is None set. Friendly names describe the catalog package, not proof of an installed service or a service-specific display label.

The existing local `app_list_for_device` command reads compiled-in app-list data only; it performs no ADB/device lookup. Its result uses the same load-generation and connection guard as the other Tweaks reads, and names clear on connection changes. Catalog errors degrade to Custom screensaver. No catalog data, setting values, original-value identity rules, preset selection, write order, lock/paywall behavior or restore writes changed.

Tests use the actual checked-in Ambient Mode catalog entry and cover known/custom/Off/missing, lookalike and multiple-component values, raw details via keyboard, catalog error, stale catalog responses across reconnect, exact existing restoration and partial-write guards. All new display fixture flows assert no setting writes. Existing write tests still verify the exact component strings and write order.

Screenshots at 320px show [known](known-320-simulated.png), [custom long component](custom-320-simulated.png), [Off with retained raw component](off-320-simulated.png) and [missing](missing-320-simulated.png). These are synthetic browser fixtures. No phone/TV query, install, permission, launcher-icon or native-export change occurred. No physical accessibility or Android acceptance is claimed.

Focused suite with evidence: 29/29 passed; Svelte check 0 errors/0 warnings and production build passed. All 302 mobile tests passed; see [validation receipt](validation.json). No Rust source changed.
