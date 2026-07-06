# Agent OS Design

This directory contains implementation-facing Agent OS design documents. These documents
define object language, Agent contracts, acceptance expectations, and package mapping.

## Documents

- [domain-model.md](domain-model.md): shared business object language.
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
