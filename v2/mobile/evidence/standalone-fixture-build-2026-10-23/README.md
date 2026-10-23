# Standalone mobile fixture build repair

Base: contributor commit `222aecf0b741f27dfd62548c8825e7c13d01e5a5` on
`mobile-parity-pass`. Its original commit and authorship are preserved. Ownership
reconciliation found a clean primary checkout and the originating Claude session's
completed 00:43 UTC handoff, with no subsequent repair activity or overlapping PR.
All repair work uses a separate Developer worktree.

[Failed mobile CI job](https://github.com/bryanroscoe/shield_optimizer/actions/runs/39879563781/job/120173389492)
passed type checking, then failed production build while transforming the newly
shared desktop `demo-mock.ts`: desktop's tsconfig extends generated
`.svelte-kit/tsconfig.json`, absent on the isolated mobile runner. Browser tests
were consequently skipped. A fresh local checkout with only mobile dependencies
reproduced that exact failure (`baseline-build.txt`).

The mobile Vite config now supplies its existing compiler options as a JSON
**string** to `esbuild.tsconfigRaw`. In Vite6, the string form bypasses per-file
configuration discovery; an object still loads each file's nearest tsconfig first.
The build therefore uses the mobile options for imported fixture/shared TypeScript
without needing desktop-generated files. No dependency, compiler-option value,
fixture, device command, app screen or safety logic was changed.

`tests/standalone-build.test.mjs` creates a temporary source tree with a dangling
desktop tsconfig and only the mobile dependency cache. It builds production and
explicit demo modes separately, verifies fixtures are absent from production and
present in demo output, and asserts no desktop dependency directory or generated
SvelteKit directory exists. The regression fails with the old Vite config at the
same missing-extends error and passes with the fix. It is included by the existing
`npm test` glob, without changes to CI setup or generating desktop artifacts.

Local validation on macOS:

- Isolated production and demo build regression passed (`isolated-builds.txt`).
- Fresh mobile production build passed with no desktop dependencies or SvelteKit
  files present. `npm run check` reports 0 errors and 0 warnings. Tool configuration
  discovery still prints a nonfatal missing-desktop-base warning; it is not a
  Svelte diagnostic or a required generated-file dependency after this fix.
- All **344 mobile tests passed**, zero failures/skips (`mobile-tests.txt`).
- The contributor's `npm run screenshots` captured all **16 simulated screens** and
  contact sheet (`capture.txt`), using only fixture responses. Capture output stays
  gitignored as designed.

Screenshots/video are inapplicable to this configuration-only repair: rendered UI
and fixture content are unchanged. The full capture smoke test verifies the new
contributor workflow still runs; the isolated build test verifies production does
not include it. No real phone/TV query or mutation, APK operation, permission or
security change, paid model call, deployment or release occurred.

Exact-head CI and independent review gate this repair. This evidence does not
satisfy PR163's physical-device or release acceptance gates and does not authorize
merging PR163. PR213's explicit merge block, PR211/216 catalog holds, PR180 and
other drafts remain separate and unchanged.
