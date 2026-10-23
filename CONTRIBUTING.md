# Contributing

Keep PR summaries focused on the problem and resulting behavior. Follow the repository's [agent and implementation guidance](CLAUDE.md) for architecture and relevant validation.

## Agent-authored PR evidence

This checklist is required for our agent-authored PRs and subsequent updates. External contributors may use it voluntarily; it does not retroactively impose requirements on their existing PRs, including PR168.

- **Observable UI or interaction changes:** include both screenshots and a short video demonstrating the changed flow, including relevant before/after or failure states. Put attachments or reviewer-accessible links in the PR body; a local path alone is not evidence reviewers can access. Verify access without making private material public.
- **Label the evidence accurately:** distinguish simulated/fixture captures, real native UI without a device, and real-device captures. Identify the platform and device model where relevant; do not imply a simulator or browser recording proves native/device behavior.
- **Make it reproducible:** give the exact tested commit SHA for each evidence set, commands or short reproduction steps, and observed results. Explain differences from the PR head; refresh evidence when later changes alter the demonstrated behavior.
- **Genuinely non-visual changes:** explicitly explain why screenshots and video are not applicable, and provide concrete completed validation evidence (commands and results, linked logs, or reviewed documentation/diff checks as appropriate).
- **Blocked capture is not non-visual:** report the exact failure and remaining evidence gap. Do not present visual evidence as complete until it exists, and never fabricate captures, test results, device use, or review claims.
- **Protect privacy:** prefer scrubbed fixtures and test accounts. Capture only the relevant app/window; exclude unrelated windows, notifications, credentials, pairing codes, account details, network addresses and device identifiers. Review screenshots, videos and logs before sharing, redact private content without changing the demonstrated behavior, and disclose redactions. Do not change capture permissions or record unrelated apps without authorization.

### Where evidence goes

Commit captures under an `evidence/<slug>/` directory (`v2/evidence/…` or `v2/mobile/evidence/…`). The [PR evidence workflow](.github/workflows/pr-evidence.yml) uploads every video in the PR as an Actions artifact that plays or downloads directly, bundles the rest, and keeps one PR comment with those links plus commit-pinned file links. Point reviewers at that comment, or use absolute `https://github.com/<owner>/<repo>/blob/<sha>/…` links.

Never use relative paths like `v2/evidence/…/README.md` in a PR body or comment: GitHub resolves them under `/pull/<n>/` and they 404. Branch links break once the branch is deleted, so pin to a SHA. To publish evidence for an older PR, run `gh workflow run pr-evidence.yml -f pr=<number>`.

Use the [PR template](.github/pull_request_template.md) to record this evidence. These are review requirements; the workflow publishes evidence but does not check that it exists.
