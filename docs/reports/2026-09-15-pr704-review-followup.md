# PR #704 Reviewer 建议处理

审查基线：`f26c5b1`，Reviewer Guide
[5673994730](https://github.com/qintopia-agent-studio/qintopia-agent-os/pull/704#issuecomment-5673994730)。

完整指南有两项建议，未提交独立 review 或 inline
comment。自动审查说明了覆盖范围不足，其绿色检查不是全量代码正确性的证明。

## 欢迎案例准入滞留：采纳

`state.rs` 创建案例后，冲突更新漏写
`admitted`，导致基线/回读先建立的案例不能被后续合格 live 事件准入。更新为已有准入与本次准入的逻辑或，继续保留人工暂停。现有事件类型、来源模式、启用、重建和时间窗检查不变。

回归覆盖 baseline、historical
correction、非准入事件、禁用来源、重建中来源、时间窗外事件、合格 live 事件，以及之后的回读和重复事件。断言案例 ID/数量稳定、人工暂停不被清除、准入不绕过身份门禁。原实现在合格 live 事件处稳定失败，运行
`20260915030722-17cc64220f`；用例已加入本地业务清单和原生 CI 模块执行入口。

## 顶层研究目录：采纳

两份材料从 `研究学习/` 迁入
`docs/architecture/study-notes/`，保留内容及历史定位，增加目录说明和架构索引，更新设计文档/验收报告引用。修复相对链接与作者机器绝对路径，外部 Green
PMS 文档改为来源说明，不制造无法打开的本地链接。

## PostgreSQL CI 失败

[失败 job](https://github.com/qintopia-agent-studio/qintopia-agent-os/actions/runs/34923153516/job/104235536352)
在权限到期测试期望 `denied` 时得到
`confirmation_required`。重新任职可能因期限变化创建替代 appointment；测试却修改了原 appointment。不同主机的时间精度使原测试偶然通过。修复为显式续期，操作返回的当前 appointment，再验证到期立即失权。未降低权限断言，未修改生产授权逻辑。

## 验证与范围

- 学习笔记的全部仓库内 Markdown 链接目标存在。
- `pnpm test:business`：30/30 通过、0 跳过，运行
  `20260915030754-1256439379`；新增准入和权限到期回归均通过。
- `pnpm test:harness`：8/8 通过。
- Rust 1.96.0 全功能单测：865 通过、61 个专项 ignored；新 ignored 用例已由业务套件执行。
- 无默认功能和全功能两组 Clippy（`-D warnings`）：通过。
- `pnpm check:light`：通过。
- 本次未改 UI 行为；先前 Chrome 插件验收阻塞仍如实保留，未使用 Playwright。
- 无迁移变更、外部发送、生产操作或新工具栈。
