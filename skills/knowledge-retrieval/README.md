# Knowledge Retrieval Skill

This package defines the future extraction boundary for WenYuanGe/Dify/public-safe
evidence retrieval.

The active implementation currently lives inside `skills/qintopia-tools` and the sidecar
context MCP. New retrieval behavior should be documented here before it is added to
shared profile tools.

## Capability

- retrieve public-safe answer basis for frontline Agents;
- keep raw Dify tools scoped to Wenyuange;
- return filtered answer basis, source metadata, risk flags, and safe reply guidance;
- avoid exposing raw internal chunks to Erhua, Xiaoqin, or public channels.

## Validation

```bash
pnpm skills:knowledge-retrieval:check
pnpm skills:qintopia-tools:check
pnpm check:light
```
