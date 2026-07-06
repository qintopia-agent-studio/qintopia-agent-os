# CI Tools

`tools/ci` owns repository checks that decide which Agent OS domains changed and which
validation commands are required.

CI helpers must:

- treat docs-only changes differently from runtime/artifact builds where safe;
- keep required checks explicit for skills, workflows, MCP, runtime, deploy, and agents;
- fail closed when production-adjacent files change;
- never require secrets for pull request validation.

## Validation

```bash
pnpm tools:ci:check
```
