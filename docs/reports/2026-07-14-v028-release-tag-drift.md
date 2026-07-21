# v0.2.8 Release Tag Drift

Date: 2026-07-14

## Observed Evidence

`v0.2.8` was published at 2026-07-14T12:32:47Z with tag target
`c572d3b86dd641e37a975dba6991ca89dbb448d1`.

The release-triggered `Deploy Production` workflow run `29332893048` failed in
`build-release-artifacts` before artifact build, COS upload, deploy request creation, or
server interaction.

The failed step was `Resolve release tag`:

```text
Release tag must point to current origin/master HEAD.
release tag commit: c572d3b86dd641e37a975dba6991ca89dbb448d1
origin/master HEAD: f466d72e688454f227b540c7fd56857ef90e4533
```

The only commit on `origin/master` after `v0.2.8` was
`f466d72 fix: compile-gate huabaosi live adapter (#124)`.

## Root Cause

The draft Release was prepared from the then-current `master`, but `master` advanced
before the draft was published. The production deploy workflow intentionally rejects a
release tag that is no longer the current `origin/master` HEAD so production secrets run
only with the latest reviewed workflow code and release payload.

## Resolution

Do not retry or manually dispatch deployment for the stale `v0.2.8` tag.

Use the next Release Please PR generated from current `master` (`#132`, `0.2.9`) after
its exact-head manual CI validation passes. Publish the resulting draft Release only
after confirming its tag target equals current `origin/master`.

## Validation

- `gh release view v0.2.8 --json tagName,isDraft,isPrerelease,publishedAt,targetCommitish,url`
- `gh run view 29332893048 --json status,conclusion,headBranch,headSha,event,workflowName,jobs,url`
- `gh run view 29332893048 --log-failed`
- `git log --oneline c572d3b86dd641e37a975dba6991ca89dbb448d1..origin/master`

## Remaining Boundary

No production artifact was built or uploaded for `v0.2.8`. No deploy request was
created, no server runner consumed a request, no `release/current` symlink moved, and
the Huabaosi WeCom production route remains the existing Hermes route.

Before any Huabaosi WeCom production routing PR, the release/current evidence must come
from a successful deploy of a Release tag that points to the current `origin/master`.
