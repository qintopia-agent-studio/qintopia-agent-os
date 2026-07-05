# M10 Hermes Profile Runtime Inventory

Inventory date: 2026-07-05

Mode: read-only server inventory. This document records the post-M9-F current state for
Hermes profile, plugin, script, and cleanup planning. It does not copy `.env`, sessions,
logs, cache, auth files, generated memory, private chat logs, or raw runtime state into
git.

## Current Runtime Shape

- `Hermes runtime`: `/home/ubuntu/.hermes` remains the live Hermes runtime root. Keep it
  in place and do not replace it wholesale from CI.
- `Agent OS release runtime`: all nine sidecar/worker services run from
  `/home/ubuntu/qintopia-agent-os-releases/current`. M9-F runtime cutover is complete.
- `Context MCP`: Erhua and Wenyuange use the release wrapper under
  `qintopia-agent-os-releases/current`. M9-F MCP cutover is complete.
- `Collab MCP`: Huabaosi, Silaoshi, and Xiaoman still run
  `/home/ubuntu/.hermes/scripts/qintopia-collab-mcp`. This is the next M10 migration
  candidate.
- `Profile plugins`: active plugins are still directories under
  `/home/ubuntu/.hermes/profiles/*/plugins/*`. Migrate them as reviewed skill/profile
  bundles; do not copy a live profile root.
- `Deprecated profile`: Xiaoqin profile exists but its WorkTool gateway service is
  inactive/disabled. Keep it for M12 decommission, not active migration.
- `Deprecated OpenClaw`: OpenClaw and old embedding units are inactive/disabled, but
  unit files still reference legacy paths. Keep them for M12 decommission.
- `Release rollback`: `/home/ubuntu/qintopia-agent-os-releases/previous` is not yet an
  effective previous-release symlink. Establish it in the next real release window.

## Active Hermes Profiles

- Erhua
  - service: `hermes-gateway-erhua.service`
  - root: `/home/ubuntu/.hermes/profiles/erhua`
  - plugins: `qiwe-platform`, `qintopia-tools`
  - MCP: release-managed `qintopia-context`
  - M10 action: migrate `qintopia-tools` first, then `qiwe-platform` with dedicated QiWe
    validation.
- Xiaoman
  - service: `hermes-gateway-xiaoman.service`
  - root: `/home/ubuntu/.hermes/profiles/xiaoman`
  - plugins: `qintopia-tools`
  - MCP: `/home/ubuntu/.hermes/scripts/qintopia-collab-mcp`
  - M10 action: migrate collab MCP wrapper and shared `qintopia-tools`; keep
    external-send controls closed.
- Wenyuange
  - service: `hermes-gateway-wenyuange.service`
  - root: `/home/ubuntu/.hermes/profiles/wenyuange`
  - plugins: `qintopia-tools`
  - MCP: release-managed `qintopia-context`
  - M10 action: migrate shared `qintopia-tools`; preserve knowledge/disclosure
    boundaries.
- Huabaosi
  - service: `hermes-gateway-huabaosi.service`
  - root: `/home/ubuntu/.hermes/profiles/huabaosi`
  - plugins: `qintopia-tools`, `qintopia-base-read`
  - MCP: `/home/ubuntu/.hermes/scripts/qintopia-collab-mcp`
  - M10 action: migrate collab MCP wrapper, then review `qintopia-base-read` separately.
- Silaoshi
  - service: `hermes-gateway-silaoshi.service`
  - root: `/home/ubuntu/.hermes/profiles/silaoshi`
  - plugins: none in observed plugin directory
  - MCP: `/home/ubuntu/.hermes/scripts/qintopia-collab-mcp`
  - M10 action: migrate collab MCP wrapper; script/workflow migration remains separate.
- Guanerye
  - service: `hermes-gateway-guanerye.service`
  - root: `/home/ubuntu/.hermes/profiles/guanerye`
  - plugins: none observed
  - MCP: none observed in config
  - M10 action: no immediate plugin/MCP migration; profile template later.

## Deprecated Or Cleanup-Only Profiles

- `Xiaoqin`: profile and `worktool-platform` plugin exist. Do not migrate as active
  Agent scope; archive only in M12 after disabled-unit audit.
- `WorkTool gateway`: `worktool-gateway.service` is inactive/disabled. Decommission with
  WorkTool paths; do not fold it into active Agent OS packages.
- `OpenClaw`: `qiwe-openclaw-adapter`, `openclaw-embedding-proxy`, `oclak-ep`, and old
  embedding worker are inactive/disabled. Decommission with unit files, nginx routes,
  and root-user OpenClaw paths in M12.

## Plugin And Script Candidates

- Collab MCP wrapper
  - source: `/home/ubuntu/.hermes/scripts/qintopia-collab-mcp`
  - observed size: 24K
  - consumers: Huabaosi, Silaoshi, Xiaoman
  - target: `mcp/qintopia-collab` or `mcp/context-server` extension
  - risk: medium
  - next step: M10-B package and release-managed wrapper migration
- Shared Qintopia tools
  - source: `/home/ubuntu/.hermes/profiles/*/plugins/qintopia-tools`
  - observed size: 308K-1.2M per profile
  - consumers: Erhua, Xiaoman, Wenyuange, Xiaoqin
  - target: `skills/qintopia-tools`
  - risk: medium
  - next step: compare profile-local copies; extract common package and per-profile diff
- QiWe platform plugin
  - source: `/home/ubuntu/.hermes/profiles/erhua/plugins/qiwe-platform`
  - observed size: 7.7M
  - consumers: Erhua
  - target: `skills/qiwe`
  - risk: high
  - next step: M10-C reconcile server dirty git state before symlink/release migration
- Huabaosi Base read plugin
  - source: `/home/ubuntu/.hermes/profiles/huabaosi/plugins/qintopia-base-read`
  - observed size: 64K
  - consumers: Huabaosi
  - target: `skills/feishu-base` or `mcp/base-read`
  - risk: medium
  - next step: review after collab MCP migration
- Silaoshi temporary script
  - source: `/home/ubuntu/.hermes/profiles/silaoshi/tmp_query_brief.py`
  - observed size: small
  - consumers: Silaoshi
  - target: workflow package candidate
  - risk: medium
  - next step: classify as workflow/script; do not migrate with shared plugins
- Xiaoqin WorkTool platform
  - source: `/home/ubuntu/.hermes/profiles/xiaoqin/plugins/worktool-platform`
  - observed size: 340K
  - consumers: inactive Xiaoqin WorkTool profile
  - target: `deprecated/worktool-hermes-plugin`
  - risk: medium
  - next step: M12 archive/decommission only

## Git State Observations

- `/home/ubuntu/.hermes/profiles/erhua/plugins/qiwe-platform`
  - state: `main...origin/main`, untracked backup
    `adapter.py.bak.home-group-send-20260607-1050`, HEAD `6f69794`
  - migration impact: reconcile before production migration; do not overwrite with local
    package blindly.
- `/home/ubuntu/.hermes/hermes-agent`
  - state: dirty, ahead/behind upstream, several modified Hermes core and backup files
  - migration impact: review-pool only; Hermes core is not part of the Agent OS release
    pipeline.
- `/home/ubuntu/qintopia-msg-sidecar`
  - state: old branch `codex/huabaosi-localization-shadow`, HEAD `b16c247`
  - migration impact: no active runtime refs after M9-F; archive candidate after M11
    readiness.

## Migration Order

1. M10-B: package and release-manage `qintopia-collab-mcp`.
   - Affected profiles: Huabaosi, Silaoshi, Xiaoman.
   - Validation: restart one profile at a time; confirm profile active, MCP child
     process path, and no old script process remains.
2. M10-C: compare and package shared `qintopia-tools`.
   - Affected profiles: Erhua, Xiaoman, Wenyuange first.
   - Validation: per-profile plugin load check and targeted Hermes smoke; do not include
     Xiaoqin in active scope.
3. M10-D: reconcile and package Erhua `qiwe-platform`.
   - Affected profile: Erhua.
   - Validation: QiWe plugin tests, local health endpoint if still applicable, Hermes
     profile active check, no external-send behavior change.
4. M10-E: review Huabaosi `qintopia-base-read`.
   - Affected profile: Huabaosi.
   - Validation: read-only Base access smoke; no approval of unreviewed Huabaosi
     shadow/Rust work.
5. M10-F: profile template/symlink planning for reviewed `config.yaml` and `SOUL.md`.
   - Do not replace whole profile directories.
   - Keep `.env`, sessions, logs, cache, state DBs, auth, and runtime-generated memory
     under `.hermes`.

## Cleanup Gate Before M12

M11 should mark directories as `archive-ready` only after all checks pass:

- no active process references the path
- no active or enabled systemd unit/timer references the path, except disabled units
  explicitly included in the same decommission batch
- no Hermes profile config references the path
- no nginx route or cron job references the path
- rollback no longer needs the path
- owner approves the archive batch

Do not delete anything in M10 or M11. M12 starts only after this inventory and the
per-directory readiness evidence are updated in git.
