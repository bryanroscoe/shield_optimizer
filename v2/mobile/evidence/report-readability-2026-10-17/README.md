# Diagnostic report readability — simulated evidence

Preserves checkpoint `5bc1f5e7fb59b80317accd46057fd9cfc363f581` and normally merges
`mobile-parity-pass` at `3a1881e9b73c768be3a417abbea14cf524cfbe40` (integration `51d7195`).
The only conflict was Diagnostics imports; both the report component and reviewed health helpers remain.

Playback now has separate video, display, audio/frame-rate and interpretation groups. Each display
mode has its own row and active label. Resource samples separate CPU/duration and each interface's
receive/send rates. Labels precede values, notes stay beside the report, and warning text is retained.
Rem text sizing, wrapping, flexible columns, 44px minimum actions and keyboard focus outlines support
small screens and text enlargement. The viewport still permits pinch zoom. No backend, report payload,
export formatter, precision, unit conversion or request lifecycle changes are made by this slice.
Failed rereads retain the existing behavior: clear the old playback report and show the error.

## Evidence

All values are deliberately synthetic. PNGs and a sampled video frame were visually inspected.

| Viewport / root text | Playback | Resources |
|---|---|---|
| 320px / 100% | [Full Diagnostics screen](playback-320-100-simulated.png) | [Resource card](resources-320-100-simulated.png) |
| 384px / 100% | [Full Diagnostics screen](playback-384-100-simulated.png) | [Resource card](resources-384-100-simulated.png) |
| 320px / 200% | [Full Diagnostics screen](playback-320-200-simulated.png) | [Resource card](resources-320-200-simulated.png) |

[5.44-second scrolling report video](reports-simulated.webm) shows the playback sections and resource
interfaces. Raw intermediate recordings are ignored. No desktop gallery screen changes in this PR.

## Reproduce

From `v2/mobile`, with existing dependencies and Chromium:

```sh
REPORTS_EVIDENCE=1 node --test tests/playback-audio.test.mjs
npm test
npm run check
npm run build
```

The 14 focused cases include narrow/large-text overflow checks, long unbroken labels, warnings,
unknown policy/encoding text, precise mode rates, null versus zero counters, large rates, unchanged
source payloads, empty/error states, keyboard activation, reconnect invalidation and request admission.
The full combined mobile suite passed 263 tests, including current settings/modal/selection/thermal
regressions. Mobile Svelte check: 0 errors / 0 warnings; build passed. The parent v2 tsconfig emitted
its existing missing `.svelte-kit/tsconfig.json` startup notice before those successful mobile gates.

Rustfmt workspace check, all-target Clippy with warnings denied for core/desktop/mobile/plugin/license,
and offline core/mobile/license tests passed (459 core tests). Vendored adb_client tests passed
54 with one ignored after allowing local loopback listeners; the restricted-sandbox attempt blocked
four socket tests. This is macOS host validation; cross-OS CI and independent review remain pending.

No phone/TV queries, installations, physical text-scaling/pinch gestures, Android native behavior,
release, or merge were performed. Root-font enlargement is browser simulation, not Android font-scale
verification. Desktop frontend check could not start because the existing checkout lacks `svelte-kit`; its build
was therefore not run, and no dependencies were installed. Follow-up modal P2s are recorded
separately in [the release checklist](../../RELEASE.md).
