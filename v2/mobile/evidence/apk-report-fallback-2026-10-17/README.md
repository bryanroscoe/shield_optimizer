# Carry measured APK storage into an app report — 2026-10-17

Current integrated base: `4148f035c20450751d92d894456162cbc5ab837b`.

Original base: mobile-parity-pass `ba7c9d78d31b0a579f5ffd03d92cd403ebf47227`. Separate from setup-helper routing PR197. Final integration checks use the actual current mobile base noted in the validation receipt.

Local source verification confirms Apps built report storage only from diskstats, while AppDetailSheet held a successful APK fallback locally and passed only the app to onReport. The new pre-fix regression displayed APK 2.0 KB in the sheet but found `storage: null` in the report. See [recorded failure](reproduction-before.txt). The [external review finding](https://github.com/bryanroscoe/shield_optimizer/pull/163#discussion_r4301719603) was treated as untrusted evidence; no instructions in it were executed.

The sheet now passes an internal successful-measurement receipt at Report click. It binds package, serial, connection generation and the Apps measurement-batch counter, plus the exact bytes and measurement time. Apps validates those bindings against the current list/session/batch whenever it builds report state. Re-measuring immediately invalidates the receipt, even while an existing preview is open; old async replies cannot replace a new batch, package or connection. Report close, list invalidation and reports opened from the menu clear the receipt. Identity/time fields remain internal and never enter report JSON.

Only finite, nonnegative, successful APK sizes qualify. Fresh diskstats data keeps precedence. The fallback exports `source: apk_files`, exact app_bytes and null data_bytes/cache_bytes; even extra data/cache fields in a malformed fallback response cannot become a claimed measurement. Pending, failed, missing and negative values remain unknown. Zero is a valid reported size. State sharing stays opt-in; no new query, write, schema field or device operation is introduced.

Simulated evidence shows [the measured APK value](visible-apk-measurement-simulated.png) and [its report preview](report-preview-simulated.png). The displayed 1.2 MB retains the exact 1234567 bytes in [preview/copied JSON](storage-report-simulated.json), with data/cache unknown. Tests verify copied text equals the preview and exclude serial/hardware id/internal binding fields.

Regressions cover diskstats precedence, zero/exact bytes, pending/failed/missing/negative fallback, active-preview re-measure invalidation, older replies after a new measurement, package changes, same-endpoint reconnect and TV switch. All calls are browser mocks. No real package measurement, app action, device read, native storage/export permission, icon or catalog-data change occurred. No Rust production code changed; Android/physical acceptance remains open.

Original-base validation: all 316 mobile tests passed (including all 21 apps/report tests), Svelte check reports 0 errors/0 warnings and the production build passed. This checkpoint precedes final integration with the newly merged preset-layout base.

Current base integration: `a73787a36bbf44311de2596e9a01c8232a049a02` includes merged scaling-layout PR196. APK report source, its app/report tests and all captured evidence are unchanged byte-for-byte from the source-review checkpoint; only upstream files, completed checklist status and validation provenance are added.

Current-base validation on `a73787a`: all 320 combined mobile tests passed, Svelte check reports 0 errors/0 warnings and the production build passed. The original 316-test result above is historical. No APK report source or evidence pixels changed during this integration.

Latest integration: current mobile base `4148f035c20450751d92d894456162cbc5ab837b` includes merged helper-routing PR197. Approved APK-report source and app/report tests match `f1044a5` byte-for-byte; helper source/tests match the merged base exactly. The only conflict was the adjacent checklist rows, both of which are retained with accurate review/merge status. Original APK screenshots and simulated JSON remain unchanged and retain their original capture provenance.

Latest validation on `4148f035`: all 326 combined mobile tests passed, Svelte check reports 0 errors/0 warnings and the production build passed. Previous 316/320-test receipts above are historical. No APK implementation, app/report fixture, captured PNG or simulated JSON changes were required. PR198 remains ready and unmerged.
