# PMS Operations

Owner：PatrickLiveCool。风险：high。岸岸专用客房能力包，默认关闭。

本包拥有 Skill、Hermes Plugin 和受控 Green PMS HTTP 客户端。Agent
OS 共同服务拥有可信人员、当次授权、WorkItem、确认、执行键和恢复记录；PMS 继续拥有订单、库存、资金和住宿事实。包内不建人员库或订单副本，不提供任意 URL/HTTP 工具。

## 实施契约

契约基线：Agent OS `b42db0d`；Green PMS
`0254fbabdda0b76b56370248f2ad24e44e4e950a`。业务和 T01—T14 以[开发计划](../../docs/plans/active/anan-pms-event-integration.md)为准。

- 模型输入只含业务参数，人员及消息来自 Hermes 官方 WeCom 宿主。服务证据绑定具体方案，模型不能提交批准布尔值。
- 当次权限取人员当前授权、岸岸支持的命令与 PMS `/me`
  权限交集；每次调用重新检查。人员不因 Token 有 WRITE 获权。
- 技术预览和人类确认分开；同一方案有效明确决定不重复批准。方案变化、过期或撤权使旧执行依据失效。
- 每个动作先持久化意图和唯一执行权，再调用外部 API；网络调用不占长数据库事务。报价也独立记账。
- 超时或丢失响应保留 UNKNOWN，先查原键，必要时 resolve 封存原键；不能自动换键重做。部分成功不整单重放。
- 对外字段按用途裁剪，群内不返回证件和完整联系方式。原始响应、请求体及凭据不进入错误日志。
- 收款 feed 按已批准的源 head 快照原子保存基线与同值检查点；事件和任务原子落盘后才推进游标。真实 head 与主动投递待 PMS 项目提供，不能用时间过滤代替，也不能把补拉轮询称为主动通知。事件不授权业务写入。
- 申请只由统一来源入口分派，按可靠来源版本修订；同名、同金额不是关联依据。欢迎沿用既有 WorkItem 消费者。

## 本地配置与验证

`GREENPMS_BASE_URL`、`GREENPMS_API_TOKEN`
是本包绑定名，不是 Hermes 内置配置。本批客户端只允许显式 `QINTOPIA_PMS_LOCAL_ENABLE=1`
的 loopback
HTTP 模拟服务。正式 HTTPS/Token 隔离及 Profile 发布待独立评审；不读取用户现有 Profile。

```sh
python3 -m unittest discover -s skills/pms-operations/tests -v
pnpm registry:check
pnpm check:pr:auto
```

客户端测试使用 loopback 外部边界；共同服务和跨仓联合验证另行记录，不能互相替代。停止插件入口即可停用，保留全部持久事项、原请求键和回执；未知结果先核对，停用不撤销已提交 PMS 事实。

实际测试、环境限制和剩余验收见[本地验收记录](../../docs/reports/2026-09-24-anan-pms-local-acceptance.md)。

## 身份、确认与发布边界

正式 `anan`
身份按[已获批的 A 专项方案](../../docs/plans/active/anan-pms-contract-proposal.md)登记为
`runtime.management: unmanaged`。已修改获批的 schema、两检查器及既有验证入口；没有修改 resolver、restart
rules、安装 payload 或生产 Profile registry。未受管表示 Agent
OS 尚未接管发布，不表示用户服务器不存在岸岸 Profile。新增业务包仍受发布路径门禁约束。

自然确认如“确认预订”“确认登记这笔收款”“确认已到店，办理入住”，只绑定同人、同会话内唯一且未过期的当前对应方案。有多个候选时拒绝猜测，先明确所办方案。内部确认编码不会要求客服复制粘贴。完整明确交办在 PMS 预览后逐项核验；目前自动复用支持普通单住客、无会员权益、无额外安排的整间预订，以及完整当笔收款指令：

> 请为「模拟住客」（昵称「小张」）预订「101」整间，2026-09-24入住，2026-09-26离店，1位住客，总价240.00元，企微渠道，无会员权益，不加其他安排。

当笔收款也可明确交办“请为订单「订单号」登记银行转账收款120.00元，流水号「已核对流水号」。”（也支持企微方式）。只有订单、流水、金额、方式与最终预览完全一致才复用；不接受长期授权或到账事件替代人的确认。

该受限语句不是一般业务必填表；普通自然交办照常查询和准备，完整性或效果有歧义才呈现方案供自然确认。不把模型对自由文本的解释当批准，不以单纯放宽时间判断复用旧确认。取消/暂停会阻止旧方案执行，恢复后重新预览；更正先取消未执行方案，再准备新方案。原方案在途或结果未知时先恢复，不能用取消或更正换键重做。

本地 PMS HTTP 子链、真实 broker 联测、官方 Hermes
ContextVar 与真实渠道/模型验收分别记录；前几者不等于 T14 生产验收。

## 本地支付接收与补拉

支付接收固定为 `POST /api/v1/ingress/pms/events`，仅在本地 collaboration
listener 显式启用时挂载。需同时设置 `QINTOPIA_PMS_EVENTS_LOCAL_ENABLE=1` 和
`QINTOPIA_FOUNDATION_LOCAL_ENABLE=1`，并配置
`QINTOPIA_PMS_EVENT_BINDING`、`QINTOPIA_PMS_EVENT_SOURCE`、`QINTOPIA_PMS_EVENT_PROPERTY`、
`QINTOPIA_PMS_EVENT_KEY_ID`、`QINTOPIA_PMS_EVENT_KEY_FILE`。密钥文件为绝对路径、普通文件、仅属主可读写；不使用生产密钥或把内容提交到 Git。

宿主补拉入口为 `python payment_feed.py`，不注册模型工具。它沿既有 Unix broker 使用独立
`QINTOPIA_FOUNDATION_HOST_TOKEN`，真实读取 PMS
head 后原子初始化 baseline/checkpoint，后续每次先读持久游标。 `GREENPMS_BASE_URL` 与只读
`GREENPMS_API_TOKEN`
指向本任务本地 PMS；默认单次最多 10 页，达到页数上限返回 caught_up=false，再次运行从持久位置继续。失联不自行重试财务写入，重启不覆盖基线。推送不会推进补拉 checkpoint。

事件仅形成待核对事项；准备关联收款方案前实际查询对应 billId，核对类型、可用状态、流水引用、金额与 WECOM 方式。实际 PMS 预览和当笔人类确认仍必需。

MATCHED、REFUND、历史事件不新建收款催办；无可靠联系人时保持待联系，不发送消息。本地同 UID 宿主隔离仍不等于生产强隔离；正式配置与真实渠道尚未启用。
