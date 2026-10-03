# Runtime: systemd

## C1 Foundation Broker Lifecycle (2026-10-01)

The specifically approved C1 adds a dedicated non-login `qintopia-foundation-broker`
service, sharing only the verified Erhua runner's primary group (`ubuntu`) and retaining
separate root0600 environment credentials. Installation remains disabled/inactive. The
fixed immutable helper owns prepare/activate/quiesce/verify-closed, reusing the existing
FD8 then FD9 maintenance ordering and request-bound hold. Closing inherits FD9; it never
reacquires the caller's lock. Installer replacement and rollback's first pointer change
verify closure without stopping again. The caller already quiesces.

A single stop submission has a 35-second monotonic budget. Snapshot the original
InvocationID, PID start identity and cgroup before stop; success requires a definite
same-invocation exit, no old/current cgroup tasks, no broker process or residual socket.
A deferred 30-second runtime drain keeps waiting. A 35-second failure returns75 to the
existing hold stage, terminates only the waiting systemctl client, and never SIGKILLs
business work, resets failure, deletes residual files, clears hold or replays work.
Natural late completion cannot resume deployment. Preparation and explicit activation
are separate; activation requires current immutable identity, no hold/claimed request,
verified account/group/private env, production enable and the existing owner gate.

This contract depends on the real #733 draining runtime and final shared release; C1
does not upgrade Hermes core or grant production deployment/certificate/activation. See
`docs/reports/2026-10-01-steward-install-preparation.md` for approval and evidence.

`runtime/systemd` owns the monorepo-native systemd template boundary for Agent OS
services and workers.

## Responsibility

- Keep service unit templates and dry-run render checks versioned.
- Render units against immutable release directories and the `current` symlink model.
- Avoid server-local build paths or legacy standalone checkout references.
- Keep environment files and secrets out of git.

## Production Boundary

- Rendering templates is safe and non-mutating.
- Installing or restarting systemd units requires an owner-approved runbook and smoke
  evidence.

## Validation

```bash
pnpm runtime:systemd:check
pnpm deploy:systemd:check
```
