# 阿靓图片产物飞书镜像计划

Status: adapter and production enablement implemented on separate PRs; merge, manual
Release publication, production configuration, activation, and first-record evidence
remain

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
- 每次需要同步时重新读取并校验稳定媒体 URI 的精确 JPEG，重新上传后覆盖 `最终JPEG`
  附件。这样飞书中的附件即使被人工替换，也会恢复为 AgentOS 已记录的不可变字节；`last_synced_at >= artifact.updated_at`
  时不发起重复同步。
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
6. 配置中的 production release SHA 是 40 位小写 commit SHA，且与 systemd 注入的
   `QINTOPIA_DEPLOYED_COMMIT_SHA` 完全一致；
7. profile env path、媒体 host allowlist 和固定 schema version 校验通过。

生产 artifact 固定编译 `huabaosi-production-adapter` 和
`huabaosi-feishu-mirror-adapter`，不编译 staging 或 QiWe
adapter。仅编译 feature 不会启用写入；普通 release
installer 安装固定 preflight/read-only observation/worker/timer
unit，但不自动启用 timer。`--dry-run`
可读取 Postgres 并生成脱敏 preview，但不读取媒体、不请求飞书、不写数据库。`--fixture-mode`
只验证本地映射。

## Production Boundary

生产 enablement 边界是：

- PR 增加固定 systemd
  preflight/worker/timer、构件 feature、只读 observation、显式 activation 和 rollback；
- 普通 release 部署不自动启用外部写 timer；
- 只有 owner 手动发布 Release、配置生产门禁并运行 activation 后，worker 才能写入；
- activation 先执行无网络 preflight，再 enable/start timer；
- Release Please PR 不自动合并，Release 不自动发布。

目标 Base 已由 owner 在真实飞书工作台中创建“阿靓图片产物版本表”并核对 20 个字段；真实 Base
token、table
id 和凭据不进入 git。首条真实记录仍需在生产激活后核对附件、不可变 identity、审核字段和 Postgres 脱敏 sync
audit。

Rollback 先立即 disable/stop 独立 timer 和 worker，再检查持久化 sidecar
env 中镜像开关已通过受控配置渠道设为唯一的 `0`。开关缺失、仍为
`1`、重复、配置文件缺失或无法确认时，脚本必须返回失败且不得宣称回滚完成；修正配置后再次运行 rollback。既有 AgentOS
artifact 和审计保持不变，飞书镜像记录可保留只读或由 owner 单独归档。

生产 observation 必须从 `release/current` 发现 immutable sidecar；显式
`QINTOPIA_SIDECAR_BIN` 也必须解析到同一 release-local 文件并通过 manifest
feature 校验，不得回退到源码树 `cargo run`。shell 只以纯文本读取 enable flag；child
launcher 只传递该 flag 和 non-secret release SHA，不使用 `source`、`eval`
或含 secret 临时文件。observation 只检查 unit/timer 状态和非 secret mirror observation
preflight；完整配置 preflight 仍只属于 activation/受控运行时配置路径。observation 不执行 worker
`--dry-run` 队列预览，不得读取数据库/Base/table/Feishu
secret，不得上传媒体、写飞书/Postgres、审核、发送或发布。

## Validation

```bash
cargo fmt --manifest-path runtime/sidecar/Cargo.toml -- --check
RUST_MIN_STACK=33554432 cargo test --manifest-path runtime/sidecar/Cargo.toml huabaosi_feishu_artifact_mirror
cargo test --manifest-path runtime/sidecar/Cargo.toml --features postgres-integration-tests huabaosi_feishu_artifact_mirror::tests::postgres_mirror_state_is_idempotent_and_redacted -- --ignored --exact
cargo clippy --manifest-path runtime/sidecar/Cargo.toml --all-targets --all-features -- -D warnings
pnpm mcp:adapters:check
pnpm workflows:check
node tools/deploy/test-huabaosi-feishu-mirror-production-observation.mjs
node tools/deploy/test-huabaosi-feishu-mirror-production-activation.mjs
pnpm deploy:runner:check
```
