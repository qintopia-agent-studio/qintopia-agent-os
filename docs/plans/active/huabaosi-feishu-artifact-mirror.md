# 阿靓图片产物飞书镜像计划

Status: implemented and locally validated on a dedicated PR; production writeback
remains disabled and unscheduled

Scope: 将 AgentOS `generated_image`
单向镜像到“画报司 | 设计产出库”的图片产物版本表，不改变图片生成、人工审核、企微发送或 Release 发布边界

## Goal

阿靓生成的最终 JPEG 已由独立媒体存储和 Postgres `artifacts`
保存，但飞书设计产出库目前只有人工维护的任务字段，没有 AgentOS 写回。目标路径是：

```text
generated_image artifact
  -> revalidate immutable JPEG identity
  -> upload the exact JPEG bytes to Feishu Base
  -> idempotently create or update one artifact-version record
  -> record the Feishu reference and sanitized sync audit in Postgres
```

Postgres/AgentOS 继续是事实源。飞书只提供图片预览、人工协作和业务检索；删除、修改或重建飞书记录不得改变 artifact、审核或发送事实。

## Table Contract

写入目标是同一 Base 内独立的“图片产物版本表”，字段定义由
`mcp/feishu/config/huabaosi-generated-image-v1.json` 固定。每个
`generated_image_artifact_id`
对应一条记录，同一图片的审核状态变化更新该记录，不覆盖其他生成版本。

“海报生产任务表”的 `成品图`
保持业务汇总字段。本 PR 不猜测旧 Hermes/Kanban 记录与 AgentOS work
item 的关系，也不自动更新该字段。后续只有在任务表增加稳定的 AgentOS workflow
ID 并完成历史映射后，才可把已审核版本提升为任务当前成品。

## Eligibility

镜像 worker 只能处理同时满足以下条件的 artifact：

- `artifact_type=generated_image` 且 `created_by_agent=huabaosi`；
- 对应 work item 是 `huabaosi.generate_image_asset` / `image_generation_request`；
- 存在匹配的 `generated_image_created` 审计；
- URI 是无 userinfo、query、fragment 的 allowlisted HTTPS JPEG；
- SHA-256、MD5、MIME、尺寸、字节数、源 PNG hash 和固定 transform metadata 完整；
- 从媒体 URI 读取的字节与 Postgres 中的最终 JPEG identity 完全一致。

`pending`、`approved`、`rejected` 和 `changes_requested`
都可以镜像，方便人工查看版本历史。镜像不得批准 artifact，也不得让飞书字段替代 AgentOS
review decision。

## Idempotency And Failure

- worker 先按固定字段 `AgentOS产物ID`
  搜索飞书记录；零条时创建，一条时更新，多条时失败关闭。
- 网络调用成功但本地事务提交前进程退出时，下次运行通过同一搜索恢复，不重复创建记录。
- 每次需要同步时重新读取并校验稳定媒体 URI 的精确 JPEG，重新上传后覆盖
  `最终JPEG` 附件。这样飞书中的附件即使被人工替换，也会恢复为 AgentOS
  已记录的不可变字节；`last_synced_at >= artifact.updated_at` 时不发起重复同步。
- 外部失败只追加脱敏的 `generated_image_feishu_mirror_failed`
  审计；不得改变 artifact 或 work item 状态，不得自动审核、发布或发送。
- 成功后使用 `human_workbench_refs` 记录 provider、artifact/work
  item 关联和最后同步时间；metadata 不保存 Base token、table id、file
  token、媒体 URI、凭据或原始响应。

## Runtime Gates

真实写入必须同时满足：

1. 非默认 Cargo feature `huabaosi-feishu-mirror-adapter` 已编译；
2. `QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED=1`；
3. exact owner phrase
   `QINTOPIA_HUABAOSI_FEISHU_MIRROR_APPROVAL=approved-huabaosi-feishu-artifact-mirror`；
4. Base token 和目标 table id 分别在显式 allowlist 中；
5. AgentOS database URL 的 SHA-256 与 owner-reviewed 值一致；
6. profile env path、媒体 host allowlist 和固定 schema version 校验通过。

默认和当前生产 artifact 不编译该 feature。`--dry-run`
可读取 Postgres 并生成脱敏 preview，但不读取媒体、不请求飞书、不写数据库。`--fixture-mode`
只验证本地映射。

## Production Boundary

本 PR 不增加 systemd service/timer，不修改 deploy
runner，不启用生产 feature，不读取生产凭据，不写真实飞书，不调用 QiWe，不发送或发布。生产启用需要单独 owner-reviewed
PR，包含远端 schema preflight、隔离 Base smoke、`base:record:retrieve` /
`base:record:create` / `base:record:update` / `docs:document.media:upload`
最小权限清单、目标 Base 明确授权、首条记录证据和立即 rollback。

Rollback 是禁用镜像开关并停止未来独立 timer；既有 AgentOS
artifact 和审计保持不变，飞书镜像记录可保留只读或由 owner 单独归档。

## Validation

```bash
cargo fmt --manifest-path runtime/sidecar/Cargo.toml -- --check
RUST_MIN_STACK=33554432 cargo test --manifest-path runtime/sidecar/Cargo.toml huabaosi_feishu_artifact_mirror
cargo test --manifest-path runtime/sidecar/Cargo.toml --features postgres-integration-tests huabaosi_feishu_artifact_mirror::tests::postgres_mirror_state_is_idempotent_and_redacted -- --ignored --exact
cargo clippy --manifest-path runtime/sidecar/Cargo.toml --all-targets --all-features -- -D warnings
pnpm mcp:adapters:check
pnpm workflows:check
```
