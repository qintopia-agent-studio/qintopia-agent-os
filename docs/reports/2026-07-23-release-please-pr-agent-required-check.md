# Release Please PR-Agent Required Check

## Incident

Release Please PR `#261` passed the authenticated full CI dispatch on its exact head,
including light, runtime, Rust quality, disposable PostgreSQL, and the final
`Release Please validation` status. GitHub still reported `mergeable_state=blocked`.

The active `protect-master` ruleset requires both `check` and
`PR-Agent review assistant`. The full CI dispatch attached `check` to the release head,
but the Release Please bot update did not trigger PR-Agent because GitHub suppresses
recursive workflow events created with `GITHUB_TOKEN`.

## Contract

- `pr-agent.yml` accepts an explicit Release Please PR number for manual validation.
- The dispatch reads the PR through the GitHub API and fails unless it is open, targets
  `master`, is bot-authored, has the generated Release Please body marker, and its head
  SHA exactly matches the checked-out workflow ref.
- An authenticated generated Release Please PR skips the external PR-Agent action. The
  successful workflow job provides the required check run on the exact head without AI
  review, PR edits, comments, secrets in output, or a forged commit status.
- Ordinary PR and member slash-command review behavior remains unchanged.
- Release merge and draft GitHub Release publication remain manual owner decisions.

The change does not modify repository rulesets, merge a Release, publish, deploy,
activate an Hermes profile, access production, or send externally.
