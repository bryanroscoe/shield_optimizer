# Mobile app-detail state regressions — simulated

Base: `d14363bc76c05f2342289d9503e95175b2d44da2` on mobile-parity-pass. This focused follow-up verifies two PR163 review findings against source contracts, independently of the external review text.

The pure engine's Restore planner returns Enable only when an app is installed and disabled (`engine/optimize.rs`). `planItemApp` carries `planRowState` into AppSheetHost and AppDetailSheet, whose toggle uses that state. Mapping Enable to enabled therefore offered Disable. The corrected mapping retains disabled, missing and already-enabled skip semantics.

Optimize reloads its plan on `onChanged`; Diagnostics reloads health and installed/safety lookups. Neither callback clears its selected AppItem. AppSheetHost already treats false mutation results as potentially landed and refreshes, but previously retained the old child and parent selection. It now closes both after false disable/uninstall responses, using the existing invalidation path. The error message remains an error, with no success claim. Confirmed uninstall still exposes Reinstall; identity, generation, liveness, fresh-safety and stale-request guards are unchanged.

## Reproduction and validation

`before.txt` records all six new regressions failing on unchanged production source: Restore state and UI, plus false disable/uninstall responses in both callers. The mock command changes the synthetic installed/plan state before returning false. The tests require the sheet and parent selection to close, the error to remain visible, a caller refresh, and no stale reopening after reconnect.

`after-target-guards.txt` records 35 passing row/identity tests. `after-removal-recovery.txt` records seven passing focused browser tests, including both callers retaining Reinstall after a confirmed uninstall. `planner-tests.txt` records all five engine planner tests passing. All 334 combined mobile tests pass in `combined-mobile.txt`; Svelte check is zero errors/warnings and the production build passes with existing lock-matching mobile dependencies. No native source changed; Android compilation and physical-device validation were not repeated for these two frontend fixes.

## Observable evidence

All screens and commands use synthetic browser fixtures at 384×812; no device or ADB calls occurred.

- [Restore detail with Enable](restore-enable-simulated.png), [short video](restore-enable-simulated.webm) (2.16 seconds): the only toggle mutation dispatched is mocked enable_package.
- [Before inconclusive removal](before-inconclusive-simulated.png), [selection cleared with error](inconclusive-cleared-simulated.png), [short video](inconclusive-cleared-simulated.webm) (2.96 seconds): a mocked mutation lands but returns false; the sheet closes and the caller refreshes without claiming success.

Reproduce evidence with `DETAIL_STATE_EVIDENCE=1 node --test --test-concurrency=1 tests/sheet-opening-identity.test.mjs` from v2/mobile. Raw recorder intermediates are omitted. Desktop screenshot galleries are unchanged because these are mobile-only changes.

No real app mutations, device queries, installs, credentials, network settings, releases or merges were performed. Physical phone/TV acceptance and independent review remain required.
