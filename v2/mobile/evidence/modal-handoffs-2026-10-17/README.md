# Modal handoffs and stock takeover — simulated evidence

Fresh branch from `mobile-parity-pass` at `3a1881e9b73c768be3a417abbea14cf524cfbe40`.
Report PR185 remains untouched at `330275bd9858c9ea6ef8e22ab8df424350a498ef`.

## Reproduced before fixing

Both new screen regressions failed on the unchanged base. Menu → Report → Escape failed to
return focus to the original app row. Stock takeover left the document unlocked when its
confirmation opened. [Original failing test output](reproduction-before.txt) records both failures.

## Scoped changes

- Stock takeover registers with the existing shared modal action, declares `aria-modal`, and
  scrolls internally on short screens. Focus trapping, Escape cancellation and document locking
  now use the same ownership rules as other dialogs. The shared empty-dialog fallback recognizes
  `alertdialog` too.
- Empty-stack cleanup waits one microtask so Svelte can mount a replacement in the same update.
  The replacement inherits the retained original opener and document lock. Stack revisions prevent
  superseded cleanup callbacks from restoring focus into a new modal. A genuine close still
  clears the lock/opener before a later asynchronous opening, which then captures current focus.

No launcher logic, report fields, exports, device commands or safety gates changed. The two modal
P2s documented separately in the report PR are implemented here; independent review is still pending.

## Evidence

All screens use mocked Tauri replies. No destructive confirmation is accepted by the new modal
screen cases; assertions pin zero stock-disable/save-snapshot calls in the takeover case.

- [Report opened from app menu](menu-to-report-simulated.png)
- [Original app row focused after closing](original-app-row-restored-simulated.png)
- [Scrolled Launcher before opening](launcher-before-simulated.png)
- [Stock takeover heading on a 384×420 viewport](stock-takeover-heading-simulated.png)
- [Keyboard wrap reveals Cancel while the document remains locked](stock-takeover-keyboard-simulated.png)
- [Original Launcher scroll and opener restored](launcher-restored-simulated.png)
- [Menu → Report → close video](menu-report-close-simulated.webm)
- [Takeover cancellation video](takeover-cancel-simulated.webm)

Raw intermediate recordings are ignored. Screenshots and sampled video frames are visually inspected.
No desktop gallery screens changed.

## Reproduce

From `v2/mobile`, with existing dependencies and Chromium:

```sh
node --test tests/modal-lifecycle.test.mjs tests/apps-catalog.test.mjs tests/launcher.test.mjs tests/parity-screens.test.mjs
MODAL_HANDOFF_EVIDENCE=1 node --test --test-name-pattern='menu Report returns|stock takeover dialog locks' tests/apps-catalog.test.mjs tests/launcher.test.mjs
npm test
npm run check
npm run build
```

The focused suites cover real menu/report and takeover flows plus same-update replacement,
resolved/canceled/rejected asynchronous transitions, genuine close followed by an unrelated opener,
nested/out-of-order teardown, superseded cleanup, removed opener, duplicate teardown, empty
alertdialog focus, Escape/Tab, short-screen Cancel visibility, backdrop cancellation, scroll and
style restoration. Existing nested app-sheet/paywall/confirmation and connection-invalidated
selection regressions remain part of the full suite.

## Limits

Host Chromium simulation on macOS only; no phone/TV query, installation, real launcher mutation,
physical touch/pinch/text-scaling claim, release or merge. Native Android callbacks, cross-OS CI and
independent review remain pending. The existing parent v2 missing `.svelte-kit/tsconfig.json` notice
appears before successful mobile checks/build. No dependencies were installed.
