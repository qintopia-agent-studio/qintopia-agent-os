# Agent OS 本地业务测试框架

状态：实施中。本文档是文档、基础框架和示范案例的共同验收契约。

## 目标和技术决策

提供给 Codex 使用的本地业务测试入口：可按自然语言定位已有覆盖，可补充缺失场景，可一次运行全部已登记业务测试，并通过本地 Allure 报告查看业务场景、步骤、失败证据和模拟边界。

固定技术选择：pytest 承载跨模块场景；Rust、Python
`unittest`、Node 原生测试继续保留；Docker PostgreSQL 运行真实 migration；Allure Report
2 + `allure-pytest` 展示结果；Allure 报告运行时只要求 Java 17；NATS、MCP
transport 和真实进程仅在专项场景启用；第一版数据库场景串行执行。基础设施锁定 pytest
`8.4.2`、`allure-pytest` `2.16.0`、Allure CLI `2.43.0`。

第一版不做通用 YAML 工作流引擎、不迁移全部已有测试、不把真实模型评估或真实 channel 发送混入本地确定性全量套件、不开发自定义前端。

## 阶段一：文档先行

- 建立
  `docs/testing/README.md`、`agent-guide.md`、`authoring.md`、`architecture.md`，说明使用方式、Codex 流程、边界、清单语义、案例和验收规则。
- 在根 `AGENTS.md` 增加测试文档入口和命令索引，在 `docs/README.md` 和
  `docs/engineering/change-routing-index.md` 增加测试文档链接。
- 文档用明确的 `passed/failed/broken/skipped/blocked`
  运行摘要语义，要求依赖缺失、空选择和未执行不能报告为通过；Allure 原生状态中将
  `blocked` 映射为带原因的 `broken`。

验收：文档能指导协作方直接请求定向测试或全量测试；读者能知道早报案例是文本路径、默认模拟 channel、不能证明真实送达。

## 阶段二：最小基础框架

- 增加 `tools/testing/run.mjs`
  及统一命令：`setup`、`doctor`、`list`、`business`、`report`、`harness`；`package.json`
  提供 `pnpm test:*` 短入口，默认全量，支持 `--feature` 和 `--scenario`。
- 增加经过 schema 校验的
  `tools/testing/catalog.json`，登记 feature、scenario、别名、executor（`pytest`/`unittest`/`cargo`/`node`）、固定 target/argv、依赖、超时和真实/模拟边界。
- 增加本地 Python venv、pytest/Allure 依赖准备和 Java 17/ Docker/
  PostgreSQL 检查。运行时创建唯一 run ID、临时 Compose 项目和独立结果目录。
- 使用真实 PostgreSQL migration 和合成 seed；通过 `QINTOPIA_SIDECAR_DATABASE_URL`
  注入本地连接，通过 `QINTOPIA_TEST_RUN_DIR` 注入
  `.local-testing/runs/<run-id>/`；失败、超时、中断后保留报告/摘要并清理本次资源。NATS 只由需要传输语义且已登记的专项场景启动。
- 通过 fixture 驱动真实业务模块；模型、channel、COS 等外部边界使用严格脚本或 loopback 假服务；未声明请求直接失败。
- 汇总 Allure 结果、机器可读摘要和真实调用轨迹；单个场景失败后继续独立场景，最终以明确非零状态退出；不自动重试。

验收：全量、feature、scenario 三种选择准确；零测试、未知 ID、缺依赖、超时、非零退出和旧报告混入均失败；运行三次无资源泄漏和数据串扰；Allure 可查看中文业务/场景/步骤和附件。

## 阶段三：二花早报参考链

实现一条较长但边界明确的文本发布链：合成输入 → 真实早报业务逻辑 → 状态/审核/最终确认 → 文本发送 worker
→ 本地 QiWe 假服务 → 真实状态与审计更新。

至少包含四个场景：

1. 正常文本发布：获批内容只发送一次，状态和审计符合当前实现。
2. 重复执行发送 worker：终态任务不重复请求、不重复成功记录。
3. 缺少最终确认：不进入可发送状态，假服务收到零请求。
4. 请求已送出但响应断开：记录当前实现的不确定状态，不自动重发、不误报成功。

案例不覆盖卡片/图片上传，不触发真实 Hermes
08:10 定时器，不连接真实 channel；报告必须显示这些未覆盖边界。独立增加早报 schedule/wrapper 契约检查时，只能说明
`08:10 Asia/Shanghai` 配置接线正确。

## 阶段四：回归和交付

- 将统一入口接入 CI，使用同一清单和命令；成功失败均保留摘要与 Allure 结果。
- 先跑
  `pnpm test:harness`、案例 feature 测试，再跑已登记全量和受影响包原生测试；框架和案例自身通过后才创建 PR。
- 运行 `pnpm lint:md`、相关 package 检查、`pnpm check:pr:auto`；若 touching
  runtime/database/channel，按 change-routing index 扩大到 heavy/runtime 验证。
- PR 中报告变更文件、验证命令及结果、未覆盖边界和生产边界；本计划完成后移入
  `docs/plans/completed/`。

最终验收：协作方对 Codex 说“测试二花早报是否会重复发送”即可定位并运行对应场景；说“运行全部业务测试”即可执行完整已登记集合并打开统一报告；任何未执行或模拟结果都不会被描述成真实线上送达。
