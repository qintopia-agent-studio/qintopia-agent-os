# 岸岸收款事件首次基线：最小跨仓接口提案

日期：2026-09-24。状态：首次已提交事件头快照方案已获用户同意；接口与主动通知由 Green
PMS 项目开发，Agent
OS 岸岸任务只读参考 PMS，负责接收与补拉恢复。PMS 独立开发任务已启动，任务 ID 为
`01a0d311-8700-75a0-8a7b-de4bb71b7b77`；接口已由该任务提交
`608c53b`，回报 PG18.6 专项 16/16 通过；Agent
OS 联合接收尚待验证。主动投递的范围见[项目交接](green-pms-payment-events-handoff.md)，不等于下列五文件只读接口切片。本提案不授权生产操作。

## 1. 已确认的问题与依据

岸岸首次启用收款事件消费时，需要跳过启用前的历史积压，并接住之后提交的事件。不能以本地时间和事件
`occurredAt` 比较代替首次游标基线。

核对 Green PMS 提交 `0254fba`：

- `packages/db/src/migrations/060_external_payments.sql` 的
  `qintopia_external_payment_event` 在同一事务内锁住物业的
  `external_payment_event_heads`
  行、递增序号并写入事件。按物业串行分配的序号与提交可见性共同构成消费边界。
- `external_payment_events.created_at` 使用 PostgreSQL
  `now()`，表示事务开始时间。事务可能在消费者启用前开始、启用后才提交；按时间过滤会把这笔新提交事件误判为历史事件。
- `packages/db/src/external-payments.ts::readExternalPaymentEvents`
  按序号升序分页，每页最多 100 条，`nextCursor`
  只是当前页最后一条序号，不是整个事件流的当前最大序号。不断翻页到空也不能固定首次启用边界。
- 现有 `/api/v1/external-payment-events`
  没有独立返回已提交最大序号的接口。现有运行数据库角色已具备事件头表的 SELECT 权限，无须新增迁移或数据库授权。

首次启用的业务边界定义为“成功取得并持久保存的事件头快照”，不是用户点击启用的客户端时钟。该边界及已保存状态需能被查验。

## 2. 建议接口

```http
GET /api/v1/external-payment-events/head?propertyId=synthetic-property
```

```json
{
  "schemaVersion": "pms.payments.v1",
  "propertyId": "synthetic-property",
  "headCursor": "42"
}
```

- 复用 `requirePrincipal` 和
  `requirePropertyAccess(..., "READ")`，按当前主体和物业授权查询。
- 在单次查询快照中读取该物业已提交的 `last_sequence`；尚无事件头时返回字符串
  `"0"`。不等待尚未提交的事件，不用独立时间戳推断。
- 游标沿用十进制字符串和 PostgreSQL bigint 范围，不转换成 JavaScript Number。
- 返回
  `Cache-Control: no-store`；响应只包含版本、物业和游标，不含个人资料、流水金额或消息正文。
- 保持既有分页接口和写入事务不变，不增加新服务或同步进程。

## 3. 岸岸消费者的保存与恢复

1. 以可信 PMS 来源实例和物业共同标识事件流。首次读取事件头 H 后，在一个本地事务中保存
   `baseline_cursor=H` 与 `checkpoint=H`，成功后才报告初始化完成。
2. 取得 H 后、保存 H 前提交的事件序号大于 H，后续分页可以接住。尚未成功保存基线时不得宣称启用成功；保存结果不明先查本地状态，已有基线就复用，不能重采样覆盖。并发初始化只允许一个结果生效。
3. 基线持久化后，从 `sequence > checkpoint`
   开始消费。每页 Inbox、事件去重、事项关联或创建、checkpoint 推进须在同一事务中完成。重启和重试不能丢事件，也不能重复创建提醒。
4. `sequence <= H` 不补发历史提醒；PMS 中 `HISTORICAL`、`MATCHED`
   等业务状态仍由 PMS 决定。事件只提供信息，不代替人员授权、方案确认或实际到店依据。
5. 来源实例改变或发现游标回退，应明确报告数据缺口，不能静默重置基线或重放全部历史。缺少所提接口时保持阶段 3 的真实接线缺口，不回退到时间过滤或把一页尾游标当基线。

Agent OS 任务继续消费者逻辑、模拟契约测试和其余已授权工作；不得把模拟接口当作 Green
PMS 已实现，不在此任务修改 PMS 源码。

## 4. 最小 Green PMS 修改范围

| 文件                                                      | 变更                                                                        |
| --------------------------------------------------------- | --------------------------------------------------------------------------- |
| `apps/api/src/external-payments.ts`                       | 新增按现有身份及物业 READ 权限保护的事件头 GET 路由、响应 schema 与禁止缓存 |
| `packages/db/src/external-payments.ts`                    | 查询已提交事件头，缺省为 `"0"`，完整保留 bigint 字符串                      |
| `packages/contracts/src/external-payments.ts`             | 增加事件头响应类型                                                          |
| `tests/integration/external-payments.integration.test.ts` | 在原集成套件中覆盖权限、事务可见性和序号语义                                |
| `docs/implementation/spec-wecom-external-payments.md`     | 记录接口、首次基线含义和恢复边界                                            |

新增数据库迁移、数据库权限、依赖、CI
job、workflow、部署机制：均为 0。无订单、房态、价格或资金写入变更。遵循 PMS 原测试锁和隔离数据库约定，不占用其他任务的人工验收实例。

## 5. 验证与交付要求

- PMS：空流为
  `"0"`；超过一页历史数据时得到真实事件头；未认证、跨物业、授权撤销被拒绝，合法 READ 主体可读。
- 事务：在采样前开始、采样后提交的事件仍可从 H 后读到，即使 `occurredAt`
  早于采样；覆盖同物业并发串行化、回滚不产生可见事件、重试不重复记事件。
- 游标：超过 JavaScript 安全整数的合法 bigint 序号在响应及消费中保持字符串完整性。
- Agent
  OS：首次初始化并发与崩溃恢复、已保存基线不可覆盖、跨页追赶、事项与 checkpoint 原子提交、重复消费和已匹配流水不重复催办。
- 记录实际 PostgreSQL 版本。PG17 本地通过不等于 PG18 或生产已经验证；真实端到端证据仍须贯穿人员授权、broker、插件及 PMS。

PMS 项目独立任务承接源接口与主动投递；两项目保留独立分支、提交、测试环境和报告，由总指挥协调版本化契约。提交、PR 准备与源码测试不授权合并、发布、部署、真实 PMS 操作或外发消息。
