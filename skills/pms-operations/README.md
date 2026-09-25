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

`anan`
已按[生产接入方案](../../docs/plans/active/anan-production-rollout.md)登记为 managed，现有服务的重启目标为
`hermes-anan`。发布包含本包源码；不自动安装插件或打开本地开关访问真实 PMS。核心入口、凭据隔离、生产接线与真实验收保持独立门禁。未知包路径仍被拒绝。

自然确认如“确认预订”“确认登记这笔收款”“确认已到店，办理入住”，只绑定同人、同会话内唯一且未过期的当前对应方案。

有多个候选时拒绝猜测，先明确所办方案。

内部确认编码不会要求客服复制粘贴。

完整明确交办在 PMS 预览后逐项核验；目前自动复用支持普通单住客、无会员权益、无额外安排的整间预订，以及完整当笔收款指令：

> 请为「模拟住客」（昵称「小张」）预订「101」整间，2026-09-24入住，2026-09-26离店，1位住客，总价240.00元，企微渠道，无会员权益，不加其他安排。

当笔收款也可明确交办“请为订单「订单号」登记银行转账收款120.00元，流水号「已核对流水号」。”（也支持企微方式）。只有订单、流水、金额、方式与最终预览完全一致才复用；不接受长期授权或到账事件替代人的确认。

该受限语句不是一般业务必填表；普通自然交办照常查询和准备，完整性或效果有歧义才呈现方案供自然确认。

不把模型对自由文本的解释当批准，不以单纯放宽时间判断复用旧确认。

取消/暂停会阻止旧方案执行，恢复后重新预览；更正先取消未执行方案，再准备新方案。

原方案在途或结果未知时先恢复，不能用取消或更正换键重做。

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

## 本地申请回读

四老师统一 Bridge 的本地申请模式调用
`application_intake.py`，不注册模型工具。固定宿主配置包含
`QINTOPIA_APPLICATION_LOCAL_ENABLE=1`、`QINTOPIA_APPLICATION_BINDING`、
`QINTOPIA_APPLICATION_RESOURCE_ALIAS`、`QINTOPIA_APPLICATION_LOCAL_CONFIG` 和
`QINTOPIA_APPLICATION_LOCAL_API_TOKEN`，沿既有独立 HOST_TOKEN 访问 broker。

私有 JSON 配置仅指向显式 loopback 模拟 HTTP，包含 base_url、base_token、table_id、resource_alias、fields、consent_value，可选 withdrawn_value。

fields 将 name/nickname/phone、consent 及可选 arrival/nights/room_type/occupation/interests/status 映射到许可字段名。

只读固定记录，不跟随重定向、不取附件、不输出字段正文或凭据。

生产 Feishu URL 当前被拒绝。

回调只唤醒授权回读。

新的 read_token 使旧读响应失效；

CAS 冲突后重读来源，禁止换 token 重交旧结果。

内部 revision 与源 last_modified_time 分开，后者仅是观察证据。

姓名/昵称/电话身份指纹和普通内容摘要分开；

字段未变不会抹掉已确认 Person，身份字段变化则失效本申请的关联并留审计。

404或暂时不可读保留原状态；

撤回需实际状态字段证据。

共同服务复用 welcome_applications，并持久派发岸岸办理和四老师运营两个原事项。事件不批准业务，完成/取消/执行中的事项不因来源更新自动重做；欢迎需可靠案例及公共确认入口。

成功回读后，宿主以原记录引用调用 `reconcile_welcome`。只有现存申请、案例和有效 PMS
occupant 身份关系一致，且当前住宿事实与范围匹配，才幂等生成独立 `welcome_event`
并进入公共欢迎确认。缺关联保留原两事项等待；模型不提供 case、scope 或产物引用，不自动认人或发送。交接失败保留 Bridge 唤醒，下次先重读来源再恢复同事项。

## 来源与办理阶段汇合

prepare 可沿原申请事项办理。

相同事项的订房或相同 quote 重复准备复用原动作；UNKNOWN、执行中或完成后不能通过换 quote、换消息或重启另开订单。

明确 NOT_EXECUTED 后的新修订是新动作，旧结果保留；同人另次住宿仍用独立事项和 quote。

已有对话订房可通过 `qintopia_pms_link`
关联来源：插件读取宿主指定的实际订单，展示来源摘要和当前订单；员工回复“确认关联”后，宿主绑定当前唯一提议。

订单版本或申请修订变化需重审，不要求员工手输内部 UUID。

多提议有歧义时不批准，“取消关联”只清除本人当前对话的待确认提议。

原完整有效的订房/当笔收款交办仍可直接复用；关联不新增业务或财务批准。

原申请、支付事件和回执保留各自来源，后续阶段沿已确认的原办理事项记账并核对同一订单。来源撤回或修订不撤销 PMS 成功事实、不重放已完成阶段。仅本地启用；真实渠道自然交互仍需验收。

## 人工 PMS 接手与完成核对

handoff 先读订单基线，再登记人工接手；在途或 UNKNOWN 先沿原键恢复。重复接手不覆盖基线。人工接手后不能用暂停再恢复绕回执行；先核对已办事实。

入住、退房、取消、改期、续住、缩住、换房使用接手之后的唯一不可变历史，完整效果须与原预览一致且仍是当前订单版本；旧历史或后来改动不认作本次成功。有流水收款核对新增事实、原流水、方式/金额/币种，冲正、转出和多义保持待核对。

无流水现金展示唯一符合原方案的新事实，员工回复“确认这笔人工收款是本事项的办理结果”后重新回读，再记原动作人工完成。完整可信回报准确指明同笔事实且验证通过时直接采用，不重复确认。确认只记录已有事实，不登记新收款、不赋予重放权。

CREATE_ORDER 允许人工采用不同合法方案。

通过订单查询选具体候选，再用 reconcile 的可选 order 参数核对；员工不必手输内部 ID。

工具并列原方案、当前订单和差异，回复“确认此订单承接原订房事项”后重新读当前版本与关键事实。

过期、变化、越权、缺事实或其他事项冲突都保持等待；多提议不猜测。

审计明确区分原效果的历史核验、人工采纳具体收款与人工采纳订单，不伪造原 PMS 回执，不从订单承接推导身份、收款或到店确认。

停用仍保留基线、原预览、执行键与人工决定；不删除后重放。

## 阶段5本地工作提醒契约

提醒沿现有客房 WorkItem，状态保存在 metadata，尝试写入 work_events；不新建日程、业务事实库或迁移。Hermes
cron 只唤醒单次有界宿主入口，不由模型调用，也不安装或启用真实 cron。

有效约定使用既有知识规则键 `anan.pms.reminders`，限定当前物业绑定作用域。

规则显式提供跟进类别、必需申请字段、群受众、星期与每日时段、UTC 偏移、首次等待、重复间隔及可选升级时限与升级群。

未配置、规则失效或当前授权不足时仅保留待办。

规则不提供任意渠道地址；

目标必须同时属于岸岸有效主动受众和当前作用域群绑定。

宿主协议为
`context/observe/claim/validate/settle`：context 返回当前范围内事项及可信来源引用；

observe 接收宿主实际回读，核对订单/物业或申请来源版本；

claim 合并同一事项仍待办类别，并持久取得唯一发送权；

validate 在模拟发送前重验规则、事项、受众及内容摘要；

settle 只接受原 claim 的具体模拟回执。

丢失回执或宿主退出留下 UNKNOWN，不自动重发或换键。

缺信息只来自可信申请回读与规则必需字段交集；待收款只按 PMS 当前合同与净实收差额；待到店只按实际订单状态和规则规定时间，到账不能证明入住。来源被可靠关联后沿 canonical
WorkItem 汇合；无可靠关联不猜测合并。完成、撤回或取消停止相应提醒；办理暂停及明确暂缓抑制提醒，恢复需再次实际回读。提醒完成不伪造 PMS 业务完成。

首批只提供显式本地模拟渠道，身份固定 `anan`，不回退其他 Profile、不调用真实发送。

群内容只列待办类别与事项引用，不含住客姓名、电话、证件或金额。

停止宿主入口即可停用，保留事项、暂缓、尝试与 UNKNOWN。

共同服务 PG 测试、插件测试及本地模拟适配证据分别记录，真实群与模型验收另行授权。

### 宿主配置与规则形状

`QINTOPIA_PMS_REMINDERS_LOCAL_ENABLE=1`、`QINTOPIA_PMS_REMINDER_BINDING`
固定当前绑定；同时要求既有 PMS/Foundation 本地开关。`QINTOPIA_PMS_REMINDER_SIMULATED_OUTBOX`
是仅属主访问的绝对目录，适配器以原 claim 排他创建模拟消息和回执，不接受网络发送配置。入口
`python reminder_host.py`
每次最多轮转扫描100个既有事项；扫描位置记在同一 WorkItem，崩溃不会永远饿死后续事项。不注册新计时器，不创建真实 Hermes
job。

规则 content 使用下列明确字段，无默认周期或目标群：

- `collaboration`：当前有效岸岸 hospitality 工作连接，需 `read_business`
  与自主 proactive 受众；读订单还须精确 `pms.read.order` 授权。
- `group`、`kinds`、`required_fields`：群引用、`missing_information/collection/arrival`
  的子集及所需申请字段（name/nickname/arrival/nights/room_type/occupation/interests）。手机号新提交必填不转为历史提醒；本入口不接受 phone 催补规则。
- `weekdays`：1—7；`start_minute/end_minute`：本地日内左闭右开时段；`utc_offset_minutes`：明确固定偏移；`arrival_due_minute`：到店日期开始催办的分钟。
- `initial_delay_seconds/repeat_seconds`：从首次观察到当前待办开始等待、成功发送后的最小间隔；`escalation`
  为 null 或 `{after_seconds,group}`，升级群也重新核对受众。

`qintopia_pms_reminder_snooze`
使用当前可信人员的客房办理授权，参数为当前 binding、原 work_item 和明确 until；

它只延后提醒，不取消订单。

合并来源保留原回执，继承暂缓与最后发送时间；

来源尚有未知发送时 canonical 事项不另发同类提醒。

规则修订、物业绑定变化、撤权或目标群变化均不能复用旧发送计划。

### 申请候选投影薄接线

同一次可信 HTTP 读取可在宿主内保留规范化白名单 values；先将不含 values 的原 Observation 保存到004，再以独立 HOST_TOKEN 调
`welcome_source_projection`：封套
`schema_version=1`，arguments 只有宿主固定 binding、本次保存返回的 application 与 fields。

随后照常 reconcile_welcome。

候选投影失败或回执不明返回 projection_unconfirmed，已保存申请与岸岸/四老师原事项不回滚、不伪称保存失败；

无有效许可或已撤回不上传资料。

响应不含个人字段，候选存储不代表已确认身份。

最终资料决定为新申请手机号必填、手机号为主要匹配依据，姓名昵称辅助；

历史不追补、不催补、不阻断已有业务。

共享手机号及冲突继续走既有确认，不自动合并人员。

取消此前中间证件方案，不新增证件字段、不改004摘要或普通工具脱敏。

当前入口是回读已提交资料，不能把首次回读误作新提交并追罚历史；

当前唯一入住申请表的手机号已只读核实为 required=true 且可见；实际空号提交拦截仍待来源侧外部验收。

### 欢迎手机号私有回读

`stay_contacts_host.py` 提供不注册模型工具的 `StayContactsHost`。宿主以固定 PMS
READ 凭据和独立 Foundation HOST_TOKEN 运行； `synchronize(work_item)` 沿
`welcome_stay_contacts`
的 open/save/failed/status 读取服务端给出的完整候选池，最多200个订单，不按姓名截取候选。

仅上传订单 id、property_id、整数 version 与完整当前 occupants 的 id/phone。

形状错误或读取失败记为失败，不能填 null 或空集合伪装缺电话；当前 phone=null 原样保留，不回退历史号码。

基础线群宿主的固定挂点为
`WelcomeHost.callback(refresh_contacts=contacts.refresh_contacts)`。
`refresh_contacts(work_item,presentation)`
使用同一原可信消息上下文调用 open(refresh=true,presentation)，逐订单重新 GET/save 后返回脱敏 status。

只有服务端 status=complete 且 scan_complete=true 才能继续原确认。

读取代次不充当人类确认依据；相同业务依据继续原确认一次，实际变化由服务端拒绝。本模块不实现人员匹配、关系确认、消息发送或额外重试。

`from_environment(trusted_context)`
只接受宿主捕获的上下文，固定网关必须一致；要求现有 PMS/Foundation 本地开关及
`QINTOPIA_APPLICATION_LOCAL_ENABLE=1`。凭据沿原 GREENPMS 配置，仍仅显式 loopback 模拟。号码和读取 token 只经私有宿主请求，不返回模型、群、日志或普通工具；停用调用入口即可停止回读，保留服务端原事项与恢复状态。

## 生产传输准备（尚未启用）

受控执行层可显式构造 `Client(..., production_enabled=True)`，只接受固定
`https://pms.qintopia.cn`
目标，以系统信任库验证 TLS 证书和主机名。生产模式与本地模拟模式互斥，不接受任意 origin、明文 HTTP、代理或重定向。

此入口不注册新工具，不改变插件的本地开关，不把生产凭据注入 Hermes。凭据隔离、可信宿主认证、安装接线与用户触发验收完成前不得生产启用。HTTP 只用于本地模拟，生产传输测试使用模拟连接，不调用真实 PMS。
