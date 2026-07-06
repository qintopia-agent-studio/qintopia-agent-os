# WorkTool Decommission Plan

This plan covered WorkTool and the current Xiaoqin WorkTool runtime cleanup. It does not
approve server deletion by itself and does not decide whether Xiaoqin can return later
through a non-WorkTool integration.

M12-C archived the server runtime under:

```text
/home/ubuntu/qintopia-agent-os-backups/m12-worktool-xiaoqin-20260706T014342Z
```

## Current Evidence

Read-only checks on 2026-07-03 initially found:

- `/home/ubuntu/worktool-gateway` exists.
- `/home/ubuntu/.hermes/profiles/xiaoqin` exists as the current WorkTool-bound runtime
  profile.
- `/home/ubuntu/.hermes/profiles/xiaoqin/plugins/worktool-platform` exists.
- No matching `worktool`, `xiaoqin`, or `openclaw` systemd service or timer was found in
  the system or ubuntu user service list.

Follow-up read-only checks on 2026-07-03 found:

| Item                                      | State                                            | M7 disposition                             |
| ----------------------------------------- | ------------------------------------------------ | ------------------------------------------ |
| `/home/ubuntu/worktool-gateway`           | directory exists; no `.git` observed             | retain until final migration cleanup       |
| `worktool-gateway.service`                | ubuntu user unit loaded, disabled, inactive      | remove or archive during final migration   |
| `/home/ubuntu/.hermes/profiles/xiaoqin`   | profile directory exists with live runtime state | retain until final migration cleanup       |
| `hermes-gateway-xiaoqin-worktool.service` | ubuntu user unit loaded, disabled, inactive      | remove or archive during final migration   |
| `hermes-gateway-xiaoqin.service`          | not found                                        | no action                                  |
| `/opt/qiwe-openclaw-adapter`              | directory exists                                 | retain until final migration cleanup       |
| `qiwe-openclaw-adapter.service`           | system unit loaded, disabled, inactive           | remove or archive during final migration   |
| `openclaw-embedding-proxy.service`        | system unit loaded, disabled, inactive           | remove or archive during final migration   |
| `openclaw-gateway.service`                | root user unit loaded, enabled, inactive/dead    | owner decision before disabling or removal |
| ports `18557` and `8787`                  | no listener observed                             | verify again during final migration        |
| nginx current config                      | still references `127.0.0.1:18557`               | reconcile during final migration           |
| ubuntu crontab and process scan           | no WorkTool/Xiaoqin/OpenClaw hit observed        | verify again during final migration        |

Local checks on 2026-07-03 found:

- `../worktool` exists at `b95e746e0254894705bc63051937a3afbf4013c1` and is ahead of
  `origin/master` by 1 commit.
- `../worktool-hermes-plugin` exists at `04e95e1556cb820f5630a0f4781073cddf23c4f4`.

## Decommission Preconditions

- Owner approves WorkTool removal.
- Server changes are deferred until the final migration/cutover window.
- No active Agent package sources from WorkTool or the current WorkTool-bound Xiaoqin
  runtime.
- `pnpm policy:check` passes.
- Server read-only check confirms no service, timer, cron, process, Hermes subscription,
  or profile route depends on WorkTool.
- Any useful historical protocol note has been copied into this deprecated package as an
  audit note, not as active runtime code.

## Read-Only Server Recheck

Run before any cleanup:

```bash
for p in \
  /home/ubuntu/worktool-gateway \
  /home/ubuntu/.hermes/profiles/xiaoqin \
  /home/ubuntu/.hermes/profiles/xiaoqin/plugins/worktool-platform \
  /opt/qiwe-openclaw-adapter
do
  if [ -e "$p" ]; then
    printf 'exists %s\n' "$p"
  else
    printf 'missing %s\n' "$p"
  fi
done

systemctl list-units --all --type=service --no-pager | grep -Ei 'worktool|xiaoqin|openclaw' || true
systemctl list-timers --all --no-pager | grep -Ei 'worktool|xiaoqin|openclaw' || true
systemctl --user list-units --all --type=service --no-pager | grep -Ei 'worktool|xiaoqin|openclaw' || true
crontab -l 2>/dev/null | grep -Ei 'worktool|xiaoqin|openclaw' || true
```

Also inspect Hermes profile subscriptions before archiving the current WorkTool-bound
Xiaoqin runtime:

```bash
find /home/ubuntu/.hermes/profiles/xiaoqin -maxdepth 2 -type f | sort
```

Do not print or commit `.env` values.

## M7 Closure Decision

M7 classifies WorkTool, the current Xiaoqin WorkTool runtime, and OpenClaw as
deprecated/audit-only. It does not remove server files or units. Final server changes
should happen during the M9 migration window or another owner-approved cleanup window.

This classification applies only to the WorkTool-based runtime. A future Xiaoqin Agent
is allowed as a separate product/engineering decision if it uses a non-WorkTool channel
and gets a reviewed Agent package contract.

M12-C completed the WorkTool/Xiaoqin WorkTool runtime archive. Keep this plan as
historical evidence and use
`docs/operations/archive-readiness/m12-worktool-xiaoqin-decommission.md` for the final
archive record and rollback commands.

## Cleanup Sequence

After approval, prefer archive-first cleanup:

1. Stop any discovered service or timer.
2. Disable any retained legacy unit that is still enabled.
3. Move server directories and unit files to an owner-approved archive path with date
   suffix.
4. Reconcile nginx routes that still point at legacy ports.
5. Re-run service, timer, cron, process, port, and nginx checks.
6. Record the archive path and validation output in a follow-up migration note.
7. Remove archive only after the owner confirms no rollback or audit need remains.

## Rollback

If removal breaks a live path, restore the archived directory, restart the previous
service if one existed, and record the dependency that was missed. Do not rebuild
WorkTool as a future Agent OS channel. Do not restore the archived WorkTool-bound
Xiaoqin runtime as the future Xiaoqin implementation.
