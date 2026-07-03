# Agent Checks

`check-agents.mjs` validates the active Agent package contract.

Run:

```bash
pnpm agents:check
```

The check verifies:

- required active Agents are registered
- `xiaoqin` is not registered as an active Agent
- every registered Agent has `README.md`, `agent.yaml`, `profile.template.yaml`,
  `capabilities.md`, `runtime-notes.md`, and `docs/source-snapshot.md`
- profile templates include purpose, prompt sections, capabilities, forbidden actions,
  runtime mounts, excluded runtime state, and dry-run expectations
- package-like allowed capabilities point to active registry entries
- active Agents do not depend on deprecated packages
- `huabaosi` remains draft/review-pool until owner approval
