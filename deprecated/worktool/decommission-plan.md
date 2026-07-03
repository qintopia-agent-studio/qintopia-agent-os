# WorkTool Decommission Plan

This plan covers WorkTool and Xiaoqin WorkTool runtime cleanup. It does not approve
server deletion by itself.

## Current Evidence

Read-only checks on 2026-07-03 found:

- `/home/ubuntu/worktool-gateway` exists.
- `/home/ubuntu/.hermes/profiles/xiaoqin` exists.
- `/home/ubuntu/.hermes/profiles/xiaoqin/plugins/worktool-platform` exists.
- No matching `worktool`, `xiaoqin`, or `openclaw` systemd service or timer was found in
  the system or ubuntu user service list.

Local checks on 2026-07-03 found:

- `../worktool` exists at `b95e746e0254894705bc63051937a3afbf4013c1` and is ahead of
  `origin/master` by 1 commit.
- `../worktool-hermes-plugin` exists at `04e95e1556cb820f5630a0f4781073cddf23c4f4`.

## Decommission Preconditions

- Owner approves WorkTool removal.
- No active Agent package sources from WorkTool or Xiaoqin.
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

Also inspect Hermes profile subscriptions before removing Xiaoqin:

```bash
find /home/ubuntu/.hermes/profiles/xiaoqin -maxdepth 2 -type f | sort
```

Do not print or commit `.env` values.

## Cleanup Sequence

After approval, prefer archive-first cleanup:

1. Stop any discovered service or timer.
2. Move server directories to an owner-approved archive path with date suffix.
3. Re-run service, timer, cron, and process checks.
4. Record the archive path and validation output in a follow-up migration note.
5. Remove archive only after the owner confirms no rollback or audit need remains.

## Rollback

If removal breaks a live path, restore the archived directory, restart the previous
service if one existed, and record the dependency that was missed. Do not rebuild
WorkTool as a future Agent OS channel.
