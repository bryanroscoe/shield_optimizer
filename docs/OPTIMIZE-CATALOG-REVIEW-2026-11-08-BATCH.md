# Daily Optimize catalog review — 2026-11-08

One implementation agent and exactly one independent read-only reviewer
researched the same four previously unreviewed IDs: **one accepted, three held**.
This continues open PR329 in its existing clean bound checkout, starting at
`b7cd4d0b588eab9ec8dd259406126fbefbd89c61`; actual main remains
`eb6279061bef5ea5c4c3b08be6fc3106b95d3744`. The previous batch's implementation,
review and hosted CI were complete before today's single active batch began.
No duplicate PR/checkout, unchanged-accept research or unchanged-hold research.

## Accepted: CANAL+

Both agents independently opened the [exact GROUPE CANAL+ developer listing](https://play.google.com/store/apps/details?id=com.canal.android.canal)
for `com.canal.android.canal`. Its substantive description establishes viewing
and download functionality, with subscription, regional and DRM qualifications.
Disable loses viewing through this client; it does not cancel a subscription.
Re-enable requires the installed package and does not restore account access,
settings or download rights. These limits are in the app-facing descriptions.

Pinned observations remain separate: [Freebox actively lists it](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/freebox-player-pop/debloat.txt#L74),
[Horizon20 comments it out](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/xgimi-horizon-20/debloat.txt#L73),
and [MoGo4 keeps it by choice](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/xgimi-mogo-4/debloat.txt#L110).
Counts (1 active / 1 commented / 1 prose profile) are not safety votes.

The functional assessment supports only optional Caution, Disable suggestion,
review=true, both defaults=false and initial Keep. Existing explicit Uninstall
for store-listed apps remains available; this is not a Disable-only control.
Common lookup identifies an installed exact ID, not a certified model, firmware,
platform, region, DRM capability or source-profile match. No performance or
physical removal/recovery claim. Protected/caution, last-HOME, setup/network,
user-0, confirmation and async identity authorities remain unchanged.

## Held

| Exact ID | Evidence and reason | Original counts |
| --- | --- | --- |
| `fr.tf1.mytf1` | [Exact e-TF1 developer listing](https://play.google.com/store/apps/details?id=fr.tf1.mytf1) establishes streaming function. However, [G10:192](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-g10-google-tv/debloat.txt#L192) and [Horizon20:74](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/xgimi-horizon-20/debloat.txt#L74) are only commented entries: zero active observations. Preserve the queue's no-active boundary; functional identity does not authorize promoting comment-only leads as Disable candidates. | 0 active / 2 commented / 0 prose |
| `com.tcl.ocean.instructions` | Exact Play URL inaccessible; bounded exact-ID `site:tcl.com` search found no sufficient primary implementation/caller/recovery contract. Source says [on-screen manual](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-43c645/debloat.txt#L74), but also [onboarding/tips](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-55q6cx1/debloat.txt#L53); scope remains unresolved. | 34 / 2 / 0 |
| `com.tcl.repairguide` | Exact Play URL inaccessible; bounded exact-ID `site:tcl.com` search found no sufficient primary diagnostic/support dependency or recovery contract. Source calls it [self-diagnosis](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-55c745/debloat.txt#L65), records [firmware absence](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-55a300w/debloat.txt#L6) and [commented treatment](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-75p8k/debloat.txt#L89). | 27 / 1 / 1 |

[TCL's generic settings/manual guide](https://support.tcl.com/en_US/us-googletv-setup-configuration/tcl-google-tv-settings-and-user-manual-guide)
and [troubleshooting advice](https://support.tcl.com/en_US/how-do-i-fix-problems-with-my-tcl-google-tv-device)
do not map either held TCL ID to safe package disabling. Search absence is not
proof of absence. Both agents agree on these holds, reconsider **2026-12-08** or
on material new source/inventory/scope evidence. A due date is not acceptance.

## Progress and evidence

The queue moves from 1,625 to **1,622 pending active candidates**. It now reports
79 curated source identities, 35 holds and 235 with no active observation.
Held TF1 moves from the no-active bucket to held bookkeeping; it never became
an actionable candidate. The previous 19 accepts and 32 holds are unchanged.

The shared demo/fixture adds CANAL+ with visible consequence assertions. Existing
loader/planner/safety regression logic checks all reviewed fixture IDs and
excludes held entries from action lists. Capture focus is
`OPTIMIZE_CATALOG_FOCUS=com.canal.android.canal`; desktop/mobile captures are
labeled Chromium fixtures with a synthetic Shield identity, not CANAL+ tested
on physical Shield hardware. No backend removal calls occur in capture tests.

The task receipt/PR bind actual checks, regenerated galleries and inspected
captures to their tested revision. At 14 GiB free, no dependency install, native
target or multi-GiB job is started: only small existing-dependency frontend
checks/captures, with scratch cleaned afterward; hosted CI supplies native Rust
validation. Prior mobile372/372 is not a final-head claim. Source/static review
is not formal approval, release or hardware proof. This run prohibits merging.

The source stays pinned at `8672e68e8a24c1258e4662cd11daa4ed13795dad`, unreviewed
and non-actionable. Original observations, full MIT notices and proprietary app
license are unchanged. The existing daily job is unchanged.
