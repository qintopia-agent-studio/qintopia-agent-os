# PMS Operations

Owner：PatrickLiveCool。风险：high。岸岸专用客房能力包，默认关闭。

本包拥有 Skill、Hermes Plugin 和受控 Green PMS HTTP 客户端。Agent
OS 共同服务拥有可信人员或工作账号、当次授权、WorkItem、确认、执行键和恢复记录；PMS 继续拥有订单、库存、资金和住宿事实。包内不建人员库或订单副本，不提供任意 URL/HTTP 工具。

2026-09-26 起，共用企微工作账号可直接成为岸岸业务授权主体，不需要关联自然人。管理者在现有工作台从可信观测候选登记账号，按物业和
`operations.json` 逐项授权；`operator`/`admin` 只标记该物业业务角色，不授予 Agent
OS 全局配置权限。审计保留稳定来源账号与消息，不声称识别共用账号背后的员工。见[账号授权数据设计](../../runtime/postgres/docs/data-design/2026-09-26-anan-work-account-access.md)。

## 实施契约

契约基线：Agent OS `b42db0d`；Green PMS
`0254fbabdda0b76b56370248f2ad24e44e4e950a`。业务和 T01—T14 以[开发计划](../../docs/plans/active/anan-pms-event-integration.md)为准。

- 模型输入只含业务参数，人员及消息来自 Hermes 官方 WeCom 宿主。服务证据绑定具体方案，模型不能提交批准布尔值。
- 当次权限取人员或工作账号当前授权、岸岸支持的命令与 PMS `/me`
  权限交集；每次调用重新检查。账号不因 Token 有 WRITE 获权。
- 技术预览和人类确认分开；同一方案有效明确决定不重复批准。方案变化、过期或撤权使旧执行依据失效。
- 每个动作先持久化意图和唯一执行权，再调用外部 API；网络调用不占长数据库事务。报价也独立记账。
- 超时或丢失响应保留 UNKNOWN，先查原键，必要时 resolve 封存原键；不能自动换键重做。部分成功不整单重放。
- 对外字段按用途裁剪，群内不返回证件和完整联系方式。原始响应、请求体及凭据不进入错误日志。
- 收款 feed 按已批准的源 head 快照原子保存基线与同值检查点；事件和任务原子落盘后才推进游标。真实 head 与主动投递待 PMS 项目提供，不能用时间过滤代替，也不能把补拉轮询称为主动通知。事件不授权业务写入。
- 申请只由统一来源入口分派，按可靠来源版本修订；同名、同金额不是关联依据。欢迎沿用既有 WorkItem 消费者。

## 本地配置与验证

`GREENPMS_BASE_URL`、`GREENPMS_API_TOKEN`
是本包绑定名，不是 Hermes 内置配置。本地模式要求 PMS/Foundation 的 LOCAL 开关同时为 1，只连接 loopback
HTTP 模拟服务。

本批候选生产入口要求对应 PRODUCTION 开关同时为 1、拒绝 LOCAL 混用，并在读取 Profile 外私有凭据前检查官方工具隔离配置。固定 HTTPS、受信会话与共享 live
broker 已有本地接线；完整隔离、生产安装与业务验收尚未完成。

收款补拉和申请回读的独立宿主入口也须先通过同一候选隔离检查，再打开私有文件；申请配置文件须额外纳入实际容器的受保护路径。

当前不提供独立宿主豁免：缺少受管岸岸运行条件时拒绝读取和外联，Bridge 保留原待回读事项。Bridge 的真实宿主安装接线仍待完成，单独打开 PRODUCTION 开关不能替代它。

尤其不能把 terminal
Docker 的模拟结果外推为浏览器隔离通过。当前默认关闭，待完成[生产接入计划](../../docs/plans/active/anan-production-rollout.md)中的剩余路径和授权配置后才能启用。

生产受管配置须位于完整 root 所有且不可被其他用户改写的真实目录链。末级目录及
`config.yaml`
不可公开读取，服务受控组可读、不可写；不接受符号链接、硬链接或其他文件类型。

当前官方核心的部分浏览器路径直接读取 Profile 原始配置，未应用受管覆盖。安装时须保留既有 Profile 内容，仅将已审阅的浏览器隔离字段与受管配置对齐；预检同时检查两处及实际容器。只有受管文件正确、Profile 仍使用默认路由时必须拒绝执行，不能退回宿主浏览器。

MCP stdio 运行在宿主，不继承 terminal
Docker。候选策略在读取凭据前核对有效 native 配置、已加载 portable 配置及当前 Profile 与全局注册的 MCP 工具；存在未经隔离核验的启用项或残留工具时拒绝。

这只是调用时的检查：官方配置调和可能先启动 MCP，插件加载失败也会继续启动 Gateway。

因此还须在安装真实凭据前验证固定插件加载及实际工具集合，并维持配置来源不可变和正确的启动顺序。

当前尚未完成该生产启动接线，不能用一次成功预检或工具拒绝代替进程隔离。

显式隔离模拟脚本分别为 `tests/production_isolation_journey.py`（官方工具与 Docker）、
`tests/browser_isolation_journey.py`（离线路由）、
`tests/browser_container_journey.py`（真实容器文件系统与 CDP 发现，无页面自动操作）及
`tests/managed_policy_journey.py`（一次性 Linux 容器内的真实目录权限）。它们的环境前提、实际结果与范围见接线报告；默认单元套件不代表这些检查已经执行。

官方 Generic webhook 的签名、route 解析、事件构造与入站 hook 回归在
`tests/workitem_official_hook_journey.py`，不进入默认单元套件。显式设置
`ANAN_HERMES_SOURCE` 为干净的官方核心 checkout（脚本固定核对提交
`d337b736aa1e8ebecfab043842d13e4a2d2f48a3`），用具备该核心运行依赖的 Python
3.12 执行；缺少来源或版本不符会失败。默认单元仍覆盖 WorkItem
body、会话权限及六字段投影。

`tests/workitem_broker_journey.py`
是隔离 PostgreSQL 集成模拟的显式子进程接头，由 Sidecar 测试驱动：PMS 接收事务提交后，实际发送内部 HMAC
webhook，经官方 HTTP handler、入站 hook 和 ContextVar，再由原插件传输回读真实 Unix
broker 的同一 WorkItem。该联合模拟明确替代安装与隔离前提；完整 Linux 工具隔离另有独立记录，没有模型、渠道外发或 PMS 业务写入。

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

## 申请回读

四老师统一 Bridge 的申请模式调用
`application_intake.py`，不注册模型工具。固定宿主配置包含
`QINTOPIA_APPLICATION_LOCAL_ENABLE=1`、`QINTOPIA_APPLICATION_BINDING`、
`QINTOPIA_APPLICATION_RESOURCE_ALIAS`、`QINTOPIA_APPLICATION_LOCAL_CONFIG` 和
`QINTOPIA_APPLICATION_LOCAL_API_TOKEN`，沿既有独立 HOST_TOKEN 访问 broker。

本地 JSON 配置仅指向显式 loopback 模拟 HTTP，包含 base_url、base_token、table_id、resource_alias、fields、consent_value，可选 withdrawn_value。

fields 将 name/nickname/phone、consent 及可选 arrival/nights/room_type/occupation/interests/status 映射到许可字段名。

只读固定记录，不跟随重定向、不取附件、不输出字段正文或凭据。

生产模式需同时设置 `QINTOPIA_APPLICATION_PRODUCTION_ENABLE=1` 和
`QINTOPIA_FOUNDATION_PRODUCTION_ENABLE=1`，且两个 LOCAL 开关均不得为 `1`。
`QINTOPIA_APPLICATION_PRODUCTION_CONFIG` 固定指向 `/etc/qintopia/` 下的宿主私有0600
JSON，`QINTOPIA_APPLICATION_RESOURCE_ALIAS` 与 JSON 的 `resource_alias`
必须一致。JSON 只接受 `resource_alias`、`base_token`、`table_id`、`fields`、
`consent_value`、可选 `withdrawn_value`、受管应用 `app_id/app_secret` 与独立
`foundation_host_token`；目录不得被其他用户改写，文件不得为链接或进入 Profile。宿主只向固定
`https://open.feishu.cn` 取得租户 Token 并 GET 单条 Base 记录，然后通过原 Unix
broker 保存观察、投影候选并交接欢迎。非 200、权限拒绝、404、响应错误或来源暂不可读保留原唤醒待回读，不解释为撤回。

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
只接受宿主捕获的上下文，固定网关必须一致。本地模式沿原 PMS/Foundation/Application 三项 LOCAL 开关和 loopback 模拟。生产模式要求三项 PRODUCTION 开关均为
`1`、三项 LOCAL 均不为 `1`，先执行 `production.require()`
隔离检查，再从 Profile 外受管 PMS 凭据文件读取 Token；
`Client(..., production_enabled=True)` 固定通过 HTTPS 回读 PMS，原独立 Foundation
HOST_TOKEN 只经私有 Unix broker 使用。群确认刷新继续保留原可信消息上下文与完整池
`open/GET/save/failed/status` 流程；响应 `local_only`
必须与实际模式一致。号码和读取 token 只经私有宿主请求，不返回模型、群、日志或普通工具；停用调用入口即可停止回读，保留服务端原事项与恢复状态。真实生产凭据、群确认和通知未在本地验证。

## 生产传输准备（尚未启用）

受控执行层可显式构造 `Client(..., production_enabled=True)`，只接受固定
`https://pms.qintopia.cn`
目标，以系统信任库验证 TLS 证书和主机名。生产模式与本地模拟模式互斥，不接受任意 origin、明文 HTTP、代理或重定向。

此入口不注册新工具，不改变插件的本地开关，不把生产凭据注入 Hermes。凭据隔离、可信宿主认证、安装接线与用户触发验收完成前不得生产启用。HTTP 只用于本地模拟，生产传输测试使用模拟连接，不调用真实 PMS。

## 私有凭据文件输入

`QINTOPIA_PMS_CREDENTIALS_FILE` 指定 Profile 外的绝对 JSON 路径。

文件包含三个键：`GREENPMS_API_TOKEN`、`QINTOPIA_FOUNDATION_TOKEN`、`QINTOPIA_FOUNDATION_HOST_TOKEN`。

三个值必须不同。Foundation Token 为 32–256 个可打印非空白 ASCII 字符，PMS
Token 为 16–512 个。

文件仅当前运行用户所有、权限 0600、无软链接或硬链接。父目录仅 root 或当前用户所有，其他用户不可写。

不要把文件放在共享临时目录、Profile、Skill 或工具挂载目录。

启用文件输入后，三个同名环境变量必须全部移除，包括空值。文件错误直接拒绝，不回退环境变量；内容和路径不进入工具错误。

未配置文件时，仅原有显式本地模拟模式可沿用环境变量。此输入不启用生产插件，也不证明工具隔离。

官方远端工具会接受 Skill 声明的自定义环境变量，因此生产不能把这些 Token 放进 Gateway 环境。

部署须证明模型文件工具、Skill 凭据挂载和远端执行均不能读取私有路径，且不会自动挂载 Gateway 的宿主目录。
