# Deploy Template Secret Remediation

Date: 2026-07-13

## Observed Evidence

A historical `server-deploy.sh` environment template contained non-placeholder Feishu
Base runtime identifiers. The values originated in the 2026-07-03 source-adoption commit
and were not introduced by the Aliang image-adapter PRs.

## Risk

Repository-visible runtime credentials or Base identifiers violate the AgentOS secret
boundary and can be copied into newly created server environment files. Removing a value
from the current tree does not invalidate the historical value or update an existing
server environment file.

## Resolution

- Replace the deployment-template values with explicit placeholders.
- Extend the repository secret checker to reject non-placeholder Feishu/Lark Base token
  and table-id assignments in tracked text files.
- Update the deployment runbook to require provider-side rotation and server-local
  replacement after an exposure. The server is not edited by this remediation.

## Validation

- `pnpm secrets:check` rejects non-placeholder protected runtime assignments.
- Shell syntax, repository checks, and CI must pass before merging this remediation.

## Owner Action

1. Revoke or rotate the historical Feishu credential in the provider.
2. Put replacement values only in `/etc/qintopia/message-sidecar.env` through the
   approved server change runbook.
3. Deploy the reviewed release and run the documented read-only preflight to confirm the
   runtime remains healthy without printing the new values.
