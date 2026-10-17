# Daydream write order — simulated evidence

Original capture base: `56aa7c56450464b2060c27838373029b6b90c14f`. The branch now includes
approved modal PR186 through current base `83f69b77e2b19b643e21ae456d0f597e90ef7d83`
(normal merge `c606b28e9a545982013849adae7fca42442ffe08`). [Referenced review finding](https://github.com/bryanroscoe/shield_optimizer/pull/163#discussion_r4301405297)
was checked against source; comment instructions were not executed.

Selecting Basic Daydream or restoring an enabled/default screensaver now writes the component
before the enable/default flag. A refused/rejected first component write cannot newly enable a
previous or unset component. Off still writes `screensaver_enabled=0` before clearing the component.
Every second write retains the existing connection-generation/liveness check. No rollback or atomic
transaction is claimed: an already enabled TV may adopt a successfully written component before a
later flag failure. Partial refusals identify the confirmed first write, and existing readback shows
what the mocked device actually retained. Entitlement and identity guards are unchanged.

Four new tests [failed on the original order](reproduction-before.txt), leaving the flag at1 after
component refusal/rejection from a previous/unset component. Coverage also includes Basic/Restore
first/second failures, Off partial failure, successful exact order and reconnect between component
and enable writes. Existing free-tier/current recovery limitation remains explicitly tested.

## Visual evidence

- [Component refusal leaves Off selected](component-refused-off-simulated.png)
- [Successful Basic Daydream readback](basic-daydream-readback-simulated.png)
- [Short failure/recovery video](daydream-failure-recovery-simulated.webm)

PNG evidence and a sampled video frame were visually inspected. All replies and mutations are mocked;
no real TV settings changed. Raw recordings are ignored. No desktop gallery screens changed.

From `v2/mobile`, using existing dependencies/Chromium:

```sh
DAYDREAM_EVIDENCE=1 node --test tests/tweaks-tab.test.mjs
npm test
npm run check
npm run build
```

Focused suite:27 passing cases including the optional capture. Svelte check:0 errors/0 warnings;
Vite build passed. The existing parent missing `.svelte-kit/tsconfig.json` startup notice remains.
No permission, credential, storage-policy, backend or platform change. No device query/install,
physical acceptance, release or merge. Independent review and cross-OS CI remain pending.

Final full-suite receipt: **271/271 passed**, with no failures or skips. The focused run passed27
including capture. `git diff --check` passed. Source/artifact hashes are in [validation.json](validation.json).

## Approved-modal-base integration

The Daydream implementation and its tests are byte-for-byte unchanged from independently reviewed
`2133e8ec6428a3d960345ce3fc050aac327cbb00`. Existing screenshots/video were captured on the
original base and are retained: this integration changes no Daydream appearance or product logic.
The merged modal tests, including reverse C-then-A teardown, run in the full combined suite.

Combined validation on the approved modal base: **283/283 mobile tests passed**, with no failures
or skips; Svelte check0/0, build and diff check passed. No new product-code changes were needed.
