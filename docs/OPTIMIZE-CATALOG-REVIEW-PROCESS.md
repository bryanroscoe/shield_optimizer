# Expanding Optimize from source catalogs

Bryan requested ongoing catalog expansion on 2026-11-05, with two agents
checking packages before they enter Optimize. The objective is broader shared
app coverage on desktop and mobile, not another research interface.

## One daily batch

Use the pinned CleanYourTV snapshot at
`v2/catalogs/cleanyourtv/catalog.json`. Run the queue tool from the repository:

```sh
python3 -m unittest discover -s v2/tools/catalog-review -p 'test_*.py'
python3 v2/tools/catalog-review/review_queue.py --limit 25
```

It reports missing exact identities and their original active/commented/note
counts. Counts prioritize investigation only. The output is unreviewed and
non-actionable; it never edits app lists or assigns safety verdicts. Packages
already in the reviewed lists and unchanged held decisions are excluded.
Comment-only references remain visible in the summary but are not promoted as
disable candidates. Original source comments and context must still be read.

The implementation agent researches up to 25 candidates per run; a second,
independent agent double-checks the same proposed entries and the evidence.
Use exactly one additional agent, the inherited provider/model and this shared
checkout, with read-only review instructions. Do not buy model/API credits or
call external model APIs. No additional checkout or native build is needed for
research. An agent must not review its own implementation as independent work.

## Accepting an entry

Both agents must agree on the exact identity, purpose, dependencies, removal
consequences, recovery limits and applicable scope. Record primary evidence
from the actual developer, vendor or source repository. A package-name guess,
a store display name alone, an upstream disable choice or frequent sightings
cannot establish removal safety. Read the original keep warnings and conflicts
even when another profile disabled the same ID.

Use the existing shared JSON app lists and canonical planner/safety logic.
Include `reviewed_at`, primary evidence, pinned source observations and a short
assessment in `sources`; write a package-specific consequence and recovery
description. Choose scope from evidence, not from profile-name similarity.

Known optional app functionality may justify Caution with explicit
Disable-if-unused review. Prefer reversible Disable; keep new entries at Keep
with both defaults false unless a separate assessment substantiates a stronger
decision. A global Safe verdict or default action requires affirmative evidence
of removal safety and applicability; ordinary functional identity review is
insufficient. Do not turn unresolved data into a convenient Caution action
either: hold it when dependencies or consequences cannot be assessed.

Accessibility, input/remote, launcher, setup, network, update and vendor system
components require particular attention to dependency evidence. When model or
firmware-specific evidence cannot be enforced by the current app-list loader,
hold the proposal rather than broadening it to a whole family or `common`.
No device/profile auto-matching or compatibility certification comes from this
snapshot. Existing protected/caution, last-HOME, setup/network, user-0 and
confirmation gates remain authoritative.

## Progress and deferred decisions

Record decisions in `v2/tools/catalog-review/progress.json`, using exact package
IDs and the reviewed source commit. Accepted entries also live in the normal
action catalog; this progress file is research bookkeeping and is never loaded
by either app. For a hold, record evidence gaps, the reason, and a review date.
The date is a reminder to reconsider, never acceptance or deletion permission.
Revisit an unchanged held package only when its date is due or material new
evidence arrives. Do not repeatedly re-research the same unresolved set.

## Delivering a batch

Read repository instructions, actual branch/PR/main state and the task manifest
before editing. Respect sole ownership and existing native/Android scopes.
Continue the existing integration PR while it is open; do not create duplicate
PRs or overwrite another owner's changes. A later batch needs an appropriate
clean branch after integration, following the application's supported checkout
lifecycle. Do not make another checkout merely to bypass ownership or CI.

Keep one implementation batch in flight. Resolve actionable reviewer objections
and concrete CI failures; keep disputed proposals held. Run the relevant loader,
planner, safety, frontend and provenance checks for changed behavior, reuse
unaffected evidence honestly, and regenerate affected screenshots. Respect disk
and compiler safeguards; use hosted checks when local native compilation would
violate capacity limits. Do not rebuild native packages or control hardware.

The desktop `test:optimize-plan` and mobile `reviewed-optimize.test.mjs` accept
`OPTIMIZE_CATALOG_EVIDENCE` as the task evidence directory and
`OPTIMIZE_CATALOG_FOCUS` as an exact reviewed package to show in captures. Use a
newly accepted entry as the focus; the tests still check every fixture entry.

Commit and push completed work, attach actual evidence and maintain accurate PR
scope. Routine PR wording needs no user approval. Preserve all upstream MIT
notices, original observations and the app's proprietary license. Do not edit
PR284's approved data-only wording. Bryan explicitly updated the recurring job
on 2026-11-09 to authorize normal merging after independent review and successful
substantive CI, followed by main-push CI verification. Do not stop at an open
green PR or request routine merge approval. Deployment, release, credential
changes, spending and hardware acceptance remain outside this recurring job.

Report candidates assessed, entries accepted, held reasons, both review scopes,
exact commit, actual validation state and the next unreviewed batch. Do not claim
the whole source catalog is reviewed or that a larger list guarantees better
performance. Keep the existing PR watch for CI; do not add polling schedules.
