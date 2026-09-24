# 岸岸运行接入契约

本地欢迎使用 `workflows/resident-welcome/scripts/local_agent_runtime.py`，固定加载
`fixtures/agents/anan/welcome_runtime.py` 并实际注册、执行任务工具。运行边界为
`local_scripted_agent_runtime`，不等于真实 Hermes 会话或 LLM 理解验收。

Rust 从已锁定的持久 WorkItem 构造一次性 stdin：任务引用、固定 Agent、能力、来源和受控业务输入放在
`trusted_context`。模型工具参数只能是空对象，不能携带人、租户、目标群、任务或任意地址。岸岸
`request_card` 输出交给阿靓的制卡请求，`prepare`
输出逐内容计划；宿主必须实际消费输出，继续复核当前资格、授权、规则和版本。阿靓与二花通过各自独立的本地注册处理器返回 PNG 或转发指令。

每次返回绑定 call、task、agent、operation、工具名及插件版本，记录原始输入、输出和插件源码的 SHA256。宿主核对绑定与哈希后记录持久回执。子进程不继承数据库或 token 环境，不提供网络、子进程或直接外发工具。该入口处理已受信任宿主生成的模拟任务，不是任意调用方的授权接口。

通用人类对话工具仍使用
`skills/person-foundation`，固定调用 Agent，网关 session 提供来源身份，Rust 解析 Person、tenant、范围及当前授权；它与欢迎任务宿主上下文分别验证。

`fixtures/agents/anan/profile.template.yaml` 是本地测试模板，不是负责人已建立的 Hermes
Profile 配置。 `runtime/hermes/profile-registry.yaml`
不加入岸岸；生产 Profile、服务、凭据和重启 allowlist 无修改。本地恢复从 WorkItem 和结果事件开始；撤销、换栋、内容更正和未知结果按欢迎流程的已有版本及恢复策略处理。

## 正式能力身份与 PMS 本地接入

2026-09-24 正式身份登记采用 `runtime.management: unmanaged`，表示尚未由 Agent
OS 接管发布，不表示服务器没有岸岸。能力由 `skills/pms-operations/`
自身 Plugin 注册；正式受管安装仍需单独评审，不直接安装或覆盖用户现有 Profile。

本地必须启用模拟开关、可信 WeCom
host 证据、隔离共同服务 socket 和模拟 PMS 服务；缺少任一项拒绝操作。宿主观察基于 Hermes
`pre_gateway_dispatch` 与
`gateway.session_context`；只有真实模型/渠道试点才能完成 T14，不以包内替身替代。
