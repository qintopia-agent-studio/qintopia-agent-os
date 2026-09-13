# 本地业务测试编写规范

## 选择测试边界

用最小边界验证可观察行为：

| 测试层        | 真实内容                                                | 替换内容                            |
| ------------- | ------------------------------------------------------- | ----------------------------------- |
| 单元测试      | 解析、校验、状态规则、序列化                            | 无或最小依赖                        |
| 契约测试      | channel/MCP 请求结构、权限和错误映射                    | loopback 或固定协议 fixture         |
| 业务场景      | 真实路由、work item、Agent runner、审批、幂等、状态更新 | 模型 completion、外部网络和平台响应 |
| 进程/传输专项 | 真实进程、NATS、MCP transport、socket、恢复             | 外部平台和凭证                      |
| 模型评估      | 真实模型和 prompt/profile                               | 外部业务数据、真实发送              |

不要用内存数据库证明 PostgreSQL 的事务、锁、claim、幂等或隔离行为。业务场景使用 Docker
PostgreSQL 和真实 migration；NATS 只有需要验证传输语义时才启动。时间和调度入口使用可控时钟或显式 tick，不等待真实日期。

## 场景最小结构

每个业务 feature 在 `tools/testing/catalog.json`
登记；每个场景至少包含稳定 ID、名称、别名/模块归属、executor、固定 target/argv、依赖、超时和边界说明。executor 只能是
`pytest`、`unittest`、`cargo`、`node`；target 和 argv 由仓库维护并校验，场景文件不能携带任意 shell 命令。场景数据通常放在所属业务的
`fixtures/`，描述：

```json
{
  "id": "duplicate-send",
  "initial_state": "合成的审批通过早报任务",
  "inputs": ["第一次执行发送 worker", "再次执行同一任务"],
  "external_responses": ["模拟 QiWe 成功响应"],
  "expect": {
    "send_requests": 1,
    "final_state": "当前实现定义的发送完成状态",
    "duplicate_side_effects": 0
  }
}
```

字段名称可以随实现演进，但语义不能丢失：初始状态、输入序列、外部响应、最终结果和禁止发生的副作用都要可追溯。每次运行的证据写入
`.local-testing/runs/<run-id>/`；通过 `QINTOPIA_TEST_RUN_DIR`
传给测试进程。PostgreSQL 连接通过 `QINTOPIA_SIDECAR_DATABASE_URL`
注入，必须是运行器创建的本地测试库。

## 编写步骤

统一按照：

```text
准备隔离环境和合成数据
  → 注入事件、tick、审批、回执或故障
  → 驱动真实模块/worker/进程
  → 等待明确状态、事件或回执
  → 断言最终状态、关键过程和禁止副作用
  → 保存证据并清理
```

场景应同时检查“应该发生”和“不应该发生”。例如重复事件只产生一份业务任务，越权调用为零，审批前发送请求为零，未确认送达时不进入已送达状态。只约束业务必要关系，如“授权先于调用、回执先于完成”；不要锁死不影响业务的内部函数顺序。

异步驱动必须有明确结束条件、最大步数和超时。测试失败、超时或中断后仍要留下可定位的运行轨迹，并清理本次容器、端口、socket 和临时目录。并发共享数据库状态的场景默认串行，只有隔离充分且有证据时才并行。

## 替身和数据

- 模型替身用固定响应脚本；未知 completion 或未声明工具请求直接失败。
- channel 假服务校验方法、目标、正文/媒体引用、调用次数和响应类型；可显式模拟限流、超时、断连、无回执和重复回执。
- MCP 测试需要验证工具发现、schema、身份、授权、响应解析和断线；直接调用函数不能替代 transport 契约测试。
- Agent 间通信要经过真实任务派发、领取、执行和结果关联；不能只 mock 一个
  `call_agent_b()`。
- fixture 使用合成身份、内容和 channel ID，不复制生产消息、token、URL 或原始画像。
- 预期文件独立于生产实现；不能用被测函数计算 expected。

## 报告和命名

Allure 步骤只记录实际观测到的动作。每个失败至少提供场景、步骤、预期、实际和相关请求/状态证据；没有步骤级 instrumentation 时只能报告测试组级状态，不得从日志猜造步骤。

命名使用业务语言，例如
`erhua-morning-brief/duplicate-send`，避免只写函数名。一个场景应表达一个主要业务风险；正常、重复、越权、超时/故障可以作为独立场景或明确参数变体，但每个变体都必须有独立可读的预期。

修改发送、定时、MCP、Agent 协作或状态边界时，至少新增一个相关失败路径。修改 prompt/profile 时，补充真实模型评估；确定性脚本场景不能替代该评估。

## 早报参考案例

参考案例采用二花早报**文本发布**链：生成内容 → 创建状态 → 审核 → 最终确认 → 文本发送 worker
→ 模拟 QiWe 请求 → 状态和审计更新。它默认不走卡片/图片上传，也不触发真实 Hermes
08:10 定时器；本地模拟响应通过只表示发送适配器契约正确，不表示真实 channel 送达。

案例至少覆盖：正常文本发布、重复执行不重复发送、缺少最终确认时零发送、请求已送出但响应断开时不自动重发/不误报成功。案例说明中要写明当前实现定义的状态语义，不能引入系统尚未承诺的回执含义。

## 清单接入细节

以 `tools/testing/catalog.json` 的实际条目为模板，schema 为同目录
`catalog.schema.json`。pytest 参数变体通过 `target.nodeid` 精确选择，例如
`test_text_delivery[duplicate-send]`。 `target.path`
始终相对仓库根；旧 unittest 的包导入目录通过 `target.pythonpath` 显式登记（例如
`["skills/qiwe"]`），运行器不继承用户的 `PYTHONPATH`。Cargo 目标必须提供完整
`target.filter` 并使用 `--exact`，禁止零匹配报告成功。

新增后依次运行
`pnpm test:harness`、单场景、所属 feature、`pnpm test:business`。每个 PostgreSQL 场景独立 seed，不依赖上一个场景的数据。无需新增通用 fixture
DSL；参考案例的参数表已经表达输入事件、外部响应和独立预期。
