# 文档中心

[English](README.md)

这里是 Qintopia Agent
OS 的文档入口，用来串联当前 monorepo 结构、服务器上只读盘点到的文档，以及本地历史文档中仍然有价值的设计证据。

## 从这里开始

1. 当前架构：[architecture/agent-os-overview.md](architecture/agent-os-overview.md)
2. 产品范围：[product/agent-os-prd.md](product/agent-os-prd.md)
3. Agent OS 设计：[agent-os/README.md](agent-os/README.md)
4. 运行时基线：[operations/runtime-baseline.md](operations/runtime-baseline.md)
5. 协作和 PR 流程：[engineering/collaboration-model.md](engineering/collaboration-model.md)
6. Package 契约：[engineering/package-contract.md](engineering/package-contract.md)
7. 迁移规则：[engineering/migration-policy.md](engineering/migration-policy.md)
8. 服务器变更规则：[engineering/server-change-policy.md](engineering/server-change-policy.md)
9. 源文档盘点：[operations/source-document-inventory.md](operations/source-document-inventory.md)
10. M1 来源盘点：[operations/inventory/README.md](operations/inventory/README.md)
11. M9 服务器最终迁移 Runbook：[operations/m9-server-cutover-runbook.md](operations/m9-server-cutover-runbook.md)
12. 服务器目录规划：[operations/server-directory-plan.md](operations/server-directory-plan.md)
13. 报告索引：[reports/README.md](reports/README.md)
14. 当前迁移计划：[plans/active/monorepo-migration.md](plans/active/monorepo-migration.md)

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
- 迁移状态放在
  [plans/active/monorepo-migration.md](plans/active/monorepo-migration.md)。
- 源文档盘点放在 [operations/](operations/)。
- 不把 secrets、live runtime state、成员画像原文或私聊原文写进文档。
- 用路径和 disposition 关联来源文档，不把大型历史文档整篇复制进来。
