# Deprecated: WorkTool

WorkTool is not a future Qintopia Agent OS channel. This package exists only to keep
decommission evidence and audit notes.

## Sources

- Local source: `../worktool`
- Local branch observed on 2026-07-03: `master`
- Local reference observed on 2026-07-03: `b95e746e0254894705bc63051937a3afbf4013c1`
- Local state observed on 2026-07-03: ahead of `origin/master` by 1 commit
- Server source: `/home/ubuntu/worktool-gateway`
- Server state observed on 2026-07-03: directory exists, no matching systemd service or
  timer found by read-only scan

## Decision

Do not build new Agent OS work on WorkTool. Keep only audit evidence needed to remove
runtime references safely.

## Decommission Entry

Use [decommission-plan.md](decommission-plan.md) before deleting server directories or
local source checkouts.
