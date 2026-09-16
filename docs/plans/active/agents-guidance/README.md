# Guidance reorganization

Baseline: `6b792228b42f9e2a628323dac614caf8a08ef021` (root AGENTS.md: 2,089 lines;
Sidecar AGENTS.md: 279 lines). The owner approved relocation and deduplication, not
changes to permissions, conditions or exceptions.

The five AGENTS.md files are short operating entries. Detailed rules belong to existing
package README files and runbooks. Do not maintain a second rule database, per-paragraph
hash ledger or migration-specific wrapper in those documents. Compare the original files
and this PR through Git history when reviewing policy preservation.

## Destination map

| Original subject                                                 | Owning document                                                                                                                                                                                 |
| ---------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| General engineering, worktrees, package placement and validation | [Programming guardrails](../../../engineering/programming-agent-guardrails.md#operating-rules)                                                                                                  |
| Release identity, installation and rollback                      | [Deploy runner](../../../../deploy/runner/README.md#operating-rules)                                                                                                                            |
| Sidecar Rust, SQLx, capabilities and artifact gates              | [Sidecar](../../../../runtime/sidecar/README.md#operating-rules)                                                                                                                                |
| Hermes Profile compatibility                                     | [Hermes](../../../../runtime/hermes/README.md#operating-rules)                                                                                                                                  |
| Cron ownership and migration                                     | [Cron source of truth](../../../operations/hermes-cron-source-of-truth.md#operating-rules)                                                                                                      |
| QiWe callbacks, transport and media delivery                     | [QiWe](../../../../skills/qiwe/README.md#operating-rules)                                                                                                                                       |
| Erhua identity and recognition                                   | [Erhua](../../../../agents/erhua/README.md#operating-rules)                                                                                                                                     |
| Huabaosi image generation and WeCom                              | [Huabaosi](../../../../agents/huabaosi/README.md#operating-rules)                                                                                                                               |
| Xiaoman activity and posting                                     | [Activity](../../../../skills/xiaoman-activity/README.md#operating-rules)                                                                                                                       |
| Xiaoman daily report                                             | [Daily report](../../../../workflows/xiaoman-daily-case-report/README.md#operating-rules), [cron runbook](../../../operations/xiaoman-daily-case-report-hermes-cron-runbook.md#operating-rules) |
| Erhua morning brief                                              | [Morning brief](../../../../workflows/erhua-morning-brief/README.md#operating-rules)                                                                                                            |
| Xiaoman production evidence and weekly acceptance                | [Evidence](../../../operations/xiaoman-production-evidence-runbook.md#operating-rules), [weekly loop](../../../operations/xiaoman-weekly-minimum-loop-runbook.md#operating-rules)               |
| Historical exceptions awaiting review                            | [Pending decisions](pending-review.md#operating-rules)                                                                                                                                          |

## Review and validation

During restructuring, compare all 343 original blocks against their destination text;
merge only exact duplicates. Keep differing conditions and historical exceptions. Review
credential protection, production authorization, unknown delivery outcomes, immutable
release identity, rollback and Profile preservation explicitly. This one-time comparison
is not a permanent restriction against ordinary reviewed documentation edits.

Walk five tasks through the change routing index: Erhua reply, Hermes upgrade, Sidecar
schema, deployment rollback and documentation-only work. Each route must expose its
scoped rules, owning documentation and validation commands.

Permanent checks cover entry existence/scope, byte budgets, readable lines, indexed
commands and local links/anchors. They do not prove semantic equivalence or production
safety. Run `pnpm collaboration:check`, `pnpm lint:md`, formatting, deployment contracts
and the applicable PR check tier. The PR records actual results and size statistics.

No production changes, automatic merge, release publication or pending policy decisions
are part of this work.
