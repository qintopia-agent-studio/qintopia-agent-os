# Qintopia Agent OS Monorepo

[English](README.md)

这个仓库是 Qintopia Agent OS 的单一事实源。它采用 Capability / Plugin
Monorepo 结构：目录按 Agent OS 领域和业务能力组织，而不是按编程语言组织。

## 目标

Qintopia Agent OS 用一个 git 仓库统一管理 Hermes profiles、受控 skills、workflows、MCP
adapters、runtime templates、部署脚本、fixtures 和工程文档。

这个仓库要替代当前混合模式：一部分代码在多个独立仓库里，一部分文件通过 scp 或手工方式上传服务器，一部分运行态资产直接在
`.hermes` 目录下被修改。

## 仓库结构

```text
qintopia-agent-os-monorepo/
├── AGENTS.md                 # Codex 和编程 Agent 协作规则
├── CLAUDE.md                 # Claude Code 协作规则
├── README.md                 # 英文入口
├── README.zh-CN.md           # 中文入口
├── registry/                 # Agent / Skill / Workflow / Deployment 索引
├── agents/                   # 每个 Agent 一个 profile package
├── skills/                   # 每个可复用能力一个 package
├── workflows/                # 跨 Agent / Skill 的业务流程
├── mcp/                      # MCP server 和 adapter
├── runtime/                  # 运行配置模板和渲染规则
├── deploy/                   # release manifest、部署脚本、smoke、rollback
├── docs/                     # 架构、运维、产品、报告
├── fixtures/                 # replay fixture 和验收数据
├── tools/                    # inventory、registry、CI 工具
└── deprecated/               # 历史 POC 和废弃路径
```

## 领域规则

- `agents/`：人格、prompt、memory policy、可用 skills、禁止动作和 profile 级测试。
- `skills/`：可复用能力，例如 QiWe、天气、飞书 Base、Postgres
  context、知识检索和 Qintopia 业务工具。
- `workflows/`：受治理的业务流程，例如小满活动信号、视觉素材申请、二花咨询、日常运营。
- `mcp/`：MCP server 和 adapter。运行时凭据不进入 git。
- `runtime/`：Hermes、systemd、nginx、Postgres、sidecar 的模板。这里只存模板和渲染检查，不存服务器 live
  state。
- `deploy/`：按已评审 commit SHA 发布、smoke 检查、回滚记录和部署 manifest。
- `deprecated/`：WorkTool、WorkTool Hermes plugin、Hermes
  Kanban、OpenClaw 等历史 POC，只保留审计或迁移参考价值。

## 协作方式

所有变更都通过 git：

1. 新建分支。
2. 阅读相关 package README 和 manifest。
3. 做小范围改动。
4. 运行 package 级验证。
5. 有仓库级检查时再运行仓库级检查。
6. 提交 PR，并写明验证结果和生产边界。
7. 只部署经过确认的 commit SHA。

服务器是部署目标，不是编辑现场。不要直接在服务器或 `.hermes`
运行目录里修改文档、代码、脚本、wrapper、worker、runbook 或 runtime template。

## 文档

架构、工程规则、源文档盘点、迁移规则和运维参考从
[docs/README.zh-CN.md](docs/README.zh-CN.md) 开始阅读。

产品和 Agent OS 实现上下文优先读：

- [docs/product/agent-os-prd.md](docs/product/agent-os-prd.md)
- [docs/agent-os/README.md](docs/agent-os/README.md)
- [docs/operations/runtime-baseline.md](docs/operations/runtime-baseline.md)

## 迁移

迁移状态、源目录 inventory、adoption 顺序和进度更新统一维护在
`docs/plans/active/monorepo-migration.md`。

## Package 契约

未来每个 package 都应包含：

- `README.md`：说明 package 做什么、owner、范围和命令。
- `manifest.yaml` / `agent.yaml` / `workflow.yaml`：机器可读元数据。
- `tests/` 或 `fixtures/`：replay 或验证证据。
- 明确的生产边界说明。
- 如果触碰 runtime 行为，要有 rollback 或 decommission 说明。

## 当前验证方式

提交 PR 前运行仓库检查：

```bash
pnpm check
```
