# Private saves and clipboard-only reports — simulated evidence

Base: `83f69b77e2b19b643e21ae456d0f597e90ef7d83`, including approved modal PR186.
Pending Daydream/thermal/diagnosis branch changes are not included. This is an existing-flow
frontend clarification, not an export implementation.

## Scope and retained behavior

- Screenshot preview labels the returned private app location, distinguishes it from public
  Gallery/Downloads and explains that Close only hides the preview. The native capture command,
  returned path/base64 bytes, automatic save behavior and close state transition are unchanged.
  Its close control now has the accessible name “Close screenshot preview”.
- Files' success toast, actual returned destination and session-only list explicitly describe private
  app storage. No phone-file-browser, Files destination, Open or Share shortcut is promised.
- App-report, unknown-diagnostic JSON and debug-log descriptions identify clipboard text rather
  than file save. The oversized-report GitHub-opening clipboard fallback is still described and
  tested; report serialization, redaction, preview, clipboard/browser fallback and warnings remain.

Only local layout adjustments needed for readable clarification/actions are included: screenshot
notes use scalable text/line spacing and word-aware path wrapping; the preview image uses border-box
sizing to keep its existing border inside the available width (the new test exposed a two-pixel
preview overflow); More's existing copy toolbar can wrap so Copy diagnostic JSON is not clipped.
No Diagnostics header styles or other general layout changes. The separately tracked 320px
Diagnostics header overflow remains open.

No native provider/path, Android permission, security, SAF, Share, backend, dependency or export
formatter changes. Existing generated-provider configuration is untouched.

## Tests and visuals

New mocked tests cover screenshot success/error/close/recapture at 384px and 320px with 200% root text;
exact returned PNG bytes/path; no deletion/export IPC on Close; actual copied filename/path after
collision naming; failed file copy without false success; report preview/clipboard fidelity; long
GitHub-report copy fallback; failed clipboard fallback; diagnostic JSON and debug-log copy success/
failure; and copy-toolbar wrapping at 320px/384px.

All native replies, clipboard writes and browser-opening calls are stubbed. The screenshot fixture
is a canvas-generated PNG explicitly labeled “SIMULATED TV SCREEN”; no device is connected or queried.
This verifies UI behavior, not native Android persistence or an installed app.

- [Private screenshot preview](screenshot-private-preview-simulated.png)
- [384px preview](preview-384-100-simulated.png)
- [320px  / 200% text, top](preview-320-200-simulated.png) and [scroll tail](preview-320-200-tail-simulated.png)
- [Capture failure, without false saved state](screenshot-error-simulated.png)
- [Actual private file-copy destination](private-file-copy-simulated.png)
- [App-report copy-only description](copy-only-report-simulated.png)
- [Diagnostic JSON copy-only description](copy-only-diagnostic-json-simulated.png)
- [Debug-log copy-only description](copy-only-debug-log-simulated.png)
- [Short simulated interaction recording](private-save-copy-clarification-simulated.webm)

PNG evidence and a sampled video frame are visually inspected. Raw intermediate recordings are
ignored. No desktop gallery screen changes.

From `v2/mobile`, using existing dependencies and Chromium:

```sh
EXPORT_COPY_EVIDENCE=1 node --test tests/export-discoverability.test.mjs tests/apps-catalog.test.mjs tests/files.test.mjs tests/unknownDiagnostics.test.mjs
npm test
npm run check
npm run build
```

The existing parent missing `.svelte-kit/tsconfig.json` startup notice remains. No physical save,
clipboard, browser, pinch/accessibility-font-scale or provider behavior is claimed. Independent
review and cross-OS CI remain pending; no release, merge, installation or device operation occurred.

Final host validation: **282/282 full mobile cases and 37/37 focused cases including capture passed**,
with no failures or skips. Svelte check found 0 errors / 0 warnings; Vite build and diff check passed.
The video is5.44 seconds. [Validation receipt](validation.json) records source/artifact hashes.
