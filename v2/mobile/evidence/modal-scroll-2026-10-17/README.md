# App-detail modal scroll/focus evidence

Base: `mobile-parity-pass` at `5f4fc141ceda0307cb86b68bfc50433b9ffcc49d`. Separate from PR181; no settings/inset/icon changes in this branch.

Root cause: AppDetailSheet used a fixed overlay, but neither it nor its hosts locked the document. Its independently scrolling panel allowed scroll chaining. Closing a child dialog also had no shared ownership or focus-restoration rule.

The `modal` Svelte action locks the document once, preserves individual inline style values/priorities and both scroll coordinates, and releases only when its last owner closes. App detail, action menu, report, confirmation and paywall layers share this ownership. Scrolling stays available inside long sheets with overscroll containment. Only the top layer handles Tab/Escape; closing it restores its opener. Last focus is remembered across asynchronous safety checks that temporarily disable a triggering button. Destroyed/disconnected launchers are not refocused. No global touch/pinch/text-size restriction is introduced.

## Reproduce

From `v2/mobile`, with existing dependencies and Playwright Chromium:

```sh
MODAL_EVIDENCE=1 node --test --test-name-pattern='modal' tests/parity-screens.test.mjs
npm test
npm run check
npm run build
```

Focused checks cover background wheel/edge scroll, actual inner-sheet scrolling, repeated open/close, exact existing style/scroll restoration, focus return, nested confirmation and paywall, Tab wrap, Escape/backdrop dismissal, screen teardown, disconnect, and out-of-order parent/child destruction. App catalog/report/menu tests run in the full suite.

## Visual evidence

Synthetic TV data, Chromium/macOS, 384×812; these are browser simulations, not phone or TV recordings. No real device was connected, no app/device setting changed, and no app installed. The mock Disable confirmation is canceled, not executed.

- [Scrolled diagnostic page before opening](diagnostics-before-simulated.png)
- [App details open](app-details-simulated.png)
- [Nested confirmation](nested-confirm-simulated.png)
- [Diagnostic page restored](diagnostics-restored-simulated.png)
- [Short open/scroll/nest/dismiss video](modal-scroll-simulated.webm)

Frames were visually inspected. The evidence fixture uses invented app/package names and contains no personal data. The named video is the deliverable; intermediate raw recordings are ignored.

Browser coverage does not prove Android touch/overscroll, software keyboard or accessibility behavior. On a phone, check long-sheet touch scroll at both edges, backdrop dismissal, keyboard, rotation, pinch zoom, enlarged text and repeated nested flows. No physical validation is claimed.

## Deferred owner findings

The durable general list is [PR181's release checklist](https://github.com/bryanroscoe/shield_optimizer/blob/3c15c2147a0e4b2cf0768592d33b8f682811bcbc/v2/mobile/RELEASE-POLISH.md). RAM/storage, thermal context, reports, exports and app inventory remain separate tasks. Independent review also found an inherited density-only display override label issue: `scaleLine` can show physical density and “device default” while selection recognizes the override. Preserve PR181's approved head; add a density-only fixture and label correction in the next settings slice.

Validation: **230 full mobile tests passed**, **6 focused modal checks including capture passed**, Svelte check zero errors/warnings, Vite build passed, and `git diff --check` passed. The existing fresh-worktree parent `.svelte-kit/tsconfig.json` Vite notice remains. No native source changed; no native APK/phone validation is claimed for this slice.
