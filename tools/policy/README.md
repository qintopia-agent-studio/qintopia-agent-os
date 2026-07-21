# Policy Checks

`tools/policy/check-anti-drift.mjs` enforces migration guardrails that are too
project-specific for generic schema validation.

It checks that:

- inventory records keep WorkTool and Xiaoqin out of active migration paths
- server Huabaosi shadow work stays in review-pool until owner approval
- the sidecar deploy script remains marked as a legacy snapshot unless a reviewed deploy
  package converts or removes it
- Postgres migrations have matching data-design notes
- active package registries do not point at deprecated or review-pool sources

Run:

```bash
pnpm policy:check
```
