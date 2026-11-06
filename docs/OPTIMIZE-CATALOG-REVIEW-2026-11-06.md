# Daily Optimize catalog review — 2026-11-06

One implementation agent and exactly one independent, read-only source reviewer
separately assessed the same 25 previously unreviewed exact IDs from the queue.
Both agree on **2 optional Caution additions and 23 holds**. The previous 16
accepted entries and 8 unchanged holds were not re-reviewed. This is source
research and fixture validation, not formal GitHub approval or physical testing.

The batch starts from main `c3624ecf09a6a8f32f8c7b866bf1dd3fd54ee613`, after
PR295 merged. Main subsequently advanced to
`196a401d446cbaefd75704e139f9b6088667a53f`, including mobile Optimize changes;
those committed changes were merged into this branch and affected galleries and
frontend validation refreshed. The package decisions are unchanged.
Source observations remain pinned to CleanYourTV commit
`8672e68e8a24c1258e4662cd11daa4ed13795dad`. Neither source membership nor
active/commented/note frequency is a safety vote. Original source files and their
MIT notices are unchanged; the main app remains proprietary.

## Accepted exact identities

Both developer listings were opened independently by both agents. The evidence
includes substantive viewing descriptions, rather than only display names.

| Exact package | Primary developer evidence | Consequence and recovery limits |
| --- | --- | --- |
| `com.tcl.waterfall.overseas` | [TCL Group listing](https://play.google.com/store/apps/details?id=com.tcl.waterfall.overseas) describes movie, television, live-channel and video viewing. [TCL's S5400 manual](https://www.tcl.com/content/dam/brandsite/region/turkey/user-manual/tv/S5400-User-Manual.pdf) describes recommended content and its dedicated shortcut, without establishing exact package mapping on every model. | Disable loses viewing/recommendation access through this app; its shortcut may stop working. The TV updater can reactivate it. Re-enable requires the installed package and does not restore account access, settings or content availability. No recommendation to disable an updater. |
| `com.tcl.channelplus` | [TCL Group listing](https://play.google.com/store/apps/details?id=com.tcl.channelplus) describes the TCL Live viewing application and live content. | Disable loses TCL free streaming channels, potentially all available Live TV content if no antenna channels are tuned. Rows can remain unplayable and the TCL Channel shortcut may fail/open a placeholder. Re-enable may need a TV reboot before channels return; app settings, account access and content availability are not restored. |

Original observations constrain these functional assessments:

- Waterfall's [updater reactivation](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-50p615/debloat.txt#L25-L31)
  is a report from a particular firmware, not a persistent-disable guarantee.
  The [65C725 block](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-65c725/debloat.txt#L62-L75)
  is historical already-disabled state, not a fresh removal test.
- Channelplus has reports of [unplayable retained channel rows](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-g07-google-tv/debloat.txt#L76-L80),
  [loss of the only Live TV source and recovery requiring reboot](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-g10-google-tv/debloat.txt#L128-L132),
  and a [shortcut opening a placeholder](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-g03-google-tv/debloat.txt#L90).
  Owners also keep it when they use its channels; those observations remain intact.

The assessment supports explicit review of optional content functionality:
`risk: medium`, `method: disable`, `review: true`, both defaults false, initially
Keep in Optimize and excluded from Select all safe/Recommended. It does not
support Safe, automatic actions or a claim that disabling always succeeds.
The existing explicit Uninstall option for a listed app remains available;
these are Disable suggestions, not Disable-only controls.

Shared `common` lookup covers an installed **exact ID**. It does not auto-match a
source profile or certify any model, firmware or platform. Related package IDs
are separate. HDMI, tuner, system-TV, remote/input, launcher, setup, network,
updater, protected/caution, last-HOME, user-0, confirmation and async identity
authorities are unchanged. No performance benefit, privacy elimination or
successful hardware removal/recovery is claimed.

## Held identities

Each row was assessed independently by both agents. Bounded exact-ID searches of
the relevant developer/vendor/source domains did not establish sufficient
dependency and recovery evidence. A no-result search is not evidence of absence.
Vendor-hosted community posts were excluded as vendor documentation. Full
per-ID reasons, primary-search scope, original active/commented/note counts and
pinned observation URLs are in the [progress ledger](../v2/tools/catalog-review/progress.json).

| Exact package | Material unresolved concern |
| --- | --- |
| `com.google.android.syncadapters.calendar` | Samsung documents the phone package, not TV callers or unused-calendar assumptions. |
| `com.tcl.esticker` | Retail-overlay label lacks exact dependency evidence; source records updater reactivation. |
| `com.google.android.onetimeinitializer` | Historical Android sample lists the ID, not when setup/recovery stops depending on it. |
| `fusion.android.tv.demo` | Source explicitly keeps it for `TV_INPUT_BUTTON` on some devices. |
| `com.tcl.messagebox` | Source records a ttvs receiver and firmware-specific keep decisions. |
| `com.google.android.tv.bugreportsender` | Exact reporting/support/recovery dependencies unresolved. |
| `com.tcl.partnercustomizer` | Persistent TV-service widget-provider and remote/input dependencies; scope cannot be enforced. |
| `com.tcl.usercenter` | Account/settings bindings, reactivation and shell permission limits; reported binding bug predates debloating. |
| `android.autoinstalls.config.google.gtvpai` | Exact provisioning/setup implementation and permission/skip behavior unresolved. |
| `com.tcl.guard` | Persistent system-UID memory manager/installer can keep running after Disable. |
| `com.tcl.tv.tclhome_passive` | Home/content service and launcher dependency boundaries unresolved. |
| `com.miui.tv.analytics` | Exact vendor implementation absent from bounded evidence; source spans Fire TV too. |
| `com.qterics.da.product` | Device-agent support/update boundaries, tool skipping and reboot reactivation unresolved. |
| `com.sony.dtv.da.service` | Remote-support label does not establish full caller/recovery contract. |
| `com.xm.webcontent` | Embedded web/RN container may serve more than promotions. |
| `com.tcl.smartalexa` | Voice/bridge, persistent-process and reactivation behavior unresolved. |
| `com.mstar.android.tv.disclaimercustomization` | Setup/disclaimer/consent function; kept and tool-skip observations. |
| `com.sony.dtv.multiscreendemo` | Build-specific alternate demo name lacks exact dependency/recovery evidence. |
| `com.sony.dtv.sonyselect` | Featured apps, registration and APPS-button roles do not establish exact-package removal scope. |
| `com.tcl.bi` | Reporting/monitor/provider boundaries and callers unresolved. |
| `com.tcl.ui_mediaCenter` | USB/media player dependencies; some owners keep it for USB playback. |
| `tv.samba.ssm` | Settings opt-out does not prove exact-package disabling equivalence; TV-service binding remains. |
| `com.tcl.logkit` | Remote diagnosis/support callers, recovery and varying factory state unresolved. |

Limited primary context actually found includes [Samsung's phone inventory](https://docs.samsungknox.com/CCMode/G935V_O.pdf),
[historical Android battery-statistics documentation](https://android.googlesource.com/platform/docs/source.android.com/+/26a6e1b2badf69ddc616ec51687c1d3ecfd8b646/en/devices/tech/power/batterystats.html),
[Sony's Select/registration instructions](https://www.sony.com/electronics/support/televisions-projectors-lcd-tvs-android-/kd-49x750f/articles/00131548),
and [Samba's settings/privacy controls](https://samba.tv/privacy-center/your-privacy-handbook).
None supplies the missing TV exact-package removal/recovery contract. The
independent reviewer additionally inspected Sony's [Samba settings](https://www.sony.com/electronics/support/articles/00151645)
and [feature/privacy description](https://www.sony.com/electronics/support/articles/00182856)
with the same limitation.

## Progress and evidence scope

The queue moves from 1,652 to **1,627 pending active candidates**, with 77 source
identities already curated, 31 held until new evidence or their due date, and
236 identities with no active-disable observation. Today's 23 holds reconsider
on **2026-12-06**, or earlier on material new evidence. That date grants no action
or acceptance. The next frequency-ranked batch starts with dashboard/instructions
and calendar/assistant leads; these still need dependency review, not automatic
promotion. Later batches can prioritize ordinary optional app gaps within the
same 25-ID bound.

The shared demo catalog and substantive loader/planner/browser fixture include
both new IDs. New assertions require the material consequences in both
frontends; the loader regression also asserts that held progress entries stay
out of action lists. Production does not consume the research progress ledger.
Capture focus is `com.tcl.channelplus`, with explicit simulated/no-TV labels.
The fixture's Shield identity is synthetic and does not imply TCL software was
found or tested on Shield hardware. Gallery regeneration and focused tests are
recorded with their exact revision in the PR/task validation receipt. Historical
mobile 372/372 evidence is not a full run on this batch. Hosted cross-platform
Rust checks remain separate from local frontend/provenance validation.
