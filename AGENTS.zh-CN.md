# 项目指令

## 地图

- 人类入口：`README.md` / `README.zh-CN.md`
- Agent 协作规则：`AGENTS.md` / `AGENTS.zh-CN.md`
- Claude Code 规则：`CLAUDE.md` / `CLAUDE.zh-CN.md`
- 文档中心：`docs/README.md` / `docs/README.zh-CN.md`
- 架构概览：`docs/architecture/agent-os-overview.md`
- 产品范围：`docs/product/agent-os-prd.md`
- Agent OS 设计：`docs/agent-os/README.md`
- 运行时基线：`docs/operations/runtime-baseline.md`
- 协作模型：`docs/engineering/collaboration-model.md`
- 迁移规则：`docs/engineering/migration-policy.md`
- 服务器变更规则：`docs/engineering/server-change-policy.md`
- 编程 Agent 护栏：`docs/engineering/programming-agent-guardrails.md`
- 当前路线图：`docs/plans/active/current-roadmap.md`
- 源文档盘点：`docs/operations/source-document-inventory.md`
- Registry 索引：`registry/`
- Agent packages：`agents/`
- Skill packages：`skills/`
- Workflow packages：`workflows/`
- MCP adapters：`mcp/`
- Runtime templates：`runtime/`
- 部署脚本和 manifests：`deploy/`
- 工程文档：`docs/engineering/`
- 运维文档：`docs/operations/`
- Fixtures 和 replay 数据：`fixtures/`
- 历史 POC：`deprecated/`

## 命令

- 安装依赖：`pnpm install`
- 格式化：`pnpm format`
- 仓库级检查：`pnpm check`
- Markdown lint：`pnpm lint:md`

搜索文件和文本时使用 `rg` 和 `rg --files`。

## 核心规则

- 按 Agent OS capability 组织，不按编程语言组织。
- Rust、Python、TypeScript、shell、SQL 都只是 package 内部实现细节。
- 不要创建顶层 `python/`、`rust/`、`typescript/` 等语言桶。
- 不要直接在 `master` 上开发；先创建 feature branch。
- 新功能、行为变更、迁移、runtime 改动或生产相邻改动必须先写文档。
- commit message 必须使用 Conventional Commits。允许的类型只有 `build`、`chore`、
  `ci`、`docs`、`feat`、`fix`、`perf`、`refactor`、`revert`、`style` 和 `test`。
- 不要引入 Java、Gradle、Maven、Kotlin、Go、Swift、C#、PHP、Ruby、Elixir 或新的语言/工具链栈，除非 owner 明确批准架构决策。
- 不要直接热改生产服务器。
- 不要把 secrets、live `.env`、tokens、table ids、私聊原文、成员画像原文、server-only
  runtime state 放进 git。
- WorkTool 不用于新的 Qintopia Agent OS channel 工作。WorkTool 和 WorkTool Hermes
  plugin 只能作为 deprecated 或 audit-only material。
- Hermes Kanban 不再是未来任务和编排主干。不要基于 Hermes Kanban 新建 workflow。
- Postgres/AgentOS 是系统事实源。飞书是 human workbench 和镜像，不是事实源。
- Hermes 仍然是 Agent runtime，不是业务数据库。

## Package 放置规则

- Agent profile、prompt、可用 skills、memory policy、禁止动作： `agents/<agent>/`。
- 可复用 channel 或业务能力： `skills/<capability>/`。
- 跨 Agent 的业务流程： `workflows/<workflow>/`。
- MCP server 或 adapter： `mcp/<adapter>/`。
- Runtime template 或 render/check 逻辑： `runtime/<runtime-area>/`。
- Release、smoke、rollback 或 server install 逻辑： `deploy/<area>/`。
- 历史 POC 或废弃方向： `deprecated/<topic>/`。

## Package 契约

每个 adopt 进来的 package 最终都应包含：

- `README.md`
- `manifest.yaml`、`agent.yaml` 或 `workflow.yaml`
- `tests/` 或 `fixtures/`
- owner 和 risk level
- validation command
- production boundary
- 触碰 runtime 时的 rollback 或 decommission 说明

这些信息缺失时，不要把 package 视为 production-ready，除非有明确记录的例外。

## 迁移规则

迁移必须 inventory-first：

1. 确认当前 source path。
2. 标记为 `adopt`、`template`、`runtime-only`、`deprecated` 或 `remove`。
3. 保留 source hash 或 commit reference。
4. 添加 package metadata。
5. 添加聚焦的 tests 或 fixtures。
6. 最后再接入 registry 和 deployment。

服务器 `.hermes/profiles/*` 下的 runtime 目录必须视为 live runtime
state。它们可以产生 inventory record、template 或 diff；不能整目录复制进这个仓库。

## 服务器变更规则

服务器是部署目标，不是编辑现场。

允许的服务器操作：

- 只读 inventory
- service status 检查
- log 检查
- smoke 检查
- 按 runbook 部署已批准 commit SHA
- 紧急回滚，但必须补 follow-up patch 和 owner record

禁止的服务器操作：

- 直接编辑文档
- 直接编辑代码
- 直接编辑 `.hermes` runtime 文件
- 用 scp 覆盖单个源码文件
- 在服务器提交未评审实验，并把它当作产品方向

## 验证要求

提交 PR 前：

- 运行 package 级测试。
- 有 fixture replay 时运行 fixture replay。
- 有 registry/manifest check 时运行。
- runtime/deploy 改动必须包含 dry-run 输出和 rollback notes。
- 用户可见 HTML 报告必须运行 HTML parse 和浏览器 overflow 检查。
- 生产相邻改动必须说明是否触碰 external sends、database writes、profile
  runtime、secrets、Feishu、QiWe 或 systemd。

## 文档规则

- 决策必须进 git，不只留在聊天里。
- 优先写短而聚焦的文档，不写一个巨型手册。
- 服务器侧探索在 owner review 确认前必须标记为 unapproved。
- 内部工程文档避免形式主义表达。
- 技术报告要具体：现状、证据、风险、下一步动作。

## 新 Agent 第一阅读顺序

1. `README.md` / `README.zh-CN.md`
2. `AGENTS.md` / `AGENTS.zh-CN.md`
3. `docs/README.md` / `docs/README.zh-CN.md`
4. `docs/architecture/agent-os-overview.md`
5. `docs/plans/active/current-roadmap.md`
6. `docs/engineering/programming-agent-guardrails.md`
7. 产品范围改动先读 `docs/product/agent-os-prd.md`
8. Agent OS 设计改动先读 `docs/agent-os/README.md`
9. 历史迁移证据读 `docs/plans/completed/monorepo-migration.md`
10. 目标 package README 或 manifest
11. `docs/engineering/` 或 `docs/operations/` 下相关文档

做大范围改动前，先报告读过哪些文件、计划触碰什么、验证命令和生产边界。
