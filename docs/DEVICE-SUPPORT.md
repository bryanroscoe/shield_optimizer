# Device support tiers

How ATV Optimizer decides what to tell a user about their TV, and how a TV moves from one tier to
another. Written 2026-11-01, when the telemetry backend went live.

## The tiers

Both apps show one of five tiers for a connected TV. The wording below is what users see
(`v2/shared/support.ts`).

| Tier | Label | What it tells the user | What it means for us |
| --- | --- | --- | --- |
| `unknown` | Not verified | "We haven't verified this TV model yet." | No rule matches, or the TV didn't say what it is. Claims nothing either way. |
| `beta` | Beta | "Beta: this TV may not work fully. … Reports from it help us improve support." | A few people use it and actions mostly work. |
| `supported` | Supported | The rule's note, or "This TV is supported." | Many installs, a clean last 30 days, no unexplained error pattern. |
| `fully_supported` | Fully supported | The rule's note, or "This TV is fully supported." | The owner has run the physical test script on one. Never set from data alone. |
| `not_supported` | Not supported | "This TV isn't supported: <note>" | The platform blocks what the app does (Fire OS, say). Owner-confirmed. |

A tier is a label, nothing more. It never feeds `engine::safety`: a fully supported TV gets
exactly the same do-not-disable gates as an unknown one, and `not_supported` adds a stronger
warning before destructive actions but never blocks re-enabling an app, restoring a snapshot or
Recovery. Whoever can change the tier list can change a label, not what the app is allowed to do.

## The evidence rule

A tier is only ever claimed from what a TV actually reported about itself. A rule matches the TV's
own `ro.product.manufacturer`, `ro.product.brand`, `ro.product.model` (prefix) and
`ro.product.device`. Empty values and the literal `unknown` count as not reported, and a field
that wasn't reported can't match. An unauthorized or unreadable TV has no properties, so it is
`unknown`. That is the same identity rule as the rest of the repo: unreadable means unknown, and
unknown claims nothing.

The same goes for the data behind a tier change. Only what TVs reported to the telemetry backend
counts: installs, action results and error codes. A forum post, a spec sheet or "it's the same
chipset as X" is a reason to look, not evidence for a tier. The weekly report never suggests a tier
for a row with no manufacturer or model.

## One source of truth

[`v2/crates/core/data/device-support.json`](../v2/crates/core/data/device-support.json) is the
list. Everything else is a copy of it:

- **The apps** embed it at compile time (`crates/core/src/commands/loader.rs`) and match it in the
  pure `engine/support.rs`. A release ships whatever the file said when it was built.
- **The live Worker** serves `GET /v1/support-tiers`. When reports are on, the apps fetch it, cache
  it for about an hour and use it in place of the embedded list, unless its `updated` is older than
  the embedded file's. The [Support tiers sync](../.github/workflows/support-tiers-sync.yml)
  workflow pushes the file there on every merge to `main` that touches it, then reads it back and
  fails unless the served rules are exactly the file's.
- **The Worker's seed migration** (`v2/telemetry-worker/migrations/0001_init.sql`) holds the list as
  it was on 2026-10-31. It only matters for a fresh database; the sync overwrites it.

Never edit the live list by hand (`PUT /v1/admin/support-tiers` with curl). The next sync would
silently undo it, and released apps would still embed the old file. If the live list is ever wrong,
re-run the sync from the Actions tab (**Support tiers sync → Run workflow**).

### The file

```json
{
  "schema": 1,
  "updated": "2026-11-01",
  "rules": [
    { "match": { "manufacturer": "NVIDIA", "model_prefix": "SHIELD" }, "tier": "fully_supported", "note": "..." }
  ]
}
```

- `match` keys: `manufacturer`, `brand`, `device` (exact, case-insensitive) and `model_prefix`
  (case-insensitive prefix of the model). Every key present must match. At least one key.
- Rules are checked in order and **the first match wins**, so a specific rule (`TCL` + `QM7`) goes
  above a broad one (`TCL`). The check refuses a rule that an earlier rule makes unreachable.
- `note`: one sentence, 200 characters at most, shown to the user. Say what the tier is based on.
- `updated`: `YYYY-MM-DD`, **today's date in every change**. Apps keep a cached copy of the live
  list, and only a newer embedded date makes a new release override that cache. A future date is
  worse: every release that ships it would ignore the live list until that day. The PR check
  enforces both.
- Limits: 200 rules (the Worker's cap), 64 characters per match value, no surrounding whitespace.
- Prefer `manufacturer`, `model_prefix` and `device`. The device report has no brand column, so it
  can't tell which TVs a `brand` rule covers and shows them as `? (brand rule)`.

## The weekly report

[Device support report](../.github/workflows/device-support-report.yml) runs every Monday at
14:23 UTC (and on demand from the Actions tab). It reads `GET /v1/admin/devices` for all time and
for the last 30 days, applies the repo's rules, and rewrites one standing issue titled **Device
support report**. Comments on the issue are kept; only the body is replaced. It never changes a
tier.

Columns:

- **Manufacturer, Model, Device**: exactly as TVs reported them (`ro.product.*`; Device is the
  codename). Use these strings in a rule.
- **Current tier**: what the apps show for that TV today.
- **Installs**: distinct anonymous install ids that saw the TV. One person with two phones counts
  twice; a reinstall or turning reports off and on counts again.
- **Actions ok / fail** and **Success all / 30d**: tracked actions (connect, disable, optimize,
  snapshot apply, launcher, settings…) that succeeded or failed.
- **Top error codes**: from `v2/shared/errors.ts`. A `*` marks a code that counts against the TV;
  the unmarked ones are the user's setup (network, pairing, adb missing, our own do-not-disable
  refusal, the Pro lock).
- **Suggested** and **Why**: the criteria below, with the numbers that triggered them or what is
  still missing.

The **Suggested changes** table at the top is the to-do list. "keep" means nothing to do.

### Criteria

Implemented in [`v2/telemetry-worker/scripts/report-lib.ts`](../v2/telemetry-worker/scripts/report-lib.ts)
(`CRITERIA`, `EXPLAINED_CODES`) and tested in `test/report-lib.test.ts`. Change them there and here
together.

| Suggestion | When |
| --- | --- |
| `unknown` → **beta** | ≥ 3 installs, ≥ 5 actions, ≥ 80% of actions succeeded (all time). |
| `beta` → **supported** | ≥ 10 installs, and in the last 30 days ≥ 20 actions, ≥ 95% succeeded and no unexplained error code on more than 5% of actions. |
| **fully_supported candidate** | A `supported` TV that still meets the bar with ≥ 25 installs. Only the owner's physical test makes it `fully_supported`; the report never suggests the tier itself. |
| **not_supported?** | ≥ 5 installs and ≥ 5 actions, and either ≤ 30% succeeded or one unexplained code is on ≥ 50% of actions. Needs the owner to confirm the cause first. |
| **review** | A `supported` or `fully_supported` TV under 80% success over ≥ 10 actions in 30 days, or a `not_supported` TV that now meets the beta bar. |
| none | The TV didn't report a manufacturer and model (or the server redacted them). It never gets a tier. |

An "unexplained" code is anything outside the setup list: `permission_denied`, `pm_failure`,
`unsupported`, `partial_failure`, `unknown`, `panic` and anything new. The error rate is the code's
count over the TV's actions in the same window. The Worker returns a TV's top five codes, so when
all five are setup codes and even the fifth is over 5%, the report can't rule out a hidden sixth
and holds off on suggesting `supported`.

These are starting points chosen before any real data existed. Tune them once a few weeks of rows
show what normal looks like.

## Changing a tier

A tier changes by a pull request that edits `device-support.json`. Nothing else.

1. **Decide from the report.** Read the row: installs, success, error codes, last seen. For
   `not_supported`, find out why it fails (the error codes, a user report, the platform). For
   `fully_supported`, run [`docs/DEVICE-TEST-SCRIPT.md`](DEVICE-TEST-SCRIPT.md) on the TV.
2. **Edit the file** on a branch:
   - Add or change the rule, using the report's exact Manufacturer and Model strings. Use
     `model_prefix` for a model family and add `device` when one model name covers different
     hardware. Put it above any broader rule for the same manufacturer.
   - Write a note that says what the tier rests on ("Works across 14 installs since October.").
   - Set `updated` to today's date.
3. **Add a test** in `v2/crates/core/src/engine/support.rs` (`mod tests`) that classifies the
   report's exact strings and asserts the new tier, for example:

   ```rust
   #[test]
   fn tcl_qm7_is_supported() {
       // Strings from the 2026-11-09 Device support report.
       let p = props("TCL", "TCL", "QM7L Pro", "x");
       assert_eq!(classify_support(Some(&p), &embedded()).tier, SupportTier::Supported);
   }
   ```

   Keep the existing tests passing; if one breaks, the change moved a TV you didn't mean to move.
4. **Check locally** (optional; CI runs the same):

   ```sh
   node v2/telemetry-worker/scripts/support-tiers.ts check v2/crates/core/data/device-support.json
   cd v2 && cargo test -p shield-optimizer-core support
   ```

5. **Open the PR.** CI runs the file check (with the date rule against `main`), the Rust tier tests
   on three OSes and the Worker suite, which also checks the file round-trips through the Worker.
6. **Merge.** The sync workflow pushes the rules live within a few minutes and verifies them; its
   run summary shows the served `updated`. Apps with reports on pick the change up within about an
   hour. The next release embeds it, which reaches users who turned reports off.

To undo a change, revert the PR. The revert is a change too, so it needs today's date in `updated`.

### For agents

- Only propose a change the report supports, and quote the row (strings and numbers) in the PR
  body. Never infer a tier from a similar model, a chipset or a product page.
- `fully_supported` and `not_supported` are the owner's calls. An agent may open the PR with the
  evidence, but says so plainly in the body and leaves the merge to the owner.
- Don't touch the live Worker directly and don't widen a rule beyond the TVs the report shows
  (a bare `manufacturer` rule covers every model that brand ever makes).
- Don't create the issue by hand or edit its body; comment on it instead.
- `updated` is today's date (UTC), the test uses the report's exact strings, and the PR follows the
  repo's evidence checklist.

## Where things live

| What | Where |
| --- | --- |
| The list | `v2/crates/core/data/device-support.json` |
| Matching (apps) | `v2/crates/core/src/engine/support.rs`, cached fetch in `v2/crates/core/src/telemetry.rs` |
| User-facing wording | `v2/shared/support.ts` |
| File check and sync | `v2/telemetry-worker/scripts/support-tiers.ts`, `scripts/tiers-lib.ts`, `.github/workflows/support-tiers-sync.yml` |
| Weekly report | `v2/telemetry-worker/scripts/report.ts --issue`, `scripts/report-lib.ts`, `.github/workflows/device-support-report.yml` |
| Backend, secrets, redeploy | [`v2/telemetry-worker/README.md`](../v2/telemetry-worker/README.md#production) |
