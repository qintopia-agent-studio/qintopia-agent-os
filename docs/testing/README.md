# Agent OS 本地业务测试

这组文档规定 Agent
OS 的本地业务测试方式。它服务于两类使用者：不需要理解测试框架的协作方，以及负责定位、补充和运行测试的 Codex。先看
[Codex 操作指南](agent-guide.md)；需要编写场景时再看
[测试编写规范](authoring.md)，需要理解边界时看 [架构说明](architecture.md)。

## 先做什么

基础框架完成后，默认使用下面的命令。它们是仓库级稳定入口，具体实现和版本以
`package.json` 为准；调试时也可以直接调用 `node tools/testing/run.mjs <subcommand>`。

| 目的                     | 命令                                             |
| ------------------------ | ------------------------------------------------ |
| 检查本机依赖和环境       | `pnpm test:doctor`                               |
| 准备专用测试 Python 环境 | `pnpm test:setup`                                |
| 查看已登记业务和场景     | `pnpm test:list`                                 |
| 运行全部本地业务测试     | `pnpm test:business`                             |
| 运行某个业务的全部场景   | `pnpm test:business -- --feature <feature-id>`   |
| 运行单个场景             | `pnpm test:business -- --scenario <scenario-id>` |
| 打开最近一次 Allure 报告 | `pnpm test:report`                               |
| 打开指定运行的报告       | `pnpm test:report -- --run <run-id>`             |
| 检查清单和运行器自身     | `pnpm test:harness`                              |

第一次使用时执行 `pnpm test:setup` 和
`pnpm test:doctor`。全部业务测试默认一次选择所有已登记场景；一个场景失败不会阻止其他独立场景运行，命令最后仍会以非零状态退出。运行资料写入
`.local-testing/runs/<run-id>/`，不会写入 Git 跟踪目录。

## 这套测试验证什么

业务场景执行真实的业务代码，数据库使用本地 Docker
PostgreSQL。模型 completion 和外部平台使用合成输入或本地假服务。

v1 默认只准备 PostgreSQL。NATS、MCP
transport 与真实模型评估是后续专项场景，当前不自动启动。场景支持
`pytest`、`unittest`、`cargo`、`node` 四种执行器。

基础设施依赖锁定为 pytest `8.4.2`、`allure-pytest` `2.16.0`、Allure CLI `2.43.0` 和 Java
17。数据库测试通过 `QINTOPIA_SIDECAR_DATABASE_URL` 指向本次本地实例，运行资料目录通过
`QINTOPIA_TEST_RUN_DIR` 传给子进程；两者都由运行器生成和保护，不能指向线上资源。

本地确定性测试不需要生产凭证，也不向真实 channel 发送消息。报告中的“发送成功”只表示模拟 channel 收到了符合契约的请求并返回了脚本响应，不表示真实用户已收到消息。真实模型质量、平台权限和实际送达属于单独的评估或受控线上验证。

框架分为四层：包内单元/契约测试、跨模块业务场景、进程和传输专项、真实模型评估。现有 Rust
`cargo test`、Python `unittest`
和 Node 检查继续保留；本地业务套件负责把长链路放在一个可重复入口中，不要求迁移所有旧测试。

## Codex 如何使用

用户可以直接说“测试二花早报是否会重复发送”或“运行全部业务测试”。Codex 必须先按
[Codex 操作指南](agent-guide.md)
查找功能、阅读已有测试并判断覆盖情况，再选择已有测试或补充测试。找到一个同名测试不等于本次行为已经覆盖。

测试清单是检索入口，不是覆盖事实的替代物。测试实现、fixture 和运行结果才是最终依据。新增业务场景要登记到
`tools/testing/catalog.json`，让定向和全量命令都能发现它。

## 结果怎么看

Allure 报告按业务和场景展示状态、步骤、耗时、预期/实际结果及合成证据。状态含义如下：

| 状态      | 含义                                                            |
| --------- | --------------------------------------------------------------- |
| `passed`  | 断言和必要步骤均通过                                            |
| `failed`  | 业务断言失败                                                    |
| `broken`  | 测试运行器、子进程或环境错误                                    |
| `skipped` | 明确未执行；必须场景被跳过时不能宣称全量通过                    |
| `blocked` | 运行摘要中的环境阻断状态；Allure 中映射为 `broken` 并附阻断原因 |

每次运行使用独立的结果目录。报告之外还会保留机器可读摘要和经过筛选的调用轨迹，便于 Codex 根据“哪一步、预期是什么、实际是什么”解释失败。

## 当前案例边界

第一条参考案例是二花早报的**文本发布链**：合成输入 → 真实早报业务逻辑 → 本地状态/审核/确认 → 文本发送 worker
→ 模拟 QiWe 请求。它不覆盖卡片或图片上传路径，也不把真实 Hermes
08:10 定时器当作本地场景已经执行。它默认使用模拟 channel，因此不能证明真实 channel 的权限或送达。

新增场景时必须明确哪些代码真实执行、哪些外部边界被模拟，以及哪些能力仍未覆盖。详细字段和断言规则见
[测试编写规范](authoring.md)。
