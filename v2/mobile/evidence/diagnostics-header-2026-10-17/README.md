# Diagnostics header at narrow widths — simulated evidence

Verified current remote base: `dccfe99a805ffcb0c11f32a4fa9f227306ba1cf3`.
Merged thermal PR188 (`e05891dfbd33bb513eafb6794dcfd03c41d19ba4`) and private-save/copy
PR190 are both present. Before header implementation, the combined base passed **293/293 mobile
tests**, Svelte check 0/0 and build. Active PRs were inspected; no competing header fix was open.

## Reproduction and focused change

The [new 320px regression failed on unchanged base](reproduction-before.txt): the single-row
header forced the page wider than its viewport. The separately tracked measurement was 304px of
header content inside 272px.

Only Diagnostics-scoped CSS changes: the header can wrap, its title can take a separate row and
shrink/wrap within available width, and the Back/Find remote cluster stays together. Title and Live
font sizes use rem units at their original default 22px/11px sizes. Live now keeps at least 44×44px
of target area like the existing icon buttons. No controls, DOM order, accessible names, handlers,
report fields, data/units, shared-header styles, viewport directives or text-size/zoom restrictions
changed. At 384px/default text the header remains a single row; narrow or enlarged-text layouts
flow onto additional rows instead of hiding controls.

Tests cover 320px/384px at 100%/200% root text; full-page/header overflow; all control bounds/target
sizes; exact scalable text sizes; unchanged control text/names and viewport zoom policy; keyboard
Tab order; Live pressed state and mocked health reads; Back navigation; Shield confirmation with
Cancel only; and non-Shield/disconnected conditional controls. The mock refuses any `find_remote`
IPC, and tests assert none occurred.

## Evidence

- [320px/default text](header-320-100-simulated.png)
- [384px/default text](header-384-100-simulated.png)
- [320px/200% root text](header-320-200-simulated.png)
- [384px/200% root text](header-384-200-simulated.png)
- [Live on](live-on-simulated.png)
- [Remote confirmation, canceled without ringing](remote-confirm-cancel-only-simulated.png)
- [Short layout/control recording](header-layout-controls-simulated.webm)

PNG evidence and sampled video frames are visually inspected. Video remains 384px wide while
changing text size; the 320px states are captured separately to avoid recording resize padding.
All native replies are mocked; no physical device is connected or queried. Raw recordings are
ignored. No desktop gallery screens changed.

From `v2/mobile`, with existing dependencies and Chromium:

```sh
HEADER_EVIDENCE=1 node --test tests/diagnostics-header.test.mjs tests/health-presentation.test.mjs tests/playback-audio.test.mjs
npm test
npm run check
npm run build
```

Focused suite: 30 passing cases including optional capture; post-change Svelte check 0/0 and build
passed. The existing parent missing `.svelte-kit/tsconfig.json` startup notice remains. Root-font
simulation is not physical Android accessibility scaling or pinch verification. Native inset,
physical device, release/publication and app-store acceptance remain separate gates. No permission,
provider, dependency, device mutation, installation, merge or publication changes/actions.

The existing Amphetamine timer observed after host recovery did not match the prior recorded
bounded-session timing. Its ownership was not assumed, so it was preserved; this task did not
start or end another wake session.

Final validation: **298/298 full mobile tests and30/30 focused cases including capture passed**,
with no failures or skips. Post-change Svelte check0/0, build and diff check passed. The video
is3.28 seconds. [Validation receipt](validation.json) includes baseline/final checks and hashes.
