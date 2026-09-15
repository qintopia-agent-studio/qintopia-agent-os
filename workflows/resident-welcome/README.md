# Resident Welcome V1

Owner: PatrickLiveCool。状态：draft，本地开发；风险：high。

唯一业务与 PMS 协议为[共同契约](../../docs/plans/active/unified-person-welcome-v1-contract.md)。实现位于
[resident_welcome](../../runtime/sidecar/src/resident_welcome/)，数据设计见[版本化设计](../../runtime/postgres/docs/data-design/2026-09-09-resident-welcome-v1.md)。

## Responsibility

人员任职、智能体训练授权和协作职责属于整个 Agent
OS 的统一设置；本流程只使用这些设置，不要求用户为欢迎重新任命小管家或舍长。具体约束见共同契约第 5.3 节。

当前 `welcome_grants`
仍是欢迎专用的本地实现，尚未接通统一训练授权与职责解析，不能视为通用人员基础设施已完成。

客房领域通过结构化 WorkItem 协调，阿靓由 huabaosi 处理产物，二花面向社区/楼栋，四老师处理内部协作。

当前本地工作项技术协调宿主复用 silaoshi；没有注册新的客房 Agent、恢复 call_agent 或改动任何 profile。

小满不是前置条件。

复用既有 Person、WorkItem、Artifact 和 work_item_events；来源链接的 namespace、逐人案例、申请修订、有效同意、目标范围与人类审批决定资格。

群和内容部分独立恢复，不等一小时。

通用 Operations MCP 仍不承担本流程的真实任务写入。

## Production Boundary

本地 Store 必须显式连接 literal-loopback 的
`qintopia_test`，不使用环境中的生产数据库配置。影子数据使用独立数据库；所有真实制卡、附件上传、PMS 写和发送硬关闭。

有 synthetic-only
loopback 接收器/工作台，以及逐作用域的单次消费、扫描和回读函数。本地工作台每 15 秒恢复一次条件汇合；没有安装生产调度、部署或切换。

已有生产流程继续运行。回滚先停新动作并核对 unknown，不删除业务记录或把积压重新准入。未完成的外部映射和旧渲染器不能用模拟成功替代生产验收。

## Acceptance Scenarios

共同契约 T01—T16 是验收总表。包测试覆盖签名、body 字节保留、JSON 边界、附件追加、自写回调和影子硬门禁；隔离 Postgres 测试覆盖 durable
accept、冲突回滚、claim fencing、逐人绑定、同意、审核、入群/入住汇合及稳定 action。

PMS 事务回滚/晚提交、真实名单身份转换、卡片视觉验收与真实 provider 回执仍需要各自权威实现及联调，不能由本仓 fixture 证明。

详细覆盖和未完成范围见[本地验收记录](../../docs/reports/2026-09-09-resident-welcome-v1-local-acceptance.md)。

## Local Workbench

这是本次住宿关联和内容审核的合成验证页面，不是“小管家/舍长任职与权限”的正式设置入口。用户当前无需在此录入真实人员；统一人员与职责设置尚待实现。

`run-welcome-local --port 18870`
只监听 127.0.0.1，拒绝非 synthetic 来源。它要求显式设置以下变量，不读取生产 `.env`
或 Hermes profile：

- `QINTOPIA_WELCOME_LOCAL_ENABLE=1`
- `QINTOPIA_WELCOME_LOCAL_DATABASE_URL`：隔离的 loopback `qintopia_test`
- `QINTOPIA_WELCOME_LOCAL_SOURCE`、`QINTOPIA_WELCOME_LOCAL_PROPERTY`
- `QINTOPIA_WELCOME_LOCAL_OPERATOR_LINK`：夹具中已确认的独立人工身份
- `QINTOPIA_WELCOME_LOCAL_SIGNING_KEY`：仅合成验收使用的独立签名材料

数据库应用迁移后，可载入 `fixtures/local-workbench.sql`
查看合成空态。工作台支持查找、首次确认、新建 Person、撤销、住宿/申请绑定和文案审核。图片未连接可验证预览时禁用图片审批；不存在后台真实发送按钮。

`Store::scan_once`、`consume_one`、`pull_once`、`refresh_orders`
接受固定读取适配器。扫描页与代次/检查点同事务提交；`finish_scan`
不自动重新启用来源。生产调度须补充容量、权限、准入与回滚验收，不能直接把本地入口升级为生产入口。

## Validation

推荐使用仓库框架自动创建和清理独立随机端口数据库：

```bash
pnpm test:setup
RUSTUP_TOOLCHAIN=1.96.0 pnpm test:business -- --feature resident-welcome
```

两项 PostgreSQL 场景同时接入 CI 和
`pnpm check:pr:postgres`，不是默认忽略后的普通单元测试结果。测试框架适配仅在 test 编译中启用；本地工作台仍使用显式合成配置，不回退到生产数据库。

```bash
RUST_MIN_STACK=33554432 cargo test --manifest-path runtime/sidecar/Cargo.toml resident_welcome
QINTOPIA_WELCOME_TEST_ENABLE=1 RUST_MIN_STACK=33554432 \
  cargo test --manifest-path runtime/sidecar/Cargo.toml \
  --features postgres-integration-tests resident_welcome_postgres -- --ignored
pnpm workflows:check
pnpm check:pr:heavy
git diff --check
```

数据库测试还需显式配置 `QINTOPIA_WELCOME_TEST_DATABASE_URL` 指向本次独立的loopback
`qintopia_test`；测试会应用版本化迁移并写合成数据。没有此配置不会尝试其他数据库。不要把真实数据导入测试库。
