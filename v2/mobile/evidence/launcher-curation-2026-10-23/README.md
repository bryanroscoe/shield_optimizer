# Mobile launcher curation — simulated evidence

The shared catalog correction from PR220 is merged normally onto mobile base `9e8b0543abff01a332e7a7caac71fecb32edce41`, preserving PR218 and the contributor’s phone gallery commit. No mobile launcher command, safety or lifecycle code changes.

`launcher-simulated.png` and `launcher-simulated.webm` capture the existing mobile screenshot harness at 412×915, with a visible simulation label added before recording. Fixtures only; no real device queried or changed. The full phone contact sheet and gallery are regenerated through `npm run screenshots` from `v2/mobile/`.

The seven pre209 custom entries remain curated. The removed shared package is no longer offered as a missing recommendation. Generic installed HOME support is unchanged and covered by the shared Rust catalog/row regression and desktop browser installed/disabled/current fixture matrix in `v2/evidence/launcher-curation-2026-10-23/`.

Physical phone/TV acceptance and PR163 release gates remain open. The preexisting mobile tooling base-config warning is still nonfatal; the standalone build regression preserves PR218’s fix.
