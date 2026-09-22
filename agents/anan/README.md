# Agent：岸岸（anan）

岸岸是客房协作智能体。本批登记欢迎编排的最小独立 Agent 包，状态为本地开发用 draft；没有生产 Hermes
Profile。

## 职责与接入

- 消费有资格的实际入住人、有效申请与住宿及逐群成员状态，推进同一欢迎事项。
- 通过持久 WorkItem 向阿靓（`huabaosi`）申请卡片，取得受控 Artifact 与申请 Base 附件回执。
- 将具体卡片、文字和规则版本交给目标楼栋的二花，接收审核、逐目标发送、等待或失败结果。
- 使用与工作台及二花相同的可信身份、通用职责授权、目的及受众裁剪服务；执行前重新核验。

跨 Agent 协作必须绑定 WorkItem、Artifact 及结果事件。不得调用四老师入口冒充岸岸，不以函数命名或注册声明证明已完成运行接线。本地欢迎运行消费者和合成适配证据由
[resident-welcome](../../workflows/resident-welcome/README.md) 管理；通用可信工具接口由
[person-foundation](../../skills/person-foundation/README.md) 管理。

本地欢迎运行器固定加载本包 `welcome_runtime.py`，经 `register_tool` 实际调用
`qintopia_welcome_request_card` 和
`qintopia_welcome_prepare`。前者返回交给阿靓的制卡请求，后者返回卡片和文字的逐项计划；宿主消费这些输出继续真实渲染或准备，并保存绑定任务的调用和源码哈希。

可信业务输入来自 Rust 已锁定 WorkItem 的一次性管道，模型工具参数必须为空。详细协议见[Runtime 契约](runtime-notes.md)。这是本地脚本 Agent 运行时，真实 Hermes/LLM 仍未验收。

## 生产边界与恢复

`hermes-anan` / `hermes-gateway-anan.service`
只是预留命名契约，不进入当前部署白名单、重启规则或七个生产 Profile 清单。本批只用合成资料与测试上传/发送适配器，完整订房、支付、真实 LLM 和真实渠道验收另行记录。

本地停用入口或停止消费者可撤回接入；保留持久任务及审计，未知外部结果不得自动重试。未来启用生产须独立评审不可变发布、Profile 与凭据绑定、渠道权限、回滚及真实效果。

## 验证

```sh
pnpm agents:check
pnpm registry:check
pnpm agents:profile-bundles:check
pnpm deploy:runner:check
python3 -m unittest discover -s agents/anan/tests -v
pnpm test:business -- --feature person-foundation
```
