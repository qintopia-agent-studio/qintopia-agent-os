# Agent：岸岸（anan）

岸岸是配合小客服和刘珊操作 Green
PMS 的客房协作智能体。负责人已于 2026-09-24 确认在 Hermes
Runtime 建立岸岸；该实际 Profile 的模型、工具、权限与发布归属尚待后续联调核对。本目录保留能力文档，欢迎测试替身位于
`fixtures/agents/anan/`，本批不接管真实 Profile 部署。

## 职责与接入

- 消费有资格的实际入住人、有效申请与住宿及逐群成员状态，推进同一欢迎事项。
- 通过持久 WorkItem 向阿靓（`huabaosi`）申请卡片，取得受控 Artifact 与申请 Base 附件回执。
- 将具体卡片、文字和规则版本交给目标楼栋的二花，接收审核、逐目标发送、等待或失败结果。
- 使用与工作台及二花相同的可信身份、通用职责授权、目的及受众裁剪服务；执行前重新核验。

跨 Agent 协作必须绑定 WorkItem、Artifact 及结果事件。不得调用四老师入口冒充岸岸，不以函数命名或注册声明证明已完成运行接线。本地欢迎运行消费者和模拟适配证据由
[resident-welcome](../../workflows/resident-welcome/README.md) 管理；通用可信工具接口由
[person-foundation](../../skills/person-foundation/README.md) 管理。

本地欢迎运行器固定加载 `fixtures/agents/anan/welcome_runtime.py`，经 `register_tool`
实际调用 `qintopia_welcome_request_card` 和
`qintopia_welcome_prepare`。前者返回交给阿靓的制卡请求，后者返回卡片和文字的逐项计划；宿主消费这些输出继续真实渲染或准备，并保存绑定任务的调用和源码哈希。

可信业务输入来自 Rust 已锁定 WorkItem 的一次性管道，模型工具参数必须为空。详细协议见[Runtime 契约](runtime-notes.md)。这是本地脚本 Agent 运行时，真实 Hermes/LLM 仍未验收。

## 生产边界与恢复

`hermes-anan` / `hermes-gateway-anan.service`
是测试模板中的预留名称，不据此推断真实服务配置。本地替身已移至
`fixtures/agents/anan/`，不登记为生产 Agent，也不进入部署白名单或重启规则。本批只用模拟资料与测试上传/发送适配器，完整订房、支付、真实 LLM 和真实渠道验收另行记录。

本地停用入口或停止消费者可撤回接入；保留持久任务及审计，未知外部结果不得自动重试。未来启用生产须独立评审不可变发布、Profile 与凭据绑定、渠道权限、回滚及真实效果。

下一阶段见[岸岸客房协作开发计划](../../docs/plans/active/anan-pms-event-integration.md)与
[开发交接](../../docs/plans/active/anan-pms-development-handoff.md)：先实现岸岸专用 Skill/插件工具与 PMS 客户端，完成直接对话办理；再接 PMS 收款及飞书申请，复用同一办理流程。

事件和新共享中转服务不是对话能力的前置。正式能力包位于
[skills/pms-operations](../../skills/pms-operations/README.md)，Agent 身份以 unmanaged 登记。已通过模拟身份、官方 Hermes
ContextVar、真实本地 broker 和未修改 Green
PMS 的联合办理子集；完整 T01—T14 的实际状态见本批验收报告。真实 Bot、Livecool 模型、生产 PMS 和欢迎送达尚未验收，不把 Gateway 连通或本地运行器当作已上线。

## 验证

```sh
pnpm agents:check
pnpm registry:check
pnpm agents:profile-bundles:check
pnpm deploy:runner:check
python3 -m unittest discover -s fixtures/agents/anan/tests -v
pnpm test:business -- --feature person-foundation
```
