# 文档中心

[English](README.md)

这里是 Qintopia Agent
OS 的文档入口，用来串联当前 monorepo 结构、已完成迁移证据、release/current 运维模型，以及仍有审计或设计价值的历史来源文档。

## 从这里开始

1. 当前架构：[architecture/agent-os-overview.md](architecture/agent-os-overview.md)
2. 产品范围：[product/agent-os-prd.md](product/agent-os-prd.md)
3. Agent OS 设计：[agent-os/README.md](agent-os/README.md)
4. 运行时基线：[operations/runtime-baseline.md](operations/runtime-baseline.md)
5. Agent 能力矩阵：[operations/agent-capability-matrix.md](operations/agent-capability-matrix.md)
6. 当前路线图：[plans/active/current-roadmap.md](plans/active/current-roadmap.md)
7. 编程 Agent 护栏：[engineering/programming-agent-guardrails.md](engineering/programming-agent-guardrails.md)
8. 改动路由索引：[engineering/change-routing-index.md](engineering/change-routing-index.md)
9. 协作和 PR 流程：[engineering/collaboration-model.md](engineering/collaboration-model.md)
10. Package 契约：[engineering/package-contract.md](engineering/package-contract.md)
11. 迁移规则：[engineering/migration-policy.md](engineering/migration-policy.md)
12. 防漂移规则：[engineering/anti-drift-policy.md](engineering/anti-drift-policy.md)
13. 服务器变更规则：[engineering/server-change-policy.md](engineering/server-change-policy.md)
14. 源文档盘点：[operations/source-document-inventory.md](operations/source-document-inventory.md)
15. M1 来源盘点：[operations/inventory/README.md](operations/inventory/README.md)
16. M9 服务器最终迁移 Runbook：[operations/m9-server-cutover-runbook.md](operations/m9-server-cutover-runbook.md)
17. 服务器目录规划：[operations/server-directory-plan.md](operations/server-directory-plan.md)
18. Release/current 模型：[operations/release-current-model.md](operations/release-current-model.md)
19. QiWe 图片发送 staging
    runbook：[operations/qiwe-image-send-staging-runbook.md](operations/qiwe-image-send-staging-runbook.md)
20. 小满生产证据 runbook：
    [operations/xiaoman-production-evidence-runbook.md](operations/xiaoman-production-evidence-runbook.md)
21. 报告索引：[reports/README.md](reports/README.md)
22. 已完成迁移归档：[plans/completed/monorepo-migration.md](plans/completed/monorepo-migration.md)

## Package 入口

- Profiles: [../agents/](../agents/)
- Skills: [../skills/](../skills/)
- Workflows: [../workflows/](../workflows/)
- MCP adapters: [../mcp/](../mcp/)
- Runtime contracts: [../runtime/](../runtime/)
- Deploy contracts: [../deploy/](../deploy/)
- Replay fixtures: [../fixtures/](../fixtures/)

新协作者开始改动前，应先阅读目标 domain 的 registry：

- [../registry/skills.yaml](../registry/skills.yaml)
- [../registry/workflows.yaml](../registry/workflows.yaml)
- [../registry/mcp.yaml](../registry/mcp.yaml)
- [../registry/runtime.yaml](../registry/runtime.yaml)
- [../registry/deploy.yaml](../registry/deploy.yaml)

## 目录地图

```text
docs/
├── README.md                         # 英文文档入口
├── README.zh-CN.md                   # 中文文档入口
├── architecture/                     # 系统边界和数据流
├── engineering/                      # 协作、package、迁移规则
├── operations/                       # 服务器证据和运维参考
├── plans/                            # 进行中和已完成计划
├── product/                          # 产品和业务文档
├── reports/                          # 内部同步报告和生成报告
└── agent-os/                         # 领域模型、契约、验收测试
```

## 来源处理规则

服务器上的文档是证据，不会自动变成产品决策。迁入前必须先分类：

- `adopt`：足够稳定，可以成为 monorepo canonical doc 或 package 输入。
- `template`：可作为模板使用，但需要移除 live state 和 secrets。
- `runtime-only`：只描述线上部署状态，作为运维证据保留。
- `review-pool`：有参考价值，但需要 owner review 后才能成为方向。
- `deprecated`：历史材料，只保留审计或迁移参考价值。
- `remove`：没有未来产品价值，也没有审计价值。

服务器侧 Rust 和 Huabaosi shadow 文档目前归类为
`review-pool`，除非 owner 明确批准。WorkTool 和 Hermes Kanban 不再作为未来产品路径。

## 文档规则

- 稳定规则放在根文档和 `docs/engineering/`。
- 当前方向放在 [plans/active/current-roadmap.md](plans/active/current-roadmap.md)。
- 历史迁移证据放在
  [plans/completed/monorepo-migration.md](plans/completed/monorepo-migration.md)。
- 源文档盘点放在 [operations/](operations/)。
- 不把 secrets、live runtime state、成员画像原文或私聊原文写进文档。
- 用路径和 disposition 关联来源文档，不把大型历史文档整篇复制进来。
