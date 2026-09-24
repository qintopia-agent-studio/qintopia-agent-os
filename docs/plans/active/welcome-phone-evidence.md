# 手机号候选：固定读取与确认前复验契约

本增量承接用户最终决定：新申请手机号必填、手机号主要匹配、姓名昵称辅助；历史不追补。首次关联仍需明确确认，电话不证明聊天账号归属，也不授予 PMS/财务权限。本文件为服务端与岸岸宿主共享契约，接口已冻结；服务端与群宿主增量已完成本地专项验证，岸岸私有HTTP宿主联合结果以实施报告为准。

## 唯一入口与存储

沿原 Unix 协议 `schema_version:1`、`agent:anan`、`trusted_context`，操作
`person_foundation_ingress`、工具
`welcome_stay_contacts`，使用独立 HOST_TOKEN。固定网关和 `QINTOPIA_APPLICATION_BINDING`
必须与004申请的当前绑定/范围一致。所有请求拒绝额外字段，不能提交目标URL、Person、tenant、scope或自报证据hash。

以004 `application_intake_states.anan_work_id`
对应的现有申请 WorkItem 为锚，解析申请引用。只更新该 WorkItem.metadata 的私有
`welcome_contact_read_v1`
键，行锁与局部更新保护其他模块。保存读取token、失效依据、期限及脱敏逐case比较结果，不保存号码或完整订单，也不新增迁移/队列。普通任务、模型和页面序列化不透传私有键、token、basis或重放摘要。

## 请求 DTO

- `open`：`{action:"open", work_item:UUID, refresh?:boolean, presentation?:UUID}`。
- `save`：`{action:"save", work_item:UUID, read_token:UUID, order:Order}`。
- `failed`：`{action:"failed", work_item:UUID, read_token:UUID, reason:Reason}`。
- `status`：`{action:"status", work_item:UUID}`。

`Reason` 仅 `read_failed` 或 `read_unavailable`，不接收异常原文。 `Order` 仅
`id:string, property_id:string, version:integer, occupants:Occupant[]`。version 为非负整数，拒绝bool、浮点与string；PMS实际最小版本为1。
`Occupant` 仅
`id:string, phone:string|null`。occupants必须完整、ID不重复，不以空数组掩盖读取失败。

岸岸固定使用现有 `Client.read('order', property_id, resource=order_id)`
的宿主私有读取路径：先 `/me` 核验该物业 READ/WRITE，再 GET
`/api/v1/orders/{id}`，没有propertyId query。读取 `raw.order.id/property_id/version`
与完整
`raw.occupants[].id/phone`。只读凭据由现有固定环境提供，本批仅显式loopback模拟；电话不经过普通工具输出或public/redact。不使用
`current_primary_guest` 替代逐人电话，也不回退被更正为null的旧号码。

## 响应 DTO

所有成功响应均含以下摘要字段（无号码）：

- `work_item`、`application`：服务端解析的引用。
- `status`：仅 pending、complete、incomplete、awaiting_source_sync、stale。
- `pool_count`、`orders_total`、`orders_done`、`failed_count`：当前受控池及读取覆盖计数。
- `scan_complete`：只有全部订单有效读取、当前池未变且未过期才为true。
- `local_only:true`。

`open` 另返回 `reads` 数组，每项仅
`read_token, order_id, property_id, order_revision`。order_revision为十进制string，映射自PMS
`String(order.version)`，对应归一化projection.revision，不是事件aggregate
revision。无待读取时reads为空，不能由空数组推断匹配成功。

`save` 另返回 `stored:boolean, replayed:boolean, read_status:string`。 `failed` 另返回
`stored:boolean, read_status:"failed"`；`status`
不返回token或完整证据。read_status仅pending、complete、failed、awaiting_source_sync、stale。单订单完成不代表聚合complete；读取失败不得标成电话缺失。

## 全池与当前性

open枚举全部同范围有效逐occupant
case池，沿既有200条上限，超限明确报错且无完整排序。按订单去重读取，不能先取姓名评分前20，也不依赖先选中case或已存在Person。上下文绑定当前申请修订/身份摘要、物业绑定版本、case版本、订单projection修订及active
occupant集合。

save校验完整order/property/version及逐人集合和当前投影一致。新PMS版本先于事件投影时返回awaiting_source_sync，不接纳旧证据。PMS读取内容来自固定HTTP宿主；nonce/revision单独不是内容证明。同token同内容重放不重复写或延长期限，不同内容拒绝。相同依据的普通open复用当前token，显式refresh或依据变化产生新一代并废弃旧token。读取证据5分钟后不可用于匹配；TTL不是强一致保证。

服务端只在内存比较申请投影手机号与逐人phone。规范化去空白、括号、短横线及大陆+86/0086前缀，严格11位大陆手机号。结果区分match、different、missing、unusable；unusable另给固定原因。只有完整读取后才对全池按电话主排序；同号多case保留ambiguous，不自动绑定。未读、失败、历史无号码与无法规范化不互相冒充，也不阻塞已有有效关联。

## 群确认前真实刷新

群宿主增加只读
`welcome_group_host / confirmation_context`：读取并校验当前可信持久消息、具体呈现引用与明确效果，返回
`requires_contacts:boolean, work_item:UUID|null, presentation:UUID`。work_item为原申请锚点，presentation为消息明确引用的事项呈现。

若确认依赖电话建议，宿主先调用
`open(refresh:true,presentation)`，保持原可信消息上下文；服务端将读取代次绑定该已认证消息、呈现与申请。宿主重新实际GET完整候选池并save后才调用callback。callback不仅检查TTL，还检查本消息对应的最新完整读取及稳定比较依据；变化时拒绝旧引用。相同效果、相同来源与电话结果的普通刷新不更改原业务批准，不强迫用户重复确认。

原人工核对路径仍保留，须明确记录使用原始资料核对而非声称刚完成电话复验；历史已确认身份不因缓存到期自动撤销。UI的人工关联操作继续要求逐段明确勾选，不能将电话分数当批准。

群宿主可由岸岸注入固定 `refresh_contacts(work_item,presentation)`
函数，负责上述open/GET/save。该函数是宿主私有回调，不注册模型工具，不接受自选URL，也不发送真实群消息。

## 固定宿主挂点与复验结果

`WelcomeHost.callback(*, refresh_contacts=None)` 先调用
`broker({action:"confirmation_context"})`。响应固定为
`{requires_contacts:boolean,work_item:UUID|null,presentation:UUID,replayed:boolean}`。当requires_contacts为true，调用注入的
`refresh_contacts(work_item,presentation)`，要求返回上述status响应且
`status=="complete" && scan_complete==true`，随后仍以同一broker、原可信消息调用
`broker({action:"callback"})`。刷新失败保留原事项；不编造消息、不另造operation键。该注入函数由岸岸实现，只做open(refresh=true,presentation)→逐订单真实GET/save或failed→status，不用返回任何手机号。确认宿主不自造通用路由。

服务端依据呈现中保存的电话建议以及当前明确效果决定requires_contacts；申请住宿关联或按申请建档才可能依赖电话。仅账号、内容、撤销、退回及原已独立确认的申请住宿关系不受电话刷新阻断。

显式命令“确认 W-... 人员1 关联住宿 人工核对”或“建档 W-... 人工核对”记录人工原始资料核对，保留已有授权与版本校验，不能冒称电话复验。

读取generation只防止旧token写入。稳定比较依据包含申请/绑定/完整候选池版本及逐人比较内容，不包含generation、token或读取时间。原可信消息内刷新后稳定依据相同，原确认继续执行一次；真实依据变更或授权过期拒绝并重新呈现。已成功的原消息重放先回原收据，不重新刷新或重复写关联。验收覆盖同依据刷新、真实变化拒绝、重放、独立人工核对和私有metadata不外泄。
