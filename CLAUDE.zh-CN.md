# Claude Code 指令

这个仓库是 Qintopia Agent OS 的 Capability / Plugin Monorepo。`AGENTS.md`
是主要操作契约；本文件强调 Claude Code 会话中最重要的规则。

## 从这里开始

编辑前先做：

1. 阅读 `README.md` / `README.zh-CN.md`。
2. 阅读 `AGENTS.md` / `AGENTS.zh-CN.md`。
3. 阅读 `docs/README.md` / `docs/README.zh-CN.md`。
4. 阅读 `docs/architecture/agent-os-overview.md`。
5. 可能影响产品范围时阅读 `docs/product/agent-os-prd.md`。
6. 可能影响 Agent OS 设计时阅读 `docs/agent-os/README.md`。
7. 可能影响 runtime 行为时阅读 `docs/operations/runtime-baseline.md`。
8. 判断目标 domain：`agents`、`skills`、`workflows`、`mcp`、`runtime`、
   `deploy`、`docs`、`fixtures`、`tools` 或 `deprecated`。
9. 如果目标 package 已有 README 或 manifest，先阅读它。
10. 如果任务是迁移，先阅读 `docs/plans/active/monorepo-migration.md`。

## 核心边界

- 不要直接编辑生产服务器。
- 不要把 live secrets 或 server-only runtime files 复制进 git。
- 不要把服务器侧实验当成已批准架构。
- 不要基于 WorkTool 或 Hermes Kanban 新建 workflow。
- 不要按编程语言组织顶层目录。
- 没有 package owner、manifest 和 validation path 时，不要添加宽泛抽象。

## 仓库形态

使用这个模型：

```text
agents/<agent>/          profile package
skills/<capability>/     reusable capability package
workflows/<workflow>/    governed business workflow
mcp/<adapter>/           MCP server or adapter
runtime/<area>/          runtime templates and render checks
deploy/<area>/           release, smoke, rollback
docs/<area>/             architecture and operating docs
fixtures/<area>/         replay and acceptance data
deprecated/<topic>/      historical POC only
```

编程语言只是 package 内部实现细节。

## 迁移指导

把现有代码迁入这个 monorepo 时：

1. 先分类为 `adopt`、`template`、`runtime-only`、`deprecated` 或 `remove`。
2. 在 package README 或 manifest 里保留 source path 和 commit/hash。
3. 添加或保留 tests/fixtures。
4. 在 registry、CI、smoke、owner review 完成前，保持生产 wiring disabled。

WorkTool material 只有在有审计价值时才进入 `deprecated/`。

## 报告格式

非平凡任务需要报告：

- 读过哪些文件
- 改了哪些文件
- 影响哪个 package/domain
- 验证命令和结果
- 是否触碰生产边界
- 剩余风险

保持实现改动小而可 review。
