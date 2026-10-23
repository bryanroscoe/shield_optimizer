# Dependency remediation receipt — 2026-10-16

This changes dependencies/tooling only. No application feature, payment, account, permission,
font binary or license is changed. No force upgrade or dependency override is used.

## Mobile

- Verified no application import or build input uses `geist`; `src/app.css` directly loads the
  checked-in WOFF2 files. Only package metadata, font provenance and a notices warning referred
  to the npm package. Remove that unused dependency and its Next/Sharp peer chain (50 packages).
- Retain checked-in Geist 1.7.2 assets and complete OFL attribution/license. Update the provenance
  wording and the optional installed-package notices check; regenerate both notices outputs.
- Target only compatible affected transitives: devalue 5.8.1 -> 5.9.4, nanoid 3.3.15 -> 3.3.20,
  PostCSS 8.5.16 -> 8.5.30 and source-map-js 1.2.1 -> 1.2.2. Svelte/Vite major versions unchanged.
- Fresh audit and clean `npm ci --ignore-scripts`: **0 vulnerabilities**. Audit is time-dependent;
  repeat at release. Install scripts were not enabled for this validation.

## Desktop tooling

The separate desktop lockfile had affected Vite, SvelteKit and shared build transitives. Its
compatible update keeps existing majors: Vite 6.4.2 -> 6.4.4, Kit 2.61.1 -> 2.70.3, devalue
5.8.1 -> 5.9.4, nanoid 3.3.12 -> 3.3.20, PostCSS 8.5.15 -> 8.5.30, source-map-js 1.2.1 -> 1.2.2.
Npm also synchronizes the stale root lockfile version metadata 2.1.0 -> the already existing
package.json version 2.3.0; this is NOT a version bump, release or tag.

Fresh desktop audit: **0 critical/high/moderate; 3 low reports** for cookie 0.6.0 and its
SvelteKit/adapter-static dependency chain. These are one underlying advisory, not three distinct
vulnerabilities: https://github.com/advisories/GHSA-pxg6-pf52-xh8x.
Kit 2.70.3 still declares cookie ^0.6.0; fixed cookie >=0.7.0 is outside that range. Do not force
an override without a separate compatibility review. These are dev/build dependencies and this
app ships a static SPA (`ssr=false`), not a Node/SvelteKit cookie-serving backend. This limits the
observed exposure; it is not proof of safety for every build input or future server deployment.
The production-filtered desktop audit is zero. The affected Kit import is server-side.
Monitor the upstream range and re-audit before release.

## Validation and typography

Mobile/desktop clean installs, checks and ordinary builds pass. After preserving merged mobile
head `ff7d2e1` through normal merge `d120b8c`, the integrated mobile suite passes 224/224.
All 13 inherited release-verifier tests pass. All 26 esbuild platform lock entries are retained.
Desktop real-component launcher and Optimize safety tests pass against simulated devices.

The before/after mobile screenshot is byte-identical at 384x812; Geist, Geist Mono and Material
Symbols all report loaded. All checked-in font bytes are identical:

- Geist: a369fcf5628ea2aa4e1b9e2ec6a5b3624e365bda588e1f0f2f12b564f728fbb8
- Geist Mono: fba8f577f38a2bbcbe818efa6348dd58f36303a10b8737c42fefad275be563ab
- Material Symbols: 920a58b8f7793d89dadacd5d14102d103a0059c8076c79bd32347751b3b4bd44


Desktop demo comparisons covered 15 screens in both themes. 23 initial captures were identical;
three light frames initially captured a blank/wrong view. Targeted re-capture with explicit
rendered-heading assertions matched those three baselines byte-for-byte, yielding 26/30 exact.
The other four differ only in spinner phase or dynamic App List read timestamps; typography and
layout were inspected. No modified screenshot harness or regenerated gallery is committed.
A fixture-only compiled-demo probe hit the existing top-level-await/target constraint; normal
shipping builds pass and the supported dev-demo path was used for the comparison. This is not
physical-device, Android signed-artifact or general navigation-regression evidence.
