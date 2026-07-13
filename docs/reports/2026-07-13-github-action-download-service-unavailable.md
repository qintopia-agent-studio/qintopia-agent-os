# GitHub Action Download Service Unavailable

Date: 2026-07-13

## Scope

Record the Rust quality CI infrastructure failure on PR #100 after the observation
secret-diagnostic remediation commit.

## Observed Evidence

- CI run `29253300686` completed its main check and Xiaoman PostgreSQL integration jobs
  successfully.
- `Rust quality baseline` job `86826837419` stayed in `Set up job` for five minutes and
  failed before checkout or any repository command ran.
- GitHub annotations reported `Failed to resolve action download info` and
  `Service Unavailable`.

## Root Cause

GitHub Actions could not resolve or download an action during hosted-runner setup. The
failure happened before source checkout, Rust tool installation, coverage, Clippy, or
tests, so it was not caused by repository code or the PR changes.

## Resolution

Record the infrastructure failure and trigger a fresh CI run from the follow-up
documentation commit. Do not weaken, skip, or bypass the Rust quality gate.

## Validation

- Confirm the next PR #100 `Rust quality baseline` reaches repository steps.
- Require coverage and Clippy to pass before merge.
- Keep the already passing main check and disposable PostgreSQL integration results as
  supporting evidence only; they do not replace the Rust quality job.

## Remaining Boundary

If the hosted runner repeats the same action-download failure, retry only after GitHub
service recovery. Do not merge while the required check is red, and do not change
production or repository policy to work around an external outage.
