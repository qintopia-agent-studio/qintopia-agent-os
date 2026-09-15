# 统一人员与入群欢迎 V1：共同接口契约及 Agent OS 实施清单

日期：2026-09-09。状态：Agent
OS 已获本地开发授权并开始 A1—A4；未部署。PMS 已完成 P-01—P-08 只读回传并获授权独立开发。实际完成与待联调项见[本地验收记录](../../reports/2026-09-09-resident-welcome-v1-local-acceptance.md)。

本文件直接承接[盘点评估](unified-person-identity-foundation-assessment.md)已确认的模型、责任和业务规则，只补实现所需的边界。与
[Green PMS 交接清单](green-pms-event-integration-handoff.md)共同交付。本文标为“拟定”的接口、字段和默认参数是 V1 技术选择，不表示源码或生产已具备。

## 1. 交付范围与不变量

- 复用现有 Postgres
  `persons.id`、身份/事实/任务/产物/审计；无申请的住客与员工也可有 Person。
- 人员任职、智能体训练授权和协作职责是整个 Agent
  OS 共用的基础设施，一处设置、多流程使用。入住欢迎消费这些设置，不建立自己的人员任命与长期权限体系。具体要求见第 5.3 节。
- 欢迎资格是有效申请与 PMS 本次实际入住人/住宿可靠关联。预订占库存后可制卡审核、发预告；实际入住后，目标群成员与审核/授权都就绪即正式欢迎。没有一小时计时。
- 小客服服务的「秦托邦的小伙伴（新）」与对应楼栋群分别判断、分别发布；历史群名称与实际渠道群的对应由受控配置核验，避免重复目标。员工内部群不要求住客加入。
- 客房流程协调，阿靓复用四老师 Pillow 制卡，附件回写申请 Base；二花负责社区/楼栋发布和私聊舍长审核，四老师负责内部工作群协作/分发，小满不作前置依赖。
- PMS 只输出住宿权威事实及变化；Agent
  OS 维护人员关联、权限、事件和 WorkItem。首个欢迎闭环不需要向 Agent 开放 PMS 写命令。
- V1 包含人工匹配/审核与异常接管，未定例外进入待处理，不由模型猜测。完整画像后台、跨社区推广和所有智能体业务不属于本次最小范围。

## 2. 现有实现复用与确切缺口

本轮新增核对以下源码，不重复外部系统盘点：

| 现有对象/入口                | 已有契约                                                                                     | 实施约束/缺口                                                                                  |
| ---------------------------- | -------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| persons / aliases            | UUID、偏好称呼、别名；初始迁移约 298 行起                                                    | 原 ID 保留；不得按名字建另一套 identity                                                        |
| channel_identities           | 唯一键 `(platform, channel_user_id, chat_id)`                                                | 加企业/应用/中转作用域与类型的兼容设计；旧未注明来源记录不能自动归入某企业                     |
| person_memberships           | 唯一键 `(person_id, community_key, role)`                                                    | 现有约束不能直接表达同角色多楼栋/多次任职；保留原兼容投影，补有范围/有效期的任职及授权关联     |
| work_items                   | 唯一 idempotency_key、parent、available_at、claimed_by、claim_expires_at、attempts；七种状态 | 复用表与领取机制；需验证原子去重、过期 claim 的 fencing，以及新类型白名单                      |
| artifacts                    | 任务/内容 hash 唯一性、审核状态/人/时间                                                      | 复用产物，补申请/逐人 Stay/存储映射、审核版本和撤销；不要凭 metadata 约定假装有唯一约束        |
| operations.rs                | 已有 group_message_request、text_announcement_request、能力白名单和确认门禁                  | 欢迎编排/卡片类型未在已读白名单中；注册新类型或明确复用映射，不能直接塞任意 task type          |
| Operations Control Plane MCP | README/manifest 指明通用三个入口仍是 dry-run wrapper                                         | 若接此入口，补受控 sidecar 写入；不能把预览当持久创建成功，也不由此否定其他业务专用 apply 能力 |
| Qintopia Collab MCP          | call_agent 已返回迁移提示，不启动另一个 Hermes                                               | 不能恢复自由文本直调作为欢迎入口                                                               |
| Feishu Base skill            | 现有只读活动/设计记录的白名单工具                                                            | 新申请读取/欢迎附件写入需要独立范围与契约；不得复用设计 Base 目标误写                          |
| 四老师制卡                   | 已静态确认三层脚本，Pillow 核心支持 JSON/照片/输出路径参数                                   | 仅采用渲染/必要映射。旧扫描、姓名去重、直接发送、本地私密回执不能原样带入；旧 dry-run 会写文件 |

源码入口：

- [控制面迁移](../../../runtime/postgres/migrations/202606300007_operations_control_plane.sql)
- [身份迁移](../../../runtime/postgres/migrations/202606240002_agent_os_data_layer.sql)
- [operations.rs](../../../runtime/sidecar/src/operations.rs)
- [Operations MCP](../../../mcp/operations-control-plane/README.md)
- [Collab MCP](../../../mcp/qintopia-collab/README.md)
- [Feishu skill](../../../skills/feishu-base/README.md)

生产迁移/版本仍待只读验收。

## 3. PMS → Agent OS 共同事件协议（拟定 V1）

### 3.1 接口与边界

| 所属     | 拟定入口                                                                  | 行为                                                                                   |
| -------- | ------------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| Agent OS | `POST /api/v1/ingress/pms/events`                                         | 一次一个事件；验证来源/范围，持久写 Inbox，返回接收回执，不等待模型/业务完成           |
| PMS      | `GET /api/v1/integration-events?propertyId=…&cursor=…&limit=…`            | 分页读取与 webhook 相同的已发布事件，用于补偿/追赶；limit 默认 100、上限 100           |
| PMS      | `GET /api/v1/integrations/agent-os/orders?propertyId=…&afterId=…&limit=…` | 所有状态订单的最小投影清单，按不可变 ID 升序；用于首次基线和周期对账，不把它冒充事件流 |
| PMS      | `GET /api/v1/integrations/agent-os/orders/:id?propertyId=…`               | 最小当前订单/住宿/实际入住人引用投影，返回版本/安排；用于执行前验证和对账差异回读      |

上述为新契约，现有 `/api/v1/orders`
的 beforeId/created_at 分页不可等价替代。PMS 可提出遵循既有路由风格的同义路径，但必须同步本文件及双方 fixture，避免两个项目各自命名。

PMS 固定一个受控订阅方，订阅物业集合与接收 URL 由服务配置指定；事件不能提供回调 URL。仅 HTTPS，固定目标和证书校验，不跟随任意重定向。Agent
OS 读取使用现有 Bearer 的最小 READ 物业范围；webhook 签名凭证与 PMS
READ 凭证分开。日志只留脱敏引用、状态、计数和错误码。

### 3.2 事件信封

字段名称是协议，以下没有真实 ID 或凭证样例。所有 ID、序号和版本用字符串，避免 JavaScript 大整数精度丢失。

| 字段                          | 必需     | 类型/定义                                                                                       |
| ----------------------------- | -------- | ----------------------------------------------------------------------------------------------- |
| schema_version                | 是       | 固定 `pms.events.v1`；破坏兼容的字段/语义更改升版本                                             |
| event_id                      | 是       | PMS 生成的不透明稳定事件 ID；重试、重放、轮询都不变                                             |
| source_instance               | 是       | 受控 PMS 实例别名，区分测试/生产；接收方从凭证配置校验，不能只信 body                           |
| property_id                   | 是       | 权威物业引用；须在该凭证和订阅范围内                                                            |
| event_type                    | 是       | 下表之一；未知类型不静默丢弃                                                                    |
| aggregate_type / aggregate_id | 是       | `order`、`member` 或 `inventory_unit` 及其源 ID                                                 |
| aggregate_revision            | 是       | 该聚合的单调十进制版本字符串；相同版本可有不同事件类型，不能比较不同聚合的版本                  |
| domain_version                | 否       | 原业务版本；仅在与 integration revision 不同且有业务版本时提供                                  |
| recorded_at                   | 是       | 源事实记录 UTC RFC3339 时间；不冒充精确事务提交时间                                             |
| effective_at                  | 是，可空 | 业务生效 UTC 时间；仅日期明确时为 null；不能用它做游标                                          |
| source_fact_ref               | 是       | command/amendment/correction 等最小权威引用，不带原因原文或完整回执                             |
| publish_seq                   | 是       | 当前物业发布序号，十进制字符串；由下述发布事务赋值，不取 order ID、业务时间或裸 bigserial       |
| refs                          | 是       | 允许 order_id、stay_id、member_id、inventory_unit_id 等源引用；不含姓名、电话、证件、聊天或照片 |
| origin                        | 是       | `live`、`historical_correction` 或 `baseline`；后两者只同步，不据此补发历史欢迎                 |

body 为 UTF-8 JSON，最大 64
KiB，拒绝重复键、异常嵌套和不支持的内容编码；未知字段不参与决策，未登记类型/版本返回契约错误。源端发布后信封字节不可变，投递变化仅在 headers；同事件不同内容必须视为冲突，不能覆盖。

### 3.3 事件与 PMS 权威写入覆盖

| 拟定 event_type                                        | aggregate      | 来源与消费目的                                                                             |
| ------------------------------------------------------ | -------------- | ------------------------------------------------------------------------------------------ |
| pms.order.created                                      | order          | CREATE_ORDER 成功提交；回读实际安排/库存资格，不把草稿/报价当预订                          |
| pms.stay.checked_in                                    | order          | CHECK_IN 或等价真实生效事实；重新判断正式欢迎                                              |
| pms.stay.arrangement_changed                           | order          | MOVE_UNIT、RESCHEDULE_STAY、EXTEND_STAY、SHORTEN_STAY 及影响当前安排的纠正；重新取当前路由 |
| pms.stay.checked_out                                   | order          | CHECK_OUT/有效完成；终止不适用未发任务                                                     |
| pms.stay.cancelled / pms.stay.no_show                  | order          | CANCEL_ORDER / MARK_NO_SHOW；终止不适用未发任务                                            |
| pms.stay.check_in_revoked / pms.stay.check_out_revoked | order          | REVOKE_CHECK_IN / REVOKE_CHECK_OUT；镜像真实状态，重新入住是否重发的例外进入人工处理       |
| pms.order.occupants_changed                            | order          | CORRECT_ORDER_OCCUPANT / MANAGE_ORDER_OCCUPANTS；使受影响身份/产物/审核重新评估            |
| pms.member.context_changed                             | member         | 资料、外部引用、删除/撤销等关联改变；只回读并校正，不直接发欢迎                            |
| pms.inventory_unit.context_changed                     | inventory_unit | 楼栋/房床层级/有效状态改变；使关联的未发任务路由失效并回读                                 |
| pms.entity.invalidated                                 | 相应类型       | 实体被权威删除/失效；含可比较的墓碑版本，不把 403/超时当删除                               |

历史补录、COMPLETE_STAY、后台纠错、会员转换等须按实际效果映射，不按命令名称盲发“新入住”。

维护/内部占用不产生住客欢迎资格。

PMS 任务需交付“每个相关写路径 → 事件/无需事件的理由”表；相同业务版本多事件按 event_type 去重并共同处理，不只保留一个。

**版本覆盖要求**：订单优先复用现有 version；成员/库存等没有完整单调版本的对象，需要 PMS 在同一事实事务维护集成修订号。

使用何种内部列由 PMS 规格决定，外部必须符合本表；资料/楼栋变化不能因为订单 version 没变就被丢掉。

成员/库存事件先更新其源镜像并使关联订单失效，不和 order revision 作大小比较。

### 3.4 签名、ACK 与重试

拟定 header：`X-QT-Key-Id`、`X-QT-Sent-At`（Unix 秒十进制）、`X-QT-Delivery-Id`（每次尝试唯一）、`X-QT-Signature`（HMAC-SHA256
hex）。密钥值只在安全配置中，模型和文档不持有。

签名原文是下列 UTF-8 串，换行统一 LF，末尾无换行；body
hash 针对收到的原始字节，不能 JSON 重序列化后验签：

```text
POST
/api/v1/ingress/pms/events
<sent_at>
<delivery_id>
<lowercase sha256 of raw body>
```

接收方从 key_id 定位允许实例/物业，常量时间比较，默认容忍 ±300 秒。重放历史事件时保持 event_id/body 不变，使用当前 sent_at/new
delivery_id；时钟窗口不会阻止业务重放。轮换允许当前/前一 key_id 的受控重叠，具体密钥配置走后续发布机制。

| 响应                                         | 含义与源端动作                                                     |
| -------------------------------------------- | ------------------------------------------------------------------ |
| 202 + `status=accepted,event_id,receipt_id`  | Inbox 与可恢复待处理状态已提交；ACK 不是已创建欢迎或已发送         |
| 200 + `status=duplicate,event_id,receipt_id` | 已存在且 body hash 相同，原记录可恢复；源端可完成此次投递          |
| 400 / 413 / 422                              | 格式、大小、事件版本/类型错误；隔离并报契约错误，不无限重试        |
| 401 / 403                                    | 签名/时钟/权限异常；暂停该配置投递并告警，保留积压，修复后受控重放 |
| 409                                          | 同 event_id 不同 hash；保留原事件，冲突隔离，不重写                |
| 429 / 5xx / 超时 / ACK body 不匹配           | 结果未获确认，保持原 event_id/body 重试，接收方负责去重            |

正常 ACK 目标低于 1 秒；不等待模型。

重试默认立即首试，后续指数退避、抖动、上限 15 分钟，遵守合理范围的 Retry-After；持续失败 24 小时转死信并通知运营控制面，事件不删除。

人工重放同 event_id，修正内容须新事件且引用原事件。

以上时长为拟定可配置运维默认，不是住客发布等待规则。

### 3.5 Outbox、发布游标及对账

1. PMS 在业务事实同一事务写 Outbox，唯一键至少覆盖 source/property/aggregate/revision/event_type/source_fact_ref。

   业务回滚不得留下可投递事件；投递网络不在业务事务内执行。

2. 独立发布器只读取已提交 Outbox。

   **建议按物业锁定发布计数器，在一个发布事务中给尚未发布行分配 publish_seq、保存不可变事件信封并推进水位**；提交后 webhook/事件查询才可见。

   后来才提交的事实取得后续发布序号，即使它的 ID 或 recorded_at 更小，也不会被游标越过。

   该锁只保护短发布事务，不串行化所有 PMS 业务命令。

3. 不能以预分配自增 ID/时间戳最大值直接作“已完整可读”水位；SKIP
   LOCKED 与高水位混用也不能跳过未发布空洞。其他实现可用，但必须证明同样的并发语义。
4. 首次 events 请求不带 cursor，返回保留窗口内事件；响应字段
   `schema_version,source_instance,property_id,events,next_cursor,has_more,head_cursor,retention_floor_cursor`。

   非空 cursor 为不透明令牌，绑定来源/物业/协议；不允许客户端自造或跨物业使用。

   无新事件也返回可继续使用的 cursor。

5. 每页先把全部事件 durable accept，才在同一 Agent
   OS 事务推进查询 cursor。业务异步处理失败不回退投递 cursor，由 Inbox 恢复；非法/未知事件未成功隔离并登记之前不得悄悄跳过。跨页重放 event_id 不变。
6. 默认补偿轮询 60 秒；周期扫描已知待发订单并对账，完整物业扫描拟定每日一次。这些是需压测调整的运维默认，无论事件还是扫描，都进入同一 Inbox/状态处理器。
7. 事件最小保留窗口拟定 30 天，未确认死信另保留。超期 cursor 返回 `410 CURSOR_EXPIRED`
   与重建要求，不能伪装空结果；最终期限在上线前容量/保留策略中确认。
8. 首次/游标过期：关闭该来源自动发布，记录 head_cursor，扫描所有状态的最小订单投影，随后消费该 cursor 后的事件并按聚合版本收敛。

   扫描是可对账的当前视图，不宣称跨页事务一致快照；补偿日志覆盖扫描期间及晚提交变化。

   扫描不完整/权限失败不得推断成员或订单已删除，缺项须回读权威墓碑或有权限的不存在结论。

9. 基线/历史补偿只建关联和状态，不自动发送。只有正式启用边界之后的合格 live 变化，或明确人工准入的本次案例，才能创建可发布流程；影子积压不自动转 live。

事件 feed 能修复 webhook 丢失，但不能发现“某条写路径根本没写 Outbox”；因此保留独立状态对账和写路径覆盖测试。生产对账完成标准包括发布积压状态、分页完整性和关联差异，不能仅看 HTTP
200。

### 3.6 最小订单投影

字段：`source_instance,property_id,order_id,order_revision,stay_id,stay_status,fulfillment_state,checked_in_at,effective_arrangement,occupants,member_ref,related_revisions,projection_hash,observed_at`。

`checked_in_at`
仅在源有准确业务时间时提供，否则 null；正式欢迎依据当前有效入住状态，不用订单创建时间替代。`effective_arrangement`
含 segment_id、房/床库存引用、building_code、日期和是否当前有效；已结束记录的 last
arrangement 不可当作当前住宿。

`occupants`
只返回实际入住人 ID、角色、active/removed 与关联所需源引用；不含姓名/电话/证件快照。member_ref 可空且不代表 occupant。`related_revisions`
包含使用到的 member/inventory
revisions，投影 hash 只覆盖这个最小投影。同版本 hash 冲突进入待校正，不覆盖派生人员资料。多人整房订单不能按 ordinal 猜测个人床位；源缺逐人分配时返回未知，由人工确认需要的住宿关系。

源查询返回明确字段白名单。不得直接把现有含私密身份、金额或完整回执的 order
detail 透传给 Agent/队列。读取/投递的技术事实仍只到 Agent
OS 服务层，模型取进一步裁剪的业务上下文。

## 4. Feishu 与 QiWe 接收规范

使用独立 ingress/认证适配器，共用下游 Inbox，不给三种来源套同一种签名假设。

| 来源            | 最小接收契约                                                                                                                             | 补偿与防循环                                                                                                                                                                  |
| --------------- | ---------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Feishu Workflow | 拟定 `/api/v1/ingress/feishu/applications`；固定受控 resource_alias、record_ref、kind、可用源 revision/event_ref；无版本时只是“回读信号” | resource_alias 服务端解析 Base/table，禁止模型指定任意目标；回读许可字段形成 application_revision/hash。新增、修改、撤回均需覆盖；忽略欢迎附件/内部同步字段自写造成的制卡触发 |
| QiWe            | 复用已有渠道入口并追加成员事件适配；允许账号/群范围内提取 provider event ref、事件类型、群引用、设备/账号来源和观察时间                  | 源不保证签名/重试；本地接入验证方案需实际平台支持，未验证不得信任 body 的账号声明。三秒内完成最小持久接收；异步名单刷新，周期对账                                             |

Feishu
HTTPClientAction 能否使用所需 header/动态版本须在切换准备阶段核对；若仅支持静态凭证，使用隔离的接收凭证、资源白名单、限流与权威回读，不声称具备 HMAC 时间签名。凭证不可嵌入 URL；CLI
user 读成功不是 Worker 应用权限证明。

QiWe 的外层 senderId 可能是群操作人，不能直接绑定新增成员。使用已验证的群名单增量/完整快照更新成员关系；只有全量完整快照才可按缺席批量标离开。userId
→
external_userid 正向转换有文档，反向“开发中”不作依赖。转换存来源账号/企业/应用证据；未知映射留待确认，不按名字合并。

仅有同意展示而无可靠申请/Stay/联系人关联，不产生卡片资格。已有授权渠道身份可在发言前关联 Person。成员事件、PMS 变化、申请修订、关联确认、审批/撤销、任职/路由变化，都可重新唤醒同一流程。

## 5. Agent OS 数据与受控服务契约

### 5.1 逻辑对象和唯一性

不在本轮创建表/迁移；实施时将下列关系映射到现有表或必要的关系扩展，重要唯一性必须由数据库/事务保证，不能只靠 prompt。

| 对象                             | 必须持久化/唯一关系                                                                                                                                                |
| -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Application                      | `application_id` UUID；来源实例/资源/record 唯一；修订、同意策略版本、字段 hash、Person 链接状态                                                                   |
| SourceIdentityLink               | 有类型的 PMS member/occupant、官方/中转账号 → Person；namespace、源引用、confirmed/pending/revoked、证据/确认者、expected_version                                  |
| StayParticipant / welcome_case   | source/property/stay/实际 occupant 唯一案例；Person 可待定；成员/申请链接修订、当前安排、baseline/live 准入；稳定 case_id 不随人员合并或昵称变化重发               |
| ConversationParticipant          | identity + conversation + membership episode，current/stale/left 和 observed_at；群作用域与自然人分离                                                              |
| RoleAssignment / Grant           | 基于 person_memberships 兼容扩展：person、角色、群/楼栋、动作、起止/撤销、版本和任命者；同角色多楼栋/复任历史可表达                                                |
| Event Inbox / source checkpoints | 传输唯一 `(source_instance,event_id)` + hash；物理来源类型与处理状态；cursor、聚合版本、重试/隔离原因。扫得的 snapshot 事件有独立来源 ID，不能与 PMS event_id 冲突 |
| Artifact / Approval              | 复用现有 id；case/application/person/link revision、content hash、storage_ref、审核人/版本/撤销；欢迎附件存申请 Base                                               |
| DeliveryAction                   | case/phase/逻辑目标/内容部分唯一；关联 WorkItem、实际渠道目标绑定版本、授权/审核版本、发送尝试/回执、execution_epoch                                               |

人工误绑定撤销先失效受影响未发任务和产物审批，再改链接投影；合并保留历史 Person/来源证据，以稳定案例/DeliveryAction 防止合并前后双发。

已发内容不因合并自动重发，交人工评估纠正。

共享工作账号须能证明实际批准者；普通 display_name 或数据库“不是 bot 字符串”的检查不足以鉴权。

### 5.2 工具的最小输入输出（拟定，非当前工具已存在）

身份从认证会话/受控 Worker 取得，调用者不能在 JSON 中自报为管理员。写工具均需 operation_id、expected_version，重放同请求返回原结果，同 key 不同请求返回冲突。

| 能力                                 | 最小输入 → 输出                                                                                  | 授权                                                                         |
| ------------------------------------ | ------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------- |
| person.search                        | query、scope、limit ≤20 → 候选 person_ref、偏好称呼、楼栋/角色、关联状态                         | 当前操作者可见范围；不返回电话/证件用于消歧                                  |
| identity.link.confirm / revoke       | 候选 link_refs、目标 person_ref、证据引用、expected_version、operation_id → link/revision/status | 指定小客服/身份管理员；同名仅候选                                            |
| role.assign / revoke                 | 已选择 person_ref、角色、逻辑范围、有效期、expected_version → assignment/revision                | 任命权限；四老师对话与最小 UI 共用服务                                       |
| welcome.case.evaluate                | case_ref、cause_event_ref → readiness/reasons/已有任务引用                                       | 受控流程 Worker；模型不能直接指定发送群                                      |
| artifact.generate_welcome_card       | case_ref、application_revision、公开字段投影引用、模板版本 → artifact_ref/hash/storage 状态      | 客房流程调用阿靓；无目标群参数，不允许上传任意路径/URL                       |
| review.decide / grant.template       | work_item/artifact/template_ref、exact_version、decision、scope → approval/grant revision        | 社区负责人内部群、二花私聊相应舍长；确认语须绑定当前具体任务，不记录整段聊天 |
| delivery.prepare / execute / resolve | delivery_ref、expected_version、claim/fence → 预览、回执或 unknown                               | 服务端验证当前资格/授权/目标；execute 不接自由文案或自由群参数               |

模板持续授权绑定任职/目标/动作/允许变量/版本，默认至撤销或离任；变更范围或披露字段需新授权。文案免审不代表卡片免审。服务端保存具体批准人，不伪造人类审批来穿过现有 final-confirmation 门禁。

### 5.3 人员、职责与智能体授权统一设置（用户纠偏，2026-09-09）

用户设置的是“小管家是谁、一栋舍长是谁、二栋舍长是谁，以及各自能做什么”，不是为入住欢迎重新配置人员。统一设置需要回答：

1. 当前操作者是哪位自然人，担任哪些岗位，任职是否有效。
2. 此人可以训练哪些智能体、哪些内容，作用于哪些楼栋、群或工作领域。
3. 智能体遇到某类事情应找谁协作、找谁审核；一栋与二栋分别解析负责人。

岗位、任职和具体授权分别表达。一个人可兼任多个岗位；岗位可提供已明确批准的职责配置，但岗位名称本身不自动授予所有训练、审批、发布或转授权能力。允许训练不等于允许修改安全边界，也不等于允许发送消息。

统一的“人员与职责设置”面向人员称呼、岗位、智能体、工作范围和职责；用户通过候选选择确认自然人，不手填数字 ID，不按姓名自动绑定。长期任职、授权、离任和交接只维护一次，各业务通过同一服务查询有效权限和负责人，不复制到各流程分别维护。

入住欢迎保留本次住宿关联、卡片/文案审核、发布状态等业务操作。某次内容是否通过审核仍单独记录；谁有权审核、应交给谁，由统一职责设置决定。没有负责人、多人职责冲突、已离任或未授权时进入人工出口，模型不能自行挑人或扩大范围。

当前实现差距：`welcome_grants`
将长期授权绑定欢迎目标，仅含 identity/appoint/review/publish；既有二花训练写入口仍使用独立训练员白名单。因此当前工作台与测试不能证明统一人员基础设施已经完成。后续应复用 Person、渠道身份及任职基础，统一授权查询和职责解析，整合既有训练入口；欢迎流程改为消费者。不可仅重命名页面，也不可把欢迎专用授权自动升级成通用授权。生产旧入口保持运行，切换另行授权。

在原 T01—T16 之外，补充以下统一基础设施验收，不以欢迎流程测试替代：

- 同一次任职设置同时供训练鉴权、协作找人和业务审核使用；新增业务不需要重新任命同一个人。
- 一栋范围不能用于二栋；某智能体/训练领域的授权不能越到其他智能体/领域；群到楼栋的关系由受控配置解析。
- 同一人兼任、同岗位多范围、离任后复任可追溯；撤销或交接后旧人的未执行权限及时失效，新人不继承历史内容审批。
- 未确认渠道身份、共享账号不能证明操作者、无负责人或职责冲突均不自动放行。
- 通用设置与欢迎内容审核分离；实际训练入口和欢迎执行门禁均查询当前有效授权，不能保留可绕过的另一套授权源。

本节只修正 Agent OS 内部职责边界，不更改 PMS 事件、查询或住宿事实协议。

#### 已确认的本栋自治与普通问候边界

业务补充来源：[一栋训练包评估第 8 节](building-1-erhua-training-pack-assessment.md#8-负责人已确认的决定与后续责任)。该评估不是第二份接口契约；以下决定延续统一设置、多业务共用的方向。

- 一栋舍长可在一栋及自身职责范围内持续训练二花，调整本栋知识、语气、文案、提醒和运营规则。本栋厨房及收费规则、基金由谁确认等安排由舍长在其范围内决定，无需逐条向总负责人申请相同授权；不能跨楼栋、智能体或未授权领域，也不能把一栋材料整包设为二花全局最高优先级提示词。
- 授权分别表达提供信息、修改规则、审核、发布和指定业务确认人。指定本栋基金确认人不等于全社区财务权限或全局任命权；群内一句话不自动变成全范围授权。
- 普通入群问候与正式入住欢迎分开。短住者或资料不完善者每次入群如何问候，由舍长与二花在本栋范围内确定，不以有效申请、PMS 入住或完整居民资料为普通问候前提，不要求总负责人逐条确定文案；不得编造缺失背景、据此制卡或绕过正式欢迎准入。
- 正式欢迎继续遵守第 6 节：可靠申请/本次实际入住人及住宿关联、预订占库存后的审核/授权预告、实际入住后按目标汇合正式欢迎，无固定一小时等待。社区与楼栋各自判断，同时协调普通问候、介绍与正式欢迎，避免重复轰炸；具体普通问候频率仍由本栋设置决定。
- 自然人、参与关系、岗位与权限分别表达。朋友、家人、守护者不是互斥 Person 类型；入群不等于入住；守护者和值日者不因这些关系自动获得训练、任命或财务审批权。沿用稳定 person_id 和已确认渠道身份，姓名/昵称只用于查找选择。

后续验收需覆盖本栋持续训练与跨范围拒绝、指定业务确认人与全局任命的区别、无申请普通问候与正式欢迎门禁的分离，以及多类欢迎消息的协调。厨房、值日、基金的完整业务后续按需接入，不作为欢迎上线前置条件。

2026-09-10 用户已允许恢复本地开发，先完成通用框架 F1。按“业务要求 → 已有代码/接口 → 测试证据 → 剩余缺口”增量对照，复用身份、Inbox 和欢迎恢复能力。

旧训练全局 overlay/白名单与共享授权的衔接须在 F3 实际验证。历史欢迎回归及隔离数据库记录不是全部新业务验收，也不代表真实外部联调通过。

#### 已确认的全社区协作与开放私聊边界（2026-09-10）

- 本框架面向全社区可变的协作网络，支持人员、智能体、群和关系变化。UI 是主要入口，对话是补充，两者走同一受控服务；关系连线和岗位名本身不赋权。
- 使用少量权限类别、宽泛职责范围和已确认社区原则，不穷举每件生活事项。一栋自治以传递秦托邦文化、遵守社区原则和责任边界为前提；模型不能自行编造社区原则。
- 技术负责人通过同一框架横向支持人与智能体；技术支持不自动获得业务裁定、组织授权管理、秘密访问或部署权。
- 所有智能体承担合作引导与教练职责。新责任人上任或代理时，主动介绍已有工作、提出具体建议，并与本人确认工作节点、汇报和提醒习惯；不在本次设计里替责任人固定频率。约定不能扩大权限，交接不继承旧人的个人偏好或待执行内容审批。
- 人人可以提供信息，先记录最小必要线索，按约定汇总确认、纠正、否定或转处理事项；未确认内容不能充当正式知识。私聊不整体转为共享知识，受限生活知识须按身份与范围提供。
- 二花尽量开放接待主动联系的人，公开咨询不要求先登记或证明入住。在服务职责内持续交流不逐人逐句审批；主动联系仍按具体目的、授权、联系关系及对方意愿决定，不能据此任意批量外发。
- 接待、主动发送与内容可见性分别判断。在住、过往、全部入住人员是具体通知或回访的动态筛选，依据可靠 PMS 住宿和身份关系；全部为本范围在住与过往的合集，不是全通讯录，未知状态不推定为过往。

实施引用：

- [数据模型与旧入口衔接设计](../../../runtime/postgres/docs/data-design/2026-09-10-person-agent-collaboration-v1.md)
- [界面草图](person-agent-collaboration-ui-mock.md)：供业务批注。
- [F1 实现入口与边界](../../../runtime/sidecar/src/person_collaboration/README.md)：区分本地配置和未接入的真实工具。

上述设计补充 C01—C12 验收。不新增第二份 PMS 接口契约，也不改变 T01—T16 或原生产边界。

### 5.4 人员全貌、管理智能体分派与跨 Gateway 身份（负责人确认，2026-09-11）

**人员的多维关系由 Agent OS 统一关联。**以现有稳定 `person_id`
连接渠道账号、申请、住宿、社区参与、岗位任职、授权、安全背景、产物与工作历史。保留各自权威来源、事实/推断区分、时间与撤销状态，不把所有内容压成一个可任意覆盖的画像字段，也不把群聊原文或全部私密资料复制给各智能体。

“全貌”是统一可追溯的关联能力。每次实际读取仍按操作者、智能体、任务目的和群/楼栋范围裁剪；同一个人面对不同智能体时身份一致，但可使用的背景可以不同。跨渠道认识同一个人，不等于跨渠道开放其全部对话。

**负责人可以通过 default
Agent 或其授权的管理智能体，按既定职责给其他智能体安排工作。**这些是统一的对话入口，与 UI 调用同一套身份、职责、授权和任务服务。default 只是运行入口名称，不自动获得管理员权限，也不自动证明当前说话人就是负责人。

来自人员的分派请求应先核实真实发起人及有效管理权，再验证受托管理智能体能代办的范围、目标智能体的登记能力与执行权限，以及业务条件。保留发起人、代办者、执行者、作用范围、任务关联和结果的审计关系；委托不能把某个入口包装成全局权限旁路。

职责内且无需额外审核的任务可直接形成持久 WorkItem；已有规则要求审核时进入对应等待状态，不为每次常规分派重复要求负责人批准。缺少身份、负责人、目标接入或必要授权时，说明缺少条件，不伪造“已安排/已执行”。只有受控服务持久接收后才可报告已受理，取得执行回执后才可报告已完成。

正式 Agent 间协作统一经过 Agent
OS 的受控能力及 Event/WorkItem。管理智能体可理解和拆解目标、匹配职责、查询状态；跨 Agent 任务、等待、交接与恢复由服务端保存。

MCP/Tools 是调用入口，Hermes 是运行时，不保留自由文本直调或跨 profile 内部文件依赖作为另一套正式协作路径。即时问答或查询无需为了形式而每次创建异步任务；涉及跨 Agent 的工作交接必须可追踪。

**各 Gateway 和通信工具共用渠道身份解析服务。**适配器提供经过来源验证的渠道、标识类型、账号/企业/应用等必要命名空间，以及会话、发送者或事件主体。身份服务依据有效映射解析统一 person_id，再返回本次允许使用的上下文。群成员关系与自然人身份分开保存；已可靠关联者换群或换入口不会新建另一个人。

QiWe 的 [Userid 转 Openid](https://doc.qiweapi.com/api-344613881) 文档明确提供
`/contact/openid`，返回 userId/openUserId 配对，并说明 OpenID 对应官方 external_userid。这已构成正向转换能力的文档证据，不再描述为“有没有映射接口完全未知”。实际账号权限、支持的成员类型、跨应用适用范围和接入仍需验证；反向接口在此前盘点中标为开发中，不作为必要依赖。

转换得到的是渠道账号之间的关联，仍须通过可靠来源或人工确认，将账号与申请/PMS 实际入住人对应到具体 Person。无需等待对二花发言，可通过获准的联系人或群名单先发现身份；系统事件的外层 senderId 可能属于操作人，不得当成新增成员。

内部员工 userid、普通微信账号及共享工作账号分别建模，不能套用外部联系人转换规则。共享账号无法证明具体操作者时，可以识别工作账号，但不得冒充某位自然人批准。

已验证且有效的映射可以复用，不要求每次换 Gateway 或进群重新人工确认；冲突、过期、撤销或未知身份转入待确认，并限制个人背景访问。同名、头像或 ID 格式不作为自动合并依据。

补充验收：同一已关联人员从两个接入入口出现时解析到同一 Person，分别获得符合各场景的背景；未知或冲突身份不误认；负责人通过获授权管理智能体创建符合职责的任务并看到持续状态；更换入口不扩大权限；撤销身份或委托后未执行请求重新判断；目标不可用时任务可保留或转人工，不虚报完成。

本节记录已确认的目标和实现约束，不证明上述跨 Gateway、管理分派和真实工具消费已上线。实施按[分步实施决策](person-agent-foundation-execution-plan.md)推进，不改变 PMS 事件字段、住宿事实来源或现有生产授权边界。

## 6. WorkItem 与欢迎状态机

### 6.1 复用原状态

保留现有 `queued → processing → awaiting_review / awaiting_publish → completed`，以及
`cancelled/failed`。业务条件不齐使用 case 的结构化 `readiness_reasons`，不把新 `blocked`
值直接写进现有 WorkItem
status 约束。未准备好不创建可领取发布任务；审核/来源事件到达时重评估并幂等创建或唤醒。轮询补偿可使评估恢复，不把 available_at 设成固定一小时。

按一条 case 建最小生成/审核/发布子任务；具体新 work_item_type/能力键需同步 runtime 白名单和 registry。

复用文字/群消息/产物发送机制，但新欢迎语义不能伪装成 Xiaoman 活动任务绕过字段或受众门禁。

| 阶段              | 全部前置条件                                                                             | 结果                                                 |
| ----------------- | ---------------------------------------------------------------------------------------- | ---------------------------------------------------- |
| 制卡              | 申请有效、实际 occupant/本次 Stay 已确认、存在适用的住宿安排、生成素材获授权             | 阿靓生成待审核卡片；无 PMS 申请关联不制卡            |
| 预告              | 制卡共同资格、真实预订已占库存、尚未过时、对应文案/产物审批与发布权限有效                | 预告任务；不要求新住客已入群，不能写“已入住”         |
| 社区/楼栋正式欢迎 | 共同资格、PMS 当前实际入住、对应群当前成员关系、卡片/文案审批与同意/任职有效、本目标未发 | 即时创建/唤醒发布任务，无固定延时                    |
| 员工内部发布      | 共同资格及内部人员审批/阶段策略                                                          | 不要求住客进员工群；内部正式时点未定时只准备待审任务 |

社区和楼栋各自独立，某群未入不阻塞另一个已就绪目标。先入群后入住、先入住后入群、审核最后到达、老客已在群均以条件汇合处理。撤销/终止/离群/路由变化使相关未发任务不再可执行。

### 6.2 发送事务、幂等与恢复

- 业务 action_key 由服务器按 `welcome-v1/case_id/phase/target_binding_id/part`
  生成；part 区分 text/image，Artifact 改版不生成新的已完成动作资格。来源 event_id 不是发送幂等键。
- 单次领取/生成动作和处理检查点须原子落库。复用 claim_expires_at，增加或证明递增 claim
  generation/fence；过期 Worker 不能覆盖新 Worker 状态或再调用 provider。
- 发送前验证 PMS 当前投影/实际楼栋、有效成员状态、身份链接/同意/审批/角色版本和执行权。成员证据默认要求本次尝试开始前 60 秒内的成功查询或更晚的可靠更新；过期则刷新，无法刷新就待处理。不能把失败查询当空名单。
- 输入与路由审核版本冻结在 DeliveryAction。实际群变更后若未发则撤销旧准备并重新审核，已发不自动对新群补发。系统展示名字，但绑定真实目标只能由受控服务完成。
- 准备好后记录 sending
  attempt，再调用外部 provider；成功保存回执并完成对应 part。网络超时/进程退出后处于 unknown，优先查询 provider 回执，无法确定则人工核对，不盲目重发。
- 如果 provider 不支持业务幂等/结果查询，不能承诺 exactly-once；V1 以持久尝试、单执行者、未知结果暂停降低重复风险。授权撤销与外部调用之间仍有竞态，尽量缩短窗口，并将已经在途的动作单独记录/处理。
- 不把所有目标的“发文字+图片”当一个外部原子事务；部分成功只修复未完成部分。图片同意撤回/素材换人后未发部分也暂停，不沿用旧审核。
- 外部投递失败和模型不可用不丢任务：可确定的暂时失败按退避重新排队；永久拒绝 failed；审批/身份问题进入人工任务。失败任务保留原 case/action，恢复不换幂等键。

### 6.3 卡片与附件

复用 `generate_card_v10.py`
的 Pillow 排版，包装成受控能力；禁止导入旧主程序就触发示例生成或运行旧
`--dry-run`。输入为许可字段/照片引用、显式输出位置，必须验证解码大小、类型、字体、无照片/长文案布局等边界。阿靓普通设计 Base 与欢迎附件目标分开配置。

欢迎文件上传申请表「欢迎卡片」attachment 后受控回读 hash，再登记同一 Artifact 的 storage_ref。

上传重试不覆盖其他附件；同申请多次住宿附加版本并在内部维护归属，不能只取最后一张图片。

准备/上传成功但登记失败时通过上传 intent 和 hash 对账恢复，不生成另一个业务动作；临时签名 URL 不作永久引用。

## 7. 阶段、双项目实施清单与验收

### 7.1 可并行的实施切片（仅清单，本轮不执行）

| 切片        | Agent OS                                                    | Green PMS                                                  | 完成门槛                                                |
| ----------- | ----------------------------------------------------------- | ---------------------------------------------------------- | ------------------------------------------------------- |
| C0 契约核对 | 本文件及合成 envelope/ACK/cursor 用例定义                   | 核对事件/版本/写路径、接收路径与最小投影可行性，只回传差异 | 双方使用同一版本/字段/错误语义；不重新讨论已定欢迎规则  |
| A1 / P1     | 复用身份与 WorkItem，新增 Inbox/检查点/确认撤销和权限边界   | 同事务 Outbox、成员/库存必要版本、已提交发布序号           | 回滚无事件、晚提交不漏、重复/冲突处理；以本地隔离库验收 |
| A2 / P2     | 签名接收器/补偿查询客户端/同一状态处理器                    | 固定 webhook Worker、事件 feed、最小投影/墓碑              | 相同事件 push/pull 得到同一业务动作；丢 ACK 可恢复      |
| A3          | Feishu 适配、QiWe 名单/转换确认、角色/审批工具、最小人工 UI | 修正 P1/P2 合同差异，保留 PMS 既有写门禁                   | 首次绑定可确认撤销、名字消歧、群范围授权；无真实发送    |
| A4          | 版本化卡片能力/附件、欢迎条件汇合、逐目标回执               | 对账与故障/运行验收配合                                    | 合成闭环、无申请不制卡、取消/换楼栋/退群重进不误发      |
| R1          | 影子、限定试点、单执行权切换和回滚                          | 事件源/读接口分阶段发布与积压观测                          | 影子零副作用，版本/权限核验，旧新唯一生产执行方         |

C0 是技术对接核对，后续用户授权开发后 A1 与 P1、A2 与 P2 可按共同 fixture 并行；一个任务只改自己的仓库。PMS 不依赖完整客房 Agent，Agent
OS 可先使用模拟 producer。

### 7.2 Agent OS 文件范围

- 身份/数据：`runtime/postgres/docs/data-design/`、后续必要迁移与 `runtime/sidecar/src/`
  的关联/上下文服务；保留旧读者，先显式映射旧未知 namespace。此次不新建迁移。
- 控制面：复用 `runtime/sidecar/src/operations.rs` 与 `mcp/operations-control-plane/`
  受控边界，按现有模式抽出领域实现，补新类型白名单/输入输出 schema/审计。无需把所有业务都扩大通用 MCP。
- 渠道/存储：`skills/qiwe/`、`skills/feishu-base/`、`mcp/feishu/`
  与现有媒体上传/Artifact 模块；每个目标独立范围，无任意 SQL/Base/群路由。
- 流程/卡片：拟新增 `workflows/resident-welcome/` 与
  `skills/resident-welcome-card/`；完善 README、manifest、owner、fixtures，注册 workflow/capability/restart 规则。是候选目录，本轮没有创建包。
- Agent：客房包名在注册时确定，属于住宿一个责任域；阿靓复用 huabaosi，二花/四老师只接对应能力与审核入口，不复制彼此 prompt。
- 部署：沿用 release/current、版本化配置/开关/runbook；接收/消费/生成/上传/发送分开开关。生产迁移先只读核对版本、兼容性和约束再执行，不直接编辑 profile。

### 7.3 最小验收矩阵

以下是待实现测试规格；本轮仅纸面验证，未运行程序/数据库测试。

| 编号 | 场景                                                    | 通过判据                                                       |
| ---- | ------------------------------------------------------- | -------------------------------------------------------------- |
| T01  | 有 PMS 无申请 / 有申请无安排                            | 可有 Person/案例；卡片与发布均为 0                             |
| T02  | 会员甲代乙订、多 occupants、同名/共用电话               | 没有可靠逐人绑定不生成；确认只作用被选入住人；撤销后旧任务失效 |
| T03  | 已入住再入群 / 已入群再入住 / 审核最后完成 / 老客原在群 | 每目标最终各一条欢迎，不等一小时；预订先入群不发“已入住”       |
| T04  | 重复入群、事件重放、卡片改版、人员合并                  | 稳定案例/动作不重发；新住宿是不同合法案例                      |
| T05  | 错企业同 userId、官方与 QiWe 同字面值、共享工作账号     | namespace 不串人；无法证明批准操作者不当有效审批               |
| T06  | 舍长离任/撤销授权/同意撤回/换楼栋                       | 相关未发任务失效；模型自由文本不能扩大目标或变量               |
| T07  | PMS 事务回滚、preview、历史补录                         | 不出现 live 欢迎资格；回滚无可投递事件                         |
| T08  | 事务 A 先取内部 ID 后晚于 B 提交                        | B 游标确认后仍能读到 A；基线/分页过程变化最终收敛              |
| T09  | webhook 到达后 ACK 丢失、push 与 pull 同时送达          | 单 Inbox 身份/单业务效果；同 ID 不同 body 返回 409             |
| T10  | 伪签名、越物业、时钟超窗、旧事件新签名重放              | 前三者拒绝；合法重放可接受但不重复业务动作                     |
| T11  | 游标过期/查询无权限/分页中断                            | 不把空结果当删除，不跨缺口推进；重建期间自动发布关闭           |
| T12  | claim 过期并发 Worker、调用成功后进程崩溃               | 过期 Worker 被 fence 拒绝；unknown 不盲重试，不伪造完成        |
| T13  | 图片成功文字失败 / 一个目标失败                         | 成功部分不重发；未完成部分独立恢复                             |
| T14  | 附件上传成功登记失败/附件自写回调                       | 对账复用原 intent/Artifact；不重复制卡或覆盖其他卡片           |
| T15  | QiWe 回调漏失/外层 senderId 是邀请人/名单分页失败       | 名单校正识别真正成员；不把邀请人当住客，不误标全体离群         |
| T16  | 重启、影子积压、启用边界、回滚                          | 影子没有生成/上传/发送；积压不升级真实发送，旧新单一执行者     |

后续代码验证入口：Agent OS 按修改范围运行包级检查与
`pnpm check:pr:heavy`，隔离 Postgres 与 loopback 发送验收；PMS 运行其
`npm run verify`、数据库锁保护的 integration/contract 测试及事务/权限回归。禁止为了验证而连接生产库或真实群。

## 8. 未定事项的处理与上线门槛

| 未定/未验                                    | V1 处理，不猜业务规则                                                  | 阻塞范围                              |
| -------------------------------------------- | ---------------------------------------------------------------------- | ------------------------------------- |
| 多申请/复用旧申请                            | 小客服为本次案例选择；保留全部历史引用                                 | 自动选历史申请，不阻塞基础接口        |
| 已发预告后的更正、撤销后恢复入住是否再欢迎   | 不自动更正/重发；建人工处理任务                                        | 该例外自动化                          |
| 未入群/未准备好的最晚发布窗口                | 保留待处理；在 PMS 终止后取消，不按时间自动放行；延误需负责人处置      | 自动超时/补发策略，不能回退一小时规则 |
| 内部群正式发布具体时点                       | 四老师准备待审任务，人工明确发送                                       | 内部自动发布策略                      |
| 任命人、群别名与披露字段/保留期              | 设置前显示候选/范围；未配置的角色/受众不授权；资料白名单版本随审批保存 | 实际授权/生产上线                     |
| 生产 schema、Worker 最小权限、QiWe 账号/转换 | 只读核验与测试账号/隔离 fixture 先行；不输出私密数据                   | 生产连接与自动身份识别验收            |
| 源 revision 覆盖、feed 水位、Outbox 写路径   | PMS 按交接表确认；不符合则回传具体差异                                 | 双方相关接口实现，其他独立切片可继续  |

已确认的固定 webhook + 轮询方向、现有 Person 复用、Base 保留、无一小时等待不再列为待业务决策。未定例外有人工出口，不需要再做一轮全面盘点。

影子模式仅可在隔离的模拟控制面保存最小事件/拟执行计划，不改 live
Person/授权/业务任务状态；读取生产源必须限定权限与字段。制卡、附件上传、PMS 写、群/私聊审批消息和发布调用在执行层全部关闭。真实账号入群自动识别成功、影子差异可解释、限定试点批准以及切换/回滚演练完成，才可按已授权发布流程激活。

切换执行权以 source/workflow/target 范围和 execution_epoch 记录；旧流程须暂停接新副作用并确认在途动作/回执处理完，再启新流程。

回滚先停新发送并核对 unknown，再恢复旧执行权；删除数据库或回放所有积压均不是回滚方案。

## 9. 本次完成与下一步

完成共同事件/签名/游标/最小投影契约、Agent
OS 复用与实现切片、合成验收矩阵和独立 PMS 交接。下一步由原 Green
PMS 任务对照自身实现确认接口差异，并由负责人在准备实施时明确开发授权；不需要重写已确认业务方案。

以上为最初设计交付记录。后续本地开发状态见下节；拟定运维默认及未验证接口不等于生产事实。

## 10. 2026-09-09 实施与 PMS 差异处理

用户已授权 Agent
OS 本地代码、版本化迁移、合成 fixtures 和隔离数据库测试。PMS 回传来源为其仓库
`待开发项/Green-PMS-事件集成-P01-P08-实施核对.md`。不修改 PMS 代码；双方仅维护本文件为共同协议入口。

| 差异 | Agent OS 处理                                                                                                        | 联调状态                                     |
| ---- | -------------------------------------------------------------------------------------------------------------------- | -------------------------------------------- |
| D-01 | 采纳 `pms.order.context_changed`，order 聚合，仅失效/回读，不独立授权欢迎                                            | 已收口；两侧本地实现，待联合验收             |
| D-02 | 成员/库存最小当前态和墓碑读取、来源引用嵌套结构                                                                      | 精确 DTO 已收于第 11 节；真实回读关闭        |
| D-03 | feed 的 events 数组嵌入已存原始 UTF-8 JSON 片段；接收方提取原始对象字节，禁止 parse/stringify 后求 hash              | 已收口；Agent OS 原始字节测试通过            |
| D-04 | hash 排除 observed_at 和此刻派生值；完整版本向量及营业日变化均重评                                                   | 稳定覆盖集见第 11 节；合成测试通过           |
| D-05 | 不假设一安排区间对应独立 segment；墓碑用显式 200 union；404 不删除                                                   | 精确 union 见第 11 节；待联合验收            |
| D-06 | cursor 表示已接收至 c；seq>c；c=floor 有效，c<floor 为 410；空库 head=floor=0；无效/未来 cursor 为 400，越物业为 403 | 客户端不解析/自造 cursor；错误时保留原检查点 |
| D-07 | 迟录、历史补录、管理员纠错为 historical_correction；复合事务用最终修订和各自事实引用；排空旧事务后切换捕获边界       | PMS 负责事务门闩证明，未验收前不启用 live    |
| D-08 | V1 单物业 READ 凭证；多个物业分别配置；客户端路由白名单不能冒称 PMS Token 已具备 endpoint ceiling                    | 只读权限含义已澄清                           |

上述为共同维护方的技术收口记录，不是接口已存在或双方程序联调通过。

迁移设计见[本地实施](../../../runtime/postgres/docs/data-design/2026-09-09-resident-welcome-v1.md)。

## 11. 最小投影 V1 精确收口

采纳 PMS 回传 D-02/D-04/D-05 以下结构，双方实现以本节为准；尚未实际联调。source_fact_ref 沿用字符串
`kind:opaque-id`，不改成对象；kind 是 command、amendment、correction、account_management_operation、migration 或 baseline。

所有 ID/修订为字符串，未知值显式 null、空集合用 []，所有定义字段必填。

| 结构                 | 精确字段与类型                                                                                                                  |
| -------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| Meta                 | schema_version 固定 pms.projections.v1；source_instance、property_id、projection_hash 为 string；observed_at 为 UTC RFC3339     |
| IntervalRef          | segment_id、inventory_unit_id 为 string；arrival_date、departure_date 为 YYYY-MM-DD                                             |
| ArrangementInterval  | IntervalRef 加 inventory_kind=ROOM/BED、room_id:string、bed_id:string/null、building_code:string/null、inventory_active:boolean |
| RelatedRevision      | aggregate_type=member/inventory_unit、aggregate_id:string、aggregate_revision:十进制 string                                     |
| OccupantRef          | occupant_id:string、role=PRIMARY/ADDITIONAL、registration_state=active/removed                                                  |
| ApplicationSourceRef | reference_id:string、provider 固定 FEISHU_BASE、source_container_id:string、source_table_id:string、external_record_id:string   |

OrderProjection = Meta 加以下字段：

| 字段                                               | 类型/枚举                                                                                                             |
| -------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| order_id / order_revision / stay_id                | string / 十进制 string / string                                                                                       |
| stay_status                                        | PLANNED / IN_HOUSE / COMPLETED / CANCELLED / NO_SHOW / CHECK_IN_REVOKED                                               |
| fulfillment_state                                  | NOT_CHECKED_IN / IN_HOUSE / CHECKED_OUT / CANCELLED / NO_SHOW / CHECK_IN_REVOKED                                      |
| checked_in_at                                      | UTC RFC3339 或 null；当前源无准确到店时间，返回 null                                                                  |
| effective_arrangement                              | 对象：presentation、arrival_date、departure_date、intervals                                                           |
| effective_arrangement.presentation                 | CURRENT / LAST / BEFORE_CANCELLATION / NO_SHOW_ORDER / BEFORE_CHECK_IN_REVOCATION                                     |
| effective_arrangement.arrival_date、departure_date | YYYY-MM-DD                                                                                                            |
| effective_arrangement.intervals                    | ArrangementInterval[]                                                                                                 |
| occupants                                          | OccupantRef[]                                                                                                         |
| member_ref                                         | {member_id:string} 或 null；绝不等同 occupant                                                                         |
| related_revisions                                  | RelatedRevision[]                                                                                                     |
| read_context                                       | {property_timezone:IANA时区string,business_date:YYYY-MM-DD,temporal_state:下述枚举,current_interval:IntervalRef/null} |

temporal_state：

- PLANNED：入住日前 NOT_STARTED、当天 RESERVED_TODAY、之后 OVERDUE_RESERVED。
- IN_HOUSE：半开住宿区间内 IN_HOUSE_TODAY、退房日 DUE_OUT、之后 OVERDUE_IN_HOUSE。
- 终态：TERMINAL。

仅两个 TODAY 状态返回唯一当日区间，其余 current_interval=null。

CURRENT 表示当前权威安排版本，不等同今日在住。

预告可依据 CURRENT 的未来有效计划/库存；正式欢迎须有今日实际入住区间。

不把末段房源延伸为今天区间。

未来 MOVE 在营业日边界重新评估，不伪造事件。

segment_id 是建立安排区间的权威版本引用，可被多个区间共享。

occupant 共用订单安排，不以 ordinal 推断个人床位。

床位的 room_id 是父房，整房的 room_id 是自身、bed_id=null。

related_revisions 包含 member_ref、全部安排库存及解释床位/楼栋的父房。

新增 GET 白名单：`/api/v1/integrations/agent-os/members/:id` 与
`/api/v1/integrations/agent-os/inventory-units/:id`，均带 propertyId。HTTP
200 响应为下列 union：

| 对象/分支           | 精确字段                                                                                                                  |
| ------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| Member 公共         | Meta + member_id:string、member_revision:十进制 string、resource_state                                                    |
| Member active       | resource_state=active；external_references:ApplicationSourceRef[]                                                         |
| Member tombstone    | resource_state=tombstone；invalidation_kind=BUSINESS_DELETED；invalidated_recorded_at:UTC RFC3339；source_fact_ref:string |
| Inventory 公共      | Meta + inventory_unit_id:string、inventory_revision:十进制 string、resource_state                                         |
| Inventory active    | resource_state=active；kind=ROOM/BED；parent_room_id:string/null；building_code:string/null                               |
| Inventory tombstone | resource_state=tombstone；invalidation_kind=INACTIVE；invalidated_recorded_at:UTC RFC3339/null；source_fact_ref:string    |

申请引用仅是候选，不替代逐人/本次住宿确认。

库存失效不是物理删除，恢复需更高修订；building_code 返回自身字段，不隐式继承，父房冲突须待校正。

无物业权限 403；有权限但目标不可见/不存在 404，不能据此删除镜像。

只有明确 200
tombstone 才是此读取接口的墓碑证明；本期不定义未核实来源的订单/物理删除墓碑。

投影 hash 使用 RFC 8785
JCS 的 V1 无数字子集（仅 string/bool/null/array/object）编码后 SHA-256。

仅定义字段参与，排除 projection_hash、observed_at；订单额外排除整个 read_context。

未知扩展不参与。

编码前集合按 UTF-16 code-unit 字典序排序：

- intervals：arrival_date、departure_date、inventory_unit_id、segment_id。
- occupants：PRIMARY 在前，再 occupant_id。
- related_revisions：aggregate_type、aggregate_id；不允许重复引用。
- external_references：provider、container、table、record、reference_id。

比较完整版本向量；同向量不同 hash 待校正，混合新旧/无法解释的引用集合先回读。相同 hash 的营业日变化仍重评，不是事实冲突。事件 hash 继续使用原始字节。

订单扫描列表包装：

- schema_version 固定 pms.projections.v1；source_instance、property_id 为作用域。
- orders 为 OrderProjection[]；按 order_id 升序分页。
- next_after_id 为末条 ID；空页保持入参或 null。
- has_more=true 必须有非空推进位置。

已只读核对 PMS 当前 `scanPmsOrders` 实现采用这些字段。

PMS 原任务随后回传：接口兼容、无新增协议差异，扫描/最小投影/墓碑及共享 fixture
hash 一致。这不表示双进程故障联调或真实来源连接已经完成。
