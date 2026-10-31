## Summary

<!-- Briefly describe the problem and resulting behavior. -->

## Evidence

Required for our agent-authored PRs; optional for external contributors. Existing external PRs are not retroactively covered. See `CONTRIBUTING.md`.

- Change type: visual/interaction or genuinely non-visual
- Exact tested commit SHA(s), including any differences from the PR head:
- Reproduction commands/steps and observed results:
- Screenshots and short video, attached with `gh pr edit <n> --attach <file>` or `gh pr comment <n> --attach <file>` (label simulated/fixture, native without a device, or real-device; include platform/model):
- If non-visual, why screenshots and video are not applicable, with completed validation evidence:
- Any capture blocker, redaction, or remaining evidence gap:

### CI readiness

Keep this PR draft during implementation and review fixes. Before merge, remove
`skip-ci`, mark ready, and verify that the applicable suites ran and passed on the
latest revision. Skipped checks are not test evidence. See `CONTRIBUTING.md`.

### Agent evidence checklist

- [ ] Observable UI/interaction changes have **both screenshots and a short video**, or a genuine non-visual reason and concrete validation evidence are provided above.
- [ ] Screenshots and video are uploaded as GitHub attachments (`--attach`), not linked by local or repo-relative paths.
- [ ] Tested revision, reproduction steps and results are accurate; any captures are labeled by their actual simulation/native/device basis.
- [ ] Screenshots, videos and logs were reviewed for private content; redactions and capture gaps are disclosed, with no fabricated evidence or claims.
