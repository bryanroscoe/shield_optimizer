# Unknown app diagnostics: local collection and review

Navigator UX handoff for `so-fb3.6` / `.6.1`; registry ownership `so-fb3.7`
belongs to Mechanic. This is the pre-implementation design/source-review contract;
the later `.6.1` integrated-local implementation and its separate test evidence are
tracked in `HANDOFF.md` and `LOCAL-STATUS-2026-09-05.md`.
Source inspected at `48cba2384ca3938e6655b77ccf85716dd86cca5f` in
`crew/navigator`. No app/device interaction, build or tests in this pass.
`.6` is P1: `.6.1` local diagnostics is immediate bounded work and must not wait
for an external reporting backend. `.7` establishes durable registry maintenance;
`.1` discovery and `.2` canonical safety/identity retain implementation priority.

## Minimum local experience

Add an **Unknown app diagnostics** card in Settings (`screens/More.svelte`),
separate from the current Debug log. Suggested copy:

> Keeps a small local record of apps we haven't reviewed and processes we
> couldn't match to an app. Nothing is sent automatically.

Show **N distinct records**, then **Review records** and **Clear records**.
Empty state: “No unknown apps or unmatched processes have been recorded.” This
does not mean that all installed apps were scanned, reviewed or found safe.
Collection uses actual completed app/health reads and `.2` canonical outcomes;
opening Settings must not trigger device discovery, inventory scans or extra ADB
calls. There is no periodic model, background service or upload job.

The review panel shows a local snapshot with:

- **App not reviewed** or **Unmatched process**, exact validated token and a
  plain reason from a fixed reason-code vocabulary.
- First/last recorded times and occurrence count; explain that repeat refreshes
  are grouped, so the count is observations rather than distinct devices/apps.
- App and registry versions, plus generic device family/OS only when available
  through the approved typed fields. Missing registry version reads “Not
  available,” never a fabricated version or a substituted app version.

Title the available action **Copy diagnostic JSON** when the implementation only
copies text. Clipboard output is not a downloaded file, a sent report or an
Android share/export implementation. Provide visible selectable JSON when
clipboard access fails: “Couldn't copy. You can select and copy the text below.”
If a real local file export is supported, label it **Save diagnostic JSON** and
show the actual result; cancellation is not success. Never claim the export was
shared with developers merely because a local copy was created.

The preview must be the exact snapshot copied/saved; new observations can update
the record count but must not silently alter an open payload. **Refresh preview**
explicitly takes a new snapshot. Suggested note:

> Includes app or process identifiers, observation times and version information.
> Review the contents before sharing. No network addresses, device serials,
> license keys or full debug logs are included.

Identifiers may reveal which unknown apps were encountered, so do not call this
anonymous. No account, destination, consent dialog for a nonexistent service, or
purchase flow is needed for the local minimum.

**Clear records** confirmation: “Delete the diagnostics stored in this app?
Copies you saved or shared will remain.” Confirm label: **Clear records**.
On success, clear the preview and report “Local diagnostics cleared.” Collection
may record future encounters; this is not an opt-out or a change to app risk,
saved TVs, device state, license or ordinary debug logs. A storage-clear failure
must remain visible and must not claim deletion. Prevent in-flight persistence
of an older buffer from restoring cleared records.

For unavailable/full storage, use “Couldn't save diagnostics on this device.”
The TV screen and app actions continue normally. If a bounded collector evicts
older records, say “Only the most recent records are kept”; export metadata can
record truncation without leaking rejected input. Do not promise complete history
or survival after uninstall/clear-data.

## Collection boundary accepted from Mechanic

Use a typed collector alongside `lib/log.ts`, with schema/version, generic
family/OS when available, kind (`installed_package` or `unresolved_process`),
validated package/process token, reason code and first/last/count. Repeated
observations deduplicate by the agreed version/family/kind/token/reason key; the
count saturates and both record count and serialized byte size are bounded.
Mechanic/Sol must specify the numeric limits in the implementation and evidence.

Do not export raw command arguments/errors or copy the existing combined log.
At baseline `lib/log.ts::summarizeArgs` redacts several secret-name patterns but
still retains ordinary `serial`/host arguments, and `More.svelte::combinedLog`
includes native logs. That mechanism cannot satisfy this diagnostic allowlist
unchanged. No serial, host/IP/MAC, auth/license/input, user-defined TV name, raw
shell output, unrelated inventory or full build fingerprint belongs here.

Strict token validation rejects paths, control strings and address-like values.
Rejected raw values must not be smuggled into a reason/error field. Render token
text safely and never treat it as HTML, a command, URL or instruction. Reports
are untrusted evidence. A process such as `system` remains inspect-only, never a
package action target. A transient failed risk lookup is not canonical Unknown;
if collected as a lookup failure, it needs a distinct approved reason code.

The collector must consume `.2` Unknown and process-resolution results, without
introducing frontend classification. A stale response from a previous device or
connection generation must not create a falsely attributed record. Deduplication
across devices with the same generic family is intentional under the minimal
schema: show observation counts, not unique-device counts, and do not add device
identifiers to resolve that ambiguity.

## App detail and registry-review follow-up

The immediate Settings preview/export does not wait on an external reporting
service or on rewriting AppDetailSheet. That screen is reserved by `.2`.
Once `.3` details and a separate reservation are ready, **Review diagnostic
record** can open the same local preview for an Unknown item. An unresolved
process has a process-details view with no package mutation actions.

A later **Report an app** flow may add a user-written reason and optional selected
context, but its payload/destination must be visible before an explicit send.
Keep free-form text out of the automatic minimal collector. Never silently
upload local records, infer consent from export, or mark an app reviewed because
someone reported it.

Mechanic's `.7` workflow is event-driven: a diagnostic export or new source
evidence starts strict batch validation/deduplication, a bounded Beads correction,
source/risk/device-scope review, tested versioned registry changes, then integration
and a separate release handoff. A new record remains Unknown during triage.
Protected overrides optional/reviewed catalog entries at every step. Store review
provenance and actual status; “Under review” requires a real tracked review, and
“Available in update” requires a released version. Local evidence or an accepted
patch is not a delivered registry change.

App purpose, effects, recovery and risk wording should come from the same reviewed
registry revision in Apps, memory details and Optimize. Registry review must
state what is known and what is device/build dependent. No separate screen-side
list or report-triggered automatic recommendation.

## Bounded implementation acceptance and next owner

Mechanic reserves the collector module/tests and Settings UI after the current
dispatch hold is explicitly lifted. `.1` discovery and `.2` safety/identity keep
priority; no third or overlapping worker is justified. Collector scaffolding can
remain disjoint, but hooks require the actual `.2` typed result. Navigator reviews
copy and payload consistency at the resulting milestone.
No new user implementation permission is required once Mechanic has the approved
runtime path and file reservations; the current gate is dispatch activation.

Meaningful lightweight tests should cover: unknown installed package versus
unresolved `system`; duplicate polls/count saturation and record/byte eviction;
token/address/path rejection; absent/full/corrupt storage; immutable preview/export
snapshot; clear racing a pending save; stale TV/session results; clipboard/export
failure without false success; no sensitive fields in serialization; no changes
to safety, entitlement, session or mutations. Use synthetic identifiers, not a
real app inventory. Browser/layout and full code gates remain deferred under the
current host-use constraint and must be reported separately when actually run.

This handoff does not itself implement the collector, the `.7` runnable batch
triage tool, external submission, or desktop UI. Track those deliverables and
their test limits explicitly rather than closing `.6` from documentation alone.
