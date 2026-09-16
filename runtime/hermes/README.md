# Hermes Runtime Templates

This package defines the reviewed distribution boundary for Hermes profile bundles.

Hermes remains the live Agent runtime under `/home/ubuntu/.hermes`. This package is not
a copy of live Hermes state. It defines which reviewed files may be distributed from the
monorepo release and which files must stay runtime-local.

## Bundle Inputs

Allowed git-managed profile bundle inputs:

- `SOUL.md` templates
- `config.yaml` templates
- skills and plugin declarations
- MCP command declarations
- cron or scheduled-job declarations
- non-secret channel directory templates

Cron declarations live in `cron/` (per-profile `*.job.json` templates plus the
`cron/reviewed-cron-jobs.json` allowlist registry) and cron wrapper script templates
live in `scripts/`. Hermes cron is the source of truth for recurring Agent tasks; the
repository keeps only sanitized declarations and drives version history through the
server-local snapshot sync. See `docs/operations/hermes-cron-source-of-truth.md`.

Runtime-local files that must not enter git:

- `.env`
- sessions, logs, cache, pairing, auth, and locks
- generated memory and state databases
- private chat logs and raw member profile data
- server-local config overrides
- live `cron/jobs.json` files and deployed `scripts/` (snapshot-synced server-side
  instead)

## Release Model

Whole-profile bundles should render into immutable release directories under
`/home/ubuntu/qintopia-agent-os-releases/<sha>` and become active through the stable
`current` symlink only after dry-run render checks, smoke checks, and owner review.

Do not replace a live Hermes profile `SOUL.md` or `config.yaml` directly from a feature
branch.

## Profile Registry

`profile-registry.yaml` is the single machine-readable inventory for the seven Hermes
profiles involved in core updates. It binds each profile id to its fixed user service,
reviewed WeCom enabled state, required credential binding names, and whether a separate
`qiwe-platform` integration must be preserved. The registry contains no credential
values, target ids, account ids, or message data.

Core readiness, WeCom readiness, staging parity, service switching, and rollback tools
must read this registry rather than carry independent profile arrays. For the current
baseline, WeCom stays enabled for `default`, `guanerye`, `huabaosi`, `silaoshi`, and
`xiaoman`; it stays disabled for `erhua` and `wenyuange`. Erhua's independent
`qiwe-platform` declaration remains preserved and is outside Hermes core promotion.

The registry is a release input. A change to profile membership, service mapping, WeCom
state, or QiWe preservation requires an owner-reviewed migration update and the same
staging parity and rollback evidence as a core change.

## Core Release Contracts

`core-release-contracts/` defines the strict HC-1 JSON contracts for a clean Hermes core
artifact manifest, build receipt, upstream-update receipt, and sanitized validation
summary. `tools/deploy/verify-hermes-core-artifact.mjs` validates those documents and
the complete immutable `core/` file inventory against caller-pinned public identities
and digests. The contracts contain no profile configuration values, credentials, message
data, prompts, or target identifiers.

These contracts are validation inputs only. HC-2 fixes the independent root at
`/var/lib/qintopia-hermes-core` and models the active
`(current, previous, rollback-reserve)` tuple as one immutable generation selected by
`lineage/active`. The HC-2 stager verifies and atomically installs a fixed-ingress
artifact into `incoming/<commit>` without changing pointers or services; the dry-run
planner then validates that candidate and the active lineage. The HC-2 repository
implementation also includes fixed-key ingress download, bounded streaming extraction,
two-clean-release bootstrap, immutable generation commit, and crash-recovery fault
injection. These have not downloaded a real artifact or run against production. Signed
request binding, the seven-service transaction and reverse rollback, replay, systemd
integration, and production cutover remain later work; the current server checkout,
local WeCom patches, all WeCom configuration and enabled states, and Erhua's separate
`qiwe-platform` stay unchanged.

Any lineage consumer must resolve `lineage/active` exactly once, retain that resolved
generation path, and read `current`, `previous`, and `rollback-reserve` from it. Reading
the three top-level compatibility links independently can mix roles across an atomic
generation switch and is not a valid release snapshot.

## Server Patch Review Pool

Server-local Hermes patches are extracted under `docs/operations/review-pool/hermes/`
only as source evidence. They are not release inputs and must not be applied to the
production checkout. Each stable behavior needs an owned implementation, focused
validation, and a separate production cutover PR.

The first extraction records the Huabaosi WeCom server patch at
`docs/operations/review-pool/hermes/2026-07-15-huabaosi-wecom-server-patch/`. Its
incident-specific filtering contract is owned by
`runtime/sidecar/src/huabaosi_wecom_policy.rs`; the raw Python patch remains
non-deployable because it mixes generic reliability changes, conflicting tests, and
cross-Agent copy.

## Erhua Weather Broadcast Asset

`skills/qintopia-weather/scripts/qintopia-erhua-weather-broadcast.py` is an allowed
release-owned script input. It renders the canonical forecast-first weather text to
stdout and performs no external delivery. The deploy bundle may carry it before a live
profile cutover.

Erhua's actual `cron/jobs.json` is not yet a reviewed bundle input because the
repository does not contain its schema or sanitized production structure. Do not invent
that declaration or repoint the live job until a read-only inventory records the job
shape and current script hashes. Activation and rollback belong in a separate reviewed
profile cutover.

## Erhua Model Overlay

`render_profile_overlay.py` applies the reviewed Erhua Livecool provider overlay to a
sanitized or runtime-local base config while preserving `model.default`. It also keeps
the current production Erhua WeCom state disabled by managing only
`channel.wecom.enabled=false`; existing runtime-local channel fields remain in place and
never enter reports. It rejects aliases, duplicate keys/providers, forbidden overlay
fields, and path aliasing. Reports contain changed field paths and file hashes, not
values. `migrate_erhua_livecool_env.py` creates or checks the server-local
`LIVECOOL_API_KEY` binding without printing credential material.
`verify_runtime_provider.py` runs inside the installed Hermes interpreter during both
dry-run and activation smoke. It requires Hermes's own provider resolver to return the
approved named provider and base URL.

`validate_hermes_python.py` binds that resolver to the fixed Hermes venv or an immutable
release-local interpreter. The venv base home must be unaliased except for uv's stable
CPython major/minor alias below the same runtime user's `.local/share/uv/python` root.
That alias must resolve in one absolute in-root hop to a matching patch version and
platform; arbitrary, chained, relative, or out-of-root aliases remain rejected.

The deploy runner is the only production caller. It supplies fixed profile paths; deploy
requests cannot supply paths. See
`docs/operations/profile-bundles/erhua-livecool-profile-overlay-runbook.md`.

## Initial Bundle

`agents/xiaoman/profile-bundle` is the first concrete observation-only bundle. It owns a
strict `SOUL.md` and `profile.yaml` renderer with fake fixtures, but it is not installed
or rendered by the deploy runner. Production identities remain in a server-local values
file. The bundle may be used only for read-only parity until a separate cutover PR adds
backup and rollback behavior.

Its one-time values migration command is an owner-triggered root operation, not a deploy
runner step. It may create only the fixed mode-`0600` values JSON after matching both
reviewed source hashes and complete rendered parity. Hermes does not read that file
until a future reviewed activation path exists.

## Validation

```bash
pnpm agents:profile-bundles:check
pnpm runtime:hermes:check
pnpm check:light
```

## 真实核心兼容验证

使用已安装 messaging、wecom extras 的目标 Hermes Python 执行：

```sh
<candidate-python> -B runtime/hermes/check_core_plugin_compatibility.py --core-dir <candidate-core>
```

该检查使用临时 home、禁止网络连接，并验证 QiWe 没有落入本地测试替身、QiWe 和官方 WeCom 能通过真实 Hermes 插件注册。它不发送消息，也不替代生产配置 parity 或消息回放。发布构建器仅接受 Linux
x86_64 和 Python
3.11–3.13 的 uv 可迁移 Python 分发包；artifact 携带完整标准库，不依赖构建机或服务器的系统 Python 路径。依赖安装启用哈希验证，并在最终 runtime 中实际执行隔离 CLI
smoke 后才生成通过回执。

## September 2026 production recovery

See the [recovery runbook](../../docs/operations/hermes-production-recovery.md) and
[indexed incident report](../../docs/reports/2026-09-16-hermes-production-recovery.md).
Core upgrade acceptance now requires matching console assets, fresh-process discovery
against the actual deploy payload, owner-triggered messaging, cron execution/delivery,
and Ubuntu snapshot readability. Active/readiness alone does not establish recovery.

The offline `check_cron_ack_publication.py` reports the known incomplete-ready-file race
as a blocker; it never runs a business job. `check_core_plugin_compatibility.py` accepts
`--plugin-dir` for the packaged QiWe tree and tests three independent process consumers.

## Agent operating contracts

Read the relevant topic when changing this capability. These documents retain the full
constraints behind the scoped AGENTS.md summaries.

- [Cron contract](docs/cron-agent-contract.md)
- [Hermes runtime contract](docs/runtime-agent-contract.md)
