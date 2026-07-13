# AgentOS 运营控制面部署与验收 Runbook

- Status: historical baseline (2026-06-30); current-state addendum below
- Scope: standalone `qintopia-message-sidecar` AgentOS operations control plane
- Date: 2026-06-30

## Current State Addendum (2026-07-13)

This document preserves the original standalone runbook as migration evidence. Do not
use its server checkout paths or commands for current deployment. Current code lives
under `runtime/sidecar`, and production deployment uses the release/current model.

- Production `v0.2.6` has active Xiaoman signal, promotion-starter, evidence, visual,
  send-request-starter, and group send-ready internal timers. The aggregate read-only
  preflight passed; see
  [`deploy/smoke/docs/xiaoman-production-preflight-record.md`](../../../deploy/smoke/docs/xiaoman-production-preflight-record.md).
- These timers only write internal AgentOS work items, artifacts, or audit records. They
  do not call external evidence, image, Feishu, QiWe, or publish adapters.
- Source-grounded Xiaoman evidence retrieval is implemented after `v0.2.6`: it reads
  only explicitly linked Postgres messages or a same-chat window of at most 72 hours,
  writes sanitized snippets and internal UUIDs, and fails closed without evidence. It is
  covered by disposable PostgreSQL integration but is not yet production-observed.
- The image-generation request boundary and guarded provider worker are newer than the
  deployed `v0.2.6` binary. Current deployment code renders an internal request-starter
  timer, but no provider-worker timer. The provider remains disabled and no production
  provider call, media upload, or `generated_image` artifact write has been observed.
- The blocker rows below describe the 2026-06-30 baseline. The passed Xiaoman preflight
  supersedes its evidence/visual, send-request, group send-ready, and
  aggregate-preflight observation rows; remaining external-adapter blockers still apply.

## 目标

本 runbook 用于把 AgentOS 从“单个 Agent 工具调用”推进到可验收的运营操作系统控制面。首条金路径仍是：

```text
小满创建活动宣发需求
  -> Postgres 来源证据 + 画报司视觉 brief
  -> 人工审核 brief
  -> 图片生成请求
  -> 受保护 provider + pending generated_image
  -> 人工审核图片
  -> 二花发送前最终确认
  -> 记录发送就绪但不真实外发
```

当前版本验证控制面、幂等、审计、审核和工作台镜像，不调用真实飞书任务 API、不调用画报司生产生成 adapter、不调用企微或二花真实群发 adapter。来自
`event_signal` 的小满证据任务会读取受限的 Postgres 消息事实并写入
`evidence_summary`；它不调用外部文渊阁、embedding 或知识库 adapter。

## 架构边界

- Hermes 是 Agent runtime。
- `qintopia_agent_os` Postgres schema 是运营事实源。
- 飞书任务看板是 human workbench，不是事实源。
- Hermes Kanban 不是新链路的 fallback。
- Agent 间协作必须通过 `capabilities` + `work_items`。
- 产物必须进入 `artifacts`，状态变化和拒绝必须进入 `work_item_events`。
- 人类工作台引用必须进入 `human_workbench_refs`。
- 外部发送、发布、修改业务事实都属于高风险动作，默认需要审核或最终确认。
- 所有创建、规划和 workflow start 都必须带 allowlisted `source_type` 和结构化
  `source_refs`；`daily_digest`、Hermes Kanban、任意 URL、聊天原文不能作为受信来源。

## 本地无凭证验收

在开发机或无生产凭据环境运行：

```bash
cargo fmt --check
cargo check
cargo test
scripts/operations-control-plane-smoke.sh
```

服务器上也可以让 smoke 复用已构建的 release binary：

```bash
QINTOPIA_SIDECAR_BIN=/home/ubuntu/qintopia-msg-sidecar/target/release/qintopia-message-sidecar \
  scripts/operations-control-plane-smoke.sh
```

验收通过应证明：

- 首批 capability 能离线列出。
- 非技术请求能被规则型 planner 映射到受控 capability，无法确认的群发请求会要求澄清。
- `visual_asset_request` dry-run 能创建工作项计划。
- `evidence_request` dry-run 能创建文渊阁证据工作项计划。
- collaboration worker fixture 能生成 pending-review 产物预览。
- evidence worker fixture 能生成 `review_status=not_required` 的内部证据摘要。
- artifact review dry-run 只记录审核，不发布、不发送。
- group message final confirmation dry-run 只把状态推进到 `queued`，不外发。
- group message send worker fixture 只记录 send-ready，不外发。
- evidence/visual worker 已有 systemd oneshot/timer 部署入口；timer 只把
  `evidence_request` / `visual_asset_request` 处理成内部 `evidence_summary` / pending
  `poster_brief`
  artifact，不调用真实文渊阁检索、画报司生产生成、飞书、企微或外部 adapter。
- group send-ready worker 已有 systemd oneshot/timer 部署入口；timer 只记录
  `send_executed=false` 的审计事件，不真实发送，且不会对已记录 send-ready 的 work
  item 重复追加审计；worker claim 会递增
  `attempts`，达到 3 次后不再 claim，避免无限重试。
- workflow sync 只持久化 parent 汇总和当前阻塞点，不执行 worker、不外发。
- workflow sync worker 已有 systemd oneshot/timer 部署入口；timer 只触发 parent summary
  sync，不触发任何外部 adapter。
- workbench event worker 已有 systemd
  oneshot/timer 部署入口；timer 只处理已记录的审核、最终确认、受控取消状态变更和负责人变更事件，不直接轮询飞书。
- workbench mirror fixture 只生成脱敏飞书任务描述，不泄漏 payload。
- 敏感 payload、raw prompt handoff、Hermes Kanban
  fallback、非白名单群发、取消无原因都会被拒绝。
- 未知来源、缺少来源引用、`event_signal` 缺少事件 id 也会被拒绝。

## 服务器 Postgres Apply Smoke

在服务器 checkout 部署并构建后运行。该脚本默认跳过，只有显式开启才会向 AgentOS 表写入测试行。

```bash
cd /home/ubuntu/qintopia-msg-sidecar
set -a
. /etc/qintopia/message-sidecar.env
set +a
export QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE=1
export QINTOPIA_OPERATIONS_ALLOWED_GROUP_ALIASES="${QINTOPIA_OPERATIONS_ALLOWED_GROUP_ALIASES:-community_activity_group}"
export QINTOPIA_OPERATIONS_ALLOWED_REVIEWER_IDS="${QINTOPIA_OPERATIONS_ALLOWED_REVIEWER_IDS:-operations-apply-smoke-reviewer,operations-apply-smoke-reviewer-2}"
export QINTOPIA_OPERATIONS_ALLOWED_CONFIRMER_IDS="${QINTOPIA_OPERATIONS_ALLOWED_CONFIRMER_IDS:-operations-apply-smoke-confirmer,operations-apply-smoke-reviewer}"
export QINTOPIA_OPERATIONS_ALLOWED_OWNER_IDS="${QINTOPIA_OPERATIONS_ALLOWED_OWNER_IDS:-operations-apply-smoke-owner}"
export QINTOPIA_OPERATIONS_ALLOWED_ATTACHMENT_HOSTS="${QINTOPIA_OPERATIONS_ALLOWED_ATTACHMENT_HOSTS:-example.com}"
target/release/qintopia-message-sidecar operations-readiness-check --profile apply_smoke --strict
target/release/qintopia-message-sidecar operations-readiness-check --profile production
scripts/operations-control-plane-apply-smoke.sh
```

`scripts/server-deploy.sh deploy` 和 `scripts/server-deploy.sh verify`
会自动运行无凭证 dry smoke，并会调用同一个 apply smoke 入口。由于 apply smoke 本身检查
`QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE=1`，默认行为仍然是安全跳过，不写数据库。

`operations-readiness-check`
只读检查上线所需配置，不连接外部系统、不输出 secret。`--profile apply_smoke --strict`
用于确认写入型 apply smoke 的最小前置条件；`--profile production`
用于列出真实 adapter 启用前还缺哪些 group/reviewer/confirmer/owner/attachment-host
allowlist。真实生产 adapter 启用前应让 `--profile production --strict` 通过。

该 smoke 会执行：

1. 运行 migration。
2. 从 Postgres 读取 capability registry。
3. 通过 `operations-request-submit` 从非技术请求创建一个受控
   `visual_asset_request`，并验证重复提交幂等。
4. 通过 `operations-workflow-start` 启动 `activity_promotion` workflow，创建 parent
   `activity_promotion_request`、初始 child `evidence_request` 和初始 child
   `visual_asset_request`，并验证重复启动幂等。
5. 对 `event_signal` 小满路径创建的 `evidence_request` 定向运行 evidence
   worker，确认按内部消息 UUID 精确读取 Postgres、写入清洗后的 `not_required`
   `evidence_summary`，且不泄露平台 message id、chat id、sender
   id、电话或 URL；对不带 event-signal 来源的旧 manual/apply-smoke 合同保留现有内部占位行为。
6. 对 workflow-start 创建的 `visual_asset_request` 定向运行 collaboration
   worker，写入 pending `poster_brief` artifact。
7. 记录 artifact approved 审核，确认不会触发发布或发送事件，并确认 visual work
   item 推进到 `completed`。
8. 运行 `run-xiaoman-activity-image-generation-starter-worker`，只从 approved
   `poster_brief` 创建一个幂等 `image_generation_request`，不调用 provider。
9. 确认默认禁用的 image worker 不创建 artifact；apply smoke 仅在 disposable
   database 中创建 pending `generated_image` fixture 并通过正式审核命令批准它。
10. 运行 `run-xiaoman-activity-send-request-starter-worker`，只从 completed image
    request 下的 approved `generated_image` 创建一个 `awaiting_publish`
    `group_message_request`；approved `poster_brief` 本身不能解锁群发。
11. 记录人工最终确认，把群发请求推进到 `queued`。
12. 定向运行 group-message send worker，只记录 `group_message_send_ready_recorded` 且
    `send_executed=false`。
13. 验证 unapproved artifact 不能创建群发请求，并写入 `denied_by_policy`。
14. 验证已配置审核人/最终确认人 allowlist 时，非授权 reviewer/confirmer 会被拒绝并写入
    `denied_by_policy`。
15. 读取 parent status tree，确认 evidence、visual、image-generation、group-message
    child 都可追溯，且当前阻塞点可见。
16. 运行 `operations-workflow-sync`，确认 parent metadata 写入
    `workflow_summary`，并追加 `workflow_status_synced` 审计事件。
17. 定向运行 `run-workflow-sync-worker --once`，确认 worker 路径也能写入同样的 parent
    summary，且不执行 child worker 或外部 adapter。
18. 定向运行 workbench mirror，只写
    `human_workbench_refs(provider=feishu_task_dry_run)`，不调用飞书。
19. 记录一条 human workbench event，确认它只追加审计、不修改 work item 状态。
20. 用相同 `external_event_id` 重放该事件，确认不会重复追加审计。
21. 记录并处理一条审核请求类 workbench event，确认它通过
    `operations-artifact-review-decision` 推进 artifact 审核状态。
22. 记录并处理一条最终确认类 workbench event，确认它通过
    `operations-group-message-confirm` 把群发 work item 推进到 `queued`，但仍不发送。
23. 记录并处理一条受控状态变更类 workbench event，确认它只能把非终态 work item 取消为
    `cancelled`，并写入 `workbench_status_change_recorded`；尝试从任务看板直接标记
    `completed` 必须被拒绝，不能写入 processed 事件。
24. 记录并处理一条负责人变更类 workbench event，确认它从 `metadata.new_human_owner` 更新
    `work_items.human_owner` 并写入 `workbench_owner_change_recorded`。
25. 记录并处理一条附件类 workbench event，确认它从 `metadata.attachment_*` 创建
    `workbench_attachment` pending artifact，且不会发送或发布。
26. 重复处理同一 event，确认 `human_workbench_event_processed` 幂等。
27. 运行一次 workbench event
    worker 的空队列 dry-run，确认 comment-only 事件不会被误处理。

## Apply Smoke 后只读核查

脚本成功后，保留测试审计行是预期行为，不默认删除。可用以下只读 SQL 抽查：

```bash
psql "$QINTOPIA_SIDECAR_DATABASE_URL" -X -q -c "
SELECT capability_key, provider_agent, risk_level, review_policy, enabled
FROM qintopia_agent_os.capabilities
ORDER BY capability_key;
"

psql "$QINTOPIA_SIDECAR_DATABASE_URL" -X -q -c "
SELECT work_item_type, status, capability_key, count(*)
FROM qintopia_agent_os.work_items
WHERE source_type = 'apply_smoke'
GROUP BY work_item_type, status, capability_key
ORDER BY work_item_type, status, capability_key;
"

psql "$QINTOPIA_SIDECAR_DATABASE_URL" -X -q -c "
SELECT event_type, count(*)
FROM qintopia_agent_os.work_item_events
WHERE created_at > now() - interval '1 day'
  AND event_type IN (
    'created',
    'artifact_created',
    'evidence_artifact_created',
    'review_decision_recorded',
    'group_message_final_confirmation_recorded',
    'group_message_send_ready_recorded',
    'workflow_status_synced',
    'mirror_dry_run_recorded',
    'denied_by_policy'
  )
GROUP BY event_type
ORDER BY event_type;
"
```

## 预期不可发生事项

验收期间以下事件不应出现：

- 调用真实飞书任务 API 创建或修改任务。
- 调用真实 QiWe/企微发送群消息。
- 调用真实画报司生产 adapter 生成外部可用海报。
- `group_message_request` 在最终确认前进入 `queued`。
- 审核通过后自动发布或自动发送。
- workbench 描述里出现 token、Base table id、Base app token、内部 prompt、raw private
  chat、member dossier 或完整 payload。
- 任何新链路写入或依赖 Hermes Kanban。

## 当前阻塞项

| 阻塞项                                                | 影响                                                                    | 不阻塞的已完成部分                                                                                                                                                                                                                                                                                   |
| ----------------------------------------------------- | ----------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 真实飞书任务 API 身份、scope、任务清单/分组策略未确认 | 不能创建真实 `AgentOS · 运营协作工作台` 任务                            | 已有 `feishu_task_dry_run` mirror payload、幂等 ref 和脱敏字段                                                                                                                                                                                                                                       |
| 画报司 provider/storage/staging 尚未获 owner 批准     | 不能在 staging 或生产生成真实海报图片                                   | guarded adapter 已实现 PNG、大小、hash、allowlisted media URI、same-byte readback、claim、pending-review 和最多三次的可恢复 provider 延迟重试边界；默认禁用且没有 provider timer                                                                                                                     |
| 外部文渊阁/知识库检索 adapter 未接入                  | 不能自动拉取外部知识库证据                                              | 小满 `event_signal` 证据已可按受限范围读取 Postgres 消息事实，写入清洗后的 `evidence_summary` 和 `evidence_artifact_created` 审计；无来源时失败关闭                                                                                                                                                  |
| 二花/企微生产发送 adapter 未接入                      | 不能真实群发                                                            | 已有 approved artifact 校验、白名单群校验、最终确认和 send-ready 审计                                                                                                                                                                                                                                |
| source-grounded evidence 尚未线上部署验收             | 生产证据 timer 尚未观察新 Postgres 取证行为                             | disposable PostgreSQL integration 已覆盖精确消息 UUID、清洗、幂等和无外部调用；`qintopia-agentos-operations-evidence-worker.timer` 仍只写 AgentOS artifact/audit，不调用外部文渊阁、embedding、飞书、企微或其他 adapter                                                                              |
| image/send request starter 尚未线上部署验收           | 生产尚未观察 approved image 到 awaiting-publish 请求的内部衔接          | image starter 只从 approved `poster_brief` 创建 request；send starter 只从 approved `generated_image` 创建 awaiting-publish work item；二者都不确认、不排队、不发送、不调用飞书/企微/外部 adapter                                                                                                    |
| group send-ready timer 尚未线上部署验收               | 已最终确认的群发请求线上是否会自动记录 send-ready 还未确认              | 本地 `scripts/server-deploy.sh prepare/deploy` 已安装并启用 `qintopia-agentos-operations-group-send-ready.timer`，timer 只运行 `run-group-message-send-worker --once --apply`，且 `send_executed=false`；apply smoke 覆盖重复运行不重复写 send-ready、claim 后递增 `attempts`、达到 3 次后不再 claim |
| 飞书任务评论/状态回写 sync worker 未接入              | 人类在飞书里的修改不能自动写回 AgentOS                                  | 已有 Postgres 事实源、人工 CLI 审核/确认路径                                                                                                                                                                                                                                                         |
| 飞书任务真实事件入口未接真实 API                      | 暂时不能从真实飞书评论/状态自动触发 AgentOS 审计                        | 已有 `operations-workbench-event-record`，可校验并记录 human workbench event，且不会直接改事实状态                                                                                                                                                                                                   |
| 飞书任务处理器 timer 尚未线上部署验收                 | 已记录的审核/最终确认事件线上是否会自动处理还未确认                     | 本地 `scripts/server-deploy.sh prepare/deploy` 已安装并启用 `qintopia-agentos-operations-workbench-event.timer`，timer 只运行 `run-workbench-event-worker --once --apply`                                                                                                                            |
| workflow summary timer 尚未线上部署验收               | 线上 parent workflow summary 是否自动周期刷新还未确认                   | 本地 `scripts/server-deploy.sh prepare/deploy` 已安装并启用 `qintopia-agentos-operations-workflow-sync.timer`，timer 只运行 `run-workflow-sync-worker --once --apply`                                                                                                                                |
| Xiaoman aggregate preflight 尚未线上运行              | 还没有一条命令串起小满 runtime timer 和 downstream timer 的生产只读验收 | 已有 `xiaoman-activity-production-preflight-smoke.sh` 聚合 observation，只读运行 signal、promotion starter、evidence/visual downstream、send request starter 观察脚本；不部署、不写飞书、不发企微、不外发                                                                                            |
| 通用 DAG 执行器未实现                                 | 当前不能用一个通用调度器自动执行任意层级和任意能力                      | 状态查询会递归回溯顶层 activity parent，并返回所有后代及其直接 parent ref；workflow sync 会基于完整后代集合汇总状态和阻塞点，但只写 AgentOS summary，不执行 worker、不外发                                                                                                                           |

## 推进到生产可用的下一批验收

1. 飞书任务真实 mirror：
   - 确认使用哪个现有飞书应用或内部 sync app。
   - 确认 tasklist/section 创建权限。
   - 将 `feishu_task_dry_run` 升级为真实 provider 前，保留同样的脱敏字段 allowlist。
   - 验收：重复 mirror 不重复创建任务，任务描述无敏感字段。

2. 画报司 staging adapter：
   - owner 明确 provider
     account/budget/region、隔离媒体存储、素材授权、审核人和 rollback owner。
   - 先运行脱敏 preflight，再对一个指定 image request 运行一次受保护 staging smoke。
   - 产物必须保持 pending，人工审核通过/拒绝都进入审计；不写飞书、不发企微、不发布。

3. 二花发送 adapter：
   - 只处理 `queued` 的 `erhua.send_group_message`。
   - 只发送 approved artifact。
   - 只发送到 allowlisted group。
   - 有重试上限，失败写 `work_item_events`。
   - 验收：dry-run、测试群、生产群分阶段打开；默认仍需要人工最终确认。

4. 飞书回写 sync：
   - 评论、审核、负责人、状态变更都必须校验后写回 AgentOS。
   - 飞书不能成为事实源。
   - sync worker 应先调用 `operations-workbench-event-record` 记录 human workbench
     event。
   - 审核动作和发送最终确认可以由 `run-workbench-event-worker --once --event-id ...` 或
     `operations-workbench-event-process --event-id ...` 处理；它们会再委托到
     `operations-artifact-review-decision` 或 `operations-group-message-confirm`。
   - 状态变更 v1 只允许带原因的取消：非终态 work item 可从飞书任务看板请求变为
     `cancelled`，但不能直接变为 `completed`、`queued`、`processing` 或
     `awaiting_publish`。
   - 负责人变更 v1 只接受 `metadata.new_human_owner`，写回
     `work_items.human_owner`，并保留原负责人和新负责人审计；生产可用
     `QINTOPIA_OPERATIONS_ALLOWED_OWNER_IDS` 限制可被分配的负责人。
   - 附件补充 v1 只接受
     `metadata.attachment_title`、`metadata.attachment_summary`、`metadata.attachment_uri`
     和可选 `metadata.attachment_text`；附件会进入
     `artifacts(review_status=pending)`，不会直接外发；生产可用
     `QINTOPIA_OPERATIONS_ALLOWED_ATTACHMENT_HOSTS` 限制附件 HTTPS host。
   - 普通评论保持 audit-only，不直接改 AgentOS 事实。
   - 验收：非法状态跃迁、敏感评论、非授权审核人会被拒绝并审计。

## 回滚边界

当前控制面 migration 是 additive。若 smoke 失败：

- 不回滚 Hermes runtime。
- 不删除审计行，先记录失败 work item 和 event。
- 停止真实 adapter rollout，只保留 dry-run/CLI 路径。
- 如需禁用新创建入口，在 Agent
  profile 或 wrapper 层关闭对应 capability 调用，不删除表结构。
