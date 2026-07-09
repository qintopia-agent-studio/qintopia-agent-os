# AgentOS 运营控制面部署与验收 Runbook

Status: draft v1  
Scope: `qintopia-message-sidecar` AgentOS operations control plane  
Date: 2026-06-30

## 目标

本 runbook 用于把 AgentOS 从“单个 Agent 工具调用”推进到可验收的运营操作系统控制面。首条金路径仍是：

```text
小满创建活动宣发需求
  -> 画报司生成视觉素材草稿
  -> 人工审核
  -> 二花发送前最终确认
  -> 记录发送就绪但不真实外发
```

当前版本只验证控制面、幂等、审计、审核和工作台镜像，不调用真实飞书任务 API、不调用画报司生产生成 adapter、不调用企微或二花真实群发 adapter。文渊阁证据链路当前也只验证
`evidence_request` 控制面和 `evidence_summary`
artifact 写入，不调用真实文渊阁或消息库检索 adapter。

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
5. 对 workflow-start 创建的 `evidence_request` 定向运行 evidence worker，写入
   `not_required` `evidence_summary` artifact，确认未外部检索，且 sibling
   `visual_asset_request` 仍保持 queued。
6. 对 workflow-start 创建的 `visual_asset_request` 定向运行 collaboration
   worker，写入 pending `poster_brief` artifact。
7. 记录 artifact approved 审核，确认不会触发发布或发送事件，并确认 visual work
   item 推进到 `completed`。
8. 在同一个 workflow parent 下，用 approved artifact 创建 child
   `group_message_request`，初始状态必须是 `awaiting_publish`。
9. 记录人工最终确认，把群发请求推进到 `queued`。
10. 定向运行 group-message send worker，只记录 `group_message_send_ready_recorded` 且
    `send_executed=false`。
11. 验证 unapproved artifact 不能创建群发请求，并写入 `denied_by_policy`。
12. 验证已配置审核人/最终确认人 allowlist 时，非授权 reviewer/confirmer 会被拒绝并写入
    `denied_by_policy`。
13. 读取 parent status tree，确认 evidence、visual、group-message
    child 都可追溯，且当前阻塞点可见。
14. 运行 `operations-workflow-sync`，确认 parent metadata 写入
    `workflow_summary`，并追加 `workflow_status_synced` 审计事件。
15. 定向运行 `run-workflow-sync-worker --once`，确认 worker 路径也能写入同样的 parent
    summary，且不执行 child worker 或外部 adapter。
16. 定向运行 workbench mirror，只写
    `human_workbench_refs(provider=feishu_task_dry_run)`，不调用飞书。
17. 记录一条 human workbench event，确认它只追加审计、不修改 work item 状态。
18. 用相同 `external_event_id` 重放该事件，确认不会重复追加审计。
19. 记录并处理一条审核请求类 workbench event，确认它通过
    `operations-artifact-review-decision` 推进 artifact 审核状态。
20. 记录并处理一条最终确认类 workbench event，确认它通过
    `operations-group-message-confirm` 把群发 work item 推进到 `queued`，但仍不发送。
21. 记录并处理一条受控状态变更类 workbench event，确认它只能把非终态 work item 取消为
    `cancelled`，并写入 `workbench_status_change_recorded`；尝试从任务看板直接标记
    `completed` 必须被拒绝，不能写入 processed 事件。
22. 记录并处理一条负责人变更类 workbench event，确认它从 `metadata.new_human_owner` 更新
    `work_items.human_owner` 并写入 `workbench_owner_change_recorded`。
23. 记录并处理一条附件类 workbench event，确认它从 `metadata.attachment_*` 创建
    `workbench_attachment` pending artifact，且不会发送或发布。
24. 重复处理同一 event，确认 `human_workbench_event_processed` 幂等。
25. 运行一次 workbench event
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

| 阻塞项                                                | 影响                                                       | 不阻塞的已完成部分                                                                                                                                                                                                                                                                                   |
| ----------------------------------------------------- | ---------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 真实飞书任务 API 身份、scope、任务清单/分组策略未确认 | 不能创建真实 `AgentOS · 运营协作工作台` 任务               | 已有 `feishu_task_dry_run` mirror payload、幂等 ref 和脱敏字段                                                                                                                                                                                                                                       |
| 画报司生产生成 adapter 未接入                         | 不能生成真实海报图片                                       | 已有 `poster_brief` artifact、审核状态和审计链                                                                                                                                                                                                                                                       |
| 文渊阁真实证据检索 adapter 未接入                     | 不能自动拉取真实消息库/知识库证据                          | 已有 `evidence_request`、`evidence_summary` artifact 和 `evidence_artifact_created` 审计                                                                                                                                                                                                             |
| 二花/企微生产发送 adapter 未接入                      | 不能真实群发                                               | 已有 approved artifact 校验、白名单群校验、最终确认和 send-ready 审计                                                                                                                                                                                                                                |
| evidence/visual artifact timers 尚未线上部署验收      | 线上是否会自动生成内部证据摘要和 poster brief 还未确认     | 本地 `scripts/server-deploy.sh prepare/deploy` 已安装并启用 `qintopia-agentos-operations-evidence-worker.timer` 和 `qintopia-agentos-operations-visual-worker.timer`；二者只写 AgentOS artifact/audit，不调用真实文渊阁检索、画报司生产生成、飞书、企微或外部 adapter                                |
| group send-ready timer 尚未线上部署验收               | 已最终确认的群发请求线上是否会自动记录 send-ready 还未确认 | 本地 `scripts/server-deploy.sh prepare/deploy` 已安装并启用 `qintopia-agentos-operations-group-send-ready.timer`，timer 只运行 `run-group-message-send-worker --once --apply`，且 `send_executed=false`；apply smoke 覆盖重复运行不重复写 send-ready、claim 后递增 `attempts`、达到 3 次后不再 claim |
| 飞书任务评论/状态回写 sync worker 未接入              | 人类在飞书里的修改不能自动写回 AgentOS                     | 已有 Postgres 事实源、人工 CLI 审核/确认路径                                                                                                                                                                                                                                                         |
| 飞书任务真实事件入口未接真实 API                      | 暂时不能从真实飞书评论/状态自动触发 AgentOS 审计           | 已有 `operations-workbench-event-record`，可校验并记录 human workbench event，且不会直接改事实状态                                                                                                                                                                                                   |
| 飞书任务处理器 timer 尚未线上部署验收                 | 已记录的审核/最终确认事件线上是否会自动处理还未确认        | 本地 `scripts/server-deploy.sh prepare/deploy` 已安装并启用 `qintopia-agentos-operations-workbench-event.timer`，timer 只运行 `run-workbench-event-worker --once --apply`                                                                                                                            |
| workflow summary timer 尚未线上部署验收               | 线上 parent workflow summary 是否自动周期刷新还未确认      | 本地 `scripts/server-deploy.sh prepare/deploy` 已安装并启用 `qintopia-agentos-operations-workflow-sync.timer`，timer 只运行 `run-workflow-sync-worker --once --apply`                                                                                                                                |
| 多层 DAG 编排器未实现                                 | 只能表达一层 parent/child 状态树                           | 已有 parent work item、child work item、当前阻塞点和审计追溯                                                                                                                                                                                                                                         |

## 推进到生产可用的下一批验收

1. 飞书任务真实 mirror：
   - 确认使用哪个现有飞书应用或内部 sync app。
   - 确认 tasklist/section 创建权限。
   - 将 `feishu_task_dry_run` 升级为真实 provider 前，保留同样的脱敏字段 allowlist。
   - 验收：重复 mirror 不重复创建任务，任务描述无敏感字段。

2. 画报司真实 adapter：
   - 从 `visual_asset_request` payload 构造受控 brief。
   - 不传 raw chat、member dossier、token、Base ids。
   - 产物仍先写 pending artifact，不外发。
   - 验收：人工审核通过/拒绝都能回写。

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
