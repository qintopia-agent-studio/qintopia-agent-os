# 秦托邦 Agent OS：协同架构与身份基础设施复习笔记

日期：2026-09-08

用途：复习和研究 QinTopia Agent
OS 的统一人员身份、Agent 协作、WorkItem、MCP/Tool 与 Hermes 边界。本文是学习笔记和架构讨论记录，不是开发规格、数据库迁移或生产操作指令。

## 一、先记住十个结论

1. **Hermes 是 Agent runtime，Agent OS 是协同控制面。**
   Hermes 运行 profile、会话和工具；Agent
   OS 保存跨 Agent 的身份、权限、事件、WorkItem、Artifact、审计和恢复事实。
2. **Agent 之间的正式合作方式是 `MCP/Tool + WorkItem`。**
   MCP/Tool 负责调用能力，WorkItem 负责可靠交接和持续跟踪。
3. **不是每一次 Tool 调用都需要 WorkItem。**
   快速、同步、只读的查询可以直接调用；跨 Agent、异步、外部发送、需要重试或人工接管的工作应创建 WorkItem。
4. **`senderId` 在二花 QiWe 链路中是 `qiwe_user_id`。**
   它是中转协议内用于识别用户和回复用户的渠道身份，不是显示名、消息流水号，也不是自然人主键。
5. **`senderId` 关联已确认的 `person_id` 后，二花就能“认识人”。**
   但 Agent 只能获得授权的安全上下文，不能读取完整人员档案或未经授权的原始聊天。
6. **QiWe `senderId` 不能直接等同于企业微信官方 `external_userid`。**
   两者属于不同平台命名空间；跨平台合并需要额外映射证据或人工确认。
7. **跨 Agent 能力的契约注册在 Agent OS。**
   Agent 自己保留专业实现、内部私有能力、profile 和运行配置；需要跨 Agent 调用或持久交接的能力必须有 Agent
   OS capability 注册。
8. **Hermes Kanban 不再是业务协同事实源。** 它是旧的 runtime
   surface，可作为历史兼容材料，但身份、权限、任务状态、幂等、审核、恢复和审计应由 Agent
   OS/Postgres 承担。
9. **最成熟的现有标准协作链是小满活动链路。**
   小满产生事件和 WorkItem，文渊阁提供证据，画报司生成 Artifact，二花负责受控发送。
10. **Hermes 官方更新仍然可以跟随，但要选择性同步。**
    跟随 runtime、插件、MCP、Tool、profile 和 cron 的公开契约；保持 Agent
    OS 的领域模型独立，并用兼容测试和回滚保护升级。

## 二、项目最初是不是这样设计的

结论：**分层原则是项目设立之初的设计方向，具体的 WorkItem 控制面是后续逐步落地的实现。**

从项目入口和早期包边界可以看到，项目一开始就把系统拆成 Agent、Skill、Workflow、MCP、Runtime、Deploy 和外部系统等边界。后来又把这些原则具体化为
`capabilities`、`work_items`、`artifacts`、`work_item_events` 和审计记录。

因此需要区分两件事：

- 初始设计已经确定：Hermes 运行 Agent，业务协作不能依赖 prompt 和运行时临时状态。
- 后续实现才补齐：Agent
  OS 控制面表、能力注册、幂等键、claim/lease、Artifact 审核和后台 Worker。

项目不是一开始就把所有对象都实现完，而是沿着同一条架构原则逐步演进。

核心证据：

- [项目 README](../README.md)
- [Agent OS Architecture Overview](../../architecture/agent-os-overview.md)
- [Agent Contracts](../../agent-os/agent-contracts.md)
- [Operations control plane migration](../../../runtime/postgres/migrations/202606300007_operations_control_plane.sql)

## 三、为什么 Agent OS 适合做“合作智能总线”

“合作智能总线”不是一个会替所有 Agent 思考的超级 Agent，而是把身份、任务、权限、工具、人工操作和外部系统连接起来的控制面。

```text
渠道 / 表单 / 定时任务 / 人工输入
                ↓
      入口适配、认证、去重、规范化
                ↓
      Agent OS 身份、权限、事件和任务
                ↓
   Hermes Agent / Worker / MCP / 外部适配器
                ↓
       产物、业务结果、人工审核、审计
```

社区运营同时需要处理人员关系、住宿事实、活动、群聊、内容产出、人工确认、外部发送和失败恢复。把这些共享问题放到 Agent
OS，能避免每个 Agent 自己保存一套人员和任务状态。

架构价值主要体现在：

| 能力         | 没有共享控制面时的风险          | Agent OS 的解决方式                    |
| ------------ | ------------------------------- | -------------------------------------- |
| 统一认识人员 | 同一个人被不同 Agent 当成不同人 | `person_id` + `ChannelIdentity`        |
| Agent 交接   | 直接调用丢状态、超时后无法恢复  | 持久化 `WorkItem`                      |
| 外部发送     | 串人、串群、重复发送            | 权限、白名单、幂等键、审核和发送回执   |
| 内容产出     | 文件名或聊天消息无法追溯        | `Artifact`、版本、hash、来源和审核状态 |
| 失败处理     | Agent 重启后任务消失            | claim/lease、重试、死信和人工接管      |
| 责任追踪     | 不知道谁请求、谁批准、谁执行    | Event、审计和 human workbench 引用     |

它要真正成为总线，需要继续坚持三个条件：跨 Agent 任务统一使用 WorkItem；人员和渠道身份统一使用
`person_id` 及权限投影；所有高风险外部动作都能幂等、审计、恢复。

## 四、WorkItem、MCP 和 Tool 的关系

### 4.1 三个概念分别是什么

| 概念       | 主要问题                            | 白话解释                                                                  |
| ---------- | ----------------------------------- | ------------------------------------------------------------------------- |
| `Tool`     | “具体能做什么？”                    | 一个可调用的函数或动作，例如查询人员上下文、创建 WorkItem、发送 QiWe 消息 |
| `MCP`      | “Agent 怎样发现和调用能力？”        | 一种工具服务协议和边界，负责暴露 schema、权限和调用结果                   |
| `WorkItem` | “这件跨 Agent 的工作怎样可靠完成？” | 一条持久化任务，保存目标、状态、幂等、重试、审核和结果                    |

```text
Agent A
  -> MCP/Tool：创建 WorkItem
  -> Agent OS：持久化任务、权限和幂等信息
  -> Agent B / Worker：领取 WorkItem
  -> MCP/Tool：调用具体业务能力
  -> Event / Artifact：记录状态和产物
```

MCP 和 Tool 不是 WorkItem 的替代品。它们是调用入口；WorkItem 是异步协作事实。

### 4.2 “客房 Agent 让二花通知”时谁拥有什么

```text
客房 Agent
  │
  ├─调用 Agent OS 的 create_work_item Tool
  │  （该 Tool 可通过 Agent OS MCP 暴露）
  ▼
Agent OS 控制面
  │
  ├─保存 welcome_distribution WorkItem
  ├─校验 target_agent、capability、权限和幂等键
  └─等待二花 Worker 领取
  ▼
二花 Agent / Worker
  │
  ├─claim WorkItem
  ├─取得授权人员、入住、楼栋和 Artifact 上下文
  └─调用自己的 QiWe 发送 Tool
  ▼
宿舍群
```

推荐的任务引用如下：

```json
{
  "work_item_type": "welcome_distribution",
  "target_agent": "erhua",
  "capability_key": "erhua.send_group_message",
  "subject_refs": {
    "person_id": "...",
    "stay_id": "...",
    "artifact_id": "..."
  },
  "idempotency_key": "..."
}
```

客房 Agent 不应直接调用二花内部的 QiWe Tool。它只提交一项经过 Agent
OS 授权的工作；二花负责执行自己拥有的渠道能力。

### 4.3 什么时候不用 WorkItem

- 查询当前人员的安全称呼：可以同步调用 context Tool。
- 查询公开 FAQ：可以同步调用知识 MCP。
- 读取当前 PMS 房态：可以同步调用只读 PMS Tool。
- 创建欢迎发送、发送群消息、生成卡片、需要重试或等待人工确认：应创建 WorkItem。

一个实用判断句是：**如果调用方结束进程后，这件工作仍然必须继续，就应该使用 WorkItem。**

## 五、`senderId` 的正确定位

### 5.1 在二花链路中的语义

QiWe 适配器从中转 webhook 的 `raw_event.senderId` 读取用户标识。仓库契约规定它用于：

1. 识别当前发言人；
2. 组成 `chat_id + senderId` 的会话隔离键；
3. 调用 QiWe 用户/联系人查询接口；
4. 在私聊场景作为 `/msg/sendText` 的 `toId`；
5. 在群聊场景作为当前用户的渠道身份。

因此在统一身份模型中建议保存为：

```text
platform = qiwe
id_type = qiwe_user_id
external_id = senderId
```

### 5.2 它怎样让 Agent “认识人”

```text
QiWe senderId
    -> ChannelIdentity(platform=qiwe, id_type=qiwe_user_id)
    -> 已确认的 person_id
    -> Agent OS 权限过滤
    -> safe_summary / safe_reply_hints / 当前业务上下文
    -> 二花针对性回复
```

关联成功只是“可以开始查找授权上下文”，不是“开放完整档案”。还必须检查：

- `ChannelIdentity` 的命名空间是否正确；
- 关联是否为 `confirmed`，而不是猜测或待确认；
- 关联是否已撤销或过期；
- 当前群、当前 Agent 和当前动作是否有访问权限；
- 返回的是安全投影，而非原始档案、完整消息和不必要的敏感字段。

### 5.3 它与企业微信 `external_userid` 的关系

二者不应直接等同：

```text
QiWe senderId       = QiWe 中转命名空间的 qiwe_user_id
WeCom external_userid = 企业微信官方外部联系人命名空间
```

如果中转平台没有提供可审计的映射，就分别保存两个
`ChannelIdentity`，通过人工确认或外部证据关联到同一个 `person_id`。`senderId`
可以让二花在 QiWe 内认识人，但不能自动证明该用户在企业微信官方 API 中是哪一个
`external_userid`。

## 六、哪些 Agent 已经采用这种工作模式

### 小满：最完整的跨 Agent 协作方

小满可以把活动信号或人工确认转换成受控 WorkItem，再推动证据、视觉和发送流程。典型链路是：

```text
活动 Event
  -> 小满 activity_promotion_request
  -> 文渊阁 evidence_summary
  -> 画报司 poster_brief / generated_image Artifact
  -> 二花 group_message_request
  -> QiWe 发送 Worker
```

小满的 Agent 包明确要求不能通过 raw prompt 调用其他 Agent，必须走 Agent OS
capabilities。

### 画报司：能力提供方和 Artifact 生产方

画报司通过 `huabaosi.create_visual_asset`、`huabaosi.generate_image_asset`
接收受控请求，生成待审核的视觉 Artifact。它不直接发布，也不把图片生成等同于已经外发。

### 文渊阁：只读证据能力提供方

文渊阁通过 `wenyuange.retrieve_evidence`
提供证据摘要和来源简报，主要是只读能力。它可以被小满、画报司、司老师等调用，但不负责业务事实写入或群发送。

### 二花：部分采用标准 WorkItem

二花已经有受控群发能力和早报卡片发布能力，可以接收 `group_message_request`
等任务，并通过 QiWe 适配器发送。普通群聊问答、当前会话回复和部分投诉入口仍主要走 Hermes/QiWe 适配器，不是每条消息都经过 WorkItem。

### 司老师、管二爷和客房 Agent

- 司老师可以作为部分能力的允许调用方，参与运营任务，但目前不是最完整的 WorkItem
  provider。
- 管二爷主要负责工程计划、验证和技术交接，当前没有证据表明它已有完整的 AgentOS 业务 WorkItem
  provider。
- 客房 Agent 尚未正式接入住院事件、PMS 住宿事实和 `welcome_distribution`
  WorkItem，需要后续按统一契约接入。

## 七、为什么不用 Hermes 原来的 Kanban 作为核心协同机制

Hermes
Kanban 解决的是运行时任务展示和调度的一部分，但 QinTopia 的核心问题更广：统一人员、跨系统关联、权限、外部发送、幂等、重试、人工审核、审计和灾后恢复。

如果把 Hermes Kanban 当作系统事实源，会产生这些耦合：

- Hermes 进程或 profile 状态影响业务任务是否存在；
- Kanban 任务和 PMS、Feishu、QiWe、Person 之间的关系不一定可追踪；
- 任务可能没有统一的业务幂等键、权限上下文和数据分类；
- Hermes 升级会直接改变业务协作状态；
- 其他 Worker、人工工作台和外部系统无法稳定共享同一任务事实。

因此当前方向是：

```text
Hermes Kanban：旧 runtime surface / 兼容或审计材料
Agent OS WorkItem：业务协同事实、状态、权限、恢复和审计
```

这不是放弃 Hermes，而是把 Hermes 放回运行时应有的位置。

## 八、Hermes 官方更新还能不能跟

可以，但采用“选择性跟随、边界适配”的方式。

### 应该跟随

- Hermes runtime 和 Gateway 的稳定版本；
- 官方 plugin/platform API；
- MCP/Tool 调用协议；
- profile、会话和 cron 的公开配置契约；
- 官方安全、稳定性和可观测性改进。

### 不应重新依赖

- Hermes 内部 Kanban 作为业务数据库；
- Hermes 私有表或未承诺兼容的内部 API；
- prompt handoff 作为跨 Agent 正式协议；
- profile、缓存和会话文件作为人员或业务事实。

推荐升级路径：

```text
Hermes 新版本
    ↓
兼容层：profile / plugin / MCP adapter / QiWe adapter
    ↓
稳定的 Agent OS 契约：Person / Event / WorkItem / Artifact / Audit
```

即使当前代码和架构改变量较大，也可以跟随，因为 Agent
OS 不应该依赖 Hermes 内部业务状态。真正需要建立的是兼容矩阵：记录 Hermes 版本、插件版本、MCP
schema、profile schema、fixture 结果、升级风险和回滚版本。

## 九、低复杂度优化建议

这些优化不会引入新的总线、任务平台或微服务层。

### 1. 冻结最小交接接口

统一提供：

```text
create_work_item
claim_work_item
handoff_work_item
append_work_item_event
complete_work_item
fail_or_retry_work_item
```

业务差异只放到 `work_item_type`、`capability_key`、版本化 `payload_schema` 和
`subject_refs`。

### 2. 统一跨 Agent 的人员引用

所有涉及人员的 WorkItem 都优先引用：

```json
{
  "person_id": "...",
  "channel_identity_id": "...",
  "application_id": "...",
  "stay_id": "...",
  "artifact_id": "..."
}
```

这样入住通知、活动提醒、投诉跟进和欢迎卡片可以共享身份上下文。

### 3. 只保留一个 Agent OS 协作入口

可以由 Agent OS MCP 或受控 operations
Tool 暴露 WorkItem 操作。每个 Agent 不要各自实现一套任务创建、领取、重试和审计 API。

### 4. 规定同步与异步边界

- 同步只读查询：直接 MCP/Tool；
- 跨 Agent、外部发送、等待审核、需要重试的工作：WorkItem；
- Tool 不得绕过权限和 WorkItem 直接调用另一个 Agent 的内部能力。

### 5. 定时器只负责触发

定时任务可以放在 Agent
profile/runtime，但它产生的业务 Event、WorkItem、Artifact 和审计必须进入 Agent
OS。不要把业务状态只保存在 cron 文件、prompt 或本地缓存中。

### 6. 先用现有 Postgres 控制面

目前不需要为 Agent 协作再引入新的消息总线、队列产品或微服务。先把现有 Postgres、sidecar、MCP 和 Worker 的契约稳定，再根据真实吞吐和故障证据决定是否拆分。

## 十、学习时最容易混淆的地方

| 容易混淆                         | 正确理解                                                        |
| -------------------------------- | --------------------------------------------------------------- |
| `senderId` 是自然人的 ID         | 它是 QiWe 渠道身份；自然人由确认后的 `person_id` 表示           |
| 有 `senderId` 就可以读全部资料   | 只能读取当前 Agent 和会话有权读取的安全投影                     |
| MCP 就是任务系统                 | MCP 是调用协议；WorkItem 才是持久化任务协作机制                 |
| Tool 就是另一个 Agent            | Tool 是能力入口；提供能力的 Agent/Worker 仍有自己的边界         |
| 所有消息都必须创建 WorkItem      | 普通同步问答可以直接处理，异步和高风险工作才需要 WorkItem       |
| WorkItem 就是 Hermes Kanban 卡片 | WorkItem 是 Agent OS 的业务控制面记录，不依赖 Hermes Kanban     |
| 飞书 Base 是全部事实源           | Base 是申请入口和人工工作台；关键事件、状态和审计在 Agent OS    |
| PMS 记录和 Person 是一回事       | PMS 保存住宿事实；Agent OS 保存统一人员和跨系统关联             |
| 生成了 Artifact 就等于已经发送   | Artifact 还可能处于 pending、approved、rejected 或待发送状态    |
| Agent OS 是一个大 Agent          | Agent OS 是身份、权限、任务、事件和审计控制面，不替代专业 Agent |

## 十一、后续研究与开发前检查清单

### 需要继续核实的事实

- 现有数据库 `person_id` 的完整表结构、API、合并和撤销语义；
- PMS 事件、增量轮询、签名、版本和重放能力；
- QiWe `senderId` 与其他企微/微信官方标识是否存在中转映射；
- 欢迎卡片的 Artifact 生成、存储、审核和住客关联方式；
- 楼栋到宿舍群的服务器侧白名单映射；
- Feishu Workflow 的字段 ID、权限、重复投递、乱序和补偿路径；
- 当前生产 profile 和 Worker 是否已经使用目标版本的控制面。

### 开发前必须冻结的决策

- `welcome_distribution` 的 payload schema 和幂等键；
- 哪些人员上下文字段可以给二花，哪些只能给客房或工作人员；
- 哪些身份关联可以自动建议，哪些必须人工确认；
- 哪些人员角色可以人工接管、审核和触发外部发送；
- PMS 事件不可用时的轮询间隔和对账窗口；
- Hermes 升级的版本范围、兼容矩阵和回滚版本。

### 首个建议闭环

```text
入住确认 Event
  -> Person / ChannelIdentity / Stay 关联校验
  -> 欢迎卡片 Artifact 关联
  -> welcome_distribution WorkItem
  -> 二花领取
  -> 服务器侧楼栋到群映射校验
  -> QiWe 受控发送
  -> 回执、重试、人工接管和审计
```

首期不应把自动身份合并、PMS 写操作、历史申请全量回放和跨平台身份自动合并一起纳入。

## 十二、推荐阅读顺序

1. [秦托邦 AgentOS 系统与数据关系图](秦托邦-AgentOS-系统与数据关系图.md)
2. [Agent OS Architecture Overview](../../architecture/agent-os-overview.md)
3. [Agent Contracts](../../agent-os/agent-contracts.md)
4. [Operations control plane migration](../../../runtime/postgres/migrations/202606300007_operations_control_plane.sql)
5. [QiWe architecture](../../../skills/qiwe/docs/architecture.md)
6. [QiWe Hermes platform plugin plan](../../../skills/qiwe/docs/plans/active/qiwe-hermes-platform-plugin.md)
7. [Activity promotion control plane](../../../workflows/activity-promotion/docs/agentos-operations-control-plane.md)
8. [Unified person identity foundation assessment](../../plans/active/unified-person-identity-foundation-assessment.md)
