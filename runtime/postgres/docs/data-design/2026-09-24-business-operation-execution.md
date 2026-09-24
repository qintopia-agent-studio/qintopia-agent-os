# 通用业务操作范围与岸岸本地执行账本

日期：2026-09-24。所属：Agent OS 共同 Person/授权/WorkItem，PMS 操作目录由
`skills/pms-operations/` 持有。只支持 synthetic tenant 与模拟 loopback 联调。

## 权限和兼容

沿用 hospitality 和既有职责目录，增加通用 action
`read_business`、`execute_business`。旧任职、manage/review/publish 和旧空权限不自动获得任何新操作。精确范围单独绑定 collaboration
grant、可信物业映射、稳定 operation key；缺省没有记录即拒绝。无需改写旧已应用迁移。

精确操作记录带上级 operation
grant 引用。委派必须同时具备既有管理范围和本人有效的同操作范围，不能只凭 manage 授予自身不持有的资金或房态能力。

每次使用沿 operation 父链检查撤销、到期、物业、操作、当前人员与基础 grant；基础任职/身份/授权链无效即拒绝。

PMS `/me` 仅约束执行主体；客户端再次取精确 command
grant 交集。物业来自服务端 scope→source/property 绑定，模型只能引用服务给出的范围，不能自建物业映射。

## 稳定映射

查询用 `pms.read.<kind>`；报价用 `pms.quote`（read_business）；具体写入用
`pms.command.<COMMAND_TYPE>`（execute_business）。模型工具名不是授权 key。
`skills/pms-operations/operations.json`
为有限目录；Token 管理、退款、会员、维护等未交付命令不在目录。每个动作可独立授予/撤销，不因授予建单而获得收款、入住或退房。

## 持久执行与确认

事项使用既有 work_items；动作表只保存协作执行所必需的引用、请求版本、原键、预览、确认和结果。不会同步一套订单/库存/资金事实库。

- 同一事项内动作独立记账，来源消息与操作标识防重复。修订只影响未执行方案；不整单重放。
- 报价/预览也先持久化原键；预览失联可复用同键同载荷，Confirm 失联只能查原键/resolve。
- 执行权通过事务条件更新领取，提交后调用 PMS；在途崩溃保持结果未知，恢复不能再次 Confirm。
- 确认来自宿主捕获的消息证据与明确方案引用，绑定人、会话、方案版本与 PMS
  effectHash；模型不能自填 approved。
- 人类确认在同一方案上复用，客户端技术 Confirm 不再额外要求批准；方案过期、修订或授权失效拒绝。
- 完成依据为 PMS 真实回执和资源回读；UI 接手通过查询原键/当前事实核对，取消待办不撤销业务事实。

共享接口新增独立模块，既有 QiWe broker 协议和欢迎函数保持。官方 WeCom
host 证据单独记录，不能把 QiWe fixture 当成真实渠道；仍不删 synthetic_tenant_required。

## 事件和恢复

收款 checkpoint 与 inbox 使用 source/property 隔离；首次冻结源端已提交高水位后追赶，历史项只入基线不创建新催办。

不能按 occurredAt 划界：PMS 事件时间是事务开始时间，早开始晚提交的事件会被错误归为历史。当前 feed 未提供激活快照，高水位只能由受信源快照提供；缺失时拒绝自动初始化。

每页先在同一事务保存事件和待办，再推进字符串游标；只按可靠 bill/order/application 引用关联。

申请入口保留四老师原职责，各消费者独立持久分派。来源修订和乱序不得回滚已确认业务，不同住宿不得按姓名/金额合并。欢迎仍走既有资格和三 Agent 链，默认不外发。

## 迁移、验证和回滚

新增
`202609240001_business_operation_execution.sql`，当前目录未占用。新增表及 action 检查约束扩展，不更新旧授权行、不重编号旧迁移。schema_change_log 登记此设计。

验证：真实隔离 PostgreSQL 的空权限拒绝、委派子集、跨物业、撤权/任期到期、并发领取、原键恢复、独立动作及 checkpoint 原子性；包内 HTTP 外部边界测试；既有共同授权/欢迎回归；适用 check:pr:auto。

关闭新增本地入口即可回退。保留事项、动作、确认、游标和回执，先核对 unknown；不删除已发生事实。正式 anan 身份登记的 A 方案已获用户专项批准，按批准范围实施；登记仍不证明岸岸全链可用或生产就绪。
