# Contributing

Keep PR summaries focused on the problem and resulting behavior. Follow the repository's [agent and implementation guidance](CLAUDE.md) for architecture and relevant validation.

## CI usage

Open agent-authored PRs as **drafts** (`gh pr create --draft`) and keep them draft
while implementing, reviewing and running focused local checks. Push finished
milestones as usual; draft PRs allocate no runners in the three test workflows.
Mark a PR ready only when it is intended for merge and local validation is recorded.

| Action | Test workflow behavior |
| --- | --- |
| Open or update a draft PR | Skip all jobs; no runner minutes |
| Mark ready, open/reopen a ready PR, or push to a ready PR | Run the applicable full suites, subject to existing path/base filters |
| Add `skip-ci` | Skip jobs and cancel an older run for that PR/workflow |
| Remove `skip-ci` from a ready PR | Run the applicable suites for the current revision |
| Convert to draft or close | Skip jobs and cancel an older run for that PR/workflow |
| Change an unrelated label | No tests and no cancellation of a useful run |
| Push/merge to `main` | No duplicate test run |
| Manually dispatch a test workflow | Run its full suite on the selected ref, even if a PR is draft or labeled `skip-ci` |

Use `skip-ci` to pause CI during a batch of review fixes or while recording local
validation: `gh pr edit <number> --add-label skip-ci`. Remove it when finished with
`gh pr edit <number> --remove-label skip-ci`. Prefer draft status for ordinary
work in progress. If a ready PR needs several fixes, pause **before** pushing.
Superseded runs are automatically cancelled, and each job has a time limit.
Older branches must first incorporate this workflow change to use the policy.

Before merging, remove `skip-ci`, mark the PR ready, and verify the applicable
checks actually **ran and passed for its latest revision**. Local tests do not
prove other operating systems passed. A skipped job is not validation, even if
GitHub renders it green. The current repository ruleset does not require checks;
this is a contributor/agent requirement, not an enforced merge gate. Any explicit
owner-approved CI exception must be recorded with its scope and local evidence.

The existing test coverage and OS matrices remain intact. The release and manual
AppImage workflows are unaffected. For an exceptional direct push or integration
check on `main`, dispatch the relevant test workflow explicitly, for example
`gh workflow run v2-tests.yml --ref main`; wait for that run before relying on it.
Avoid commit-message `[skip ci]` as the normal control: GitHub can leave required
checks pending, whereas draft status and the label are visible and reversible.
See [GitHub's skip behavior](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/skip-workflow-runs).

## Agent-authored PR evidence

This checklist is required for our agent-authored PRs and subsequent updates. External contributors may use it voluntarily; it does not retroactively impose requirements on their existing PRs, including PR168.

- **Observable UI or interaction changes:** include both screenshots and a short video demonstrating the changed flow, including relevant before/after or failure states. Put attachments or reviewer-accessible links in the PR body; a local path alone is not evidence reviewers can access. Verify access without making private material public.
- **Label the evidence accurately:** distinguish simulated/fixture captures, real native UI without a device, and real-device captures. Identify the platform and device model where relevant; do not imply a simulator or browser recording proves native/device behavior.
- **Make it reproducible:** give the exact tested commit SHA for each evidence set, commands or short reproduction steps, and observed results. Explain differences from the PR head; refresh evidence when later changes alter the demonstrated behavior.
- **Genuinely non-visual changes:** explicitly explain why screenshots and video are not applicable, and provide concrete completed validation evidence (commands and results, linked logs, or reviewed documentation/diff checks as appropriate).
- **Blocked capture is not non-visual:** report the exact failure and remaining evidence gap. Do not present visual evidence as complete until it exists, and never fabricate captures, test results, device use, or review claims.
- **Protect privacy:** prefer scrubbed fixtures and test accounts. Capture only the relevant app/window; exclude unrelated windows, notifications, credentials, pairing codes, account details, network addresses and device identifiers. Review screenshots, videos and logs before sharing, redact private content without changing the demonstrated behavior, and disclose redactions. Do not change capture permissions or record unrelated apps without authorization.

- **Attach, don't link paths:** upload screenshots and videos with `gh pr create`, `gh pr edit <n>` or `gh pr comment <n>` using `--attach <file>` (gh 2.99 or newer). GitHub renders them inline and plays videos. Repo-relative paths such as `v2/evidence/…/README.md` 404 from a PR page, and branch links break once the branch is deleted.

Use the [PR template](.github/pull_request_template.md) to record this evidence. These are review requirements, not an automated CI evidence check.
