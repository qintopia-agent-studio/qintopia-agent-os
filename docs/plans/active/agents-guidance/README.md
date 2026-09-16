# Agent guidance migration

Baseline: `6b792228b42f9e2a628323dac614caf8a08ef021`.

The owner approved reorganization, not policy changes. The original two files contain
2,368 lines. This inventory tracks 343 rule/prose blocks, including maps and read-order
sections. Each record identifies its source section and line interval; SHA-256
normalizes whitespace only. The baseline remains retrievable from Git, not duplicated as
another instruction manual.

- [Baseline inventory](baseline.json): immutable source identities and rule hashes.
- [Migration table](migration.json): one destination per source block. `moved` means the
  complete rule is retained; `pending-review` also retains it and does not suspend it.
- [Pending decisions](pending-review.md): no rule retirement is authorized here.

## Execution

1. Record source inventory and planned destinations before rewriting guidance.
2. Publish five short scope-specific entrypoints and linked topic contracts. Preserve
   complete conditions and exceptions in the destination blocks.
3. Replace AGENTS-specific literal-sentence checks with structural coverage/link checks;
   retain executable deployment behavior tests. Run formatting, Markdown, collaboration,
   deployment contracts and the repository automatic PR tier.

The migration checker establishes coverage and unchanged source-block text after
whitespace normalization. It cannot prove that summaries are semantically equivalent.
Reviewers must check routing, scope and summaries, especially unknown send outcomes,
credential boundaries, owner authorization, immutable releases and rollback.

## Acceptance walkthroughs

- Erhua replies: root task entry → change routing → QiWe rules and Erhua package.
- Hermes upgrade: root task entry → Hermes rules + deploy rules → recovery/upgrade
  guide.
- Sidecar schema: Sidecar rules → Postgres design/migrations → disposable database
  tests.
- Deployment rollback: deploy rules → runner/rollback guides → identity and rollback
  checks.
- Documentation only: root task entry → engineering routing → Markdown/collaboration
  checks.

No production, release, credential or runtime mutations are part of this PR. Do not
merge or publish automatically. Topic contracts stay authoritative after migration; edit
their rules only in a separately explicit policy change, updating its baseline mapping
with a review record rather than silently dropping coverage.
