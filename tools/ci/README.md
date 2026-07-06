# CI Tools

`tools/ci` owns repository checks that decide which Agent OS domains changed and which
validation commands are required.

CI helpers must:

- treat docs-only changes differently from runtime/artifact builds where safe;
- keep required checks explicit for skills, workflows, MCP, runtime, deploy, and agents;
- fail closed when production-adjacent files change;
- enforce Conventional Commits commit message types for local and CI validation;
- validate pull request bodies so agents cannot submit empty templates;
- never require secrets for pull request validation.

## Validation

```bash
pnpm tools:ci:check
pnpm commitlint:check
pnpm pr:check-body
```
