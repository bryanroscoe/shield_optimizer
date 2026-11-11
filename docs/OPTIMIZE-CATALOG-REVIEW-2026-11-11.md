# Daily Optimize catalog review — 2026-11-11

One implementation agent and exactly one independent read-only reviewer assessed
these two previously unreviewed exact IDs: **2 assessed, 0 accepted, 2 held**.
The prior 75 ledger decisions (39 accepts and 36 holds) are unchanged. No action
catalog, demo, fixture, safety/planner authority, license or source pin changed.
This bounded batch records uncertainty instead of creating optional Caution
suggestions without sufficient evidence. Base main is
`e4a9300759ac040695e265b14f6a92e2fc5c521a`; PR356 is merged with verified main CI.
The existing clean bound checkout is retained; no additional checkout is made.

## Exact identities held

| Package | Primary evidence checked independently | Decision |
| --- | --- | --- |
| `com.disneyplus.mea` | [Exact Play URL](https://play.google.com/store/apps/details?id=com.disneyplus.mea) returned 404 in both agents' fetches; AE region query did not recover a readable listing. | Hold: exact vendor identity, dependencies, removal effects and reinstatement/migration path remain unverified. |
| `com.disneyplus.ph` | [Exact Play URL](https://play.google.com/store/apps/details?id=com.disneyplus.ph) returned 404 in both agents' fetches; PH region query did not recover a readable listing. | Hold: same unresolved exact-package evidence, assessed separately. |

Disney's indexed [uninstall/reinstall help](https://help.disneyplus.com/article/disneyplus-en-bz-uninstall-reinstall)
provides generic Android/Android TV store guidance, varies by location and
excludes preinstalled devices. The opened page had no readable body. This does
not bind either exact ID or establish its recovery, dependency or migration
behavior. The reviewer additionally found Disney's
[Philippines landing page](https://www.apps.disneyplus.com/ph/1271327451), whose
opened content was blocked by a proxy/VPN notice; its search snippet did not
establish an exact Android package binding. Neither generic branding nor a
search result is sufficient identity/removal evidence.

A failed fetch is an evidence limitation, not proof that an application is
unavailable everywhere, defunct, removable or interchangeable with the existing
Disney+ catalog entry. No unofficial migration reports are used as acceptance
evidence. No replacement package, model/firmware/platform compatibility or
recovery guarantee is inferred. Both IDs remain outside all action lists.
Reconsider on **2026-12-11**, or earlier if material primary evidence arrives.

## Pinned original observations

CleanYourTV source pin remains `8672e68e8a24c1258e4662cd11daa4ed13795dad`.
Each ID has **1 active / 0 commented / 0 prose** observations. Counts are triage
priority, not safety votes. Both agents read the original context.

- [TCL 55P7K:65](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-55p7k/debloat.txt#L65): `com.disneyplus.mea` is in the unused streaming apps block, explicitly kept if watched. Middle East TCL G10/Realtek, Google TV12, `STT2.230203.001 AS16`; the same-build Italian 55C655 unit kept streaming apps by choice. Source keep/input/remote and setupwraith warnings remain observations about those systems, not removal proof for this ID.
- [Skyworth SW-22SA:63](https://github.com/Gathin23/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/skyworth-sw-22sa/debloat.txt#L63): `com.disneyplus.ph` is in unused streaming apps, off by owner choice. SW6H/MediaTek, Android TV14, `UTT1.250517.001`; source retains explicit Live TV, BLE remote, input, Assistant and Chromecast keep decisions plus setup/network warnings. No package-specific dependency or recovery assessment comes from that choice.

The imported catalog's verified model/firmware/platform fields remain null; these
are upstream descriptions, not target-device certification or auto-matching.
Original source content, full MIT notices and proprietary app license remain.

## Validation and review scope

The independent reviewer did its own primary-source research of the same two
identities, then checks the final static delta and unchanged prior decisions.
No extra agents, paid APIs, formal self-approval or hardware claims.
The queue excludes these holds until due/new evidence: active pending count
moves **1,613 to 1,611**, held count **36 to 38**, curated count stays **87**.

Queue tests, read-only queue, provenance checks and an exact delta audit supply
local evidence; applicable hosted CI supplies current-head native checks. No
frontend/action/demo/fixture changes means screenshots, videos and galleries
are unaffected; no new capture or duplicate frontend/native build is needed.
Prior captures remain historical evidence only, not new head test claims.
Receipts bind actual checks and CI to the exact pushed head and eventual merge.

Next useful leads remain unreviewed: `com.android.providers.calendar` and
`com.tcl.dashboard` require primary provider/caller and model/firmware scope
research before any suggestion; do not promote frequency into Caution.
The existing daily job, timing and approved normal reviewed-green merge policy
are unchanged. No deploy, release, device control, packaging, billing, credential
or security change. Retain the bound checkout/warm frontend dependencies for the
daily job, review **2026-11-17**; clean only this task's disposable scratch.

## CI dependency recovery

The first two hosted attempts failed to resolve `azure.archive.ubuntu.com` while
installing browser dependencies; frontend type checks/builds passed, browser
regressions did not run. Linux native/e2e jobs remained in package downloads
until their time limits. These attempts are not substantive CI passes.

The same independent reviewer checks a scoped CI recovery: before dependency
installation in the four affected jobs, rewrite runner Ubuntu mirror-file/Azure
source URIs to the official Ubuntu archive/security HTTPS endpoints. Preserve
suites, components, signing keyrings, verification, package lists and all tests.
Third-party repository URIs remain unchanged. The helper's temporary-file tests
cover Jammy-style lists, Noble deb822, security repositories, unrelated sources,
missing files and repeated execution. No local host APT configuration is changed.
Actual replacement-head CI and exact-merge main CI must still succeed.
