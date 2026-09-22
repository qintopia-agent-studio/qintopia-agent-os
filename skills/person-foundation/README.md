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
`context`、`save_rule`、`remember`、`history`、`dispatch`、`task_status`。拒绝或传输故障均明确返回状态；写入结果未知不自动重试。

`dispatch` 成功仅说明持久受理，`task_status`
的实际事件才说明后续执行状态。跨 Agent 只传 WorkItem 与受控 Artifact 引用。

对话理解使用 Hermes 自有 `ctx.llm.acomplete`。本地测试的脚本 completion
adapter 明确记录为
`scripted_model`，不代表真实 LLM 理解通过。模型仅产生结构化意图；有歧义、建议、引用或未能确定的内容应返回澄清，不生成写入操作。持久结果返回后才可描述保存成功。

## 验证与恢复

```sh
python3 -m unittest discover -s skills/person-foundation/tests -v
python3 -m unittest discover -s agents/anan/tests -v
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
