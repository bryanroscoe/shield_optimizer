# Saved TVs, app risk, details, and product copy

Navigator source review for `so-fb3.1`–`.4`, with `.6` reporting handoff, convoy `hq-cv-z4oev`.
Workspace baseline: `48cba2384ca3938e6655b77ccf85716dd86cca5f`,
`crew/navigator`. This is an implementation brief, not implemented behavior.
No app control, discovery scan, ADB, GUI, build, or test ran for this review.
Mechanic owns integration and implementation reservations; Navigator owns this
document and UX acceptance. Existing work and archived candidates are preserved.
The [source manifest](evidence/feedback-ux-48cba23-source.json) records hashes of
the 18 baseline files supporting these findings and the local-diagnostics handoff.
Mechanic reviewed and accepted this source/copy contract as an implementation
brief. That acceptance does not establish implemented, tested or shipped UI.

## Source findings and priorities

| Priority | Source evidence at this baseline | Required outcome |
| --- | --- | --- |
| 1: saved identity | `src/lib/savedDevices.ts` uses hardware ID when both sides have one, otherwise host equality. `rememberDevice` removes conflicting old-host records; it does not retain an addressless saved record. Onboarding discovery groups services by host separately from saved TVs. | Discovery must distinguish remembered identity, discovery evidence, and a live connection. Do not merge or inherit a name solely because an address matches. |
| 1: unsupported Safe | `../crates/core/src/engine/safety.rs::classify` defaults to `Safety::Safe`; `unknown_package_is_safe` expects that. `src/lib/safety.ts` turns it into an affirmative assurance. | Introduce canonical Unknown semantics; never-disable remains dominant on every mutation path. A successful lookup returning Unknown is different from a failed lookup. |
| 1: process identity | `../crates/core/src/adb/parse.rs::parse_dumpsys_meminfo` captures `[a-zA-Z0-9_.]+` without requiring a complete process-name token. It strips colon suffixes and can truncate hyphenated names. Health turns every resulting key into `MemoryEntry.package`. | Preserve process identity before attributing memory to installed packages. Unresolved process/system entries are inspect-only. Do not invent a package from a prefix. |
| 2: shared details | Apps opens `AppDetailSheet`; Diagnostics renders separate rows/actions. Apps enrichment awaits directly into package-keyed maps without a captured device/generation check. Detail safety request identity is package-only. | Use the same package details and action state from both entrypoints, bound to device, package, and connection generation. |
| 2: measurements | Both health and app-memory commands use the Total PSS by process section. The detail sheet hides missing/zero RAM, has no per-app storage field, and treats an absent last-used value as Never opened. | Label PSS, measurement age/scope, unavailable data, and storage separately. Missing usage history does not establish that an app was never opened. |
| 2: background control | `../crates/core/src/commands/tuning.rs` reads `settings get global background_process_limit`; mobile and desktop write that key. Mobile offers only Standard/≤4/≤2/None and claims reboot reset. | Verify the actual control/readback capability before offering more choices or claiming success. See official-source discrepancy below. |

The submitted report appears mobile from its screen names and prior feedback,
but this review cannot identify the newly reported installed build. The earlier
owner-reported debug build at `44d2d66` is historical. Shared core affects desktop;
desktop `src/lib/types.ts` also has only three Safety variants, and
`src/lib/components/TweaksTab.svelte` uses the same background settings key.

## Saved-device presentation contract (`so-fb3.1`)

| Evidence | Row text and action | Identity rule |
| --- | --- | --- |
| Current confirmed session | Connected; Open dashboard | Only the live canonical session can grant this state. |
| Discovered endpoint matches a remembered address, identity unverified | Saved address · Identity unverified; Connect | Current scan results have no authenticated stable ID. Do not inherit trusted identity or merge saved TVs from this match. |
| Saved identity matched to a discovery result with sufficient identity evidence | Saved; Connect | One row, remembered name plus current endpoint. Connection/authentication/profile must still succeed. |
| Saved endpoint absent from the current scan | Saved · Not found in this scan; Retry connection | Scan absence is not proof the TV is offline. Keep the saved TV available. |
| Connection attempt failed | Couldn't connect; Retry | Show the actual failure and preserve the remembered TV. Do not assert power state. |
| Newly discovered endpoint without an established match | Found on network; Connect | No trusted saved name, silent identity merge, or Connected badge from IP/name similarity. |

An IP match with missing hardware identity is a possible match only. The resolved
profile must decide durable deduplication; a conflicting hardware ID must never
inherit the old TV's label. A service name is not automatically a hardware ID.
Keep canceled or superseded results from modifying saved rows or routing the UI.
Preserve explicit Disconnect and add-TV intent. Reuse accepted `so-vtm.3`:
saved-TV success opens Dashboard only after profile resolution; first-time/add-TV
keeps its confirmation. This brief adds recognition, not session persistence.

**Mechanic-local identity addendum (2026-09-08).** `so-fb3.1.2` is now integrated
in `crew/mechanic` as `savedDevices.ts` SHA-256
`163472a87f765f2db981ef3a9fc2e7454604154e35633818506cec6f7aa8c8b1`.
Equal hardware IDs are the only cross-endpoint identity; ID-less rows match only
the exact host and port. Conflicting IDs, or an identified connection at an
ID-less saved endpoint, remain separate saved rows. This supersedes the baseline
source behavior described above, but it is not released or device-verified.
Mechanic reports 11/11 pure saved-device tests, mobile check 0/0, and a passing
build. The intentional coexistence exposes a separate screen contract tracked by
`so-fb3.1.3`: Onboarding and Devices must key, select, filter, show progress for,
and forget the exact saved identity rather than using host alone or an MRU
host:port surrogate. Show `host:port` and neutral shared-address guidance; do not
claim that two saved rows prove two currently reachable TVs.

## Risk and action language (`so-fb3.2` and `.3`)

| Canonical outcome | User label | Explanation / action boundary |
| --- | --- | --- |
| Never-disable | Protected | This app blocks disabling or uninstalling this package. Show the package-specific reason. No override confirmation. |
| Caution | Caution | Disabling can affect a TV feature. Show the known consequence before confirmation. |
| Affirmatively reviewed optional entry, with applicable catalog evidence | Optional | Review what this app does before changing it. Keep is the default for optional choices. A catalog match alone must not override Caution or Protected. |
| Uncatalogued or insufficient evidence | Unknown | We haven't reviewed the effects of disabling this app. It may affect other TV features. No automatic selection or safe recommendation. |
| Pending/failed verdict | Checking risk… / Risk unavailable | Keep Disable and Uninstall unavailable until classification succeeds. Offer a real retry, not a claim that reopening the whole app is required. |
| Unresolved process/system aggregate | Process details | No verified app match. App actions are unavailable. Do not classify the display string as an installable package. |

“Optional” is proposed display vocabulary, not permission to introduce a second
classifier or silently reinterpret the current `safe` result. Mechanic must
define the pure core result, registry evidence and device applicability first.
Catalog `RiskTier`, default selection, and operation safety are different fields.
The existing JSON rationale and restore description should be reused; a missing
review source/date remains missing. No invented audit history or blanket device
compatibility claim.

Unknown must not become a Protected bypass. Once package identity is verified,
an explicit user-selected action may be reviewable under Mechanic's canonical
policy, with Unknown warning and current entitlement checks. No blanket ban on
all unknown user apps is implied. Backend gates remain mandatory for direct
disable/uninstall, Optimize apply, snapshot apply, launcher flows and recovery
inverses. Enable/recovery must remain distinct from disabling. Do not add a local
UI allowlist or hard-coded registry packages.

Action confirmations name the TV, app/package and operation. Suggested wording:

- Disable: “Stops this app from running for the current TV user. You can enable
  it again in Apps.” Show the risk reason and qualify recovery when losing a TV
  feature could prevent access to this app.
- Enable: “Allows this app to run again.”
- Force stop: “Stops this app now and may interrupt playback. It may run again
  later.” No guaranteed memory saving; unresolved process rows cannot invoke it.
- Uninstall: “Removes this app for the current TV user. App data may be lost.
  Reinstall availability depends on whether the app remains on the TV or can be
  installed again.” Preserve existing stronger operation-specific warnings.
- Optimize: list concrete Disable/Uninstall/settings changes in the existing
  review flow. Do not add a one-tap “Free RAM” action to app details.

## Shared detail and metric contract (`so-fb3.3`)

Apps and a verified package row in Top memory consumers open the same details.
Show the available display name, exact package, Enabled/Disabled/Uninstalled state,
catalog purpose and effect/recovery rationale, then risk and explicit actions.
Unknown description: “No description is available for this app.” Keep process
name/PID distinct from an app name when package attribution is unavailable.

Use “RAM at last refresh (PSS)” with a timestamp/age and Refresh. Explain only
when expanded: “Includes this process's share of memory used by other processes.”
PSS is a sampled RAM attribution, not installed size, a foreground/background
split, or guaranteed reclaimable RAM. Android documents the proportional sharing
and version-dependent output in its [dumpsys reference](https://developer.android.com/tools/dumpsys).
The current parser divides reported K values by 1024; the implementation must
make the displayed units consistent (MiB or an explicit conversion).

Show “Not measured” for no usable sample, “Not reported in the latest sample”
when the app is absent, and “Last refresh failed” alongside stale data. Do not
turn these into 0 or “not running.” The existing top-20/top-8 lists are truncated
views, not whole-TV totals. Attribute each process sample once; avoid duplicating
shared-process RAM across multiple app rows. PID/name/UID resolution and app
aggregation belong in the shared command/data layer, not competing screen parsers.

Use a separate “Installed storage” field only with a real storage measurement.
If only APK bytes are measured, label “App files” and disclose that data/cache are
excluded. Until a supported source exists, show “Not measured.” No source in the
reviewed detail data proves active-memory usage; omit that metric. Replace “Never
opened” with “Last opened: not reported” when usage history is missing.

After actions, refresh/invalidate shared installed state, details, memory and
Optimize/health as relevant. Keep previous values visibly stale while refreshing;
failure must not become success. Dismiss or invalidate open confirmations on TV
switch, disconnect, package change or session-generation change, including
reconnect to the same serial. A response for TV A cannot populate TV B. An
uninstalled package has no active Disable/Force stop actions.

Installed icons are optional. Reuse a generic retained glyph until a bounded,
device/package/version-keyed retrieval/cache is approved; no third-party icon
lookup, blocking icon downloads or unrelated font rebuild.

## Background-limit source discrepancy (`so-fb3.4`)

AOSP's [preference controller](https://android.googlesource.com/platform/packages/apps/Settings/+/master/src/com/android/settings/development/BackgroundProcessLimitPreferenceController.java)
(observed blob `7a7d6fa58c0736ce63df0fc71398f088377236ba`) uses ActivityManager
`setProcessLimit`/`getProcessLimit`, resetting to -1 when developer options are
disabled. It does not implement this control through the app's global settings
key. Thus a successful key write/readback does not establish effective behavior.
This is a source-based discrepancy, not a device-tested claim that every OEM
ignores the key.

AOSP [SettingsLib choices](https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/packages/SettingsLib/res/values/arrays.xml)
map Standard to -1, None to 0, and maxima 1–4 to those integers. Proposed compact
labels: Standard, None, ≤1, ≤2, ≤3, ≤4; accessible labels spell out “At most N
background processes.” ≤ means at most, not less than. Prefer a wrapping layout
or select control over six cramped buttons.

[ActivityManagerConstants](https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/services/core/java/com/android/server/am/ActivityManagerConstants.java)
initializes the override at -1 and updates it in memory. Its comments scope the
limit to cached background processes, excluding foreground/visible/service
processes. This supports an AOSP runtime-reset expectation, not the current UI's
universal “Android clears this on every reboot” assertion for a persisted settings
key or an untested OEM. It is not a cap on every background service or a promised
RAM gain. Sources were read remotely; no local device command was executed.

Mechanic's next backend decision: supported invocation/readback and capability
detection through the existing driver seam, with unsupported/error/custom values
represented explicitly. Do not use hard-coded Binder transaction numbers or
treat absent data as verified Standard. Until that exists, recommended UI is a
disabled control reading “This app can't verify the background process limit on
this TV.” No “updated” toast based only on the old key. Verified-success copy:
“Background process limit updated.” Supported explanatory copy: “Limits cached
background processes. Apps may reload more often.” Qualify any reboot note to
verified support; no background setting replay after reconnect/restart.

## Ready copy replacements (apply through reserved implementation owner)

| Surface / current problem | Replacement |
| --- | --- |
| Dashboard “audited safety list” and stale More recovery location | “Review each app before disabling it. Enable disabled apps in Apps, or use Emergency recovery in Settings.” |
| RiskGuide “same audited classifier… three tiers” | “Check the risk and what may stop working before changing an app.” Populate tiers from canonical results, including Unknown. |
| AppDetail “safety engine flagged… Caution” | “Review what may stop working before disabling this app. You can enable it again in Apps.” Keep the specific warning. |
| Diagnostics “Force stop frees the process now” | “Force stop may interrupt the app. It may run again later.” |
| Optimize “TV is already clean” empty state | “No recommended changes were found for the installed apps.” Preserve optional-review discovery when present in the candidate. |
| Optimize “shared safety engine” introduction | “Review the selected apps and changes before applying them.” |
| Paywall “Safely disable or uninstall known TV bloat” | “Review and disable or uninstall listed TV apps.” |
| Paywall “Write tweaks — CEC, frame rate, DNS” | “Adjust CEC, frame-rate matching and DNS.” Keep actual feature availability/gating accurate. |
| Missing last-used history | “Last opened: not reported.” |
| Paywall “One payment · every future update” | “Enter a license key to unlock Pro features.” Preserve the existing activation CTA; do not invent checkout, price, renewal or update terms. |
| Frame-rate decorative numeric glyph | Implement the existing `so-vtm.5` task with a retained nonnumeric glyph. Mechanic confirms nitro has zero app edits, so there is no accepted icon candidate to integrate yet. Explain matching policy and TV/app support; never present it as measured FPS. |

These replacements address reviewed claims, not merely punctuation. Existing
specific error causes, uninstall data-loss warnings and unavailable features must
survive. Desktop needs the same risk vocabulary and metric semantics at core
integration; its scope/reservations and gallery regeneration remain Mechanic-owned.
This pass is not a complete audit of every screen or a claim that copy is shipped.

## Candidate and verification handoff

`so-vtm.3` is an accepted separate navigation patch; reuse its archived source and
profile barrier. The observed `so-vtm.9` design still lists `.5` as pending and
`.7` as partial/unaccepted. Neither a hook nor an archive is integrated behavior.
Mechanic has confirmed the integration path is
`/Users/bryanroscoe/Developer/gastown/shield_optimizer/crew/mechanic`, HEAD
`48cba2384ca3938e6655b77ccf85716dd86cca5f` plus the exact uncommitted `.3` patch
(`5cb8b816f7e0e7f79b3ec8167288ad9c74c7f9ed56e7c55f463fddd8c1953a51`). Mechanic
reports all three file hashes match its accepted manifest. Source metadata is
mobile 0.1.0 (`com.atvoptimizer.mobile`) and desktop 2.1.0
(`com.shieldoptimizer.app`). A separate read-only inspection of
`/Applications/Shield Optimizer.app/Contents/Info.plist` confirms the installed
macOS bundle reports version/build 2.1.0 and identifier `com.shieldoptimizer.app`.
This did not launch or control the app and does not pin its source commit or
identify the app/version behind the reported mobile symptoms. No installed
Android metadata was read and no device calls were made.
Do not reuse baseline test counts to claim candidate correctness.

A read-only check of `../mechanic` found the same HEAD with three dirty navigation
candidate files (Onboarding, session tests, feedback), rather than a complete
combined integration. Its Onboarding calls `onConnected()` on saved success;
its safety fallback/test and numeric Tweaks glyph still match the findings above.
This confirms the problem also exists in that partial candidate. It does not
establish the state of another integration workspace or a delivered build.

After the host resource hold is lifted, reserve focused checks for: mixed
saved/new discovery, stable-ID address change, reused IP with conflicting/missing
ID, cancel/profile races; unresolved `system`, hyphenated and colon process names,
shared UID, duplicate samples and absent PSS; all risk outcomes including a
Protected package present in a permissive catalog; same-package TV switch and
same-serial reconnect; consistent list/detail state after mutation; missing versus
zero/stale metrics; each supported background choice and unsupported/readback
failure. These are proposed checks, not tests executed in this review. Existing
required checks still apply to later code changes, with platform limits recorded.

Next work: Mechanic resolves `.1` identity and `.2` canonical safety/data semantics
on the actual candidate, then reserves bounded Sol implementation for shared
details and the ready copy table. Navigator reviews that milestone against this
contract. No extra worker dispatch or overlapping application edits were made.

Current reservations: `.1` owns Onboarding/savedDevices/session discovery tests;
`.2` owns Diagnostics, Optimize, AppDetailSheet, RiskGuide, shared safety/types,
core and desktop. Navigator is approved for disjoint documentation only in this
pass. Mechanic reports the `hq-7sj.7` dispatch hold remains pending scoped
activation from Mayor; no re-sling, worker recycling or old poller workaround.
No separate implementation permission is needed once Mechanic has the approved
runtime path and has assigned the nonoverlapping file scope.
The background control needs Mechanic's backend decision before a Tweaks code
reservation. Host activity still excludes GUI, app control, scans, ADB and heavy
checks; source and bounded lightweight work may proceed.

`so-fb3.5` monetization remains separate under Mechanic; changing paywall wording
does not establish a purchase/issuance/restore flow or future-update terms.
`.6` is now P1: the immediate `.6.1` deliverable is a minimal local diagnostic log
with preview/export/clear, independent of a reporting backend. Its
[UX and acceptance contract](UNKNOWN-DIAGNOSTICS-UX.md) follows Mechanic's typed
collector design and `.2` canonical Unknown/process identity. `.7` assigns durable,
event-driven registry maintenance to Mechanic. Neither collection nor review
automatically promotes an entry to Safe.
