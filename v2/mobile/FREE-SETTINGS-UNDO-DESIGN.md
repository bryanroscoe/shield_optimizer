# Bounded Free settings undo — proposal, not implemented

Status: design only. The repository has a current-behavior characterization test for the gap
below, and nothing else: no journal, command, payment wiring or security rule. The decisions at the
end need review before implementation.

## Demonstrated gap

The new `current gap: Pro loss blocks restoring an app-changed screensaver even with the original
known` browser test changes the synthetic TV's screensaver as Pro, loses Pro, then selects the
existing Restore previous control. The current `write_setting` contract rejects the first restore
write with `LOCKED:tweaks_write`, leaving the app-written Off state in place. This test PASSES
when the gap is present; it is characterization, not evidence that Free undo is implemented.
The mocked boundary follows the real `tuning::write_setting` prerequisite. It does not exercise
Android or prove native security. Successful writes are synthetic test-fixture mutations only.

## Smallest recommended first scope

Cover the screensaver group only: `secure.screensaver_enabled` and
`secure.screensaver_components`, changed through a typed mobile screensaver action. No global,
system, arbitrary key, app permission/op, DNS, display override, Home, package, snapshot or file
mutation is authorized by a Free undo. Expand later only through reviewed typed operation types.
This first slice cannot claim Free undo for every existing Tweaks action.

- Pro remains required for a new screensaver change. The before-state must be durably recorded
  BEFORE the first TV write. A Free preview request identifies a record/revision; confirmation
  accepts only its native preview handle, never caller-provided settings, values, namespace,
  identity, path or shell expressions. Record ID/revision alone never authorizes a write.
- Every existing path that can touch either key must join the SAME native operation lock and
  journal rules, or explicitly refuse those keys. This includes generic `write_setting`, snapshot
  apply/settings-plan execution, performance/other wrappers and future driver call sites. Routing
  only the new UI through a typed action is insufficient. An excluded snapshot/plan must be
  rejected before ANY mutation, not halfway through applying unrelated settings.
- The native backend obtains a fresh normalized verified hardware serial using its existing
  device-profile path. UI identity and host:port are insufficient. No hardware ID means covered
  journaled writes and undo fail closed; connection-generation fallback is not persistent identity.
- Each request binds to live connection generation and hardware ID. Reconnect invalidates preview
  and confirmation even for the same ID/endpoint; user must preview and confirm again. Fresh
  verified ID permits later undo on the same TV's rotated port, never endpoint reuse by another TV.
- The UI explicitly says what two settings will be restored and what cannot be undone. No billing
  connection, account, Pro entitlement or network purchase verification is needed to list/preview/
  confirm eligible undo. No automatic undo or replay on downgrade, restart, reconnect or refund.

## Existing pieces to reuse, and limits

- Reuse shared checked-shell framing/parsing, shell argument quoting and normalized identity rules.
  Keep planning/validation pure in `crates/core/src/engine`; journal I/O stays in commands/native
  storage. Extend the existing ADB driver if necessary; never introduce another ADB wrapper.
- Snapshot `current_settings_map` shows checked multi-key reads and explicit absent-settings
  concepts. Its `settings get`/`"null"` interpretation alone is insufficient provenance: journal
  capture must distinguish absent, present empty, literal `"null"` and unreadable. Use successful,
  complete presence-aware reads with exact key matching; reject ambiguous/truncated/duplicate or
  unparsable responses. Retain only the two allowlisted values, never the whole settings dump.
- Do not reuse snapshot apply as undo. It can disable packages/change unrelated settings; snapshot
  files and the module-memory screensaver cache do not prove an app-issued before-state.
- `home_tracking` is best-effort, keyed by transport serial and can discard corrupt state. Those
  semantics are unsuitable for authorizing a Free write. Its JSON storage style is illustrative,
  not an adequate journal implementation.
- The current setting builder interprets empty string as delete. A new internal typed
  `Absent | Present(String)` operation is needed to preserve presence/empty semantics exactly;
  do not route typed empty restoration through the existing empty-means-delete wrapper.

## Proposed record/storage contract

Separate native-private `settings-undo-v1.json`, fixed path under app data, schema version 1.
No database or import/export endpoint; never load a caller-provided file. Do not change snapshot
schema or manufacture historical records from snapshots, cached UI values or old logs.

A record contains: non-secret random record ID, monotonic revision, verified hardware ID,
operation type `screensaver`, presence-tagged before-values, immutable planned per-step/group post-values,
checked readback results (diagnostic until exact plan equality is proven), per-step results and status (`Prepared`, `AppliedVerified`, `UndoPrepared`,
`Undone`, `Unknown`). Timestamps are informational, never an entitlement/identity proof.

Validate schema/version, identity, known operation, exact two-key set, value sizes/shapes,
revision and status before use. Reject unknown versions/corruption; do not silently start fresh
and then perform a covered mutation. Components must have bounded valid flattened-component
syntax; flags must be recognized presence-tagged values. Unsupported original vendor values
must block the new covered change before writing, rather than losing the restore target.

Proposed limits: at most 64 active device records and 1 MiB total, individual values <=4 KiB.
Never auto-evict an unresolved/undoable record or expire recovery because Pro is lost. At capacity,
block new covered writes and offer explicit record review/forget (forget itself never touches TV).
One active group per TV retains the first before-state; repeated app changes may update its last
expected post-state only after actual current state matches the previous expected state. External
changes/conflicts require explicit resolution, not silent re-baselining.

Persist by same-directory atomic replacement with file sync and required directory durability;
write failure, full disk or revision conflict before the TV write means NO TV write. Serialize
journal operations; file and revision reads must be confined, size-bounded and reject symlinks.
Do not report durable capture until the supported Android filesystem operation succeeds.

## Native execution / crash contract

A generation check before a call alone does not protect a multi-call operation. Implementation
needs a reviewed operation lease using the existing mobile connection/lifecycle lock: bind the
hardware identity check, before/readback, record preparation and each write to the same connection.
Do not hold a lock and re-enter a driver method that acquires it again. This is a material driver/
concurrency design prerequisite, not a capability asserted to exist today.

For a Pro change: verify identity/read exact before-state; persist Prepared; execute the finite
screen-saver transition plan; check command status AND read back. AppliedVerified requires EXACT equality to the pre-recorded
planned post-state for BOTH keys, including unchanged keys and every intended intermediate
state (such as temporary disable). Store the already planned values as expected provenance;
never adopt arbitrary observed values as app-owned post-state. Any mismatch becomes conflict/
Unknown, with no new undo entitlement. Plan multi-key ordering explicitly and record it before
I/O. A successful shell return alone is not a verified change.

For Free preview: load/validate record and revision; verify live hardware identity under the
lease; read both current values and require exact equality to verified planned post-values.
Native state retains a short-lived, single-use preview handle bound to record ID/revision,
verified hardware ID, live native connection generation, full expected current state and exact
restore/intermediate plan. The UI receives display information and that handle, not authority to
change the plan. No journal credential or persistent signing key is created.

For confirmation: resolve the handle natively; revalidate liveness, generation, identity, revision
and BOTH expected values under the same lease. Expired, consumed, missing or reconnect-stale
handles fail before any write; consume/invalidate the preview so retries need a fresh preview.
A matching endpoint or record ID/revision does not save a stale confirmation. UI cleanup timing
is not a security boundary. After explicit fresh user confirmation, persist UndoPrepared and
restore only the native-preview-bound before-state in the reviewed safe order,
check/read back and durably mark Undone. Repeated undo is a no-op after verifying completion, not
another arbitrary write. Concurrent confirmations must lose the revision/operation lock race.

Android settings has no general remote compare-and-swap. The connection lease prevents this
app's reconnect/concurrent actions, but cannot stop another TV app changing settings after a
read. Re-read immediately before each dependent step, abort on divergence, and disclose that
residual race; do not claim transactional Android rollback.

Crash after a TV write but before durable verified result is inherently ambiguous. Prepared/
UndoPrepared or partial-readback failures become Unknown; never replay automatically or guess
that a target value proves execution provenance. Block further covered mutation/Free undo until
reviewed reconciliation or manual device recovery. Some crash/partial failures therefore cannot
have guaranteed automatic Free undo. Preserve the record and explain the exact limitation.

## Acceptance matrix for the later implementation

- Pro write -> Pro loss/refund -> explicit Free undo on the same verified TV restores exact before
  values, including proven absence/empty values. Same TV/new port works after fresh confirmation.
- Missing/unknown ID, replaced TV/same endpoint, generation change, stale confirmation, external
  key change, malformed before-state, unknown schema and forged caller parameters produce no write.
- Disk-full/permission/sync failure before mutation, corrupt journal, capacity, competing revisions
  and symlink/path escape all fail closed. No broad reset/cleanup can destroy retained evidence.
- Repeated changes retain original before-state; repeated undo is idempotent; retries never apply
  a stale record. All grouped partial-step failures and crash windows are fault-injected.
- Native generic-setting and snapshot/performance paths cannot bypass the shared lock/journal;
  excluded plans fail before any write. A concurrent alternate path cannot relabel an external
  change as the planned app state. Exact full-state and intermediate readback mismatch tests
  must prove that arbitrary observed values never become AppliedVerified.
- Pro setter remains gated; arbitrary setting/snapshot/app-permission operations remain gated.
  Existing free package recovery still works without journal, billing or Pro.
- Installed before-component missing/unavailable is an explicit unsupported/manual case; do not
  install APKs, invent replacement components or promise recovery of missing APK/data.

## Decisions to resolve before code implementation

1. Approve the deliberately narrow two-setting scope and the stricter requirement that covered
   writes require verified identity and durable capture, including honest refusal on older TVs.
2. Approve separate schema-1 app-private storage, bounds, no auto-retention deletion and no legacy
   migration. Determine Android backup/restore exclusion and update the factual data inventory.
3. Approve the native connection operation-lease/driver design and multi-key transition ordering;
   the residual external-actor/crash limits are not solved by a JSON file.
4. Agree that app-private OS storage is the integrity boundary (no journal import, treat corruption
   as denial). This is not cryptographic provenance against a rooted/tampered phone. If stronger
   authenticity is required, a separately approved Android Keystore design is needed; this proposal
   creates no key/credential and must not pretend checksums are authentication.
5. Accept Unknown/manual-recovery behavior and the inability to recover old unjournaled changes.
   No existing saved data is rewritten and no universal Free undo claim is made.
