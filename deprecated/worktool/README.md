# Deprecated: WorkTool

WorkTool is not a future Qintopia Agent OS channel. This package exists only to keep
decommission evidence and audit notes.

## Sources

- Local source: `../worktool`
- Local branch observed on 2026-07-03: `master`
- Local reference observed on 2026-07-03: `b95e746e0254894705bc63051937a3afbf4013c1`
- Local state observed on 2026-07-03: ahead of `origin/master` by 1 commit
- Server source: `/home/ubuntu/worktool-gateway`
- Server state observed on 2026-07-06: M12-C archived `/home/ubuntu/worktool-gateway`
  and the disabled `worktool-gateway.service` user unit under
  `/home/ubuntu/qintopia-agent-os-backups/m12-worktool-xiaoqin-20260706T014342Z`.

## Decision

Do not build new Agent OS work on WorkTool. Keep only audit evidence and the private
server archive until the owner approves permanent deletion.

## Decommission Entry

Use [decommission-plan.md](decommission-plan.md) before deleting server directories or
local source checkouts.
