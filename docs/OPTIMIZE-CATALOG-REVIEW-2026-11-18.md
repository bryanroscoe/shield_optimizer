# Daily Optimize catalog review — 2026-11-18

**2 assessed, 0 accepted, 2 held; 1,605 active candidates remain queued.**
Base main: `5d829e0cae73149a02760d04af64891b00a98316`. All prior decisions
are unchanged. Source pin: `8672e68e8a24c1258e4662cd11daa4ed13795dad`.

## Amazon Music and Photos

The implementation agent and exactly one independent read-only reviewer assessed
`com.amazon.bueller.music` and `com.amazon.bueller.photos`: exact identity,
purpose, dependencies, disable consequences, recovery and applicable scope.
Both recommend holding them outside the action lists.

Amazon's [Photos personalization help](https://digprjsurvey.amazon.com/csad/help/node/TMlFkNlI8OsVODP4xs)
describes Photos as a source for Fire TV screensavers. Its
[Music single-device help](https://digprjsurvey.amazon.com/csad/help/node/G42KQB42DMBMT8DW)
describes subscriptions bound to a compatible Fire TV or Echo. Its
[Fire TV developer FAQ](https://developer.amazon.com/docs/fire-tv/faq-general.html)
describes music playback services, HDMI and Daydream interactions. These are
functional context, not an exact-package manifest or an affirmative dependency
and recovery assessment. Exact-ID search returned third-party APK/debloat material;
that cannot substitute for primary package evidence. Some Amazon storefront/help
routes refused automated access; that is an evidence gap, not proof of absence.

Stopping the relevant app could lose music playback or photo display/screensaver
functionality. The exact components, callers, firmware variations and recovery
limits remain unverified. Re-enabling an installed package is not proof that
account state, subscription access or configured screensaver behavior recovers.
No whole-family/common scope can be justified from model names.

## Original observations and conflicts

All pinned catalog observations, original comments and model/firmware contexts
remain intact. Representative source links at the pin:

- [4K 2023 list](https://github.com/jeanbuntu/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/amazon-fire-tv-stick-4k-2023/debloat.txt#L97): Music is commented at 97 and active at 142; Photos is active at 143. Its header calls Music an owner choice and reports no measured playback/DRM benefit.
- [4K Max 2023 list](https://github.com/jeanbuntu/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/amazon-fire-tv-stick-4k-max-2023/debloat.txt#L39): keeps Photos screensaver and Music. These choices are not safety votes to average.
- [Insignia F20 list](https://github.com/jeanbuntu/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/insignia-ns42f201na22/debloat.txt#L22): Photos was skipped by the source tool's input-handler check before an override; the author says these apps accept video links and do not run an input. Preserve that explanation without treating the override as firmware-wide dependency or recovery proof.
- [TCL S4 list](https://github.com/jeanbuntu/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/tcl-43s450f-ca/debloat.txt#L13) keeps Music by owner choice, while [Xiaomi Fire TV](https://github.com/jeanbuntu/cleanyourtv/blob/8672e68e8a24c1258e4662cd11daa4ed13795dad/examples/xiaomi-fire-tv-aftne4e43f/debloat.txt#L16) keeps Photos.

Reconsider on **2026-12-18**, or sooner with primary exact-package component/caller
and recovery evidence that supports enforceable scope. Frequency only selected
these leads; it supplies no removal evidence.

## Validation and scope

Queue/provenance tests and the pinned offline catalog check validate this
research-only delta. No app lists, planner, safety code, demo fixtures or UI
change. Existing loader/planner/frontend/gallery evidence is unaffected; no
new-entry capture or gallery regeneration applies. Full hosted CI remains the
merge gate. No native packaging, hardware, release or deployment is claimed.
The next batch should prioritize an optional installed-app gap with primary
exact-ID evidence; unchanged holds remain excluded until due or new evidence.
