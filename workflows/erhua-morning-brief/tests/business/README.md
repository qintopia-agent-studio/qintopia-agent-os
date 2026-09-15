# 二花早报文本发布参考测试

- 全业务：`pnpm test:business -- --feature erhua-morning-brief`。
- 单个风险：`pnpm test:business -- --scenario erhua-morning-brief/duplicate-send`。
- 结果：`pnpm test:report`。

## 真实链路

合成活动/新闻/天气 fixture → `morning_brief.py` → sidecar 创建 artifact
→ 审核 → 创建发送请求 → 最终确认 → send-ready → Rust 文本 worker → loopback QiWe →
PostgreSQL 状态、内容 hash 和审计事件。

`test_text_delivery.py` 中参数表分别定义正常、重复发送、缺最终确认和响应断开的预期。Rust
bridge 是 `cfg(test)` 下的精确 ignored 测试，调用与生产共用的
`run_apply_core`；仅允许本次带归属标记的本地测试库和 loopback 假服务。生产命令仍走原来的授权和网络门禁。

| 场景                 | 必须观察到的结果                                                |
| -------------------- | --------------------------------------------------------------- |
| success              | 假服务收到一次正确请求，数据库 completed，内容绑定获批 artifact |
| duplicate-send       | 再次执行无请求，审计事件不增加                                  |
| missing-confirmation | 无最终确认不能进入发送状态，外部请求为零                        |
| ambiguous-send       | 收到请求后断连，状态 failed 且记录不确定结果，再执行不自动重发  |
| cron-contract        | 登记的早报计划是 08:10、指定 wrapper、no-agent 和 wecom         |

## 可复制的规范

1. 用所属业务既有 fixture 生成合成输入，新增风险在参数表里表达。
2. 调真实命令或模块，外部边界用严格本地替身；不要复制业务判断。
3. 用 Allure step 标明准备、注入、驱动、验证；保存实际请求与状态证据。
4. 同时断言最终状态、内容、请求次数和禁止副作用。
5. 在测试清单登记独立 ID / 别名 / 参数 nodeid / 依赖 / 真实与模拟边界。

本例不使用真实模型，不测试卡片/图片、真实 Hermes tick、跨 Agent MCP
transport 或用户送达。 `completed`
是当前文本 worker 接受成功 API 响应的状态，不代表异步送达回执。cron-contract 检查登记表及 wrapper 存在及 Bash 语法，完整安装/wrapper 契约仍由既有 deploy 测试验证。
