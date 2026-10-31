# Contributing

Keep PR summaries focused on the problem and resulting behavior. Follow the repository's [agent and implementation guidance](CLAUDE.md) for architecture and relevant validation.

## CI usage

This public repository runs its applicable full test suites automatically on PR
opens/updates (including drafts) and pushes to `main`. Keep normal CI enabled by
default. Draft status is a review signal, not a CI pause. Existing test coverage,
OS matrices, and path/base filters remain intact.

When requested, or when deliberately batching intermediate agent work, pause PR
CI with `gh pr edit <number> --add-label skip-ci` **before** pushing. Record why
it is paused and any local validation in the PR evidence. Remove the label with
`gh pr edit <number> --remove-label skip-ci` when finished; this starts applicable
CI for the current revision, including on a draft PR. Do not label every agent PR
by default, and do not confuse local checks with cross-platform validation.

Adding `skip-ci` or closing a PR cancels an older run for that PR/workflow.
Unrelated label changes neither run tests nor cancel useful runs. Superseded runs
are automatically cancelled, and each job has a time limit. Changing draft/ready
status alone does not repeat tests. Manual dispatch always runs the selected
workflow/ref; the PR label does not suppress `main` pushes or release workflows.
Older branches must incorporate this workflow change to use the label policy.

Before merging, remove `skip-ci` and verify the applicable checks actually **ran
and passed for the latest revision**. A skipped job is not validation, even if
GitHub renders it green. The current repository ruleset does not require checks;
this is a contributor/agent requirement, not an enforced merge gate. Record any
explicit owner-approved CI exception with its scope and local evidence.

Prefer the visible, reversible label to commit-message `[skip ci]`, which can
leave required checks pending. See
[GitHub's skip behavior](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/skip-workflow-runs).

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
