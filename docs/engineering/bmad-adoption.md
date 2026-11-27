# BMAD 流程采用说明

本项目把 BMAD 作为需求、架构、实现和评审的协作流程层，现有 `AGENTS.md`、
`docs/engineering/`、change
routing、分支策略、验证门禁和生产边界继续作为项目约束。BMAD 不替代这些规则，也不自动获得合并、发布、部署、外部发送或真实数据写入权限。

## 入口和顺序

根据任务选择最小流程，不为简单维护工作套用完整阶段：

1. 需求或范围不清：使用 `bmad-product-brief` 或 `bmad-prd`，把确认后的目标写入
   `docs/product/` 或 `docs/plans/active/`。
2. 跨包边界或有多种可行方案：使用 `bmad-architecture`，把架构决策写入
   `docs/architecture/`。
3. 有交互或视觉范围：使用 `bmad-ux`，把确认后的设计约束写入所属产品或功能文档。
4. 需要拆解实现：使用 `bmad-spec` 或
   `bmad-create-epics-and-stories`，并把故事与现有 capability/package 对齐。
5. 开始实现：使用 `bmad-build` 或
   `bmad-quick-dev`。实现前仍须阅读目标包 README、manifest、change
   routing 和适用的运行时规则。
6. 请求审阅：明确使用 `bmad-review`；完成后用 `bmad-walkthrough` 做人工验收说明。
7. 维护项目指令：使用 `bmad-project-context` 的 `adopt`、`refresh` 或
   `audit`，先核对现有 `AGENTS.md`
   与具体差分。相同对象、范围和影响沿用已有用户授权；改变治理约束或删除规则且缺少授权时，先展示改动与影响，再请求确认。用户明确指令优先于 Skill 的常规步骤。

## 产物和归档

- BMAD 的中间产物写入本机配置的输出目录，不作为产品事实或验收证据；推荐使用仓库已忽略的
  `.local-workspace/bmad-output/`。沿用 `_bmad-output/` 时，先配置并验证其本机忽略规则。
- 需要长期保留的结果必须迁移到项目已有归属：产品文档放 `docs/product/`，架构放
  `docs/architecture/`，当前计划放 `docs/plans/active/`，验证和生产观察放
  `docs/reports/` 或 `docs/operations/`。
- 迁移前保留来源、状态和未验证项；不要把提示词、私密消息、凭据、实时运行状态或原始聊天记录复制进仓库。
- 中文沟通和文档沿用项目约定，用“模拟”描述测试身份、事件、数据和环境。

## 验证和门禁

- 先按仓库 read order 读取
  `README.md`、`AGENTS.md`、`docs/README.md`、roadmap、programming guardrails、change
  routing 和受影响包文档。
- 在非 `master` 分支工作；文档优先于功能、行为、迁移和运行时改动。
- 先跑目标包检查，再按变更范围运行 `pnpm check:pr:quick` 或
  `pnpm check:pr:auto`；文档改动至少运行 `pnpm lint:md`。
- BMAD 输出的“完成”只表示流程产物已生成。实现、验证、合并、发布、部署和真实业务验收分别报告，不互相推断。
- CI、服务器、Hermes Profile、外部消息、数据库写入和生产配置仍按现有 owner
  approval 和 runbook 执行；BMAD 不能绕过这些门禁。

## 本机运行依赖

项目使用全局安装的 BMAD skills，并在仓库根目录配置本机 `_bmad/`
运行脚本和 TOML。该目录只保存本机运行依赖，不进入业务分支；首次使用或更换机器时，按本机安装的 BMAD 版本重新生成它。配置中的
`project_knowledge` 指向 `docs/`，输出目录按上文选择。

分别核对运行目录与实际输出目录的忽略状态；`_bmad/.gitignore` 不会忽略同级的
`_bmad-output/`。本机忽略规则可写入 `git rev-parse --git-path info/exclude`
返回的文件，不把个人配置加入团队文档 PR。

例如，使用推荐输出目录时运行：

```sh
git check-ignore -v _bmad/config.toml .local-workspace/bmad-output/example.md
```

验证配置解析：

```sh
uv run --no-cache "_bmad/scripts/resolve_config.py" --project-root "$PWD"
```

渲染某个 skill 时，使用其绝对路径，例如：

```sh
uv run --no-cache "_bmad/scripts/render_skill.py" \
  --project-root "$PWD" \
  --skill "/absolute/path/to/installed/skills/bmad-build"
```

渲染成功只证明本机运行依赖和 skill 路径可加载，不代表对应流程已经完成。
