# Display scaling preset wrapping — 2026-10-17

Original base: `b095f697b2a24c3d2fb8f56cef7056586b6355ef`. Now integrates mobile-parity-pass `35b5deaa044bb2218c720eeaa64858486ffc195f`, including reviewed and merged density summary PR194. Screensaver-label PR195 remains separate. The scoped reviewed layout code is unchanged; only the merged density summary branch is added.

The existing display-scaling preset row overflows at 320px with all card text doubled: 271px of content inside a 240px row in this fixture. [Before screenshot](before-320-200-simulated.png) clips Reset; the [pre-fix regression](reproduction-before.txt) fails on those dimensions.

Only this segmented row now permits wrapping, uses intrinsic button widths and retains minimum 44×44px targets. At 320px/200%, Reset moves onto a second line instead of clipping. Other segmented controls are untouched. Labels, preset values, active selection, confirmation wording, readback and setting writes are unchanged; PR194's merged summary fix is now included without changing the scoped layout rules.

Four browser fixtures at 320/384px and 100/200% card text verify row and card horizontal fit, all labels, minimum touch targets, viewport-contained buttons, keyboard Tab order, Enter opening every confirmation and Cancel restoring focus. The same confirmed 1080p selection remains active after all cancellations, and no setting/scaling writes are issued.

Simulated row captures: [320px normal](after-320-100-simulated.png), [320px doubled text](after-320-200-simulated.png), [384px normal](after-384-100-simulated.png), [384px doubled text](after-384-200-simulated.png). These crop the preset row after scrolling it into view and retain visible keyboard focus on Reset. Enlarged text doubles computed card text sizes, not just the root font. This is browser simulation rather than Android accessibility or physical touch verification. Viewport zoom restrictions were not added.

Focused layout tests: 4/4 passed. All 304 mobile tests passed, Svelte check reports 0 errors/0 warnings and the production build passed; see [validation receipt](validation.json). No Rust, device, catalog, icon, native-export, permission or setting-value changes occurred.

Integration provenance: all existing preset screenshots are retained byte-for-byte. Their fixture uses both size and density overrides, so PR194’s density-only summary branch does not change these pixels. Imported PR194 captures of the old full-card overflow remain historical evidence from before this CSS fix; the preset-wrap captures demonstrate the prepared fix.

Current integration on `35b5deaa`: all 306 mobile tests passed, Svelte check reports 0 errors/0 warnings and the production build passed. Existing 304-test results above describe the original review head. Both density readback and preset-layout regressions are retained; no screenshots were regenerated.
