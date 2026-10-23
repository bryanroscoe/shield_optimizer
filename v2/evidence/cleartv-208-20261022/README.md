> Historical evidence for PR209. Its curated entry is superseded by the launcher curation correction; these captures do not describe the current featured list. Generic installed HOME support remains available.

# ClearTV Islamic shared launcher identity — issue 208

The catalog addition targets main `c78176d5ea640a0e230633a3f0b0bc0720c01af4`. Mobile was at `b7bd2c0b50fc774c3ffad10ee67d95c9c7aa4a79`; both launcher catalogs were identical. One shared catalog is changed, without a second mobile edit. Existing PR168/207 work is retained; pending PR180 and other worktrees are untouched.

## Source evidence and limits

[Issue208](https://github.com/bryanroscoe/shield_optimizer/issues/208) requests ClearTV Islamic for an Nvidia Shield 2017. The reporter supplied no installed-package or firmware evidence. Read-only inspection of upstream commit `5a563f5e9c15af5e5f34ce2153fd4dcb3ad9ce97` confirms:

- [build.gradle](https://github.com/Pokka99/cleartv-Islamic/blob/5a563f5e9c15af5e5f34ce2153fd4dcb3ad9ce97/launcher/app/build.gradle): applicationId `com.gathin.tvhome`, minSdk26 (Android8/API26).
- [Manifest](https://github.com/Pokka99/cleartv-Islamic/blob/5a563f5e9c15af5e5f34ce2153fd4dcb3ad9ce97/launcher/app/src/main/AndroidManifest.xml): exported MainActivity with MAIN/HOME/DEFAULT.
- [Original CleanYourTV build](https://github.com/Gathin23/cleanyourtv/blob/0bb7f7edbf53349c7975a29495515453ed850526/launcher/app/build.gradle) defaults to the same package. That newer source permits an opt-in alternate application id; this entry covers only the verified default identity.

Package-only discovery cannot distinguish ClearTV Islamic from CleanYourTV or other forks reusing this id. The single display name **ClearTV Islamic / CleanYourTV** discloses the ambiguity; it is not proof of the installed fork, signature, provenance or safety. No duplicate fork entry is added. The source link points to the requested [repository root](https://github.com/Pokka99/cleartv-Islamic), because its GitHub releases API returned no releases when inspected. It is not a download/release URL or a promise that an APK is available.

Shield2017 compatibility, the reporter's Android version, actual installation and Home takeover are unverified. A manifest HOME declaration and minSdk are source facts, not a hardware acceptance result. No upstream script, APK, signing or accessibility configuration was executed or changed.

## Focused implementation and tests

The JSON catalog gains one custom entry (8 total). Loader tests assert exact name/package/source URL, unique identity, missing/installed/disabled flags and deduplication against repeated discovered/tracked HOME handlers. Browser regressions check missing/installed/disabled/current UI states, source-site URL and lack of mutation calls. The existing takeover engine and safety code are unchanged.

The demo fixture defaults to missing, with an explicit cleartvState switch for other states. CI runs the browser regression with evidence capture and uploads the simulated screenshots/video beside normal demo frames. This allows visual evidence/gallery regeneration without installing local dependencies or touching a device.

## Validation and simulated visual evidence

Local workspace fmt, all-target Clippy with warnings denied and serial tests pass: core 464, desktop 114 with one preexisting live test ignored, mobile Rust 26, license 2 and registry 9. The initial loader subset passed all 20 tests. Logs are in `workspace-tests.txt` and `clippy.txt`.

Independent read-only source review approved `71f12cf5b0220a81cb7c1be93b286968a3bb9d6d` and then hardened head `8734ab4aa79d9fa414f19c2b16f984c39a0d9c82` with no blockers. Its nonblocking suggestion was adopted: clicking the source link must issue exactly one opener call and no other command. The reviewer did not independently execute browser tests.

[CI run39725797395](https://github.com/bryanroscoe/shield_optimizer/actions/runs/39725797395) passed all six jobs on that exact hardened source head: Linux/macOS/Windows Rust, desktop frontend, mobile frontend and simulated-device E2E. Desktop Svelte check was 0 errors/0 warnings; production build and the full launcher-row browser test passed. `frontend-ci-excerpt.txt` records the relevant completed-job lines. Local Svelte/Vite cache files remain offloaded, so no local frontend execution is claimed and no dependencies were installed locally.

The screenshots and video below come from that run's `desktop-demo-frames` artifact12402983993. They show the real Svelte UI against demo fixtures, with an explicit simulation overlay. The existing demo profile says Shield2019 Pro; that is synthetic data, not a test of either that model or the reporter's Shield2017.

- [Missing](missing-simulated.png): one catalog row and the requested source site.
- [Installed/enabled](installed-simulated.png): normal Set as default control.
- [Disabled](disabled-simulated.png): Enable and Enable & set default controls.
- [Current](current-simulated.png): Active/current-default state, retaining the source link.
- [7.6-second state video](launcher-states-simulated.webm).

All four screenshots and a sampled video frame were visually inspected. Both normal desktop galleries were regenerated from the same run's 16 dark and16 light frames using the existing `screenshots/build-gif.sh`; launcher frames were inspected. Raw recorder intermediates are omitted. `provenance.json` records artifact/source-head and file hashes. The final evidence-only commit is covered separately by the PR's exact-head CI and delta review; the source-run result is not substituted for that gate.

Reproduce with existing dependencies from v2: `CLEARTV_EVIDENCE=1 npm run test:launcher-rows`; normal gallery capture is `npm run screenshots`. No device operations, APK fetch/install, upstream setup execution, signature/accessibility/security changes, paid calls, store publication or release is included. Shield2017 hardware acceptance remains unverified.
