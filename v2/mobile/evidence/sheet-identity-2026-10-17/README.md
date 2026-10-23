# Immutable app-sheet opening identity — P1 safety fix

Base: current `mobile-parity-pass` `6f3deefa5e46a03d292aa4ef1e84927440b03766`. This separate branch changes AppSheetHost, its regression fixtures, evidence and checklist only; it does not rewrite PR181 or either sealed PR182 commit.

## Reproduction and change

Diagnostics and Optimize retain their selected app in parent state. The previous Host effect observed that app **and** current connection state, hid the sheet on disconnect without clearing the parent, then assigned the old package a new serial/generation on reconnect. Its later `current(t)` check accepted that rebound target. Six pre-fix cases (two screens × disconnect/same-endpoint reconnect/different-TV switch) reproduce stale-sheet resurrection in mocks.

Only a new explicit app selection now captures a frozen opening target. Session/liveness changes invalidate it instead of rebinding it: clear shown app, target, pending request/read tokens, measurements, busy/uninstalled state, confirmation and paywall, and call the parent close callback. Closing ordinarily does the same cleanup. `current(t)` also requires the **exact opening object**, rejecting a pending result from a closed selection even if a new selection uses the same serial and generation. Existing safety/verdict and native transport gates remain in place.

## Regression matrix

Both Diagnostics and Optimize cover:

- Disconnect/reconnect, direct same-endpoint reconnect and different-TV switch; no old sheet or action survives. A fresh explicit selection can target the current mock connection.
- Pending safety reply across a switch, and across close/reopen on the same connection; neither opens a stale confirmation.
- Disable and Uninstall confirmation cleanup on reconnect; no mutation from those old intents.
- Paywall cleanup and stale measurement rejection; a new opening shows its own 55.0 MB fixture, not the old 999 MB reply.
- Failed reconnect and transient liveness loss; returning live does not resurrect the old selection.
- When combined with PR182, invalidation also releases the shared document lock. Its separate keyboard/focus/scroll tests remain part of the combined suite.

The fixture supplies fresh synthetic diagnostic metrics before making a new selection after reconnect; reconnect clearing the metrics cache is not permission to resurrect a previous app sheet. Commands are intercepted through the fake Tauri bridge. Some regression cases deliberately confirm a synthetic Disable to assert the new serial, never a real device command.

## Visual evidence and reproduction

Synthetic TV A/B, Chromium on macOS, 384×812. No phone/TV connection, device query, install or setting change occurred.

- [Opened on mock A](opened-on-a-simulated.png)
- [A's pending confirmation](a-confirmation-simulated.png)
- [B requires a new selection](b-requires-fresh-selection-simulated.png)
- [Fresh B selection](fresh-b-selection-simulated.png)
- [Short A-to-B invalidation video](sheet-opening-identity-simulated.webm)
- [Scenario receipt](scenario.json): zero mutation calls in this recording; both confirmations are canceled/invalidated.

Frames were visually inspected; all identities/data are invented. The video uses a programmatic mock reconnection, not an actual phone or native transport. It demonstrates the rendered state transition; tests bind the mutation/identity assertions. Raw intermediate videos are ignored.

```sh
SHEET_IDENTITY_EVIDENCE=1 node --test tests/sheet-opening-identity.test.mjs
npm test
npm run check
npm run build
```

Physical connection/lifecycle/device enforcement remains a phone/TV acceptance gate. Browser results do not prove native command timing or Android behavior. No broad release approval is implied; independent review of this safety fix is required.

## Validation receipt

- Pre-fix: all six stale-opening resurrection cases fail on base6f3deef.
- Focused matrix:16 passed; with opt-in capture:17 passed.
- Standalone current-base safety branch:245 mobile tests passed, zero failures.
- Isolated current base + PR182 `1dd9bef7f949f66e4a6b6e94ba11e5c319c9baa0` + identical safety source/tests:252 mobile tests passed, zero failures; no source overlap/conflict. This includes both modal long-sheet keyboard regressions.
- Both trees: Svelte check zero errors/warnings; Vite build passed. `git diff --check` passed. Existing fresh-worktree parent tsconfig notice remains.
- No native source changed or physical device tested. Independent source review and CI remain required before release.
