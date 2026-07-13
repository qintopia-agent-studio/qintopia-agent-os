# PR-Agent PR Body Overwrite Remediation

Date: 2026-07-13

## Scope

Repair the required PR-body CI failure on Huabaosi staging smoke PR #99.

## Observed Evidence

- The PR was created with all required template sections and checked items.
- On the subsequent `synchronize` event, `PR-Agent review assistant` replaced that body
  with an AI-generated description and file walkthrough.
- The CI `pr:check-body` step then failed because `Summary`, `Planning`, `Domain`,
  `Validation`, `Production Boundary`, `Architecture / Tooling Boundary`, and
  `Changelog` were absent.

## Root Cause

`.github/workflows/pr-agent.yml` enabled `github_action_config.auto_describe: "true"`.
PR-Agent's automatic describe action owns the GitHub PR description, which conflicts
with the repository-owned PR template and its required CI fields. The prior
`pr_description.add_original_user_description: "false"` setting made the replacement
discard the completed body rather than preserve it. GitHub Actions reruns retain the
original event payload, so rerunning a failed job after manually restoring the PR body
still validates the stale overwritten body.

## Resolution

- Disable PR-Agent automatic describe while retaining automatic review and comment-based
  advice.
- Make CI contract validation require `github_action_config.auto_describe: "false"`.
- Trigger CI on `pull_request.edited`, so a repaired body produces a fresh event payload
  and can be validated without another source commit.
- Record that the PR body is author-owned in `AGENTS.md` and the PR-Agent operating
  guide.
- Restore the completed PR #99 body, then rerun the failed check after this workflow
  change is pushed.

## Validation

- `node tools/ci/check-ci-contracts.mjs`
- `pnpm check:light`
- `sh .husky/pre-commit`
- GitHub PR body validation after restoration and rerun.

## Remaining Boundary

PR-Agent remains advisory only. A maintainer can request `/describe`, but its output is
published as a comment and must not replace CI-required PR metadata.
