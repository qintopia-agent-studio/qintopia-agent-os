# Release PR CI Not Triggered

Date: 2026-07-14

## Observed Evidence

Release Please PR #90 was open, bot-authored, mergeable, and updated to release 0.2.7,
but GitHub reported no checks on its head SHA. The PR contained only the generated root
`CHANGELOG.md` and `.release-please-manifest.json` changes. Its accumulated release
content represented the merged #91 through #104 batch rather than a single feature PR.

The dedicated Release Please validator passed offline against the exact PR head archive,
but that local result was not attached to the PR and could not satisfy a repository
merge gate by itself.

## Root Cause

The Release Please workflow can create or update its PR with `GITHUB_TOKEN`. GitHub
suppresses recursive workflow events created by that token, so the PR's `opened` or
`synchronize` update does not necessarily trigger `.github/workflows/ci.yml`.

## Resolution

- Add a `workflow_dispatch` input for an explicit Release Please PR number.
- Require the workflow to run on the release PR head branch and compare `github.sha`
  with the API-reported PR head SHA.
- Read and validate the real PR state, base branch, head branch, bot author, and
  generated body marker before treating it as a Release Please PR.
- Reconstruct the pull-request event only for the existing dedicated manifest/changelog
  validator.
- Give only the `changes` and `check` jobs read access to pull-request metadata.
- Give only the authenticated `check` job commit-status write permission, and publish a
  fixed `Release Please validation` success/failure status on the verified head SHA.

The dispatch rejects ordinary branches, stale release heads, closed PRs, and PRs that do
not target `master`. It does not merge, tag, publish, deploy, or use production secrets.

## Validation

- `node tools/ci/check-ci-contracts.mjs`
- `node tools/ci/check-release-please-pr.mjs` against the archived #90 head and a
  synthetic event containing the API-observed bot metadata
- `sh .husky/pre-commit`
- After merge, run
  `gh workflow run ci.yml --ref <release-head> -f release_please_pr_number=90` and
  require the workflow plus PR commit status to pass.

## First Dispatch Follow-Up

The first exact-head dispatch for #90 succeeded on `077e120`, including the dedicated
Release Please validator. GitHub still returned no checks in the PR status rollup;
workflow-dispatch check suites were visible only on the Actions run. The result was
technically valid but not enforceable or visible at the PR merge surface. The follow-up
adds an authenticated commit status linked to that run. A successful Actions run without
the fixed PR status remains insufficient for merge.

## Remaining Boundary

Passing this check proves only release metadata integrity on the exact PR head. The
owner must still make one coordinated decision to merge the Release Please PR and
publish the resulting draft Release. Production deployment and Xiaoman/Aliang
observation remain separate reviewed steps.
