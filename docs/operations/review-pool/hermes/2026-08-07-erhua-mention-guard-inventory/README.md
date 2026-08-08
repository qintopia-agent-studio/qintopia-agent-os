# Erhua Mention Guard — Review Inventory（二花 @ 提及防护 治理证据）

This package is the **prerequisite governance evidence** for eventually tightening the
Erhua (二花) group-reply mention guard so it cannot "snatch" replies it should not
answer. It is **not a deployable patch** and changes no runtime behavior.

Per the routing note in the weekly-preview cron inventory (#385,
`docs/operations/review-pool/hermes/2026-08-06-xiaoman-weekly-preview-cron-inventory/README.md`,
lines 66–73), the original "duplicate job / Erhua answers when it should not" root cause
lives in the **Hermes runtime dispatch layer** (user → which agent receives "create a
cron job"): a task-creation broadcast that hit **both Xiaoman and Erhua with no `@`
filter**. QiWe already filters non-mention group messages by default
(`QIWE_PASSIVE_PIPELINE_ENABLED=false`), so the group-message layer is guarded — the
leak is the task broadcast, not the chat layer. This package inventories the current
guard posture, the residual risk points, and a governance proposal.

## Source Evidence

| Evidence                                                        | Path                                          | SHA-256                                                            |
| --------------------------------------------------------------- | --------------------------------------------- | ------------------------------------------------------------------ |
| QiWe skill mention behavior + passive pipeline note             | `skills/qiwe/README.md`                       | `f01d6a89457f7b151dcfa4b4f63b17450e02deabf57c66452a9a82907528b6c6` |
| Erhua consultation workflow (trigger boundary, draft/high)      | `workflows/erhua-consultation/workflow.yaml`  | `693c01a4f094a7dea2f2b3002db854a9974154bffd8ac6bd83264b8c5d0b6975` |
| Erhua consultation workflow README (acceptance scenarios)       | `workflows/erhua-consultation/README.md`      | `7a68a671fb0b78c857ee0765e30ae185e4ddcf80ca2fd3a72429698017739651` |
| QiWe replay fixtures semantics (mention / no-mention / private) | `fixtures/qiwe/README.md`                     | `4ed8a9b5e84f99016e6885e8b0a6a5d089075fc6a30f018225f24019edb21887` |
| Xiaoman V3 Feishu ingress bot-mention binding validation        | `runtime/sidecar/src/conversation_ingress.rs` | `83fb46b9975b1871eca4e84d974a1baa7dcccc856fa10a7b661b917959692267` |

All hashes are content SHA-256 of the files as committed on `master` at observation time
2026-08-07.

## Current Mention Guard Posture

| Layer                       | Mechanism                                                                                                                                                                                                                         | Guard status                            |
| --------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------- |
| QiWe group message layer    | `skills/qiwe` replies only when Erhua is mentioned or clearly cued (`skills/qiwe/README.md:40`); default `QIWE_PASSIVE_PIPELINE_ENABLED=false` filters non-mention group text                                                     | **Guarded**                             |
| QiWe replay fixtures        | `group-mention.json` eligible for guarded processing; `group-no-mention.json` must NOT trigger Erhua; `private-message.json` stays outside group reply (`fixtures/qiwe/README.md`)                                                | **Guarded (contract)**                  |
| Erhua consultation workflow | `status: draft`, `risk_level: high`; "Reply only when Erhua is explicitly mentioned or cued" (`workflows/erhua-consultation/README.md:8`); acceptance scenario "Private or non-mention group text does not trigger a group reply" | **Guarded (policy), not yet CI-locked** |
| Xiaoman V3 Feishu ingress   | `conversation_ingress.rs:314–330` enforces bot-mention binding: direct and group triggers require `should_trigger` + `is_mention_bot` + `mentioned_bot_ref == config.bot_open_id_ref`, else `binding is invalid`                  | **Guarded + enforced**                  |

## Residual Risk（残留风险点）

- **R1 — Erhua consultation boundary not CI-locked.**
  `workflows/erhua-consultation/workflow.yaml` `next_actions` still lists "Add replay
  fixtures for mention, missing evidence, complaint, and no-trigger cases" and "Wire the
  workflow to package-level tests before changing production reply behavior." The
  workflow is `draft`; its mention-trigger boundary is documented but **not enforced by
  CI**. A future loosening of the trigger condition could let Erhua answer messages it
  should not.
- **R2 — Passive pipeline switch could widen intake.** `skills/qiwe/README.md:83` notes
  passive processors (e.g. group-solitaire activity collection) run "when enabled."
  `QIWE_PASSIVE_PIPELINE_ENABLED` is documented as default-off, but its **production
  value is unconfirmed** in this package. If enabled on the host, non-mention group
  messages could reach Erhua-path processing. Needs an owner-triggered read of the live
  value.
- **R3 — Task-broadcast duplicate (the true "抢答" root cause) is out of repo scope.**
  As recorded in #385, the duplicate-job root cause is the Hermes runtime dispatch that
  broadcasts "create a cron job" to **both** Xiaoman and Erhua with no `@` filter. It is
  a runtime/orchestration change, not repository Python, and is intentionally **out of
  scope** for this evidence package.

## Proposal（治理提案，deployable: false）

- **P1 — Lock the Erhua consultation mention boundary.** Implement the `next_actions` in
  `workflows/erhua-consultation/workflow.yaml`: add mention / missing-evidence /
  complaint / no-trigger replay fixtures and wire them into package-level tests, so the
  "only when explicitly mentioned" rule cannot regress silently. Keep `status: draft`
  until fixtures pass.
- **P2 — Confirm the live passive-pipeline value.** Owner reads the production
  `QIWE_PASSIVE_PIPELINE_ENABLED` value and records it; fold into a periodic review so a
  future enable cannot silently widen intake.
- **P3 — Escalate R3 to a runtime/orchestration owner decision.** This repository
  package does not implement the broadcast fix; it only records the finding and routes
  it to the owner as a separate, reviewed runtime change.

## Production Boundary

- No server write, restart, profile mutation, database write, or external send.
- This directory is not included in the deploy bundle.
- Rollback is a no-op: nothing here touches runtime state.

## Validation

```bash
pnpm test:qiwe
pnpm workflows:check
```
