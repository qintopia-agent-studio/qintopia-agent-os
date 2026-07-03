# Architecture

Start with [agent-os-overview.md](agent-os-overview.md).

This directory records stable system boundaries, data flow, and architecture decisions.
Detailed historical architecture documents should be adopted here only after owner
review and source classification.

## Current Documents

- [agent-os-overview.md](agent-os-overview.md): current Agent OS architecture baseline,
  control plane boundaries, and monorepo mapping.
- [../agent-os/domain-model.md](../agent-os/domain-model.md): shared business object
  language used by architecture and packages.
- [../operations/runtime-baseline.md](../operations/runtime-baseline.md): production
  runtime baseline and migration implications.

## Adoption Notes

- Local architecture documents are the default canonical input when they conflict with
  unreviewed server-side exploration.
- Server-side Rust and Huabaosi shadow documents should stay in the review pool until
  the owner approves them.
- WorkTool, OpenClaw, and Hermes Kanban architecture material belongs in `deprecated/`
  when it has audit value.
