# 舍长与二花上线前差分验证

2026-10-01，继续 `codex/steward-production-minimal` 与 `PR #733`。

## 实际改动

个人渠道与工作账号统一检查本栋及祖先网关范围。同一人的其他栋渠道不再绕过过滤。Reviewer
Guide 的 57b02e6de191 / a693d8885811 已按同一缺陷修复并回归。

舍长通过原 `ConfigureWork` / `SetAudience` 保存可选
`audience.contacts`。使用组织管理授权及原 JSONB、preview/save、配置版本和历史，不要求岸岸权限或客房群。回读显示人员姓名、渠道昵称和平台；账号停用、撤权、来源版本变化或跨栋选择拒绝沿用。

复制的 `PR #731`
两项迁移、客房路由及测试已退出本 PR。副本留在本任务忽略目录。两项迁移的归属仍是
`PR #731`，本 PR 不新增迁移。早先 4＋4 验证不作为舍长保存证据。

Broker 收到 SIGTERM／SIGINT 关闭监听，等待已接收工作。30 秒到期报告暂缓并继续等待。客户端超时仍为
`outcome_unknown`，原操作可恢复收据，不取消数据库工作或更换操作编号。

最终 DTO 与停止契约见
[Person Foundation README](../../skills/person-foundation/README.md) 和
[实施计划](../plans/active/steward-workspace-completion.md)。

## 实际证据

本节记录截至 `cc7cecd5`
的验证；后续来源归属修复及其实际检查见文末，不将旧 Linux 二进制归到新源码。

SDK 16 项通过；原统一业务入口 7 个场景通过，运行编号
`20261001085803-984008ad86`。受限数据库角色下的联系人保存场景实际执行 1 项并通过，使用原密码会话和固定锁能力。

独立 Linux 全链入口实际执行 1 项并通过，补齐父目录归属检查后最终源码重跑耗时 60.06 秒。

其他用户拥有但 group 和权限正确的父目录在创建锁和监听之前被拒绝；恢复正确 owner 后，六个官方插件客户端阶段全部通过。Hermes 核心固定为
`663362680b6ffa4fbffeb58f6682564239a1953b`，源码未修改。

- Rust 1.96.0；实际官方依赖解释器 Python 3.14.7。
- Linux arm64，本任务镜像
  `5047f9bc923ee6a1572a380246e59bb4c5b131123d8e57293772cafa69dc3834`。
- 专用带 pgvector 的 PostgreSQL 18；原生本机验证使用专用 PostgreSQL 17。
- 运行容器没有外部网络和宿主端口，不挂载宿主数据库、配置或凭据目录。
- Broker UID 0，客户端 UID/GID 2001，错误客户端 UID 2002/GID 2001。
- 真正加载官方 PluginManager 和 registry，handler 来自本仓库真实 SDK。

正向覆盖知识保存、更正、停止、旧版本拒绝、条件记忆与回读。负向覆盖相同 UID、错误 UID、错误 token、未认证事件、跨栋、参数伪造和权限撤销。原 broker
token 与数据库环境不传给客户端；客户端无法读取权限 0600 的 broker 文件。

实际 PostgreSQL 行锁使已接收操作等待。

客户端 10 秒后超时，SIGTERM 关闭接入；

30 秒排空期限到达时操作继续等待。

释放锁后保存完成，停止并重启 broker，通过原操作恢复收据，数据库执行事件计数仍为 1。

原 UI 服务回读一致；

空闲停止也正常排空。

## 失败与处置

初始 Linux 调试编译被 SIGKILL 终止。仅本地构建降低调试信息、使用 Rust
1.96.0，重建成功。未修改 CI、共享框架、Cargo 依赖或发布构建。

普通 PostgreSQL 容器缺少项目要求的 vector 扩展，迁移拒绝运行。保留该任务容器及失败日志，换用已持有的专用 pgvector 镜像；未弱化迁移。

受限 UI 模拟角色起初缺少原草稿建档的 INSERT 权限，且模拟联系人测试误用聊天身份。按原服务契约补本地测试角色、改用原登录会话验证；不扩大生产授权。密码会话重复配置操作沿用原已处理／刷新语义，不修改旧幂等行为。

Linux 验证曾断言错误的 UI 字段 `status`。改为原服务实际返回的
`stopped_at`，保留版本、停止、回读及不重复执行断言后整条通过。

自动总检查起初因新增文档的 Markdown 换行失败，修复文档后重跑。

总检查随后发现仓库内模拟缓存的官方 Hermes 自带 C# 文件，协作检查拒绝将它视作项目源码。

已将完整固定源码移至仓库外的本任务依赖缓存，保留原 SHA 和实际验证证据；

未调整检查器。

本任务容器在验证结束后均停止，数据库和镜像保留。官方源码缓存位置记录在本地证据中。

全部功能并发回归再次发现既有 socket 测试在监听器关闭后立即构造残留场景，表现为安全检查仍判定 socket 活跃。

测试改为等待内核明确拒绝连接后再验证残留清理；保留活动拒绝、锁、路径和普通文件拒绝断言，不修改生产 guard。该场景单独执行及实际二进制连续 20 次执行均通过。

但全套并发再次失败，因此单独结果不视为充分证据。后续改用仓库既有独立子进程测试模式，全套并发连续 3 次通过，每次 875 项通过、212 项按原条件忽略。

最终基线已对齐主线 `fe6466506cd1f5d73bfacc79f102ff641ec364dc`，继承已合入的
`PR #731`。唯一冲突是报告索引，已保留双方全部报告；本 PR 相对新主线不新增迁移或客房通信配置差分。

对齐后及补齐父目录归属检查后，`pnpm check:pr:auto` 均通过 quick、heavy
Rust 和 PostgreSQL 三层。最终运行保留于本任务
`auto-owner-review.log`；原生归属专项另有 1 项通过，Linux 验证同时覆盖其他用户的目录拒绝和原六阶段全链。

| 最终检查                    | 实际结果                                       |
| --------------------------- | ---------------------------------------------- |
| 默认 Rust 回归              | 860 通过，3 按原条件忽略                       |
| 全功能 Rust 回归            | 875 通过，216 按原条件忽略                     |
| 无默认功能／全功能 Clippy   | 两组通过，warnings 为 error                    |
| QiWe 原生测试               | 325 通过，1 平台条件跳过                       |
| PostgreSQL 人员协作         | 169 通过，包含实际受限角色与群聊／私聊同一关联 |
| PostgreSQL 欢迎基础         | 24 通过                                        |
| 既有 operations apply smoke | 通过，仅本任务测试库                           |
| Person Foundation SDK       | 16 通过                                        |
| 统一业务入口                | 7 场景通过，无 broken 或 skipped               |
| 业务执行器自身检查          | 8 通过                                         |
| 官方 Hermes／Linux 不同用户 | 1 全链通过，六阶段均通过                       |

最终统一业务运行编号为 `20261001085803-984008ad86`。其 Allure 报告位于本任务
`.local-testing/runs/20261001085803-984008ad86/allure-report/`。

主线迁移按原入口应用到本任务新库，沿用既有受限 UI 角色安装契约补齐锁函数授权。旧库
`qintopia_steward_731_copy_hold`
保留，未改旧库 checksum 或数据。本机 PostgreSQL 和任务容器在交付时停止，数据库、镜像、日志和最终源码／二进制摘要保留。

SSH 拉取主线时连接关闭；改用现有 GitHub 凭据进行单次 HTTPS 拉取，未修改 remote、全局 Git 配置或查找私钥。

## 审查与生产边界

已完整读取 `9be47253` 的新 Guide、所有后续评论、review 和 inline
comment。Guide 对该提交为 partial，排除 31 文件；没有 review 或 inline
comment。新增归属意见 `f9325c9c4973`
已修复：通过内核返回的本进程 UID 明确检查父目录 owner。

原生专项 1 项、实际 Linux 全链 1 项与总检查均通过；

未增加 owner 配置、依赖、CI 或部署规则。

最终提交仍须核对最新远端审查，不把旧 Guide 的结果视为新提交已通过。

完整读取 `5eb59cb3` 的最新 Reviewer Guide、后续评论、review 和 inline
comment。原审查覆盖不完整；不会把“没有其他评论”称作独立完整审阅。最终提交的远端审查需按实际 head 核对，旧 Guide 不作为新提交通过证据；交付时读取最新评论、review 与 inline
comment。

CI 诊断必须使用 Code
Review 工具。其要求通过 ChatGPT 登录 Codex，未返回远端检查数据。不使用源码管理 CLI 绕过这一读取限制，远端 CI 仍未验证。

本轮未将 PR 合入主线、发布、部署、发送真实消息、连接生产库、授真实权限或开启真实生产 flag。Linux 的开关只用于本任务模拟环境中验证生产代码路径。

UI 接线、真实聊天／模型理解、PMS 与微信／企微身份关联和工作群确认仍需各自实际验收。

保存是完整替换，UI 需提交已有联系人；省略字段相当于清空。

已有新增字段的配置回退时，需理解该数据格式的版本，保留原记录和历史，不将旧程序的解析能力视为已经兼容。

前台互动草稿、其他工作树和旧验收数据原样保留。

住客未加好友时，不以私聊作为关联前提。候选整理后交有权工作群人员确认再回写，区分申请／住宿关联与聊天账号关联。本 PR 记录已确认方向，不新增欢迎流程或发送。

## 独立复审 P2：候选来源归属与保存一致

针对 `cc7cecd5`
完整差分的独立复审确认：同一 namespace／subject_type 被多个有效 Gateway 声明时，候选仍可能展示人员渠道或工作账号，保存却按唯一归属拒绝。

另有其他租户保留该 identity_namespace 的情况，不能把其来源当作本栋可信候选。

本次将现有有效 Gateway 归属计数与身份来源的保留命名空间规则组合为同一固定 SQL 条件，用于候选预筛选、最终结果查询和 contacts 保存校验。

人员渠道、工作账号及其渠道统一生效；

最终查询再次核验，避免其他租户在前后查询之间改变归属。

contacts
DTO、权限、版本和完整替换语义不变。未新增表、迁移、服务、配置项、CI 或部署机制。

新增三个定向 PostgreSQL 场景已经通过，使用实际密码会话和本任务受限 UI 数据库角色：

- 同租户另一栋出现相同来源的有效 Gateway。
- 其他租户出现相同来源的有效 Gateway。
- 没有第二个 Gateway，但来源 namespace 被其他租户作为 identity_namespace 保留。

每个场景分别覆盖人员渠道与工作账号：候选不返回对应引用或标签；已有选择不再 current；保存拒绝且配置版本不变；解除歧义后候选、保存及 current 恢复。专项最终日志为本任务
`.local-testing/steward-prelaunch-review/contacts-ownership-p2-targeted-final.log`，3 项通过、0 失败。

测试准备阶段的编译失败与错误 bootstrap 参数已经修正，原日志保留；未扩大生产授权来通过测试。

本次 `pnpm check:pr:auto` 已通过 quick、heavy Rust 和 PostgreSQL 三层，日志为
`.local-testing/steward-prelaunch-review/auto-contact-ownership-p2.log`。默认 Rust
860 项通过／3 项忽略，全功能 Rust 875 项通过／219 项忽略；两组 warnings-as-errors
Clippy 通过；人员协作 PostgreSQL
172 项通过，包含上述新场景；欢迎基础回归 24 项和原 apply
smoke 通过。QiWe 总计 326 项，其中 325 项通过、1 项平台条件跳过。原报告把 QiWe 总数误写为通过数，已按原日志一并纠正。

本轮不重跑未受影响的 Linux broker／SDK 专项；原 Linux arm64、broker UID 0／client UID
2001 的 60.06 秒证据仍属于 `cc7cecd5`，不能替代新版本的目标服务器架构、非 root
systemd、安装和回退验收。

PR 评论已重新分页完整读取：4 条 issue comments，0 条 reviews，0 条 inline comments。覆盖
`cc7cecd5`
的 Guide 排除 29 文件；三条历史 ACTIVE 对应缺陷经独立核对已修复，不表示机器人已关闭。新修复提交后仍需读取对应 head 的最新审查和 CI。

总指挥已核对 `cc7cecd5` 的替代 CI
`36844973117`、business 与 PR-Agent 成功。本任务的规定 CI 诊断工具仍要求登录；不会把上一提交绿灯当作本次修复通过。

详细处置表与 Linux 构建／插件加载复现步骤按总指挥要求保存在 Git 外的共同收口记录，没有为重复交接新造提交。

当前提交仅包含该有效 P2 修复及必要测试、规格和结果记录。

整批仍需独立复核、#734 同版联调与获准后的正式安装／回退验证；

不标为已上线。

## 管理员联系人候选 403 修复

UI 的 `85ae53ade0191a417dda5aac5a71020b1c365952`
联调反馈：管理员有组织管理权限，能保存目标连接受众，但 people/accounts 联系人候选返回 403。

根因是查询只读取登录者自己的二花工作连接。

已扩展现有 contact 查询的可选 `collaboration`
参数，复用 SetAudience 的组织管理授权，严格绑定同租户、同范围的有效目标连接及其已保存受众；原舍长无参数路径保留。

人员渠道、工作账号、来源唯一归属及保存契约不变，游标绑定目标。

该版本只覆盖已保存目标受众；要求先保存与实际 UI 时序不符，不能交付。后续接手修复见下方首次配置收口。

先在本任务隔离数据库中，以无自身舍长任职的管理员密码会话复现
`scope_access_denied`；新目标参数在修复前返回 `invalid_candidate_query`。

扩展已有登记场景后，受限 UI 数据库角色下的实际 HTTP 验证通过：无目标仍为 403，指定目标的人员、工作账号、两类渠道均为 200。

还覆盖原舍长路径、跨栋、外租户、无权人员、目标不存在、受众外人员、缺失受众、管理授权撤销、连接结束、非法参数与跨目标游标拒绝。

日志保留在本任务 `.local-testing/steward-prelaunch-review/`：`admin-contact-red.log`
为修复前失败；`admin-contact-targeted-http.log` 为最终定向通过。

准备中曾错误调用受限角色的 fixture 身份构造器、遇到 Rust 测试辅助闭包生命周期错误，均已修正并保留相应失败日志；未扩大数据库权限或改变检查流程。

完整工程检查结果见下方。

本次不修改 UI、CI、部署、生产配置，不连接真实库或发送消息，不合并 PR。

原 Linux broker 证据仍只归属原提交。

本次 `pnpm check:pr:auto` 已以退出码 0 完成 quick、heavy 与 PostgreSQL 三层：

- 默认 Rust：860 通过／3 忽略；全功能 Rust：875 通过／219 忽略。
- 两组 warnings-as-errors Clippy 通过。
- 人员协作 PostgreSQL 172 项、既有欢迎回归 24 项及 apply smoke 均通过。
- QiWe 共 326 项，325 通过、1 平台条件跳过。

完整日志：`.local-testing/steward-prelaunch-review/auto-admin-contact-final.log`。

首次总检查仅发现新增文档两处行长问题，修正后重新完整执行并通过；没有修改检查规则。

CI 诊断连接仍提示需要通过 ChatGPT 登录 Codex，不能据此确认新 head 的远端 CI。

## 管理员首次配置收口（接替 f975）

经 UI 实际时序复核，首次查询没有任职/连接 ID，候选先于完整 ConfigureWork 的 preview/save；已有连接也必须允许在内存草稿中扩大或更换联系对象。f975 的已保存受众路径不能覆盖这一流程。

新增 contact 查询的可选 position，复用当前 SetAudience 管理配置权；

核验同 tenant 有效岗位/角色、岗位 scope 一致及可选目标连接。

与 collaboration 不互斥，新建不需要后者。

人员复用既有可信管理目录，不增设 Person 必属一栋规则；

个人渠道仍须 known_person，账号/渠道继续共用来源唯一归属、网关范围和版本校验。

无 position 的普通舍长路径不扩权。

配置游标绑定岗位、可选目标、查询上下文及 tenant 配置版本，每页核验权限；最终完整 preview/save 仍按当前权和来源重验。无新增 DTO 必填项、数据表、权限系统或预保存步骤。

首次真实 HTTP 验证在本任务新建的一次性 PG（5432，旧实例已停止且端口无其他任务占用）及受限 UI 数据库角色下完成，密码会话通过生产 origin
handler 的本机 TCP 请求执行。

证明首建无 ID 的三类候选可读、preview 不写、save 一次完整保存；

还覆盖空/缺失受众、未保存扩大/替换、受限管理员与普通舍长、跨 tenant/栋、撤权、来源撤销和分页上下文。

初轮日志 first-config-http-3.log 通过；

后续补充边界及完整工程结果将继续记录于本节。

准备失败已保留：first-config-http-initial.log 为测试 DTO 类型名错误；

first-config-http-2.log 为模拟岗位违反既有 role/scope 唯一约束。

均仅修正本任务测试准备，不改数据模型、数据库权限或检查规则。

原 f975 和先前 Linux/broker/SDK 证据保留，不能冒充本次新差分通过。

Git 外唯一 UI 契约位于本任务 .local-testing/steward-prelaunch-review/admin-first-config-contract.md。

日志保留于同一目录；

临时 PG 位于 .local-testing/steward-first-config-pg。

UI 作者稳定版本为 48eed78ce5d01aa1cec746e9386f5fb883300bb6，后端本轮不跨写 UI。

没有修改 CI/deploy/checker/迁移门禁、操作生产、读取真实资料或发送业务消息；

不合并/发布。

补充定向结果：first-config-http-final-3.log 的完整首次配置 HTTP 场景通过（1.42 秒）。

first-config-contact-regression-2.log 的原联系人路径及三项来源归属回归全部通过（4 项）。

配置模式的个人渠道直查还重验 known_person；

来源撤销后保存返回既有 409 configuration_not_saved，状态不变。

first-config-http-final.log 曾因测试误将既有保存拒绝码预期为 400 而失败，修正为实际 409，未改 HTTP 错误协议。

first-config-contact-regression.log 发现旧路径的游标错误顺序被改动，已恢复无 position 路径的原顺序与原游标指纹。

配置模式仍先核验当前管理权再接受游标，撤权先拒绝。

定向复验通过，未更改既有断言以掩盖回归。

first-config-markdown.log 的 7 处行长错误已通过拆分新增文档段落修正，first-config-markdown-2.log 通过。
