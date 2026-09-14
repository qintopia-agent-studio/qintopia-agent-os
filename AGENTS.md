# Agent OS 开发规则

本文件是本仓库中 AI 编程代理的工作契约，面向 GPT-6
Astra 等能够自主分析、实现和验证的模型。规则强调判断依据和完成标准，不重复解释基础操作。

## 适用范围

- 默认使用简体中文沟通；代码、命令、路径和技术标识保留原文。
- 先理解目标、现状、约束和验收标准，再修改文件。遇到源码、文档和运行证据冲突，先查证并记录结论。
- 在已授权范围内持续完成准备、实现、修复和验证；只有缺少会实质改变业务规则、权限、成本或不可逆结果的决定时才提问。
- 不把推测当事实，不把计划当完成；最终报告改动、验证证据、未完成项和外部依赖。
- 搜索优先使用 `rg` / `rg --files`。手工编辑使用
  `apply_patch`，不要用 shell 重定向或脚本覆盖文件。
- 保留他人已有改动和未跟踪文件；不要使用破坏性 Git 命令回滚无关内容。

## 项目地图

- 入口：`README.md`、`docs/README.md`
- 架构与产品：`docs/architecture/agent-os-overview.md`、`docs/product/agent-os-prd.md`、`docs/agent-os/README.md`
- 工程规则：`docs/engineering/`
- 运行与生产：`docs/operations/`
- 计划：`docs/plans/active/current-roadmap.md`
- Agent 包：`agents/`；能力包：`skills/`；跨 Agent 流程：`workflows/`
- MCP 适配器：`mcp/`；运行时：`runtime/`；部署与回滚：`deploy/`
- 注册表：`registry/`；夹具与回放：`fixtures/`；历史材料：`deprecated/`

开始新任务时，至少阅读
`README.md`、本文件、目标包 README/manifest，以及相关架构、路线图和工程/运行文档。大范围改动前说明读过的材料、计划触及的文件、验证命令和生产边界。

## 架构不变量

- Hermes 是 Agent runtime，不是业务数据库。
- Agent
  OS/Postgres 保存身份、业务关联、事件、WorkItem、审计和控制面事实；PMS 是订单、房态、入住、退房等住宿事实的权威来源。
- Feishu
  Base 是表单、人工工作台和运营视图，不是关键事件的唯一存储。关键事件必须进入可追溯的服务端存储，并支持去重、重试和对账。
- Agent 间通过受控 MCP、能力、Event 和 WorkItem 协作。不得新增自由文本直连，或让一个 Agent 直接依赖另一个 Agent 的内部 prompt、表名和列名。
- 能力按业务/运营边界组织，语言只是实现细节；不得建立 `python/`、`rust/`
  等顶层语言目录。
- 身份采用稳定的内部 `person_id`
  与外部平台标识映射；允许一个人有多次申请、入住、换房和无申请的 PMS 记录。匹配必须可审计，支持待确认、人工确认、撤销和合并。
- 权限按操作者、Agent、数据域、群/宿舍范围和动作分别判断；来源不明、过期或冲突的数据不能直接决定发送目标或写入事实。

## 数据、隐私与权限

- 明确每个字段的权威来源、同步方向、版本/时间戳、冲突策略和保留期限。
- 外部联系人、内部员工、义工/舍长等协同人员都要通过平台标识映射到内部身份；不要用显示名作为唯一键。群成员和发言者识别应保存最小必要标识，并允许身份变更。
- 住客知识只向有业务需要的 Agent 和工作范围提供；按最小权限读取申请表、PMS、群足迹和档案，不把整库交给群聊 Agent。
- 禁止在代码、日志、证据、提交或聊天中暴露 secrets、Token、真实群 ID、数据库 URL、原始消息、私密档案或完整外部用户标识。证据只保留脱敏计数、哈希引用和边界状态。
- 外部发送、数据库写入、PMS 写操作、权限扩大和生产配置变更必须有明确动作边界、幂等键、审计记录、失败重试/补偿和适用审批。

## 包与变更

- Agent 配置、prompt、允许技能、记忆策略和禁用动作放在 `agents/<agent>/`。
- 可复用能力放在 `skills/<capability>/`；跨 Agent 业务流程放在
  `workflows/<workflow>/`；适配器、运行时、部署逻辑分别放在对应目录。
- 新增 workflow 必须同步更新
  `registry/workflows.yaml`、`tools/workflows/check-workflows.mjs` 和
  `deploy/restart-target-rules.yaml`。
- 采用或迁移包至少应有
  `README.md`、manifest、测试/夹具、owner、风险级别、验证命令和生产边界；缺项须在文档中说明例外。
- 新功能、行为/数据迁移、运行时或生产相邻改动先写短设计/决策文档，再实现。保持兼容入口，分阶段迁移；不要把服务器上的
  `.hermes/profiles/*` 运行状态整体复制进仓库。
- 不引入新的语言或工具链（Java、Go、Swift 等）除非已有架构决策明确允许。提交使用 Conventional
  Commits；不要手工维护根 `CHANGELOG.md`。

## 生产与外部系统

- 生产服务器是部署目标，不是编辑工作区。允许只读盘点、状态/日志/Smoke 检查，以及经 reviewed
  release/runbook 的部署和回滚；禁止直接改代码、文档、Hermes
  profile、单文件 scp 覆盖或把服务器实验当产品方向。
- 生产变更必须从版本化 runbook、固定目标和最小 `ReadWritePaths`
  进入，先验证版本、路径、哈希、owner/mode 和幂等状态，再执行；失败应 fail
  closed，并保留回滚路径。
- 不绕过 GitHub workflow、Release
  Please、审批或既有 deploy-runner 门禁；不自动合并、发布或重启无关服务。
- 生产诊断和部署只使用 `docs/operations/inventory/server-sources.yaml`
  记录的端点和密钥路径。详细命令、审批字符串和证据格式以专项 runbook 为准。

## 验证

- 常用入口：`pnpm install`、`pnpm format`、`pnpm lint:md`、`pnpm check`、`pnpm pr:doctor`、`pnpm pr:check-body`、`pnpm check:pr:auto`。
- 按风险选择验证：目标包测试和 fixture
  replay；涉及 Rust/sidecar、Postgres、deploy 或 CI 时运行 `pnpm check:pr:heavy`
  或等价专项检查；修改用户可见 HTML 时做解析和浏览器溢出检查。
- macOS 完整 sidecar 测试使用
  `RUST_MIN_STACK=33554432 cargo test --manifest-path runtime/sidecar/Cargo.toml`。`group_message_send`
  集成测试只能在 loopback 的 `qintopia_test`、显式 feature 和 apply
  smoke 开关下运行，绝不连接外部适配器。
- 运行时/部署改动须提供 dry-run、回滚说明和外部边界说明。生产相邻验证不得把真实消息、群 ID、Token 或私密文本写入产物。
- 完成前运行 `git diff --check`，检查注册表、manifest、文档链接和生成物是否同步。

## 文档与故障记录

- 决策写入 git，不只留在聊天；优先短而聚焦的文档。
- 每个生产、部署、预检或 CI 集成故障在同一 PR 的 `docs/reports/`
  留下带日期记录：证据、根因、修复、验证、剩余边界和后续 owner 动作，并同步受影响 runbook/package 文档。
- Xiaoman、Erhua、Huabaosi、QiWe 等具体业务的发送、定时、证据、激活和回滚步骤属于专项 runbook；根规则只保留上述不变量，不在此复制长命令清单。

## 交付标准

最终说明：

1. 改了什么，以及对应的业务/架构目的；
2. 跑了哪些验证及结果；
3. 哪些仍需人工验收、生产审批或外部系统执行；
4. 相关风险、回滚方式和后续文档入口。

不得把“代码已提交”“本地通过”表述成“生产已完成”。

## 既有部署契约兼容索引

以下保留现有部署检查依赖的关键约束；操作细节仍以专项 runbook 为准，不授予生产执行权限。

<!-- prettier-ignore -->
- Production evidence runbook: `docs/operations/xiaoman-production-evidence-runbook.md`
- 本地生产证据链检查：`node tools/deploy/check-xiaoman-production-evidence-chain-local.mjs`。
- Production release requests use `runtime_artifact_profile=huabaosi-production` for the
  primary artifact and install `qiwe-production` as a companion. Do not use the request
  profile as a global runtime switch.
- Production callback bridges must
  derive that path and SHA-256 from the companion manifest and `SHA256SUMS`,
  not trust runtime-local binary paths or hashes.
- A same-SHA request for an existing release must reuse the immutable
  manifest's exact runtime, runtime artifact profile, bundle, commit, scope, and
  restart-target fields.
  Any exception must follow the reviewed release runbook.
- Xiaoman activity read-through configuration uses the reviewed release-local
  `apply-xiaoman-activity-read-through-production-config.py` allowlist copier;
  sourcing the Xiaoman Hermes profile or hand-editing the sidecar environment is forbidden.
