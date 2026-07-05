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
- `Collab MCP`: Huabaosi, Silaoshi, and Xiaoman now run the release-managed
  `qintopia-collab-mcp` command under `qintopia-agent-os-releases/current`. M10-B is
  complete.
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
  - MCP: release-managed `qintopia-collab`
  - M10 action: migrate shared `qintopia-tools`; keep external-send controls closed.
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
  - plugins: `qintopia-base-read`
  - MCP: release-managed `qintopia-collab`
  - M10 action: review `qintopia-base-read` separately.
- Silaoshi
  - service: `hermes-gateway-silaoshi.service`
  - root: `/home/ubuntu/.hermes/profiles/silaoshi`
  - plugins: none in observed plugin directory
  - MCP: release-managed `qintopia-collab`
  - M10 action: script/workflow migration remains separate.
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
  - target: `mcp/qintopia-collab`
  - risk: medium
  - status: M10-B complete; production command path is release-managed
- Shared Qintopia tools
  - source: `/home/ubuntu/.hermes/profiles/*/plugins/qintopia-tools`
  - observed size: 716K-1.2M per active profile
  - consumers: Erhua, Xiaoman, Wenyuange
  - target: `skills/qintopia-tools`
  - risk: medium
  - status: M10-C complete; active profile variants are imported under
    `skills/qintopia-tools/variants/*` and production profile plugin directories now
    symlink to
    `qintopia-agent-os-releases/current/skills/qintopia-tools/variants/<profile>`
  - validation: Wenyuange, Xiaoman, and Erhua are active after restart; each profile
    plugin import smoke passed; release sidecar `check` passed; all nine Qintopia system
    services are active
  - backup:
    `/home/ubuntu/qintopia-agent-os-backups/m10c-qintopia-tools-20260705T142000Z`
  - next step: keep old profile-local plugin copies for M11 archive-ready evidence; do
    not clean them in M10
- QiWe platform plugin
  - source: `/home/ubuntu/.hermes/profiles/erhua/plugins/qiwe-platform`
  - observed size: 7.7M
  - consumers: Erhua
  - target: `skills/qiwe`
  - risk: high
  - status: M10-D complete; Erhua profile plugin directory now symlinks to
    `qintopia-agent-os-releases/current/skills/qiwe`
  - validation: Erhua gateway is active after restart; release sidecar `check` passed;
    all nine Qintopia system services are active; no running process references the old
    QiWe profile checkout, `qintopia-msg-sidecar`, or the diagnostic monorepo checkout
  - backup: `/home/ubuntu/qintopia-agent-os-backups/m10d-qiwe-platform-20260705T144000Z`
  - next step: keep old profile-local checkout for M11 archive-ready evidence; do not
    clean it in M10
- Huabaosi Base read plugin
  - source: `/home/ubuntu/.hermes/profiles/huabaosi/plugins/qintopia-base-read`
  - observed size: 64K
  - consumers: Huabaosi
  - target: `skills/feishu-base`
  - risk: medium
  - status: M10-E complete; Huabaosi profile plugin directory now symlinks to
    `qintopia-agent-os-releases/current/skills/feishu-base`
  - validation: package-local check, deploy bundle build, manifest membership,
    `pnpm check:light`, CI, server-side COS fetch verification, plugin import/tool
    registration smoke, required runtime env presence, release sidecar `check`, active
    Huabaosi service, all nine Qintopia system services, and five active Hermes profile
    services
  - backup:
    `/home/ubuntu/qintopia-agent-os-backups/m10e-qintopia-base-read-20260705T151129Z`
  - next step: keep old profile-local plugin backup and pre-change env for M11
    archive-ready evidence; do not clean them in M10
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
  - migration impact: M10-D complete; old checkout is backed up and retained for M11
    archive-ready evidence, not active runtime.
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
   - Status: complete.
   - Validation: Huabaosi, Silaoshi, and Xiaoman are active, config references point to
     release/current, and old-script Python process references are `0`.
2. M10-C: compare and package shared `qintopia-tools`.
   - Affected profiles: Erhua, Xiaoman, Wenyuange first.
   - Status: complete.
   - Current state: active variants are release-managed under
     `qintopia-agent-os-releases/current/skills/qintopia-tools/variants/*`; Wenyuange,
     Xiaoman, and Erhua profile plugin directories symlink to their profile-specific
     release/current variant.
   - Validation: per-profile import smoke passed, profile gateways are active, sidecar
     `check` passed, and all nine Qintopia system services are active; Xiaoqin remains
     excluded from active migration scope.
3. M10-D: reconcile and package Erhua `qiwe-platform`.
   - Affected profile: Erhua.
   - Status: complete.
   - Current state: Erhua `qiwe-platform` is release-managed under
     `qintopia-agent-os-releases/current/skills/qiwe`; the profile plugin directory is a
     symlink to that package.
   - Validation: QiWe plugin tests passed locally, deploy bundle was verified from COS,
     Erhua profile active check passed after restart, sidecar `check` passed, and no
     external-send allowlist/config was broadened.
4. M10-E: review Huabaosi `qintopia-base-read`.
   - Affected profile: Huabaosi.
   - Status: complete.
   - Current state: `skills/feishu-base` contains the sanitized `qintopia-base-read`
     plugin source, manifest, docs, tests, registry entry, and deploy-bundle packaging;
     Huabaosi profile plugin path resolves to
     `qintopia-agent-os-releases/current/skills/feishu-base`.
   - Validation: local package check, deploy bundle build, deploy bundle manifest
     membership, light repository gate, CI, COS artifact verification, required runtime
     env presence, import/tool registration smoke, sidecar check, and active service
     checks passed.
5. M10-F: profile template/symlink planning for reviewed `config.yaml` and `SOUL.md`.
   - Status: complete.
   - Current state: profile bundle direction is documented in
     `docs/operations/profile-bundles/m10f-profile-template-plan.md`.
   - Do not replace whole profile directories.
   - Keep `.env`, sessions, logs, cache, state DBs, auth, and runtime-generated memory
     under `.hermes`.
   - Validation: `pnpm agents:profile-bundles:check`, `pnpm agents:check`, and
     `pnpm check:light` passed.

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
