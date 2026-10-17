# Report safety fidelity — simulated evidence

Base: mobile `0c78c06823b4827aa3a4345b526f688b2813d739`, after merged PR203 documentation and PR202 Restore/removal-selection fixes. This follow-up changes only Apps.knownVerdict lookup precedence; it does not reclassify packages or change native safety enforcement.

## Contract and reproduction

AppDetailSheet calls `onVerdict(pkg, null)` only after its current guarded safety lookup fails; a successful response supplies the concrete verdict. Apps stores that result in sheetVerdicts. Previously `sheetVerdicts[pkg] ?? catalog verdict` erased the distinction between a present null and an absent sheet result, so an older concrete catalog verdict could appear in a report while the sheet said Safety unavailable.

`before.txt` records the failure on unchanged source: stale Caution and Protected verdicts appeared instead of unavailable. The fix uses an own-property check: a present sheet result, including null, takes precedence; an absent result still permits the catalog fallback. Existing openReport behavior retries a null verdict, initially exports unavailable with null source, retains unavailable after retry failure, and accepts a successful fresh result only for the current report request/package/session. Unknown remains unknown; unavailable is not recast as Safe or Protected. A successful report retry updates that report, not the underlying sheet's failed read.

## Checks

Nine focused mocked browser regressions pass in `focused.txt`. They cover stale Safe/Caution/Protected catalog values followed by sheet failure; exact preview/copied JSON while the report retry is pending and failed; successful retry to Unknown/no_record; late report responses after close/reopen, package change and same-endpoint reconnect; absent sheet cache fallback; late sheet replies across package/session changes; and reuse of a successful current sheet verdict without another report retry. Existing target, request, generation and liveness checks are unchanged. All 343 combined mobile tests pass in `combined-mobile.txt`; frontend check is zero errors/warnings and production build passes with existing lock-matching mobile dependencies.

No Rust or Android-native source changed, so native compilation was not repeated. Independent review and exact-head CI remain gates for this new branch. Historical validation and completed PR202/203 statuses remain intact.

## Simulated visual evidence

All lookups, clipboard writes and devices are browser fixtures at 384×812. Screenshots were visually inspected; no physical safety query, app mutation or device operation occurred.

- [Sheet reports Safety unavailable](sheet-unavailable-simulated.png).
- [Report retains unavailable with null source](report-unavailable-simulated.png), with [exact copied report](unavailable-report-simulated.json).
- [Fresh report retry returns Unknown](retry-unknown-simulated.png), with [exact copied report](unknown-report-simulated.json).
- [4.88-second interaction video](report-safety-simulated.webm) shows the failed sheet lookup, report copy, failed retry and fresh successful retry. Raw recorder intermediates are omitted.

Reproduce from v2/mobile using existing dependencies: `REPORT_SAFETY_EVIDENCE=1 node --test --test-concurrency=1 --test-name-pattern='report safety:' tests/apps-catalog.test.mjs`. Run the full suite with `npm test`, plus `npm run check` and `npm run build`. Desktop gallery regeneration is inapplicable because no captured desktop UI changed. Physical Android/clipboard acceptance, release and publication remain outside this slice.
