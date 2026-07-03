# Package Contract

Qintopia Agent OS is organized by capability, not by implementation language. A package
represents an Agent profile, a reusable skill, a workflow, an MCP adapter, a runtime
area, or a deployment area.

## Required Files

Every adopted package should eventually contain:

- `README.md`: scope, owner, source, commands, production boundary.
- `manifest.yaml`, `agent.yaml`, or `workflow.yaml`: machine-readable metadata.
- `tests/` or `fixtures/`: executable checks, replay data, or acceptance evidence.
- `examples/` when examples make safe local use easier.
- decommission or rollback notes when the package replaces a live path.

Missing fields must be documented as an exception before the package is called
production-ready.

## Metadata Fields

Package metadata should include:

```yaml
id: skills/qiwe
type: skill
owner: TBD
risk_level: high
source:
  path: ../qiwei-hermes-plugin
  reference: TBD
production_boundary:
  external_sends: true
  database_writes: false
  runtime_profile: true
validation:
  commands:
    - pnpm check
status: draft
```

The exact schema will be enforced after registry validation is added. Until then, these
fields are the expected contract for migration work.

The enforced schema now lives at
[`../../registry/schemas/package-manifest.schema.json`](../../registry/schemas/package-manifest.schema.json).
Run `pnpm registry:check` before adding or changing package manifests.

## Placement Rules

- Agent identity, prompt, memory policy, allowed tools, and forbidden actions belong in
  `agents/<agent>/`.
- Reusable capabilities belong in `skills/<capability>/`.
- Cross-Agent processes belong in `workflows/<workflow>/`.
- MCP servers and adapters belong in `mcp/<adapter>/`.
- Runtime templates and generated config rules belong in `runtime/<area>/`.
- Deployment scripts, smoke checks, and rollback runbooks belong in `deploy/<area>/`.
- Historical POC material belongs in `deprecated/<topic>/`.

Do not create top-level language buckets such as `python/`, `rust/`, or `typescript/`.
Rust, Python, TypeScript, shell, and SQL belong inside the package that owns the
capability.

## Templates

- `agents/_template/agent.yaml`
- `skills/_template/manifest.yaml`
- `workflows/_template/workflow.yaml`
- `mcp/_template/manifest.yaml`
- `runtime/_template/manifest.yaml`
- `deploy/_template/manifest.yaml`
- `deprecated/_template/manifest.yaml`
