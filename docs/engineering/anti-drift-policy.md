# Anti-Drift Policy

This policy prevents migration work from turning server experiments, deprecated paths,
or historical deployment scripts into approved Agent OS direction by accident.

## What The Check Enforces

Run:

```bash
pnpm policy:check
```

The check currently enforces:

- WorkTool and Xiaoqin inventory records cannot be marked as active `adopt` or
  `template` inputs.
- Huabaosi shadow work must stay in `review-pool` until owner approval.
- `deploy/sidecar/scripts/server-deploy.sh` must stay marked as a legacy snapshot until
  M9 cutover replaces or converts it.
- Sidecar monorepo cutover must have an explicit plan while the deploy script remains a
  legacy snapshot.
- Postgres migrations must reference matching data-design notes, except for the initial
  bootstrap migration that predates `schema_change_log`.
- Active packages cannot source from WorkTool, Xiaoqin, or Huabaosi shadow material
  without the correct disposition.
- Agent packages cannot include live Hermes runtime state such as `.env`, auth files,
  memories, sessions, caches, logs, state databases, request dumps, or secrets.

## Why This Exists

The monorepo is becoming the source of collaboration for humans and programming agents.
Without executable checks, future contributors can accidentally:

- copy server-side experiments into the approved roadmap
- revive WorkTool as an active channel
- treat a legacy deploy script as the current production deploy entrypoint
- add migrations without design notes
- bypass review-pool classification for high-risk runtime work
- commit live profile state into `agents/*` while trying to template a profile

The check is intentionally narrow. It catches direction changes that should require an
owner decision, not every possible style or quality issue.

## When To Update It

Update `tools/policy/check-anti-drift.mjs` when a new non-negotiable migration boundary
is discovered. Do not use it for broad lint rules that belong in Prettier, markdownlint,
registry schema validation, or package-level tests.

If an exception is legitimate, encode it explicitly with a comment or allowlist and
explain the reason in the relevant migration plan or package README.
