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

Cargo 场景须逐个登记完整测试函数名并带
`--exact`；不要用模块前缀代替单个场景。新增或修改目录后先运行 `pnpm test:list` 和
`pnpm test:harness`，确认目录能被发现且不会因零匹配误报通过。原生模块回归仍可单独使用模块过滤，证据须与统一目录的执行区分。

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
`broken` 或 `skipped` 时，不得说“全部通过”；环境阻断在 Allure 中显示为带原因的
`broken`。

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

## 本地环境与已知总检查阻断

在 macOS 运行前核对 Rust 1.96、项目指定 pnpm、Python 3.12 与 Allure 所需的 Java
17 路径；系统 Python 3.9 不能替代 QiWe 测试所需的新版解释器。

若含空格的工作目录在 `test-collect-release-deploy-results.mjs` 出现 `results.json`
缺失，核对收集脚本的 CLI 入口是否错误比较 URL 编码路径；不要通过跳过该断言或更改用户工作目录宣称总检查通过。记录总检查失败，继续运行受影响模块的原生验证。详见
[2026-09-18 本地检查记录](../reports/2026-09-18-workbench-password-login.md)。

复用演示库不能代替一次性全仓 PostgreSQL 环境。若 Space 配置测试报告
`administrator set exceeds the supported ceiling`，先只读核对 `person_memberships` 中
`qintopia` 的活跃 `owner/admin`
数量；历史合成 fixtures 可能累积超过 32。保留演示库、权限上限和失败证据，使用新的本机实例与
`qintopia_test`
重跑完整数据库 tier，不删除旧成员或放宽权限校验。见[2026-09-23 约定生命周期检查记录](../reports/2026-09-23-rule-lifecycle-local-acceptance.md)。

一次性数据库的 URL 通过 runner 的 loopback 校验，不保证通过图像 staging 的固定 URL 哈希校验。若最后
`operations-control-plane-apply-smoke.sh` 报
`database URL hash is not in the reviewed allowlist`，记录此前 Rust
PG 用例结果和该 smoke 的失败边界；不要改 allowlist、复用 staging 凭据或将总 tier 记为通过。需由维护者另行评审测试契约。见[岸岸本地验收记录](../reports/2026-09-24-anan-pms-local-acceptance.md)。

## 欢迎渲染与跨平台任期回归

PostgreSQL 欢迎集成使用业务包固定 PNG 夹具，继续执行真实进程协议、产物登记、审批与版本失效检查，不依赖 Pillow 或字库，不声称验证了实际排版。业务测试自行准备本地 HTTP 开关；生产默认门禁不变。

手工验证合成排版时，按 `workflows/resident-welcome/requirements-visual.txt`
安装依赖，并运行该包的 `visual-tests`；本地演示继续显式设置
`QINTOPIA_WELCOME_RENDER_PYTHON`
为装有 Pillow 的绝对解释器路径。真实制卡能力仍属于四老师，迁给阿靓需独立联调。

任期精度回归使用固定纳秒输入，覆盖 PostgreSQL 微秒读回后的编辑行为。
