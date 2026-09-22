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

The command also runs nine isolated checks for the frozen `202609230006` migration. Its
exact applied SHA-384 is pinned; additive migration `202609230007` supplies the missing
design reference and schema registration. Changed or missing historical SQL, missing
repair/design files, and unrelated undocumented migrations fail the check. See the
[design note](../../runtime/postgres/docs/data-design/2026-09-23-ontology-audience.md)
before maintaining this repair; never rewrite an applied SQLx checksum.
