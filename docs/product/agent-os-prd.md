# Qintopia Agent OS PRD

Updated: 2026-07-03

## Product Definition

Qintopia Agent OS is an AI copilot operating layer for Qintopia community growth and
operations. It connects community channels, SOP knowledge, content workflows, service
follow-up, human review, and audit records into governed human-Agent workflows.

It is not a generic chatbot, a raw RAG system, or a replacement for human owners in
high-risk decisions.

## Current Product Scope

### Private-Domain Community Operations

- Keep Erhua as the QiWe / WeCom group-facing profile.
- Answer only clearly supported public-safe questions in the group.
- Route uncertain, sensitive, high-risk, or low-confidence questions to a housekeeper or
  responsible human owner.
- Record consultation context, source evidence, risk, and follow-up state.
- Support Erhua trainer input as controlled memory or policy evidence, not free-form
  prompt edits.

### Content And Promotion Operations

- Model topic selection, source collection, draft creation, review, publishing record,
  metrics, and review as reusable Agent OS workflows.
- Let Xiaoman, Huabaosi, Wenyuange, Silaoshi, and humans collaborate through explicit
  work items and artifacts.
- Keep human review before external publishing or public use of member stories,
  sensitive information, price promises, or policy claims.

### Engineering And Runtime Governance

- Keep Hermes as the Agent runtime.
- Move durable business state, work items, artifacts, audit events, and review records
  into the Agent OS control/data plane.
- Use Postgres as the system fact source and Feishu as the human workbench.
- Use git-managed packages and deployment artifacts instead of server-side hot edits.

## Current Non-Goals

- Xiaoqin implementation in the current phase. If Xiaoqin is re-enabled later, it must
  use a new non-WorkTool channel and a reviewed Agent package contract.
- WorkTool as a future Agent OS channel.
- Hermes Kanban as the future orchestration backbone.
- Automatic Xiaohongshu private-message reply.
- Automatic public publishing.
- Automatic final decisions on refund, compensation, contract, member conflict, privacy,
  or policy exception.
- A full standalone control panel in the first migration phase.
- A new RAG framework, vector database, document parser, or Agent runtime.

## Roles

| Role         | Current purpose                                                                      |
| ------------ | ------------------------------------------------------------------------------------ |
| Erhua        | QiWe / WeCom group front desk, public-safe replies, consultation capture, escalation |
| Xiaoman      | Activity signal, event material, community story and content source preparation      |
| Huabaosi     | Visual material drafts, posters, image prompts, internal artifacts                   |
| Wenyuange    | Knowledge lookup, evidence retrieval, source grading, disclosure filtering           |
| Silaoshi     | Community operations, SOP, activity planning, service follow-up, review templates    |
| Guanerye     | Engineering automation, adapter/workflow implementation, validation support          |
| Human owners | Approval, policy decision, final delivery, risk handling                             |

## Success Metrics

| Area                   | Metric                                                                            |
| ---------------------- | --------------------------------------------------------------------------------- |
| Public-safe answers    | Supported SOP/FAQ questions are answered or escalated correctly                   |
| Escalation             | High-risk, low-confidence, missing-source questions route to a human owner        |
| Consultation records   | Key questions have source, risk, owner, status, and follow-up state               |
| Content workflow reuse | Content work is no longer only local scripts or private notes                     |
| Publishing records     | Published content records include channel, link, owner, and publish time          |
| Review loop            | Important content has metrics or review records that inform future topics         |
| Governance             | Production-facing changes move through git, CI, review, smoke, and rollback notes |

## Product Boundary

Agents can draft, classify, retrieve, summarize, propose, and create internal work
items. Agents must not independently commit high-risk external actions. Human owners
remain responsible for policy exceptions, final public delivery, and production route
changes.
