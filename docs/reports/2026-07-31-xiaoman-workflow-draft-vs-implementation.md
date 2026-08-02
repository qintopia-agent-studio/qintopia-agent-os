# 小满工作流草案与现有实现对照（修订版）

日期：2026-07-31

最新状态：2026-08-02，`master` 已合并 #358 / #359

对照文档：xiaoman-minimal-activity-reminder-workflow.md（2026-07-30）

代码基线：当前 `master`

## 1. 两个群 vs 当前实现

### 草案要求

| 群               | 成员                                 | 消息发送者                 | 主要用途                                     |
| ---------------- | ------------------------------------ | -------------------------- | -------------------------------------------- |
| 社区居民群       | 社区居民、活动参与者                 | 小满生成内容，通知二花发送 | 收集活动计划、发布日程和提醒、收集参与者反馈 |
| 工作群（活动群） | 研发人员、活动负责人、社区营造负责人 | 小满直接发送               | 协作、监控流程、确认接龙匹配、催办负责人回填 |

工作群中的每一条小满工作消息都要 `@活动负责人 / 社区营造负责人`。

### 当前实现

**社区居民群**：

- 小满通过 `qintopia_xiaoman_activity_announcement_prepare` 生成文案
- 生成 `operator_review_message`（给刘珊确认）和 `erhua_handoff_draft`（给二花执行）
- 二花通过 `qintopia_xiaoman_activity_text_group_message_request_prepare` 接收请求
- 最终由 `erhua.send_group_message` 发送到企微群
- 状态：存在，但 QiWe image-send 生产激活仍需外部 evidence 证明

**工作群**：

- 小满生成的文案中标记 `safe_for_operations_chat: true`，表示可以发到工作群
- 但**没有独立的工作群发送通道**
- 当前所有群发都依赖二花/QiWe，没有区分居民群和工作群的路由
- 文案中没有 `@` 功能，而是直接称呼"刘珊"

### 差距

1. 没有独立的工作群发送通道，小满不能直接发工作群
2. 没有 @负责人 的功能，当前是直接称呼姓名
3. 所有群发统一走二花/QiWe，无法区分居民群和工作群

## 2. 三张数据表 vs 当前实现

### 草案要求

| 数据表     | 一条记录代表什么                       | 主要写入者             |
| ---------- | -------------------------------------- | ---------------------- |
| 活动计划表 | 一项活动计划                           | 活动发起人或代填负责人 |
| 活动发生表 | 一次实际发起的群接龙 / 活动发生        | 二花                   |
| 活动反馈表 | 一个人对一场活动的一次反馈或负责人回填 | 飞书表单               |

### 当前实现

| 数据表     | 代码中的 table_role   | 写入者                                                 | 状态      |
| ---------- | --------------------- | ------------------------------------------------------ | --------- |
| 活动计划表 | `activity_plan`       | 人工在飞书 Base 中填写                                 | ✅ 已实现 |
| 活动发生表 | `activity_occurrence` | 二花通过 `skills/qiwe/solitaire/feishu_writer.py` 写入 | ✅ 已实现 |
| 活动反馈表 | 无                    | 无                                                     | ❌ 不存在 |

**活动发生表写入路径**：

- 二花通过 `skills/qiwe` skill 监听企微群消息
- `skills/qiwe/solitaire/activity_service.py` 的 `upsert_from_solitaire()` 解析接龙内容
- 调用 `skills/qiwe/solitaire/feishu_writer.py` 的 `FeishuActivityWriter` 写入飞书 Base
- 通过环境变量
  `QIWE_ACTIVITY_FEISHU_MAPPING`、`QINTOPIA_FEISHU_ACTIVITY_APP_TOKEN`、`QINTOPIA_FEISHU_ACTIVITY_TABLE_ID`
  配置写入路径
- 使用 `QINTOPIA_FEISHU_ACTIVITY_WRITE_ENABLE=true` 作为 live write 的开关

### 差距

1. 缺少 `activity_feedback` 表，无法存储参与者反馈和负责人回填
2. 需要新增 `activity_feedback` table_role 到 `TABLE_ROLES`

## 3. 三个工作表单 vs 当前实现

### 草案要求

- 表单 A：活动计划收集表（活动计划表的飞书表单视图）
- 表单 B：参与者活动反馈表（活动反馈表的飞书表单视图）
- 表单 C：负责人活动回填表（同一张活动反馈表的另一个飞书表单视图）

### 当前实现

| 表单                     | 当前实现                                                                          | 状态                      |
| ------------------------ | --------------------------------------------------------------------------------- | ------------------------- |
| 表单 A：活动计划收集表   | 居民直接在飞书 Base 中填写 `activity_plan` 记录，没有独立的"计划收集表单视图"封装 | ⚠️ 概念存在，未封装为表单 |
| 表单 B：参与者活动反馈表 | 无 `activity_feedback` 表，无此表单                                               | ❌ 不存在                 |
| 表单 C：负责人活动回填表 | `activity_occurrence` 有 `material_summary` 字段，但没有独立表单入口和催办逻辑    | ⚠️ 字段存在，流程缺失     |

### 差距

1. 缺少活动反馈表，无法实现表单 B 和 C
2. 缺少表单 A 的封装（虽然可以直接在 Base 中填写）
3. 缺少表单 C 的催办和逾期逻辑

## 4. 两类关联 vs 当前实现

### 草案要求

#### 4.1 反馈与活动发生的关联

- 表单 B 和 C 中设置必填项"本次活动"，显示为"日期 + 社区 + 活动主题"
- 填写者选择后，反馈记录直接关联活动发生记录
- Base 自动形成反向关联并汇总反馈数、评分和照片

#### 4.2 活动计划与活动发生的关联

- 二花把接龙写入活动发生表，暂不关联计划
- 小满按群、计划状态和时间范围筛选候选计划
- 小满比较活动主题、介绍、地点和接龙原文
- 只有一个明显候选时，小满建立双向关联
- 有歧义时，小满在工作群 @负责人，让负责人选择候选计划或"即兴活动"

### 当前实现

**反馈与活动发生关联**：

- 无反馈表，无此关联
- ❌ 不存在

**计划与发生关联**：

- 生产数据中 `activity_occurrence.notes`
  字段有"由小满匹配计划候选、判断是否需要人工确认、补录或宣发"
- 说明当前已有匹配意图，但**没有显式的关联字段**
- 小满通过 `qintopia_xiaoman_activity_list_by_date` 读取两张表
- 通过 `announcement_prepare` 中的 `_xiaoman_activity_announcement_records` 合并记录
- 但**没有建立双向关联的代码**

### 差距

1. 反馈与发生关联：完全缺失，需要先建反馈表
2. 计划与发生关联：只有启发式匹配（按日期、主题），没有稳定的外键关联，也没有歧义处理逻辑

## 5. 最简时间流程 vs 当前实现

### 草案要求

| 时间            | 社区居民群           | 工作群（活动群）                       | 数据动作                         |
| --------------- | -------------------- | -------------------------------------- | -------------------------------- |
| 每周六、周日    | 二花发送表单 A       | 小满可发送收集进度并 @负责人           | 新增活动计划                     |
| 前一天晚上      | 二花发送次日活动预告 | 小满发送完整清单，提醒未发接龙的负责人 | 合并读取计划表和发生表           |
| 每天早上        | 二花发送今日活动安排 | 小满发送今日工作清单并 @负责人         | 再次合并两张核心表，纳入即兴活动 |
| 活动前约 1 小时 | 二花发送参与提醒     | 小满提醒负责人确认接龙和现场准备       | 更新提醒状态                     |
| 活动后约 2 小时 | 二花发送表单 B       | 小满发送表单 C 并 @负责人              | 参与者反馈和负责人回填进入反馈表 |
| T+24h / T+48h   | 不公开催办负责人     | 未交表单 C 时，小满继续 @负责人        | 最后标记待人工跟进               |

### 当前实现

**systemd timer**：

| timer                                                                     | 用途                     | 触发频率 |
| ------------------------------------------------------------------------- | ------------------------ | -------- |
| `qintopia-agentos-xiaoman-activity-signal-worker.timer`                   | signal ingest            | 每分钟   |
| `qintopia-agentos-xiaoman-activity-promotion-starter-worker.timer`        | promotion starter        | 每分钟   |
| `qintopia-agentos-xiaoman-activity-image-generation-starter-worker.timer` | image generation starter | 每分钟   |
| `qintopia-agentos-xiaoman-activity-send-request-starter-worker.timer`     | send request starter     | 每分钟   |

**时间点对照**：

| 草案时间点                | 当前实现                                                                    | 状态                      |
| ------------------------- | --------------------------------------------------------------------------- | ------------------------- |
| 每周六、周日发表单 A      | 无专门 timer                                                                | ❌ 缺失                   |
| 前一天晚上发次日预告      | `announcement_prepare` 支持 `next_day_preview` mode，已用脱敏样例验证可生成 | ⚠️ 工具存在，触发机制不明 |
| 每天早上发今日安排        | `announcement_prepare` 支持 `same_day_preview` mode                         | ⚠️ 工具存在，触发机制不明 |
| 活动前约 1 小时参与提醒   | 无专门 timer                                                                | ❌ 缺失                   |
| 活动后约 2 小时发表单 B/C | 无反馈表，无法发表单 B/C                                                    | ❌ 缺失                   |
| T+24/T+48/T+72h 催办      | 内部 followup worker + 第三轮 operations-lead escalation 已合并             | ✅ 代码已合并；生产未启用 |

**二花的提醒功能**：

- `skills/qiwe/solitaire/reminder.py` 的 `ReminderWorker` 可以发送活动提醒
- `render_reminder_text()` 生成提醒文案：活动提醒、时间、地点、当前报名、参与人
- 但这是针对单个活动的提醒，不是按时间点的批量通知

### 差距

1. 除 T+24/T+48/T+72h 素材催办外，仍缺少其它按业务时间点触发的 timer（每周六日、前一天晚上、每天早上、活动前 1 小时、活动后 2 小时）
2. 缺少反馈表，无法在活动后 2 小时发表单 B/C
3. 现有的 4 个 timer 都是每分钟触发的 worker，不是按业务时间点触发

## 6. 当前已有的 skill 工具清单

### 小满（xiaoman）

来源：`skills/qintopia-tools/variants/xiaoman/__init__.py:80-89`

| 工具                                                           | 功能                                                                | 状态                          |
| -------------------------------------------------------------- | ------------------------------------------------------------------- | ----------------------------- |
| `qintopia_xiaoman_activity_record_get`                         | 按 record_ref 读单条记录                                            | ✅ 已实现                     |
| `qintopia_xiaoman_activity_list_by_date`                       | 按日期读计划/发生表                                                 | ✅ 已实现                     |
| `qintopia_xiaoman_activity_announcement_prepare`               | 生成文字预告/提醒文案                                               | ✅ 已实现（已用脱敏样例验证） |
| `qintopia_xiaoman_activity_text_group_message_request_prepare` | 准备二花发群请求                                                    | ✅ 已实现                     |
| `qintopia_xiaoman_activity_status_update`                      | 更新 AgentOS event_signal 状态                                      | ✅ 已实现                     |
| `qintopia_xiaoman_activity_gap_update`                         | 更新缺口摘要                                                        | ✅ 已实现                     |
| `qintopia_xiaoman_activity_phase_update`                       | 更新活动阶段（pre_event / in_event / post_event）                   | ✅ 已实现                     |
| `qintopia_xiaoman_activity_handoff_create`                     | 创建 handoff work item（只支持 `visual_asset_request -> huabaosi`） | ✅ 已实现                     |
| `qintopia_xiaoman_activity_promotion_review_draft`             | 生成宣发评审文案                                                    | ✅ 已实现                     |
| `qintopia_xiaoman_activity_material_summary`                   | 读取/汇总素材字段                                                   | ✅ 已实现                     |

### 二花（erhua）

| 工具                                        | 功能                           | 状态              |
| ------------------------------------------- | ------------------------------ | ----------------- |
| `skills/qiwe/solitaire/activity_service.py` | 监听企微群接龙，写入活动发生表 | ✅ 已实现         |
| `skills/qiwe/solitaire/feishu_writer.py`    | 写入飞书 Base                  | ✅ 已实现         |
| `skills/qiwe/solitaire/reminder.py`         | 发送活动提醒                   | ✅ 已实现         |
| `erhua.send_group_message`                  | 发送企微群消息                 | ⚠️ 已实现但未激活 |

## 7. 草案与现有实现的冲突点

### 已澄清（不是冲突）

1. **谁写活动发生表**
   - 草案：二花根据居民群接龙直接写入
   - 当前实现：✅ 已实现，通过 `skills/qiwe/solitaire/feishu_writer.py` 写入飞书 Base
   - 结论：一致，不冲突

### 真正的冲突/差距

1. **工作群直接发送**
   - 草案：小满直接在工作群 @负责人
   - 当前：小满没有独立发送工作群的通道，所有群发依赖二花/QiWe，且没有 @ 功能
   - 冲突：需要新增小满的发送能力或重新分配角色

2. **活动反馈表**
   - 草案：有活动反馈表，用于参与者反馈和负责人回填
   - 当前：无此表
   - 冲突：需要新增表和对应的表单、关联、催办逻辑

3. **时间点触发**
   - 草案：有多个按时间点触发的通知（每周六日、前一天晚上、每天早上、活动前 1 小时、活动后 2 小时、T+24/48h）
   - 当前：只有 4 个每分钟触发的 worker，没有按业务时间点触发的机制
   - 冲突：需要新增 timer 或扩展现有 worker 的职责

4. **海报过渡方案**
   - 7/20 纪要：海报生成调用 IMG 2 能力，生成后存入飞书多维表格
   - 草案首版边界：暂不做海报
   - 冲突：需要回到纪要重新对齐，当前代码中 `visual_asset_request -> huabaosi`
     已经是图片生成 handoff

5. **临时约饭类活动**
   - 7/20 纪要：临时约饭类活动无需宣发
   - 草案：即兴活动也进入通知流程
   - 冲突：需要在小满过滤规则中明确排除或包含

## 8. 如果要按草案落地的最小改动清单

按优先级排序：

1. **新增 `activity_feedback` 表**（或先在 `activity_occurrence.material_summary`
   中凑合）
2. **新增/调整 timer**：T+24/T+48/T+72h 催办已落地；活动前 1 小时提醒、活动后 2 小时回填等其它业务 timer 仍未实现
3. **实现负责人回填与逾期催办闭环**：内部 AgentOS 催办链路已落地；外部发送、Feishu 写回、遗漏标记仍未授权自动执行
4. **工作群发送通道**：如果需要小满独立发工作群，需新增 skill/adapter；否则把催办消息也通过二花发送
5. **对齐海报规则**：要么在首版保留 IMG 2 海报过渡方案，要么与纪要同步更新
6. **明确临时约饭处理**：在小满过滤规则中明确排除或包含

## 9. 建议的下一步

不直接按草案全量实现，而是先确认已合并的最小闭环边界：

1. 已用现有 `activity_occurrence.material_summary` 作为负责人回填判断字段（不新增表）
2. 已新增素材催办扫描和 worker：默认按小满业务时区覆盖 T+24/T+48/T+72h
3. 催办先创建内部 `xiaoman.material_followup_request` / `activity_recap_request` work
   item，不直接创建外部发送
4. 跑通 1-2 次真实活动后，再决定是否拆分 `activity_feedback` 表和新增工作群独立发送通道

## 10. 实现进展（2026-07-31 更新）

第 9 节建议的素材催办最小闭环已合并到 `master`：

- 新增 `material-followup-scan` 操作（`runtime/sidecar/src/xiaoman_activity.rs`）：扫描
  `activity_occurrence` 中指定日期已结束且 `material_summary`
  为空的记录，为每个待催办活动创建内部 `xiaoman.material_followup_request` /
  `activity_recap_request` 工作项，不直接创建
  `erhua.send_group_message`；幂等键绑定业务日期、脱敏 `source_record_ref`
  和催办轮次，避免同日同名活动或不同轮次互相去重。
- 该内部 recap 根现在能被现有 downstream starter 接上：补出复盘 evidence +
  visual 子任务；后续只有在普通 artifact 审核通过后，才会继续创建 image-generation 和 awaiting-publish
  group-message request 工作项。
- #359 补齐 T+24/48/72h 三轮扫描目标：未显式传 `date`
  时按小满业务时区扫描昨天、前天、大前天；第三轮只生成 `operations_lead`
  升级草稿，并在 payload/source refs 标记
  `material_followup_attempt=3`、`escalation_required=true` 和
  `external_send_executed=false`；第三轮还会在 source
  refs、payload、metadata 和创建审计事件中写入
  `escalation_stage=third_attempt_overdue`、 `escalation_level=operations_lead` 和
  `material_followup_terminal_attempt=true`。
- 显式传 `date` 的本地复放和预检可以同时传 `material_followup_attempt=1|2|3`
  选择对应轮次；未传时保持第一轮兼容行为，默认 timer 不依赖该字段而是一次覆盖三轮。
- 新增 CLI 命令 `run-xiaoman-activity-material-followup-worker`（`--check-only` /
  `--once` / `--apply` / `--poll-seconds`）。
- 新增 systemd 单元
  `qintopia-agentos-xiaoman-activity-material-followup-worker.{service,timer}`，渲染脚本已包含，默认 1 小时轮询，可用
  `QINTOPIA_XIAOMAN_ACTIVITY_MATERIAL_FOLLOWUP_TIMER_INTERVAL` 覆盖。
- 与第 9 节建议的差异：没有复用 `announcement_prepare` 的 `post_event_followup`
  mode，而是直接在 worker 内生成固定格式催办文案。原因：首版目标是最小闭环，固定文案可以先用起来；后续如需个性化文案，再接入
  `announcement_prepare`。群发仍必须通过已批准文本 artifact 后的独立 Erhua
  `group_message_request` 路径。
- 尚未做：生产环境 timer 启用和真实活动端到端验收（属 owner 决策，需走发布流程）；第三次逾期当前只生成升级草稿，不自动标记遗漏、不写 Feishu、不触发外部发送。

## 11. 当前结论（2026-08-02）

- 小满素材催办的仓库内主线已经完成：T+24/T+48/T+72h 扫描、幂等、内部 work
  item、下游 recap 连接、第三轮 operations-lead escalation 审计都已合并。
- 小满 V3 direct poster
  closeout 代码也已完成：#358 增加 release-local 受保护配置事务，#359 之后当前 `master`
  没有已知同类代码缺口。
- 仍未完成的是生产闭环，不是继续写催办代码：需要 owner 发布 Release、应用生产配置、启用对应 timer/服务、跑真实活动并保留 sanitized
  evidence。
- 如果继续做仓库内功能，下一批才是草案里的新能力：`activity_feedback`
  表、表单 B/C、活动前 1 小时提醒、活动后 2 小时回填、独立工作群发送/@负责人。
