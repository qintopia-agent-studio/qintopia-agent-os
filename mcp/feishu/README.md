# MCP Adapter: Feishu

`mcp/feishu` is the package boundary for Feishu Base, Doc, approval, and workbench
adapter contracts used by Agent OS.

## Responsibility

- Define Feishu adapter interfaces, permission scopes, and configuration names.
- Keep Feishu as a human workbench and mirror, not the Agent OS system fact source.
- Require explicit write allowlists for Base, Doc, approval, or message actions.
- Keep all app secrets, tenant credentials, table ids, and live tokens outside git.

## Production Boundary

- This package currently contains adapter contracts only.
- Production writes require separate owner-reviewed runtime config and smoke evidence.
- Secrets must be supplied by deployment environment, not repository files.

## Validation

```bash
pnpm mcp:adapters:check
```
