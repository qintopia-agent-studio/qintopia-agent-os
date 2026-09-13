# Codex 测试操作指南

本文件是给 Codex 的执行契约。用户只需描述想验证的功能或说“运行全部业务测试”，Codex 负责定位、判断覆盖、补齐、运行和解释结果。

## 固定流程

### 1. 定位功能

先用用户的原话、同义词和业务名称搜索代码与文档，再阅读根
`AGENTS.md`、`docs/engineering/change-routing-index.md`
和目标包 README/manifest。定位到功能入口、所属 Agent/skill/workflow/MCP/runtime，以及可能受影响的上下游。

对定时任务同时查 schedule、wrapper 和 worker；对 Agent 间协作查 work
item/任务交接；对 MCP 查 server、transport、工具 schema 和授权边界；对 channel 发送查 adapter、请求记录、回执/状态更新路径。

### 2. 发现已有测试

先执行统一运行器的 list 子命令（仓库脚本等价入口为 `pnpm test:list`）：

```bash
node tools/testing/run.mjs list
```

然后用 `rg` 搜索目标模块的 `tests/`、`fixtures/`、`#[test]`、`unittest`、`pytest`
和已有测试命令。打开相关测试，确认它是否真的驱动目标入口和断言了目标行为；不能只按文件名或清单别名判断。

把覆盖判断分为四种：

| 判断       | 处理                                         |
| ---------- | -------------------------------------------- |
| 已覆盖     | 先运行对应测试，再报告结果                   |
| 部分覆盖   | 保留已有测试，补充缺失的输入、边界或长链步骤 |
| 未覆盖     | 按规范新增最小可执行场景并登记               |
| 被环境阻断 | 说明依赖和原因；不得改报为通过               |

### 3. 补充测试

用户明确要求测试而仓库没有覆盖时，Codex 应在当前改动范围内补充测试，不只提出建议。先读
[测试编写规范](authoring.md)，选择最接近的既有 fixture/helper，并让真实业务模块处理事件、路由、授权、状态和发送决策。

测试替身只放在外部边界：脚本化模型、loopback HTTP/socket 服务、需要时的本地 NATS/MCP
transport。替身对未声明请求必须失败，不能返回默认成功。测试预期必须独立于被测实现，不能调用同一段业务逻辑生成答案。

场景完成后登记 `tools/testing/catalog.json`，保证 feature ID、scenario
ID 唯一，executor 只能是
`pytest`、`unittest`、`cargo`、`node`，目标和 argv 必须是仓库内固定值且能被选择器发现。不要创建只会通过的占位测试。

### 4. 运行测试

先运行目标范围，再根据风险扩大。命令入口和 `run.mjs` 子命令一一对应：

```bash
pnpm test:doctor
pnpm test:business -- --feature <feature-id>
pnpm test:report
```

等价的直接调用是
`node tools/testing/run.mjs doctor`、`node tools/testing/run.mjs business --feature <feature-id>`
和 `node tools/testing/run.mjs report`。每次运行的状态、日志和附件在
`.local-testing/runs/<run-id>/`；需要传给子进程的数据库连接和资料目录分别是
`QINTOPIA_SIDECAR_DATABASE_URL`、`QINTOPIA_TEST_RUN_DIR`。

用户要求全部回归时使用：

```bash
pnpm test:business
pnpm test:report
```

数据库、runtime、workflow、channel、MCP 或测试框架改动应继续运行受影响包的原生检查；例如早报包测试、`pnpm test:qiwe`、`pnpm test:sidecar`
或 change-routing index 列出的命令。不要把业务套件通过当作所有原生检查都已通过。

执行器必须识别零测试、未知选择、超时、非零退出和缺依赖。默认不自动重试。全量结果中出现必需的
`broken`、`skipped` 或 `blocked` 时，不得说“全部通过”；`blocked`
在 Allure 中显示为带原因的 `broken`。

### 5. 报告结果

报告时用业务语言说明：测了哪些场景、哪些正在执行/已结束、通过和失败数量、失败发生在哪一步、预期/实际是什么、使用了哪些模拟边界、哪些仍未验证。附上报告运行 ID 和本地报告入口。

区分证据和诊断：测试断言、请求记录和数据库状态是证据；“可能由某处导致”是诊断结论，不能伪装成测试直接证明的事实。模型脚本通过只证明系统能处理该 completion，不证明真实模型一定会作出相同决定。

## 不可违反的边界

- 不连接生产数据库、生产 MCP、真实 channel 或生产凭证。
- 不为通过测试而删除断言、放宽授权门禁、跳过失败或修改预期。
- 不在场景中复制一份路由、审批、幂等或发送决策。
- 不用固定 `sleep` 猜测异步完成；等待明确状态并设置最大步数和超时。
- 不把模拟请求接受、API 成功或本地回执描述为真实用户送达。
- 不把未登记、未执行或依赖缺失的功能说成已验证。

## 用户请求示例

用户说“测试二花早报是否会重复发送”时，Codex 应定位早报文本发送链，找到或新增“重复执行不重复发送”场景，运行 feature 定向测试并打开报告。用户说“运行全部业务测试”时，Codex 应运行默认全量集合，并报告整个已登记集合的状态，而不是只运行最近修改的模块。
