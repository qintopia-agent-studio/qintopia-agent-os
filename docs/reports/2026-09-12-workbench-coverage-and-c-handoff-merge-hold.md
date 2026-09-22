# 工作台场景覆盖与 C 最小交接（暂停合并期间）

> 后续状态更新（2026-09-18）：相关人员工作台和本地欢迎代码已于 2026-09-15 经
> [PR #704](https://github.com/qintopia-agent-studio/qintopia-agent-os/pull/704)
> 合并至主线（`171e227`）。下文保留当时的验收记录；“未提交”、旧分支、临时目录及暂停合并描述不代表当前开发入口或待合并状态。当前实现范围与未完成接入见[模块说明](../../runtime/sidecar/src/person_collaboration/README.md)。合并不证明真实身份、统一业务授权或生产迁移完成，历史测试结果也不是本次重新执行的结果。

日期：2026-09-12。Owner：工作台任务。本工作包已收口；没有启动 C/D 实施。负责人已认可协作关系页面的视觉和大体逻辑。本报告接受这项决定，旧报告保留历史状态，不以未完成原稿对照否定本次认可。

结论：本地配置层主要行为已有证据，补齐了智能体/群生命周期和同人不同岗位的合成验证。发现 1 项可复现的群停用预览说明缺陷，留待工作台责任方修复。真实身份、二花实际配置消费及管理 Agent 持久分派仍未接通；暂停合并和服务器操作继续有效。

## 1. 基线、环境与本轮动作

依据主项目最新 `person-agent-foundation-local-validation-during-merge-hold.md`
第 3 节，以及共同契约 §5.3/§5.4、实施计划第三步；只读主项目，未复制覆盖旧指令文件。

独立工作区 `/private/tmp/agentos-org-workbench-confirmed-20260911`，分支
`codex/org-agent-workbench-confirmed`，基线 HEAD
`2815ab46f2cc0efb3e34cda56f6242bb9af5abf0`。 `WORKBENCH-DELTA.json`
的 14 个交付文件和 2 个继承指令文件再次核验全部匹配。原清单 SHA256：`04f81496dbe0a7f896dc68b9ee8daac6fc7d555a42182ceb1fb81b8fc3fe1452`。162 个承接文件不算本轮新增；本轮唯一新增文件是本报告。

复用 [9 月 11 日验收](2026-09-11-organization-agent-workbench-confirmed.md)
中的 21 项协作专项（含 Postgres/HTTP）、724 项常规测试及 47 项忽略记录，不重复跑完整套件。本轮只对尚缺独立证据的三个场景使用已有 HTTP
API；另在浏览器复现预览文案问题。

本地环境先只读核实：`agentos-welcome-v1-test` 容器运行
`pgvector/pgvector:pg18`，仅绑定 loopback 55439，库为
`qintopia_test`；本轮端口 18877 原为空闲。27 个版本化迁移的 SHA384 与库中成功记录全部一致，无待应用迁移。新增独立合成 tenant
`synthetic-collaboration-hold-20260912`，初始化前任职计数为 0。以现有合成专用二进制启动并初始化该 tenant，没有结构变更或真实适配器。

初始化配置版本 11；智能体/群验证后为 19，双岗验证后为 22。原有合成关系及授权在生命周期验证前后逐项相同，未重置 A/B 或原体验 tenant。

临时浏览器标签及 18877 测试进程已关闭，保留合成数据和审计供复现。本次检查时原 18875 没有监听；未重启它，也未改原启动脚本。

## 2. 覆盖矩阵

“已验证”指注明层级的证据；“缺证”指实现存在但尚无所述端到端案例；“未实现”指接线或能力尚未落地。

| 案例                            | 现有测试/浏览器证据                                  | 实际结果、缺口及下一步归属                                                                                                                       |
| ------------------------------- | ---------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| 社区负责人管理两栋、舍长仅本栋  | E1、E2；旧浏览器组织详情、越权会话、一/二栋检查      | 配置层已验证：管理不等于执行、跨栋/Agent/动作拒绝，上级撤销使依赖授权失效。完整社区经理双栋实际消费仍属 C                                        |
| 同名、一人多岗、换舍长          | E3、E4；旧浏览器 A/B 消歧、换一栋并核对二栋；本轮 N3 | 稳定身份消歧、同岗多范围/多智能体已有覆盖；本轮补证同一人两种不同岗位各有任职，结束其一不影响另一岗                                              |
| 确认人撤权/离任、尚有待执行任务 | E2、E5；旧浏览器无资格确认人被拒绝                   | 配置查询已验证：确认人撤权/到期后 denied，不变成自主权。未实现通用审批实例到实际任务执行的重验；不能称旧确认已在所有消费者失效。Owner：C         |
| 触达无效、并发冲突、重复保存    | E3、E4、E6；旧浏览器双窗口重新读取并保存             | 已验证：任职和触达整笔回滚；同键同请求一次生效，同键异请求拒绝；旧版本不覆盖，输入可恢复。未模拟提交成功但网络断连后的 UI 重试，作为后续定向用例 |
| 人员停用/恢复                   | E4；旧浏览器待核验登记与草稿状态                     | 已验证：停用已核验人员结束其关联；恢复不恢复旧 train；同名待核验记录不合并、不能任命。四类浏览器生命周期全走读仍缺证                             |
| 智能体停用/恢复                 | N1                                                   | 本轮 API 已验证：预览回滚，保存结束 1 项连接；恢复后旧业务权和触达均 denied，ended 历史保留。缺持久化自动回归，Owner：工作台                     |
| 群停用/恢复                     | N2、E7；本轮浏览器预览                               | 本轮 API 已验证：撤销绑定、群触达 denied；岗位和 train 保留；恢复群不恢复绑定/触达。预览通用文案错误，见 D1。Owner：工作台                       |
| 岗位停用/恢复                   | E4；旧浏览器一栋停用预览、恢复为空缺                 | 已验证：结束一栋关联、保留二栋；恢复不恢复任职/授权。职责/岗位定义在用引用的拒绝与历史保留见 E8，不能将定义停用等同任职解除                      |
| 同人跨渠道、陌生入群者          | E9；E7 对未知动态对象返回需 PMS 解析                 | 有精确渠道候选、歧义/过期拒绝等基础单测；未实现两 Gateway 共用的可信解析闭环。未发言成员、事件主体与操作者区分、共享账号不能冒充个人均待 C       |
| 管理 Agent 分派                 | E2 的委托边界；已有 operations 基础模块              | 委托权限判断已测；default→二花的真实发起人保留、持久受理、等待/撤销/回执链未实现。通用 MCP 仍为 dry-run，不能报告已受理或已完成。Owner：C        |

### 精确证据索引

以下函数均在本工作区源码，只读核对断言；原通过记录来自上述验收，不是本轮新跑次数。

- E1：[tests.rs](../../runtime/sidecar/src/person_collaboration/tests.rs) 中
  `management_is_not_execution_and_scope_is_not_a_label`、
  `parent_revocation_invalid_scope_and_cycles_fail_closed`。
- E2：同文件 `delegated_envelope_cannot_outgrow_its_source`、
  `delegation_handover_proxy_expiry_and_identity_revocation`；覆盖一栋管理可授 train，二栋/其他 Agent/publish 拒绝，以及离任、代理到期、身份撤销和旧授权不复活。
- E3：[duty_store_tests.rs](../../runtime/sidecar/src/person_collaboration/duty_store_tests.rs)
  中
  `confirmed_workbench_saves_assignment_and_contact_atomically`；预览全状态不变、无效群换任回滚、重放版本相同、新旧一栋决定权及二栋不变、重开可读、过期版本拒绝。
- E4：同文件 `organization_lifecycle_disambiguates_people_and_never_resurrects_grants`、
  `replacing_and_ending_connections_is_atomic_and_leaves_other_work_intact`。
- E5：同文件 `permission_modes_persist_and_reviewer_loss_never_becomes_autonomy`；
  `tests.rs` 的
  `reviewer_must_be_another_person_with_the_same_current_duty_agent_domain_and_scope`。断言明确
  `runtime_connected=false`；不代表已执行内容审核或队列消费。
- E6：`tests.rs` 的 `preview_save_replay_concurrent_commands_and_restart`、
  `cross_tenant_group_bounds_and_authenticated_http`；幂等记录及审计各一次、并发仅一方成功、跨 tenant 和伪造 HTTP 身份拒绝。浏览器失败恢复见旧报告“实际浏览器操作”。
- E7：`duty_store_tests.rs` 的
  `contact_scope_and_hierarchy_are_explicit_and_revocable`；管理来源撤销后
  `contact_authority_revoked`，未知动态成员返回
  `pms_membership_resolution_required`，公众接待仅公开信息。
- E8：同文件 `catalog_edits_do_not_grant_permissions_and_retirement_preserves_history`。
- E9：[context_tools.rs](../../runtime/sidecar/src/context_tools.rs) 中
  `answer_context_identity_resolves_direct_chat_from_qiwe_platform_user_identity`、
  `answer_context_identity_requires_materialized_qiwe_platform_identity`、
  `answer_context_identity_reports_conflict_for_multiple_qiwe_people`、
  `answer_context_identity_ignores_stale_qiwe_room_member_exact_chat`。这些是候选选择等基础单测，不能替代实际账号转换或两个 Gateway 联调。

## 3. 本轮新增合成证据与可复现缺陷

请求从本轮 loopback 页面取得会话，经过现有 `/api/preview`、`/api/save`、
`/api/decision`、`/api/contact-decision`。操作者由合成服务固定，无客户端自报身份。每条命令使用唯一 operation_id 和当前 expected_version；未把 cookie、对象完整标识写入报告。

| 新证据 | 最小复现与已观察结果                                                                                                                                                                                     |
| ------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| N1     | 为已登记小满完善合成台账；在三栋给人员甲 B 创建 train 自主、群回复自主的协作。停用预览不改变完整 state；保存结束 1 连接，train/群回复 denied；恢复台账后二者仍 denied，旧关系 ended                      |
| N2     | 三栋群完善台账，三栋人员甲 B 与二花配置 train 和群回复自主。停用群预览不改变 state；保存结束 0 连接、绑定被撤销，群回复 denied 而 train autonomous；恢复在册后绑定不恢复、群回复仍 denied，任职仍 active |
| N3     | 为同一人员甲 B 新建另一岗位定义并安排小满协作。两个不同 role 对应不同 appointment，二者 train 均自主；结束第二个 appointment 后仅第二个 denied，第一个仍自主                                             |

三个场景及生命周期操作后原 fixture 关系/授权保持检查均通过。它们是已有服务的针对性合成操作，尚未新增为版本化测试源码；后续应把 N1/N2/N3 纳入相应模块回归。

**D1 · P2：群停用的影响预览误称会结束任职关联。**

复现：在 N2 的合成群台账点击“停用群触达”。本轮浏览器读到“1 项当前协作”，影响为“立即结束对应关联，收回权限”，同时服务端影响是“结束 0 条工作连接”。N2 实际保存证明任职和 train 保留，仅群绑定/触达失效。浏览器这次只预览，未重复保存。

定位：[workbench-catalog.js](../../runtime/sidecar/src/person_collaboration/workbench-catalog.js)
`renderLedgerDetail` 的通用 lifecycle 影响文案；后端
[store/organization.rs](../../runtime/sidecar/src/person_collaboration/store/organization.rs)
`organization_lifecycle` 对 group 撤销绑定，`ledger_connections`
仅收集 person/agent 连接。

预期：群预览准确说明“停用该群触达并撤销范围绑定，保留岗位和其他业务权限”，并表达受影响群绑定/触达数量；恢复提示说明需要重新配置绑定。这是影响用户判断操作后果的正确性问题，应在群生命周期相关切片验收前修复并补 N2 回归，不通过扩大后端撤权来迁就错误文案。Owner：工作台前端与必要影响摘要适配责任方。

本轮未复现服务端越权或事务失效。措辞便利性、信息位置等一般优化保留到真实场景走读，不借 D1 重新设计页面或否定负责人认可。

## 4. C 的最小接入交接（只读准备，未实施）

首个业务闭环沿用既定第三步：**一个获授权管理入口 default
→ 二花本栋知识/工作约定**。入口先验证真实发起人，再由共用服务判断身份、管理委托和本栋执行权；需要等待的工作持久化，实际工具落盘前再次核验，取得回执后才显示完成。不是开放任意 Agent 调度。

| 边界/拟责任方                       | 可复用入口或模块                                                                                                                                                     | 仍需接入与最低验收                                                                                                                                                   |
| ----------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 可信入口与身份：C 身份/适配器负责人 | `conversation_ingress.rs` 来源信封、`operations_intake.rs` 的 TrustedSession；Person、channel_identities、source_identity_links；`context_tools.rs` 精确身份候选逻辑 | 统一平台、账号/企业/应用命名空间；同人两个 Gateway 解析同 person_id，各自裁剪背景。陌生/歧义/过期/撤销转待确认；共享账号不能自称自然人；系统事件操作者不能当新增成员 |
| 授权服务：C 与工作台共享模块责任方  | `person_collaboration/store.rs` Actor、verify、decision、responsible；`model.rs` Policy                                                                              | 当前 Store::local/begin 明确只允许 synthetic，真实会话接线尚无入口。不得直接暴露本地固定操作者或删除门禁；先明确服务责任与测试入口，并保留现有 UI 契约               |
| 二花消费：C 二花能力负责人          | `context_tools.rs` 的 `qintopia_erhua_training_note_submit` 与背景读取；`skills/qintopia-tools/variants/erhua/`                                                      | 旧训练入口仍按 trainer 白名单，未调用本轮授权。实际写入/检索前核对当前身份、任期、范围、委托链和确认条件；知识/约定有版本，不能用旧名单绕过新权威                    |
| 持久分派：C 受控任务负责人          | `operations.rs` 的 start_workflow、create_work_item_routed、work_item_status_tree；既有 WorkItem/Event/审计                                                          | 保留真实发起人、代办者、执行者、委托/授权及范围版本、幂等请求和回执。仅复用边界明确的类型，不能直接把既有海报类型当通用任务                                          |
| MCP 接线：C 入口负责人              | `mcp/operations-control-plane/` 与 `mcp/qintopia-collab/`                                                                                                            | 前者当前为 dry-run；后者直接 call-agent 路径已禁用。新增受控实际接线后才可声明受理；不恢复自由文本直连，不修改 Hermes profile 来做本地试验                           |

最低验收门槛：

1. 未发言但已可靠映射的成员可识别；同人换 Gateway 不新建 Person，也不获得他栋背景。
2. 本栋可指导二花、他栋拒绝；未确认信息不得直接成为正式知识，版本变化后旧结果不继续生效。
3. 受理后撤销身份、委托、任职或确认人权限，排队任务在真正写入前拒绝；授权服务失败不回退旧白名单。必须让测试适配器观察到拒绝时零副作用，不能只检查设置页返回值。
4. 同键重试仅一个 WorkItem；目标不可用进入可恢复状态；已受理、等待审核、已完成按持久事实区分。“已选择确认人”不是“已批准”，旧内容审批不得随换任继承。

代码责任和服务器工作的潜在重叠：`conversation_ingress`、`operations_intake`、
`context_tools`、共享身份/任务表、入口包装及未来 release 接线。负责人已补充：架构同事正在将误放进 Hermes 的 Agent
OS 功能剥离回来，支持 Hermes 升级和运行时替换。

上述方向已确认，但具体代码范围、固定接口、提交和迁移清单尚未审查，不推断剥离已完成。

由负责人/决策任务取得其固定版本、范围和完成状态，再给 C 明确唯一修改责任。本报告不授予这些模块修改权，不另写身份或 PMS 协议，不实施 C/D。

### 运行时可替换：依赖与归属补充

依据主项目新增“运行时可以替换”约束，只读检查本地冻结源码中的下列依赖。未访问服务器或 Hermes 私有数据；这是当前本地版本的接线风险清单，不是架构同事成果审阅。

| 能力                       | 本地依赖证据与状态                                                                                                                                                                                                                        | 后续归属/接入约束                                                                                                                                                |
| -------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 人员、组织授权与配置审计   | `person_collaboration` 使用 Agent OS Postgres、稳定 person_id、身份版本和命令事务；本轮三项验证直接运行 sidecar，不需要 Hermes 会话。所查模块没有读取 Hermes prompt/profile/私有表                                                        | 继续由工作台/Agent OS 服务维护事实；C 只消费稳定接口。架构任务明确真实认证、服务生命周期与适配契约，不能让运行时 profile 名变成自然人或授权                      |
| 真实发起人、范围上下文     | `skills/qintopia-tools/variants/erhua/__init__.py` 的 `_session_env` 导入 `gateway.session_context`，投诉发起人可回退 `HERMES_SESSION_USER_ID`；`skills/knowledge-retrieval` 同样依赖该 session API。CSV 能力也取 `HERMES_SESSION_*`      | 架构任务把运行时会话封装为来源已验证的适配输入；C 接统一身份/范围服务。会话编号仅作外部引用，不替代 person_id、委托或任务编号                                    |
| 安全背景、知识与工作约定   | `context_tools.rs` 读取 Postgres member_profile_snapshots、training_notes 和 erhua_persona_overlays；这里的 profile snapshot 是人员背景，不是 Hermes profile 文件。现有 caller_profile/训练员白名单仍与旧入口耦合，overlay 查询缺楼栋过滤 | 事实与版本留在 Agent OS；架构任务隔离入口身份/配置依赖，C 负责实际消费时按人、Agent、任务目的、楼栋裁剪及撤权重验。不能以复制内部 prompt 或全局 overlay 代替接线 |
| 事件、确认、回执及幂等     | `mcp/qintopia-collab/bin/qintopia-collab-mcp` 默认数据库为 Hermes 根下 collab.db，SQLite 的 collab_events/collab_receipts 保存事件、幂等键和回执，request_confirmation 复用事件；直接 call_agent 虽已禁用，状态依赖仍在                   | 架构剥离任务负责持久事实、旧标识映射、历史/未完确认与幂等迁移。C 不连接这些私有表，也不重新实现第二套协作数据库                                                  |
| 业务待办与本地资料         | 二花插件 `_kanban_runtime` 直接导入 `hermes_cli.kanban_db`，投诉/销售路径调用 create_task；`skills/erhua-csv` 默认资料根位于二花 profile/data                                                                                             | 架构任务清点并剥离业务状态依赖、保留任务/资料历史；这些投诉、销售、CSV 迁移不属于本次工作台或 C 最小知识消费切片                                                 |
| WorkItem、审核、恢复与调度 | `operations.rs` 和 `resident_welcome/delivery.rs` 已有 Agent OS 持久任务、审核绑定、领取期限及执行代际等机制；`runtime/hermes/README.md` 仍把周期 Agent 任务的 cron 事实放在 Hermes                                                       | 架构任务划定受控领取、取消、回执和调度边界；C 接稳定服务并验证一个消费者，不借此迁全部定时任务，也不把欢迎专用门禁当通用 C 已接通                                |

内置 prompt、SOUL 和工具注册可以影响推理与临时上下文，但不得提供业务授权或成为身份、受限背景、任务状态的唯一来源。当前依赖中确有会话 API、私有数据库及 profile 目录耦合；不能因工作台已独立保存配置就宣称整个 Agent
OS 已与 Hermes 解耦。

替换/升级的后续验收由架构任务与 C 共担：

1. 关闭运行时仍能通过 Agent
   OS 保存/查询人员、授权、事件与任务；恢复或替换后person_id、WorkItem、审核及审计历史不重建、不丢失，运行时执行编号保持可追溯映射。
2. 新旧适配器都遵守执行前身份/任期/委托/确认人重验和楼栋隔离；旧 profile 名、旧 prompt或旧缓存不能绕过撤权。
3. 领取后崩溃、换运行时及旧执行者迟到回执不会重复执行；沿用稳定幂等键与服务端有效领取边界，不将旧运行时超时等同于“从未发生副作用”。
4. 在途任务、等待审核及未知发送结果可查询、接管或转人工；unknown 先核对，不能自动重发。这里只列门槛，不执行消息发送或替代运行时试验。

可独立的覆盖核对已完成；真实共享入口接线须等待架构同事的固定服务/适配接口、版本与代码责任交接。测试替身不能证明任一替代运行时可直接切换。

## 5. 收口与后续

本轮交付终点为覆盖矩阵、三项定向验证、D1 复现和 C 最小责任/验收清单。只新增本报告；原报告、两份清单、业务/UI/测试源码、迁移、共同契约及主项目文件未改。收尾核验原 14+2 文件与清单摘要仍一致，检查本报告链接、格式与差异。

本报告的 Markdown、格式与本地链接检查通过；全仓 Markdown 仍有旧 PMS 交接文档两处行长问题，本轮不修改。

下一步由工作台责任方在获准修复切片处理 D1、固化 N1/N2/N3 和断连重试用例；C 在责任与基线明确后另行启动。真实账号转换、登录与实际工具接入仍缺验收，合成通过不等同真实消费已接通。

没有提交、推送、合并、发布、部署、访问生产、PMS/Base/Workflow/Hermes 操作或外部消息发送。
