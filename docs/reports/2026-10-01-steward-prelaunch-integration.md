# 舍长页面上线前集成与审阅

日期：2026-10-01。范围：工作台前端及所属记录。正式分支：`codex/steward-workbench-closeout`。

## 目标、最小差分与工程量

负责人授权执行到上线前。完整目标是在线 UI 搜索选择可信人员、工作账号、群及已核实渠道，保存岗位、楼栋、知识和权限，二花采用同一版本，并可修改、停止或撤销。既有组织框架和舍长四标签保留。

沿用候选查询、既有 Command、版本、操作回执与正式知识服务。必要差分仅为账号及渠道选择与基础方准确 DTO 的接线，以及联合验证发现的页面问题。现有
`Audience` 只有 people/groups，未定字段不猜测；不把工作账号塞进 people。

有效工程量粗估：接口核对、前端收尾、未审文件核查和实际后端的离线联合检查约 2–4 小时。指定 Chrome 恢复后页面联合验证另约 1–2 小时。依赖等待不计入有效工程量。

当前等待基础方的渠道范围安全修正、账号/渠道通用保存 DTO，以及 Codex Chrome 认证恢复。
`scope_communication` 是 `anan/hospitality`
专属，不用于普通舍长保存，不给舍长附加 PMS 权限。

## 精确基线与 PR 安排

- 页面原提交：`d6a47637abb819d9e1f1386eab0fbf37ccb31535`。
- 页面记录提交：`ad8660a8b60c93e0ad0d97938793b303e3e14c68`。
- PR #731：`b450755cd7046e9fd55beaa3d0e1228aea63f1d6`，分支
  `codex/collaboration-channel-selection`。
- PR #733：`5eb59cb3c376e8ea9acf85521e2487e6c21c8b2d`，分支
  `codex/steward-production-minimal`。

本次 GitHub 读取确认两条 PR 开放。两者与页面的合并预检只在 `docs/reports/README.md`
冲突；共同 Rust、认证测试与测试目录可自动合并。完整输出保留在忽略目录，不覆盖基础方文件。

临时集成分支在现有工作树内准备，保留双方父提交，仅解决报告索引冲突。该版本用于实际共享服务和数据库检查。正式页面 PR 只包含原 UI 切片及必要接口收尾，使用 #731 分支作为清楚的 stacked
base；不将 #731 或 #733 的共同 Rust/SQL 再提交为页面新差分。主线顺序由总指挥协调为 #731
→ #733 → UI。

现行 PR 创建脚本硬编码 master，不能直接表达 stacked
base。正文校验和 doctor 仍复用原入口，按本轮明确授权的 stacked
base 创建页面 PR；不修改 PR 工具、CI 或部署机制。

总指挥另已核实 master ruleset 要求 strict required checks、check/PR-Agent 和 resolved
review threads。传统 protection
404 不能解释为无门禁。最终同步 master 和替代 head 的检查由总指挥依次协调，不用 admin
bypass。

## 指定 Chrome 能力核实

本次先得到
`Browser is not available: chrome`。启动已有 Chrome 后，插件库存已识别 Chrome 扩展。通过同一指定插件创建
`about:blank` 标签仍立即得到 `Codex auth token is unavailable`。

本次证据说明当前认证阻断，不能推断程序或扩展损坏。未反复重启、改系统设置或换用 Playwright/其他浏览器。页面联合验收未完成，DOM 状态模拟及 HTTP 检查均不代替浏览器验收。探测结果保留在本地忽略目录。

## 审阅与验证安排

完整读取 #731/#733 的 Reviewer Guide、reviews、conversation 和 inline
comments。#731 本轮没有 inline comments；Reviewer
Guide 未覆盖的文件由本任务补针对审阅。#733 当前渠道查询存在跨范围 gateway 信息披露意见，基础方负责修正；本任务不并行修改共享服务。

必要验证使用一次性本机 PostgreSQL、实际共享服务和原生测试。覆盖搜索分页、保存回读、修改/撤权、版本冲突、来源不可用及跨栋拒绝。真实 Hermes 独立进程验证由基础方负责，页面记录其精确版本与交接结果。未受新差分影响的已有检查直接复用，不把模拟身份或渠道验证描述为线上送达。

本次不合并、发布、部署、改真实授权或外发。旧 stash 和历史坏 ref 不作为本批新增前置。

## 第一版实际集成结果

本地临时分支 `codex/steward-prelaunch-integration-20261001` 的提交为
`5ecc0eabc62b59118fc4c76efbf3c600c357f5e1`，父提交精确为
`0ba09f429586d7e8f2752558bc6839e498a16b35` 与
`5eb59cb3c376e8ea9acf85521e2487e6c21c8b2d`。普通合并只人工保留双方报告索引条目，没有人工改共同 Rust、SQL、认证测试或测试目录。#733 的主线祖先还带入已合入的部署接线内容；本任务未新编写或发布这些改动。

本任务新建一次性 PostgreSQL 18.6，地址为本机 loopback 55483 的
`qintopia_test`，数据位于忽略目录。没有复用其他任务的验收库或生产资料。

- `steward_production_minimal_tests`：4/4 通过，0 忽略。
- 完整 `person_collaboration`，含原标记忽略的数据库用例：169/169 通过，0 忽略。
- 命令均用
  `--locked --features postgres-integration-tests --include-ignored --test-threads=1`，退出 0。

定向检查实际执行共享服务、认证 HTTP、305 个目录候选分页、人员/工作账号/渠道区分、权限撤销、错误状态，以及同一二花可信工具对两栋知识版本的读取、修改和停止。完整套件还覆盖 #731 的受限身份锁、既有业务接口和保存回执。日志及退出码保留在本地忽略目录，没有改断言或跳过失败。

这些结果是合并后的实际后端配合模拟身份和请求的证据，不是指定 Chrome 页面交互、真实 Hermes 模型或线上渠道验收。它们也不处置 #733 已知的跨范围渠道意见；待基础方修正后对实际替代 head 复核新增与受影响场景。

## PR #731 未覆盖文件的针对审阅

审阅基线为 `8173e53795062d4df53fc78f5099325b2e37ce59`，head 为原
`b450755`。本任务补读 Reviewer
Guide 列出的全部 11 个文件差分，目前没有发现新增阻断问题：

- `workbench-view.js` 与
  `workbench-catalog.js`：搜索保留选择、空结果、同名稳定引用及旧列表范围；它们是已给数组内的搜索，不作为全可信目录证据。新页面候选服务另用 workspace 接口。
- `tools/testing/catalog.json`：四个新增场景的路径、实际函数、完整 filter、数据库 feature 与 ignored 参数对应；描述明确模拟来源，不增加 CI 检查入口或新共享框架。
- 两份 `20260929000*`
  SQL：空配置不授予发送权；固定锁函数限定租户、范围、链接与 Person，锁定当前行，使用完全限定表名和固定 search_path，撤销 PUBLIC
  EXECUTE；没有创建生产角色或授予生产 ACL。
- 两份 scope communication / person
  lock 数据设计：与上述存储和锁契约一致，保留版本失效、并发和回滚边界。
- 岸岸 rollout、原本地验证报告、人员协作 README 与测试指南：明确专属业务权限、模拟与真实验收区别，保留原总检查失败和浏览器阻断，不把独立通过回填成总门禁通过。

两条 PR 的共同文件衔接已核对：store 模块同时保留 scope communication 与 workspace
candidates，认证测试、人员协作 README 与测试目录自动合并；唯一报告索引冲突保留了双方条目。本次完整数据库套件没有发现合并造成的认证、迁移或旧业务回归。

PR #733 的当前 Reviewer Guide 在 `5eb59cb`
仍列跨范围 gateway 候选信息披露为 ACTIVE，基础方负责修正。没有未读 inline
threads；不能把无 inline comments、既有 CI success 或本地数据库通过当作该意见已关闭。

## 页面 PR 交付与当前阻断

页面分支已普通推送，草稿 PR
[#734](https://github.com/qintopia-agent-studio/qintopia-agent-os/pull/734)
已建立并附到当前聊天。远端核实 base 为 `codex/collaboration-channel-selection` 的完整
`b450755`，差分恰好八个页面/记录文件，没有共同 Rust、SQL、CI 或部署文件。完整 head 随最终阶段回报，后续更新只普通推送。

本轮正文校验、doctor、collaboration
check 和正常提交 hook 通过。沿用既有五份 JS 语法、18 项状态模拟，以及正确工具链下 quick/heavy
Rust 的通过结果；新差分只增加记录，不重复与新证据无关的全量检查。上述临时集成版本的实际数据库 4/4 和 169/169 结果单独报告。

新 PR 的完整 Reviewer Guide 当前没有安全或主要问题，注明 No relevant
tests；此项表示该 PR 没有新增登记测试，不能据此宣称页面验证充分。本地状态模拟没有纳入 Git，实际共享服务的数据库用例来自依赖版本，浏览器验证仍受阻。当前没有 reviews 或 inline
comments 可处置。

针对该 PR 的 CI 诊断工具调用返回
`Sign in to Codex with ChatGPT to connect GitHub`，没有返回诊断数据。这是诊断连接阻断；本轮按负责人授权，通过无凭据公开 REST 另行核实精确 head 的检查状态元数据，没有读取 CI 日志。`fe6cd44d8bc3f5b316faa37c957b046c9c8c18f9`
的九项检查为八项 success、一项 Release Please validation skipped，包含两个 required
checks。诊断连接仍未恢复，不能把公开状态读取描述为已取得 CI 诊断。

指定 Chrome、基础准确 DTO/渠道安全修正及真实 Hermes 交接仍待完成，草稿未标为上线前全部就绪。本次文档推送产生的替代 head 必须重新读取审查和检查；上述结果仅属于明确列出的实现 head。

## PR #731 同步、替代 head 与外部合并

本轮获准在原分支普通合并当时的最新 master，未获准自行合并 PR。原 head
`b450755cd7046e9fd55beaa3d0e1228aea63f1d6` 与 master
`74899d4191b810bc0cf5dc748c742c3b84aea0b5` 的合并提交为
`7237824ffc4d2c8eb703fe95ed4f8fcc7e2870bb`，父提交、合并 tree 与预检相同，正常 hook、差分检查及新增脚本语法通过后普通推送。

同步只带入 master 已有的五份部署脚本、验证与记录；runtime、skills 及测试目录相对原 head 完全未变。本任务没有新编写 CI 或部署机制。

2026-10-01 15:05（北京时间）的公开 REST 和随后 GraphQL 读取确认：

- 替代 head `7237824` 的九项检查已结束，八项 success，一项 Release Please validation
  skipped；`check` 与 `PR-Agent review assistant` 均 success。
- 完整 Reviewer Guide 明确更新至
  `7237824`，没有安全或主要问题。未覆盖的仍为此前补读的同一组 11 个文件；本次同步未改变它们，原针对审阅处置仍适用。
- 已读全部两条 conversation comments；reviews、inline review
  threads 均为零，无下一页。没有用无评论代替人工批准或审查覆盖证明。
- 远端 master ruleset 的已读契约要求 strict latest base、上述两个 required
  checks 及 resolved review threads。传统 protection 404 不表示无门禁。
- PR 已由仓库负责人 `PatrickLiveCool` 于 **2026-10-01 14:51:05** 合并，merge commit 为
  `fe6466506cd1f5d73bfacc79f102ff641ec364dc`，也是此次查询的最新 master。本任务没有调用 PR
  merge、admin bypass 或发布／部署。

PR #734 仍开放且为草稿，stacked base 名称仍是
`codex/collaboration-channel-selection`。PR metadata 保留原 base 快照
`b450755`，该远端分支当前 ref 为
`7237824`；两者分别记录，不以旧快照冒充当前分支。#733 公开 head 仍为
`5eb59cb3c376e8ea9acf85521e2487e6c21c8b2d`，通用账号／渠道保存 DTO 和跨范围候选修正尚未交付。

同步证据与当前公开检查、完整审查及线程快照保留在忽略目录
`.local-workspace/prelaunch-strict-731/`，没有保留令牌或 CI 日志。

## 可恢复的同版本地模拟服务

入口为 `http://127.0.0.1:19274/`，登录页为 `/login`。固定服务目录是
`.local-workspace/steward-preview-5ecc0ea/`，使用完整集成提交
`5ecc0eabc62b59118fc4c76efbf3c600c357f5e1`
的源码归档与复制后二进制。五份页面 JS 的源码和实际 HTTP 摘要逐一等于 #734 实现 head
`fe6cd44d`；切换工作分支不会改变正在验收的版本。`manifest.json`
记录源码、二进制、归档和页面摘要，不含秘密。

在仓库根目录执行：

```bash
/Users/feather/.local/share/uv/python/cpython-3.12-macos-aarch64-none/bin/python3 \
  .local-workspace/steward-preview-5ecc0ea/launch.py start
```

末尾改成 `status` 查询，改成 `stop`
仅停止本任务记录且身份核对通过的服务。启动会拒绝被占用端口和他人 PID；已存在 fixture 时不会再次初始化，初始化曾尝试但状态不明时拒绝重放。数据仍在本任务的 loopback
PostgreSQL 18.6、55483 端口和 `qintopia_test`，没有复用其他任务的验收库。

模拟账号为 `admin`、`house-one`、`house-two`。随机模拟口令仅保留在本机
`fixture-password`
文件，权限 0600；不放入报告、Git、URL 或启动输出。不接真实 Hermes、broker、Profile、外部渠道及生产环境变量，fixture
observer 未启用。

HTTP 核验 14/14 通过：未登录读状态被拒绝；管理员登录、状态与人员候选查询成功；一栋舍长没有组织管理入口；本栋上下文读取成功；跨栋只返回权限拒绝错误；五份页面文件的实际服务摘要一致。该入口验证的是原二花共享服务的本栋读取，不证明真实模型使用同版知识。

初次诊断脚本误要求跨栋返回 403，断言失败；源码的 foundation
dispatch 契约将业务错误映射为 HTTP 400。复查实际返回为
`400 {"code":"scope_access_denied"}`，没有知识、身份或其他二栋内容。记录初次失败和准确契约，没有修改共享 Rust、放宽权限或将原失败记为通过。完整无秘密回执为
`http-verification.json`。

随后实际执行 stop／start／再次 start：源码与二进制身份、口令、初始化标记不变，重复 start 复用同一自有进程；管理员和一栋舍长原会话继续成功读取，无需再次登录或初始化。重启证据为
`restart-verification.json`，服务继续运行供后续浏览器验收。

本地准备中，系统 Python 3.9 不支持归档的
`filter='data'`；核实解包目标为空后使用已有 Python
3.12 恢复，保留失败证据。系统与项目解释器没有 Pillow，最终使用已有 Codex bundled Python
3.12.14／Pillow
12.3.0，不新增依赖。启动器固定该解释器并检查 Pillow，运行前核对二进制摘要，避免同一准备故障静默复现。

Chrome 继续等待总指挥交接，本轮没有并行操作浏览器。

账号／渠道准确 DTO 到齐前仅准备同版入口，未猜字段、借用 PMS 保存接口或授予舍长 PMS 权限。

沿用此前 4/4 与 169/169 的集成结果，本次仅追加报告和 HTTP／恢复证据，不复跑未受改动影响的 Rust 全量。

## 文档推送后的审查与 CI 取消记录

本次记录提交 `1dd40437cd84c39bb6e8952afef5975cad465c5f`
已普通推送至原页面 PR。最新完整 Reviewer Guide 于 2026-10-01
15:18:48（北京时间）更新至该 head，仍无安全或主要问题，保留 No relevant
tests 的覆盖限制。已读全部三条 conversation comments；reviews 和 inline review
threads 为零，无下一页。远端差分仍恰好八份页面与记录文件，不含共同 Rust、SQL、CI 或部署文件。

2026-10-01 15:22 的公开状态元数据出现同一 head 的两次 CI 运行：

- 原运行 `36829507191` 的 Runtime、Rust
  quality、Light、PostgreSQL 四项为 cancelled；其聚合 `check` 为 failure，Release Please
  validation 也被取消。原失败和取消均保留，不回填为通过。
- 替代运行 `36829591181` 的 `changes`
  success，四项实际检查仍在执行，尚未生成替代聚合结果。
- PR-Agent 的运行 `36829507249` 为 success；business 的独立运行 `36829507174` 仍在执行。

本轮推送后更新了 PR 正文。现行 `.github/workflows/ci.yml` 同时监听 `synchronize` 与
`edited`，同一 PR 的 concurrency group 使用
`cancel-in-progress: true`；聚合检查在依赖取消时仍运行且要求成功。结合正文更新和上述时间顺序，判断旧运行被更新事件替代。这是根据配置与状态元数据作出的诊断推断；没有取得 CI 日志，不能将取消归因于源码断言失败。

CI 诊断工具再次返回 GitHub 登录连接阻断，未取得诊断。公开读取仅包含状态、准确 head、check/run
ID 和时间，没有通过源控 CLI 读取 CI 诊断日志，也没有修改 workflow、触发条件、断言或批准门禁。

后续读取按精确 head 和 run ID 区分历史取消与当前运行，待替代 `check`
产生后才能报告该 head 的最终门禁结论。后续 PR 正文先准备完整再推送；本轮不继续改正文或手动重跑检查，以免再次取消正在执行的替代运行。状态快照保留在原忽略证据目录，此报告仍由原报告索引收录。

2026-10-01 15:28:34 的后续公开快照确认，替代运行 `36829591181` 的 `check`
已 success，Runtime、Rust quality、Light、PostgreSQL、changes 均 success，Release Please
validation skipped；同一 head 的 PR-Agent 也 success。至此 `1dd40437` 的两个 required
checks 已通过，旧运行的 failure／cancelled 仍保留为历史结果。该通过结论只属于
`1dd40437`，不提前授予本报告后续提交的替代 head。

## 最终 contacts 接线计划（2026-10-01 晚间，实施前）

基础接口已交付到
`cc7cecd5afaa00037b970b45b002526642d368e2`。复读该提交的共享规格、`Audience`／`ContactSelection`
模型、联系人保存校验和受众预览，按原 UI 任务继续接线，无需再次确认业务规则。此前“DTO 未到齐”的记录只代表当时状态。

本页面分支通过普通合并对齐主线 `fe646650`，合并提交为
`456fd6125f5b3c281ae1545f3ba9dc416f0f3a15`。

报告索引保留双方条目；三处 UI 冲突保留已实现的服务端候选选择器，其他主线内容自动合并。

对齐后相对主线仍恰好八份原前端与记录文件，18 项原页面检查继续通过。

完成本轮接线后，原 PR 的 base 改为 master；保留 Draft 和 #733 先合入的依赖，不合并 PR。

最小前端差分如下：

- 在原工作配置中分别搜索人员或工作账号，再查询该主体的已验证渠道；渠道查询携带
  `subject_kind` 和 `subject_ref`，每页继续有界查询。
- 保存使用原 `configure_work`／`set_audience` 的 `audience.contacts`，每项只传
  `subject_kind`、`subject_id`、`channel_source_link_id`。最多 20 项，支持同一主体多渠道、逐项移除和清空；工作账号不进入
  `people`。
- 个人渠道加入时将该人员列入明确选择的个人范围，并在预览中展示。联系渠道只是联系约定，不授予主动发送或私人信息权限。
- 已保存名称从 `contact_basis` 回显；该字段与 `authority_grant`
  都是服务端记录，不回传为 Audience 命令字段。受众预览显示 `contacts` 及
  `contacts_current`，明确标出需重新选择的失效渠道。
- 保留四标签、原任职／群选择和知识命令，不使用 `scope_communication`
  或 PMS 授权。新增改动限于现有前端组件与本报告。

验证先覆盖候选主体切换、延迟回包、403 清理、503 保留、跨页选择、三字段序列化、去重与上限，再在与精确 #733
head 的本地集成版本中实际保存、回读、更换、清空和跨栋拒绝。

旧服务及其模拟数据继续保留；新集成使用本任务的独立模拟租户。

指定 Chrome 仍待交接，实际 UI 排版与交互不以 HTTP 或 DOM 模拟代替。

基础方正在审阅的共享 Rust、SQL 和部署工作不由页面任务改写。

## contacts 实施与同版本 HTTP 联调结果

2026-10-01 18:50（北京时间）记录。UI 实现提交为
`24c5fe24ac94dda86b94a8b784cf18d1712d02db`，已按上述计划完成六份工作台 JS 接线。

原页面状态检查 18/18、新渠道状态检查 12/12 通过；新增检查覆盖三字段 DTO、主体分离、多渠道、去重、20 项上限、移除／清空、延迟回包、403／503 及失效提示。

这些脚本位于忽略目录，属于页面状态模拟，不是新增登记测试，也不替代 Chrome 验收。

实际服务源码固定为集成提交 `85ae53ade0191a417dda5aac5a71020b1c365952`，基础为
`cc7cecd5afaa00037b970b45b002526642d368e2`。普通合并仅处理报告索引，相对基础只有六份 UI 与三份记录文件不同；未人工修改共享 Rust 或 SQL。新服务地址为
`http://127.0.0.1:19275/`，使用独立模拟租户。实际 HTTP 返回的六份 JS 的 SHA-256 全部等于固定清单和 UI 实现提交。旧
`19274` 服务仍保留，不能将其旧验证结果归给新版本。

### 保存契约：已验证

管理员通过正常 `/api/preview` 和 `/api/save`
执行以下动作，每个预览／保存共用已留证的操作编号和最新版本：

- 首次保存空渠道受众：200，预览 `persisted=false`，保存 `persisted=true`。
- 同时保存同一个人的两条渠道及一条工作账号渠道：200；实际受众回读三项，
  `contacts_current=true`，工作账号不进入个人范围。
- 全量替换为一条个人渠道：200，回读一项；传入 `[]` 清空：200，回读空数组。
- 使用页面实际的 `configure_work`
  原子保存工作与两条渠道：200，回读两项；六项原有授权逐项相等，未新增权限，未修改任职范围。
- 个人和工作账号的跨栋渠道分别在预览及保存被 409 拒绝，配置版本保持不变。实际错误码均为
  `configuration_not_saved`，不虚报为更精确的联系人错误码。
- 已配置的本栋舍长读取人员、工作账号及各自已验证渠道均为 200；跨栋候选读取为 403，二栋渠道未出现在本栋候选中。

这些结果验证保存契约和已有授权的读取边界；管理员界面完整选择流程仍受下述阻断影响。模拟渠道仅写入本任务独立数据库，不涉及生产授权、真实身份或外部发送。

### 管理员候选：阻断未解决

管理员登录及 `/api/state` 均为 200，`management_available=true`，目标一栋关系
`can_manage=true`；相同范围 `purpose=assign&kind=people` 为 200。但
`/api/workspace/candidates` 的 `purpose=contact` 配合 `people` 或 `accounts`
均返回 403／`scope_access_denied`。正常保存目标受众后再次读取仍为 403，证明不是单纯缺少目标初始化。

定位于
`runtime/sidecar/src/person_collaboration/store/workspace_candidates.rs`：约 151 行计算组织管理者，约 211–268 行的 contact 分支只查请求人自己的
`erhua/community_service`
授权及已保存受众，未覆盖管理者配置他人或首次配置的路径。本次管理员自己的受众尚未保存，而目标工作已正常保存。该条件与前端要求候选就绪后才允许预览共同阻断管理员完整配置流程。

复现顺序：管理员登录 → 读取目标关系及 assign 候选 → 查询同范围 contact 人员／账号 →
403；使用正常命令保存目标受众 → 原 contact 查询仍为 403。基础任务需依据组织管理权限与候选范围契约处理；本任务不扩权、不借用 assign 作为 contact 来源、不修改共享后端，也不把舍长身份成功记作管理员验收成功。

### 证据、恢复与剩余验收

无秘密证据保留在 `.local-workspace/steward-preview-85ae53a/`：
`manifest.json`、`admin-contact-candidates-blocker.json`、`http-contacts-evidence.json`、
`served-assets.json` 及各命令记录。一次误用 business 候选端点的五项探测已显式标注
`excluded_from_acceptance=true`，不计入上述结论。该目录的 `launch.py start/status/stop`
管理固定服务，恢复时不重复初始化模拟资料。随机口令及会话留在本机，未写入 Git 或 PR。

仍待基础任务修正管理员候选阻断后，在新精确版本上复验；指定 Chrome 仍等待总指挥交接。真实 Hermes 独立进程交接由基础任务负责。本轮不复跑未受影响的 Rust 全量，旧 169 项结果只属于此前
`5ecc0ea`。PR 保持 Draft，base 对齐 master，依赖 #733 先合入。没有合并、发布、部署、改 CI／部署机制、外发消息或修改生产授权。
