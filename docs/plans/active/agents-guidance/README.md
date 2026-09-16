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

## Baseline representation

Schema v2 stores the source path once per file and the original section title once per
consecutive section. Each row follows `entryColumns`: `[id, start, end, sha256]`. Source
lines are inclusive and hashes retain all 64 hexadecimal characters. This is a lossless
representation change; the source Git SHA, file hashes, 343 rule identities, sections,
line ranges and normalized rule hashes remain unchanged.

A baseline-only Prettier override uses 140 columns to keep each short audit row on one
line; ordinary JSON and AGENTS.md formatting remain unchanged. The file is machine audit
data, not required reading for ordinary tasks. It shrank from 2,764 lines / 87,113 bytes
to 416 lines / 37,616 bytes. Expanding v2 back to the original records verified exact
equality for every field of all 343 entries and both file metadata records. Use the
entrypoints and topic contracts for instructions, and this inventory when reviewing a
rule migration.

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

## Implementation evidence (2026-09-16)

The five entrypoints replace two original instruction files. Original non-heading,
non-blank source lines are all covered by the 343 inventory blocks; both source file
hashes and each block hash were independently checked against the baseline Git commit.
All 343 destination blocks pass whitespace-normalized identity verification.

| Entry   | Before lines / bytes | After lines / bytes | Budget |
| ------- | -------------------- | ------------------- | ------ |
| Root    | 2,089 / 152,954      | 166 / 9,576         | 16 KiB |
| Sidecar | 279 / 20,010         | 122 / 5,685         | 12 KiB |
| Deploy  | absent               | 102 / 4,763         | 12 KiB |
| Hermes  | absent               | 95 / 4,445          | 12 KiB |
| QiWe    | absent               | 101 / 4,550         | 12 KiB |

Eighteen focused contract documents are indexed by eleven existing package/engineering/
operations README files. Rule headings expose their original section and stable source
ID. The inventory keeps paths and line intervals for source review. Repeated clauses
with different conditions remain intact; their entrypoint summaries share links instead
of silently choosing one formulation.

The five acceptance walkthroughs above were completed against the routing table and
package indexes. Operations review checked credential retention, owner authorization,
ambiguous delivery outcomes, immutable identity, rollback/data separation and Profile
state preservation. Historical exceptions remain conditional and pending review.

The shared validator replaces AGENTS-specific English substring assertions in both
collaboration and deployment checks. Existing behavioral deployment checks remain.
Fourteen fixed-fixture tests cover valid navigation, missing scope files, paths and
anchors, missing/duplicate migration, removal of a no-retry restriction, harmless
wrapping, byte budgets, scope/command mistakes, traversal, and missing/empty package
indexes. Fixture entry files use `.fixture` suffixes and become AGENTS.md only in
temporary test trees, so the repository has exactly five actual instruction files.

Formatting, Markdown, collaboration (14 tests), and the complete deployment contracts
command passed locally. The automatic PR tier result is recorded in the PR validation
section after completion. No live browser flow is affected by this documentation change.
