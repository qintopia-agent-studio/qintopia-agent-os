# Person Foundation 可信工具

本包是人员共同底座的 Hermes 工具适配层，由服务端持久身份、授权、记忆、知识规则和 WorkItem 服务执行业务。二花通过既有 qintopia-tools 插件注册；岸岸使用自己的最小插件，仅允许任务上下文与结果查询。默认关闭，无生产接线。

## 请求边界

模型只提交业务参数。注册方固定 Agent，host 提供可信网关会话；`gateway_id`
来自运行环境，`sender_id`、`chat_id`、`message_id` 来自 QiWe 的
`trusted_qiwe_turn_session()`。模型无法提交 actor、Person、tenant、scope、认证信息或执行地址。Rust 以网关来源绑定解析 Person 与范围，检查当前身份版本、任职和授权，不能回退 trainer
allowlist 或旧 welcome_grants。

本地 Unix socket 仅在 `QINTOPIA_FOUNDATION_LOCAL_ENABLE=1`
时可调用，路径与 token 由受控进程环境的 `QINTOPIA_FOUNDATION_SOCKET` 和
`QINTOPIA_FOUNDATION_TOKEN` 提供，模型不可覆盖。`QINTOPIA_FOUNDATION_GATEWAY_ID`
为服务端登记的合成网关。token 不进入工具 schema、回复或审计。单个请求为 UTF-8
JSON 一行，限 256 KiB，操作固定为 `person_foundation_tool`，协议版本为 1；响应是
`{ok,result}` 或 `{ok:false,error:{code}}`。服务端还核验同 UID 与本地 token。

业务工具包括
`context`、`save_rule`、`remember`、`history`、`dispatch`、`task_status`，以及舍长工作台工具
`workspace`、`change_knowledge`、`delegate_review`、`welcome_setting`、`welcome_approve`。拒绝或传输故障均明确返回状态；写入结果未知不自动重试。

`dispatch` 成功仅说明持久受理，`task_status`
的实际事件才说明后续执行状态。跨 Agent 只传 WorkItem 与受控 Artifact 引用。

对话理解使用 Hermes 自有 `ctx.llm.acomplete`。本地测试的脚本 completion
adapter 明确记录为
`scripted_model`，不代表真实 LLM 理解通过。模型仅产生结构化意图；有歧义、建议、引用或未能确定的内容应返回澄清，不生成写入操作。持久结果返回后才可描述保存成功。

## 舍长工作与页面同步

先用私聊 `workspace`
读取本范围真实知识版本、欢迎事项和当前在住代理候选，不能凭姓名猜 Person 或版本。`change_knowledge`
使用与 UI 相同的生命周期命令，支持保存、停止、取消未来版本；`rule` 使用
`change_rules`，设施事实、文化和经验使用
`confirm_knowledge`。Markdown 正文只作为内容，不能授予权限或改变范围。

`delegate_review` 必须指定当前在住候选与截止时间；更换或撤销携带 `expected_id`。省略
`delegate`
表示撤销。临时代理仅审核欢迎内容，原授权变化、代理过期、身份失效或退住时不再通过。代理不获得设置、发送或组织管理权限。

`welcome_setting`
明确区分仅本次（`case_ref`）与持续安排，支持期限和恢复先审。舍长的自主指令本身就是业务决定；上层发布确认、入住、入群和内容使用许可仍独立核验。此工具只保存，不发送。`welcome_approve`
绑定展示过的产物、哈希、目标与版本。

这些工具可供 Hermes 原生对话调用；当前可信 socket 仍为本地合成门禁，不代表生产微信已接线。Web 对话仍明确标注固定例句，MD 导入与临时代理有直接表单入口。验证工具注册和业务服务不等于验证真实模型理解。

## 验证与恢复

```sh
python3 -m unittest discover -s skills/person-foundation/tests -v
python3 -m unittest discover -s fixtures/agents/anan/tests -v
pnpm skills:qintopia-tools:check
pnpm registry:check
```

本批隔离实例及其受控 socket 已启动、进程环境已设置合成网关与本地认证绑定后，可执行真实 Python
SDK → Rust broker → PostgreSQL 校验：

```sh
QINTOPIA_FOUNDATION_SMOKE_ENABLE=1 python3 skills/person-foundation/tests/local_broker_smoke.py
```

该 opt-in 校验只接受 `synthetic-`
网关，写入并停止虚构人员的回复偏好，验证读回、条件更正、旧消息不恢复、跨范围与身份伪造拒绝；仅输出计数及边界，不输出 token、身份引用或业务内容。

关闭本地 flag 或停止 socket 服务即可停用。已写本地审计和版本不删除；恢复时先查持久状态，未知结果不重放。当前不提供生产 Profile、真实登录提供方、真实上传/外发或真实 PMS/飞书写入。

## 发布归属

二花现有 qintopia-tools 注册入口按需加载本包，因此本包源文件变更归属既有 `hermes-erhua`
重启目标。该登记经负责人确认，只复用原有部署流程，不新增服务或启用工具；本地开关、可信网关与生产接入门禁保持独立。岸岸测试替身位于 fixtures，真实岸岸接入须先核对 Runtime。
