# Agent OS Design

This directory contains implementation-facing Agent OS design documents. These documents
define object language, Agent contracts, acceptance expectations, and package mapping.

## Current Decisions And Handoff

Before proposing changes to people, permissions, memory, room collaboration, or welcome
behavior, follow these existing sources. A new conversation does not reset decisions.

1. [Common contract](../plans/active/unified-person-welcome-v1-contract.md): confirmed
   behavior, Agent responsibilities, privacy, authority, and acceptance boundaries.
2. [Foundation execution plan](../plans/active/person-agent-foundation-execution-plan.md):
   current delivery sequence, implementation gaps, and retained future goals.
3. [Command handoff](../reports/2026-09-22-agent-os-command-handoff-review.md):
   inherited project work, corrected status, and links to historical reviews.
4. [Erhua and Anan review](../reports/2026-09-22-erhua-anan-history-review.md):
   accepted, superseded, and still-proposed decisions, including the supervisor/Silaoshi
   relationship and the assessment of Anan's PMS authority.
5. [Anan room collaboration plan](../plans/active/anan-pms-event-integration.md) and
   [development handoff](../plans/active/anan-pms-development-handoff.md): implement
   Anan-owned conversational PMS operations first, then payment and application
   triggers; reuse shared identity, authorization and durable execution records.

Use the latest explicit owner decisions and applicable contracts for intended behavior;
inspect current code and dated evidence for implementation status. Historical assistant
suggestions are not approval. Search these sources and the affected package before
asking the owner to repeat a decision or treating an old design as current.

Some current decisions may still be uncommitted in the active workspace. Before moving
to another checkout, inspect and preserve that work; a clean older branch does not
contain decisions merely because a conversation was archived. Link the owning source in
implementation handoffs rather than creating another independent policy summary.

## Documents

- [domain-model.md](domain-model.md): shared business object language.
- [community-business-model.md](community-business-model.md): six community objects,
  existing storage/tool mappings, and Erhua identity/scope/revocation acceptance gaps.
- [Scoped knowledge and runtime boundaries](community-business-model.md#9-社区楼栋与领域知识的分层维护):
  community/building/domain ownership, conversation and UI maintenance, current gaps,
  and responsibilities of Agent OS, Agents, Hermes, and business sources.
- [Front-desk collection](community-business-model.md#11-二花通过互动收集信息与积累社区记忆):
  community life experience, community co-creation, temporary status, permitted personal
  memory and voluntary participation without additional employee data-entry work; nearby
  recommendations and issue follow-up are examples rather than domain limits.
- [agent-contracts.md](agent-contracts.md): Agent, tool, adapter, approval, and audit
  contracts.
- [acceptance-tests.md](acceptance-tests.md): scenario-level acceptance tests and smoke
  expectations.

## Source Notes

The initial baseline comes from stable local documents under
`../qintopia-agent-os/docs/agent-os/`. Historical WorkTool, OpenClaw, Hermes Kanban, and
WorkTool-bound Xiaoqin documents should be adopted only into `deprecated/` or a review
pool unless an owner asks for a specific audit. This does not block a future Xiaoqin
Agent designed without WorkTool.
