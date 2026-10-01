# 协作渠道选择本地验证

日期：2026-09-29。分支：`codex/collaboration-channel-selection`。本记录只涉及隔离PostgreSQL、模拟人员与来源，以及本机工作台；未连接真实企微、PMS、生产数据库或生产服务。

## 实现范围

- 原组织工作台按当前管理范围分别搜索和分页选择群、已核实联系人渠道、待登记共用账号与已登记账号；同一主体可选择多个不同可信来源。无管理权、来源撤销或版本变化时拒绝旧选择。
- 配置沿用业务预览与保存，最多选择 20 个联系人，群与联系人版本进入范围配置；大列表仅返回 256 条有界摘要，完整查询使用分页入口。
- 追加 `202609290001` 存储范围通信选择，`202609290002`
  为受限管理 UI 提供精确行锁。迁移不授予生产角色权限，也不启用真实发送。

## 验证结果

- 受限 HTTPS 管理 UI 与隔离 PostgreSQL 的定向集成 1/1 通过；渠道候选、分页回归 3/3 通过。数据库位于本任务专用的
  `127.0.0.1:32783/qintopia_test`。
- `pnpm test:doctor`、`pnpm test:harness`（8/8）和 `pnpm collaboration:check` 通过。
- 本机模拟工作台 `http://127.0.0.1:19173/`
  完成双渠道选择、预览、保存、回读与搜索。指定的 Codex Chrome 插件返回
  `Codex auth token is unavailable`；原生 Chrome 的补充检查不能替代该项验收。
- `pnpm check:pr:auto` 在 Light 阶段的 NATS
  ACL 套件被调用方 20 秒上限截断，未进入后续 Rust 或 PostgreSQL
  tier。独立运行同一套件 9/9 通过，约 46 秒；总门禁仍为失败，不据此调整 CI 超时。
- 首次 `pnpm check:runtime` 使用系统 Python 3.9，QiWe
  125 项中 5 项错误，包含新版类型语法与事件循环差异。改用仓库要求的 Python
  3.12 后，完整门禁通过：QiWe
  326 项通过、1 项平台跳过，Sidecar 默认测试 858 项通过、3 项按标记忽略，两个 smoke 通过。
- 首次 `pnpm check:pr:postgres` 缺少 `cargo`
  PATH，第二次使用默认密码被隔离库拒绝；修正后持久化定向用例通过，`person_collaboration`
  163/163 通过，`resident_welcome` 23/24 通过。失败的既有欢迎用例断言
  `prepare(&f, 0).await.is_err()`
  未成立，同一隔离库单独重跑 1/1 通过；整组仍记为失败，末尾 apply smoke 未执行。
- Clippy 的 `--all-targets --no-default-features -D warnings`
  首轮因两个仅供后续宿主接入的当前路由符号未被使用而失败。已按模块现有模式为这两个符号标明保留理由；修订后
  `cargo fmt --check`、无默认 feature 与全 feature 的 `--all-targets -D warnings`
  Clippy 均通过。

## 边界与后续

本地模拟配置与数据库结果不证明真实群可访问、Bot 可主动发送或生产管理角色已获得新锁函数权限。正式 HTTPS、真实企微与 PMS 仍按各自接入验收执行。自动总门禁的 NATS
ACL 等待上限需要负责人按 CI 变更审批规则评审；在取得同意前不修改检查入口。PR 仅供审阅，不合并、发布或部署。
