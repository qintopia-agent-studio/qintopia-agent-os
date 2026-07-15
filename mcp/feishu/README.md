# MCP Adapter: Feishu

`mcp/feishu` is the package boundary for Feishu Base, Doc, approval, and workbench
adapter contracts used by Agent OS.

## Responsibility

- Define Feishu adapter interfaces, permission scopes, and configuration names.
- Keep Feishu as a human workbench and mirror, not the Agent OS system fact source.
- Require explicit write allowlists for Base, Doc, approval, or message actions.
- Keep all app secrets, tenant credentials, table ids, and live tokens outside git.

## Huabaosi Generated Image Mirror

The fixed `config/huabaosi-generated-image-v1.json` contract defines the separate
artifact-version table used to mirror Huabaosi `generated_image` records. The Rust
sidecar owns selection, immutable JPEG revalidation, idempotent Base record writes, and
Postgres audit. Hermes tools do not receive an arbitrary Base write primitive.

The mirror is one-way from AgentOS to Feishu. Feishu attachment tokens and record fields
are display/workbench state and never authorize artifact approval, QiWe sends, or
publication.

The future isolated app permission review must cover only the fixed Base and media
operations used by this adapter: record search, record create, record update, and cloud
document media upload. The app must also have explicit manage access to the target Base;
unrelated contact, messaging, Doc, approval, or broad drive permissions are not part of
this contract.

## Production Boundary

- This package currently contains adapter contracts only.
- Production writes require separate owner-reviewed runtime config and smoke evidence.
- The generated-image write adapter is compiled only by the non-default
  `huabaosi-feishu-mirror-adapter` feature and remains unscheduled.
- Secrets must be supplied by deployment environment, not repository files.

## Validation

```bash
pnpm mcp:adapters:check
```
