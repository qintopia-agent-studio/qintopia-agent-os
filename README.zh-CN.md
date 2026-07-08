# Qintopia Agent OS Monorepo

<!-- ALL-CONTRIBUTORS-BADGE:START - Do not remove or modify this section -->

[![All Contributors](https://img.shields.io/badge/all_contributors-4-orange.svg?style=flat-square)](#contributors)

<!-- ALL-CONTRIBUTORS-BADGE:END -->

[English](README.md)

这个仓库是 Qintopia Agent OS 的单一事实源。它采用 Capability / Plugin
Monorepo 结构：目录按 Agent OS 领域和业务能力组织，而不是按编程语言组织。

## 目标

Qintopia Agent OS 用一个 git 仓库统一管理 Hermes profiles、受控 skills、workflows、MCP
adapters、runtime templates、部署脚本、fixtures 和工程文档。

这个仓库已经替代此前的混合协作模式：代码分散在多个独立仓库、服务器本地文件和 `.hermes`
运行态改动中的状态已归并为以 monorepo 为事实源的工作方式。新的 Agent
OS 工作应从这里开始；旧仓库和服务器捕获材料只有在 package 明确标注为来源时才作为迁移或审计输入。

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

1. 从 `master` 新建分支；不要直接在 `master` 上开发。
2. 阅读相关 package README 和 manifest。
3. 新功能、行为变更、迁移或 runtime 改动必须先写文档或更新 manifest。
4. 做小范围改动。
5. 运行 package 级验证。
6. 有仓库级检查时再运行仓库级检查。
7. 使用 Conventional Commits 格式提交，例如 `feat: add weather skill` 或
   `fix: resolve qintopia-tools skill path`。
8. 先运行 `pnpm pr:doctor`，再用 `pnpm pr:create -- --body-file <completed-pr-body.md>`
   创建 PR。
9. Release Please 会根据已合并的 Conventional Commits 维护 release PR 和 root
   changelog。
10. 只有 owner 手动发布已经审核过的 draft GitHub Release，才进入生产部署。

服务器是部署目标，不是编辑现场。不要直接在服务器或 `.hermes`
运行目录里修改文档、代码、脚本、wrapper、worker、runbook 或 runtime template。

新实现代码只能使用仓库已有语言和工具链：

- TypeScript 或 JavaScript、Python、Rust、shell、SQL、YAML、JSON 和 Markdown。
- 不要引入 Java、Gradle、Maven、Kotlin、Go、Swift、C#、PHP、Ruby、Elixir 或其他新技术栈。
- 需要新技术栈时，必须先有 owner 明确批准的架构决策。

## 编程 Agent 指引 Prompt

协作者使用 Codex、Claude Code 或其他编程 Agent 时，先把下面这段 prompt 发给 Agent：

```text
你正在 Qintopia Agent OS monorepo 中工作。

编辑文件前，先阅读 README.md、AGENTS.md、docs/README.md、
docs/plans/active/current-roadmap.md、docs/engineering/programming-agent-guardrails.md、
docs/engineering/change-routing-index.md，以及目标 package 的 README 或 manifest。

规则：
- 修改文件前必须从 master 新建分支。
- 不要直接在 master 上开发。
- 新功能、行为变更、迁移、runtime 变更或生产相邻改动，必须先写文档。
- 代码按 Agent OS capability 组织，不按编程语言组织。
- 只能使用现有实现语言和工具链：TypeScript/JavaScript、Python、Rust、shell、SQL、YAML、JSON 和 Markdown。
- 未经 owner 批准的架构文档，不要引入 Java、Gradle、Maven、Kotlin、Go、Swift、C#、PHP、Ruby、Elixir 或新工具链。
- 不要直接编辑生产服务器。
- commit message 必须遵守 Conventional Commits，只能使用仓库批准的类型。
- 创建 PR 时使用 pnpm pr:doctor 和 pnpm pr:create；不要把预填的 GitHub compare
  链接交给人作为默认流程。
- 不要把 secret、live .env、Hermes live state、私有日志、session、cache、auth 文件、原始聊天记录或 runtime 数据库复制进 git。
- PR-Agent 评论只是辅助审查；CI、CODEOWNERS、branch protection 和 owner review 才是合并依据。
- 普通 feature 或 fix PR 不要手动编辑 root CHANGELOG.md；Release Please 会根据已合并的
  Conventional Commits 维护 release PR changelog。

每次变更都要报告：
1. 改动了哪些文件和 package；
2. 实现前更新了哪些文档或 manifest；
3. 运行了哪些验证命令，结果是什么；
4. 是否触碰生产边界；
5. runtime 行为变化时，说明 rollback 或 decommission 方式。
```

## 文档

架构、工程规则、源文档盘点、迁移规则和运维参考从
[docs/README.zh-CN.md](docs/README.zh-CN.md) 开始阅读。

产品和 Agent OS 实现上下文优先读：

- [docs/plans/active/current-roadmap.md](docs/plans/active/current-roadmap.md)
- [docs/engineering/programming-agent-guardrails.md](docs/engineering/programming-agent-guardrails.md)
- [docs/engineering/change-routing-index.md](docs/engineering/change-routing-index.md)
- [docs/product/agent-os-prd.md](docs/product/agent-os-prd.md)
- [docs/agent-os/README.md](docs/agent-os/README.md)
- [docs/operations/runtime-baseline.md](docs/operations/runtime-baseline.md)

## 发布流程

Release Please 负责准备版本，不替代 owner 的发布审批。功能和修复 PR 合并到 `master`
后，Release Please 会持续维护一个 release PR，更新 `CHANGELOG.md` 和 release
manifest。owner 合并这个 release PR 后，Release Please 会创建 draft GitHub Release。

生产部署只在 owner 手动发布 draft GitHub Release 后开始。现有 `release.published`
workflow 随后构建 artifact、上传 COS，并创建签名的 production deploy request。

## 迁移归档

monorepo 迁移和 legacy
cleanup 已完成。历史迁移状态、源目录 inventory、adoption 顺序和进度更新统一归档在
[docs/plans/completed/monorepo-migration.md](docs/plans/completed/monorepo-migration.md)。当前工作看
[docs/plans/active/current-roadmap.md](docs/plans/active/current-roadmap.md)。

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

## Contributors

<!-- ALL-CONTRIBUTORS-LIST:START - Do not remove or modify this section -->
<!-- prettier-ignore-start -->
<!-- markdownlint-disable -->
<table>
  <tbody>
    <tr>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/qiaopengjun5162"><img src="https://avatars.githubusercontent.com/u/124650229?v=4?s=100" width="100px;" alt="Paxon Qiao"/><br /><sub><b>Paxon Qiao</b></sub></a><br /><a href="https://github.com/qintopia-agent-studio/qintopia-agent-os/commits?author=qiaopengjun5162" title="Code">💻</a> <a href="https://github.com/qintopia-agent-studio/qintopia-agent-os/commits?author=qiaopengjun5162" title="Documentation">📖</a> <a href="#infra-qiaopengjun5162" title="Infrastructure">🚇</a> <a href="https://github.com/qintopia-agent-studio/qintopia-agent-os/commits?author=qiaopengjun5162" title="Tests">⚠️</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/detroxryo"><img src="https://github.com/detroxryo.png?size=100" width="100px;" alt="detroxryo"/><br /><sub><b>detroxryo</b></sub></a><br /><a href="#review-detroxryo" title="Reviewed Pull Requests">👀</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/noraincode"><img src="https://github.com/noraincode.png?size=100" width="100px;" alt="noraincode"/><br /><sub><b>noraincode</b></sub></a><br /><a href="#review-noraincode" title="Reviewed Pull Requests">👀</a></td>
      <td align="center" valign="top" width="14.28%"><a href="https://github.com/PatrickLiveCool"><img src="https://github.com/PatrickLiveCool.png?size=100" width="100px;" alt="PatrickLiveCool"/><br /><sub><b>PatrickLiveCool</b></sub></a><br /><a href="#review-PatrickLiveCool" title="Reviewed Pull Requests">👀</a></td>
    </tr>
  </tbody>
</table>

<!-- markdownlint-restore -->
<!-- prettier-ignore-end -->

<!-- ALL-CONTRIBUTORS-LIST:END -->
