# 岸岸 PMS 本地实施与验收记录

日期：2026-09-24。分支 `codex/anan-pms-collaboration`，基线
`b42db0d8b8388c65dcf62c1f1fd41a7325298c5e`，规格继承提交
`adff875`。本报告随阶段验证更新；未完成项不记为通过。

## 范围与当前实现

- 按已批准 A 方案登记 `agents/anan`
  为 unmanaged；schema、两检查器和状态回归更新在批准范围内。缺省 managed 和现有七个 Agent 的生产要求保留；unmanaged 校验无部署字段且未进入已核对生产清单。
- `skills/pms-operations` 包含有限目录、官方 Hermes
  Plugin、受控 loopback 客户端和宿主入口。固定目标、不追重定向、不继承代理、不接受模型提供人员、批准、物业或幂等键。PMS
  `/me` 只证明执行主体上限。
- 共同服务增加 `read_business` / `execute_business`
  精确操作范围，沿既有 Person、任职和授权链验证。缺省无操作；委派只能是自身当前权限子集；物业、任期、身份、撤权和祖先链每次检查。
- WorkItem 下分别记录报价、预览和执行；每个动作独立的请求、预览、执行、恢复键和回执。失联后查原键/resolve；已成功步骤不重放；WorkItem 汇总独立动作状态。
- 自然确认绑定同人、同会话内唯一当前方案，不要求复制长编码。完整普通整间预订或当笔收款交办可按受限中文语义逐项对照最终 PMS 效果后复用，无二次机械确认。

  每笔收款均需有权人对订单、流水、金额的明确决定，长期授权和事件不替代。不完整/不支持的语言保持待确认；不让模型解释文本产生批准。

- 暂停、取消、恢复与人工接手保留原账本。在途先核对，不暗中撤销 PMS 事实。人工完成的入住/退房/取消，需可信原会话的订单引用和实际 PMS 状态回读，记为
  `manual_completed`，不伪造插件执行回执。其他命令人工接手的效果核验尚未自动收口。
- 新迁移 `202609240001_business_operation_execution.sql`
  不修改旧迁移、不为旧人员赋权。feed 表仅是后续持久入口，未因此宣称消费者或真实推送完成。

## 已有验证证据

所有测试使用本任务模拟资料；无真实 PMS 写入或消息发送。

| 检查                            | 实际结果与边界                                                                                                                                                                                                                                  |
| ------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A 状态回归                      | `pnpm agents:check` 通过，运行两个真实检查器的正常/非法状态；覆盖 Profile、restart path、请求枚举、smoke 和 payload 污染                                                                                                                        |
| registry / profile bundles      | `pnpm registry:check`、`node tools/agents/check-profile-bundles.mjs` 通过                                                                                                                                                                       |
| 客户端/插件                     | 17 项 unittest 通过；模拟 HTTP、宿主事件与授权边界，非真实模型                                                                                                                                                                                  |
| 共同服务持久化                  | 6 项 PostgreSQL 测试通过；已按仓库契约标为显式数据库用例，跨仓联测移至技能包独立入口。包含委派、父撤权、自然确认歧义、暂停恢复取消、并发领取与重启恢复                                                                                          |
| PMS HTTP 子链                   | 未修改 Green PMS `0254fbabdda0b76b56370248f2ad24e44e4e950a`，实际 HTTP 查权/查房/报价/预览/建单/订单回读/原键恢复通过；不把 Token 权限称为人类授权                                                                                              |
| 首批完整本地链                  | 独立 opt-in 1 passed：官方 Hermes ContextVar、模拟 WeCom 宿主、真实 Unix broker、Person 精确授权、持久确认、插件、真实 PMS HTTP；覆盖原交办复用、自然收款确认、重复防护与入住回读                                                               |
| 扩大后的完整联测                | 1 passed，实际官方 PluginContext/registry 注册并调用工具；失联收款原键恢复、明确当笔收款指令复用、改期/续住/换房、人工入住接手回读通过；入住当天缩住、提前普通退房按 PMS 规则拒绝，记 preview_rejected。尚未验证跨营业日缩住/普通退房的成功路径 |
| 其他共同授权/欢迎及全库 PR 检查 | auto 的 light 层通过；3.12 下 runtime、两个 adapter 边界及两组禁止警告 Clippy 通过；all-features 866 passed / 130 ignored；PG Rust 用例通过，最后 apply smoke 因既有 URL allowlist 受阻                                                         |

环境：Agent OS 与 PMS 分别使用专用 PG17 实例，端口 `52316` / `52319`；数据库都为
`qintopia_test` 但实例不同。PMS 通过原 `run-database-test-suite.ts`
测试锁启动。官方 Hermes 只读 checkout 为
`d337b736aa1e8ebecfab043842d13e4a2d2f48a3`；本任务独立 Python
3.12 环境，未修改源码或用户 Profile。宿主读取该版本 `_VAR_MAP`
中当前任务已绑定的 ContextVar；接口缺失/未绑定即拒绝，禁止环境回退。
`session_context_engaged()` 只是进程级标记，不能单独证明当前任务有可信会话。

RTK 当前不可用，实际使用原生命令。PG17 结果不代表 PG18 全套测试或生产验收。原始本地日志在本 worktree
`.local-workspace/anan/`，不纳入 Git。

PR 准备：`pnpm pr:doctor` 通过，提示尚无 upstream、工作区未提交；初始正文草稿通过
`pnpm pr:check-body` 的本地事件输入验证。统一目录 `pnpm test:harness`
8 项通过；跨仓测试使用独立入口，PG 用例仍由既有全库 PG
tier 执行，不能因 ignore 标记被跳过后误记通过。

## 全库分层检查记录

首次 `pnpm check:pr:auto`：退出 1；`check:light` 已完整通过，失败发生在误用系统 Python
3.9 的 QiWe 运行时测试。后续按原 runner 定义接续，不重复未受改动影响的部署层。

| 命令/层                                                              | 环境                                                 | 退出码与结果                                                                                                |
| -------------------------------------------------------------------- | ---------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `pnpm check:runtime`                                                 | Python 3.12.14、Rust 1.96.0；无 PG URL / apply-smoke | 0；QiWe 326 项（1 原有跳过），Rust 851 passed / 2 ignored，两个 CLI smoke 通过                              |
| 两项 `qiwe-staging-adapter` / `qiwe-production-adapter` 精确编译边界 | Rust 1.96.0；无 PG 环境                              | 均为 0，各 1 passed                                                                                         |
| `cargo clippy --all-targets --no-default-features -- -D warnings`    | Rust 1.96.0                                          | 0                                                                                                           |
| `cargo clippy --all-targets --all-features -- -D warnings`           | Rust 1.96.0                                          | 0                                                                                                           |
| `cargo test --all-features`                                          | 无 PG URL / apply-smoke                              | 0；866 passed / 130 ignored，PG 前置用例由下一层执行                                                        |
| `pnpm check:pr:postgres`                                             | 本任务 PG17 / 52316 / qintopia_test                  | 1；显式 Rust PG 用例通过（person_collaboration 77、welcome 24）；最后 apply smoke 被既有 URL allowlist 拦截 |

Cargo 命令均指定 `--manifest-path runtime/sidecar/Cargo.toml`，测试栈为
`RUST_MIN_STACK=33554432`。Python 路径为本任务独立 venv；未更改系统默认解释器。日志分别为
`.local-workspace/anan/check-pr-auto-python39.log`、`check-runtime.log`、
`heavy-remainder.log`、`full-chain-final.log`。这些日志不含业务凭据，不纳入 Git。

## 失败、修复与运行约束

1. 首轮新测试并发建空 schema 发生冲突；测试初始化改为进程内一次性执行加数据库 advisory
   lock，防不同测试进程同时初始化；不改生产迁移执行器。
2. 新业务 action 未扩展角色/职责数组约束，导致模拟 bootstrap 拒绝；补入同一新迁移，不改变已有授权行。
3. A 新测试的临时仓库根目录为 `0700`，既有 canary 要求
   `0755`；仅修临时 fixture 权限，未削弱原门禁。
4. PMS 实际预览和查询有超过 5 秒的请求；客户端改为有界 30 秒。超时保留原键，不能换键或声称未执行。
5. 完整联测最初使用系统 Python 3.9，无法导入 Hermes；改用独立 Python
   3.12 与该版本声明依赖。
6. 扩展测试将续住放在入住前，PMS 按真实规则拒绝；调整业务顺序，保留未执行事项，不修改 PMS 规则。
7. 并行重型检查期间曾出现已提交预览的响应超时，以及 PMS 测试锁 process
   snapshot 临时失败；保留日志和持久原键。联测对未知预览沿原键重取；`PREVIEW:<command>`
   是内部账本类型，不在公开恢复接口枚举内。执行才查结果/resolve，明确预览拒绝另记
   `preview_rejected`，不伪造 PMS 回执。重型部署检查与联合业务测试顺序执行。

8. 完整检查最先被新增文档的 Markdown 行长和标题问题拦截；已修复，Markdown 检查通过。
9. 新 PG 用例起初未声明显式数据库前置条件，跨仓测试也位于通用 PG 模块内。按既有契约整理用例标记；跨仓入口由技能包持有并单独经 PMS 测试锁启动，未修改 CI 门禁。
10. 插件恢复暂停方案后原先停在 draft，未继续预览；修复为重新预览并等待新确认。增加进程中断于 draft 的恢复回归，17 项 Python 测试通过。最新独立完整链已复跑：1
    passed / 0 ignored，实际持久结果和 PMS 回读通过。

11. `check:pr:auto` 的 light 层全部通过后，运行时层误用了系统 Python
    3.9，QiWe 测试报 5 项语法/事件循环错误。切换本任务 Python 3.12 后 QiWe
    326 项测试通过（1 项原有跳过）。按原定义继续运行 runtime、feature 边界、Clippy、all-features 和 PostgreSQL
    tier，不重复已通过的部署层，不把单次 auto 记为全绿。

12. PG tier 的最后 `operations-control-plane-apply-smoke.sh` 被
    `database URL hash is not in the reviewed allowlist`
    拦截，随后 JSONDecodeError 是次生错误。此前 Rust PG 用例通过，整个 applicable
    check 仍未全通过，不能记为自动检查全绿。

    诊断更正：初次仅据历史 staging 报告判断无合法本地配置，不准确。仓库公开模拟 URL
    `postgres://postgres:postgres@127.0.0.1:5432/qintopia_test`
    的 SHA256 恰为固定列表第一项。该配置在独立环境可用；本机 5432 已由基础线
    `foundation-batch1/pr720-reconcile-postgres`
    占用，不能停用、复用或重置它。本任务 52316 不匹配哈希；未改 allowlist、CI 或门禁，未寻找 staging 凭据。

    [独立测试契约评审提案](../plans/active/disposable-postgres-boundary-review.md)
    比较固定端口环境适配与复用通用 URL 边界，列九项标准及负面验证。仅供评审，未授权实施；不是必须修改门禁才能继续其他业务开发。

这些是本地集成失败和修复记录，不是生产故障。实际 PMS 拒绝应以稳定错误码/回执处理，不能复述含隐私的响应正文。

## 发布和后续边界

- 未改 resolver、restart rules、安装 payload、生产 Profile
  registry、workflow 或 job。新包非文档路径仍属于 unmatched
  production-adjacent；发布阻断保留，不能把 unmanaged 当发布豁免。
- [收款首次基线提案](../plans/active/anan-payment-feed-baseline-proposal.md)要求已提交源 head。
  `occurredAt=now()`
  是源事务开始时间，不可过滤启用前晚提交事件；一页尾游标不是 head。首次序号起点已获用户同意；接口及主动通知归 PMS 项目。原参考 PMS 基线没有该只读接口；发送方现已提交
  `608c53b`，本线已只读核对路由与响应类型。发送方回报 PG18.6 专项 16/16 通过；本线接收与联合验证尚未完成，不跨仓改源代码。
- 飞书原链在 `workflows/silaoshi-daily-ops` 官方 transform/Bridge；参考 Feishu
  Base 空目录不是缺失入口证据。原新增触发已定位，修订/撤回与当前生产绑定待核验，不启动旧制卡直发、不另建扫描器。
- 按[双线协调记录](../plans/active/agent-os-dual-track-coordination.md)，身份/欢迎群与 UI 归基础线。

  本线仅接住宿事实、可靠申请引用和既有 WorkItem，等待明确公共接口；欢迎内容确认不授予 PMS 操作权限。

- 真实 Livecool 模型、专属 Bot、主动工作群能力、实际操作者权限、生产 PMS、欢迎发布与 T14 均未验收。当前未 SSH、读取生产凭据、发布、部署、合并或外发。

## 阶段 3：本地支付接收切片

新增独立
`202609240002_business_payment_event_ingress.sql`；001 未改，003 归欢迎线。应用前回读001成功、002未应用、checkpoint/inbox为空；应用后001与002均成功。首次误查public下迁移表报不存在，改查实际qintopia_messages
schema，未重建数据库。

固定支付HMAC入口与host-only补拉已接入；原verify仅抽取authenticate阶段，旧解码、作用域、签名字节及错误语义保持。模型operation不能进入host-only分支。Actor/Grant、个人身份解析与欢迎UI未改。

关联事件事项前，插件实际读取PMS
billId并核对可用状态、流水、金额和WECOM方式。共同服务仍要求当前人员的精确收款权限及当笔确认。事件不制造批准或财务动作。

| 验证                           | 实际结果                                             |
| ------------------------------ | ---------------------------------------------------- |
| person_collaboration PG        | 82 passed / 0 ignored，含5项支付专项                 |
| resident_welcome PG            | 24 passed / 0 ignored，按原runner环境及串行参数运行  |
| Python客户端、插件、宿主补拉   | 22 passed                                            |
| all-features Rust              | 866 passed / 136 ignored；支付PG用例已在上层实际执行 |
| no-default/all-features Clippy | 两组all-targets禁止警告均通过                        |
| 业务目录harness                | 8 passed                                             |
| 普通本地sidecar构建            | 通过                                                 |
| 独立联合配置初始化             | 1 passed，不重置PMS、不生成付款                      |

支付专项覆盖非零H=42、并发首次初始化、重启不覆盖、push/feed同回执、缺口整页回滚、MATCHED先到、退款不建催办、跨来源/物业拒绝、签名持久ACK、关联权限和无确认拒绝。

保留的本地失败及修复：测试曾直接访问Actor私有字段，已改用verified_person
API。扩大PG回归曾遗漏原runner的串行参数，账户状态前后仅local_dialogue_available不同，原因是其他测试并发改变进程开关；未改断言，按原串行命令复跑82项通过。

欢迎回归首次只配置WELCOME前置，14项跨模块foundation用例拒绝缺少通用harness开关。按原runner完整环境补跑24项通过，保留payment-welcome-env-failure.log。这些环境失败不改写为首次全绿；既有图像apply
smoke的固定URL限制仍未解决。

### 联合服务与基线

18449本地receiver已启动；固定事件路径GET返回404，禁止泛用CSRF绕过。宿主真实调用发送方PMS
head并同事务保存baseline/checkpoint，独立SQL回读结果为：

- sourceInstance：`synthetic-pms-joint-20260924`
- propertyId：`prop_qintopia_demo`
- bindingVersion：1
- baselineCursor与cursor：均为0

证据位于忽略目录joint/baseline-initialize.log与joint/baseline-readback.json。本次真实空流基线与非零H=42专项分别记录；冻结前未通知发送方discover。模拟配置与密钥不进入Git，旧5432、PMS联合库及其他任务实例未改动。

发送方实际联合工具版本为`357f9cea044a1343332e6806cef1fda05899d408`；对方报告PR
51 已合入 main，merge为`e7f77163eb6d4cbc0142b356f96087b0a399458c`。HTTPS签名投递、丢ACK重试及人确认后的真实本地收款链仍待联合执行。

为避免磁盘耗尽，仅清理本任务5个未使用的旧增量编译缓存目录，未删除源码、日志、数据库或其他任务目录。

阶段3 `pnpm check:light`
全部通过（退出0），包括登记、秘密扫描、部署runner和时钟回归。既有PG apply
smoke仍受固定URL门禁限制；适用总检查未全通过，不宣称auto全绿。

### 联合首笔事件与查询修正

接收端冻结为 `ed0bb6c`，发送端工具为 `357f9ce`。真实本地 HTTPS 验证结果：

- 坏签名：上游401，发送源持久暂停；接收端事件/事项/动作均0。
- 丢ACK：上游202且代理丢回执；接收端事件1、事项1、动作0，检查点仍0。
- 正常重试：200 duplicate，发送端accepted；双方SQL回读同一receipt，事件/事项仍各1。
- 推送后补拉：宿主实际feed使cursor从0到1，仍复用同一receipt与事项。

发送方一次CA路径误拼发生于TLS加载，未到HTTP代理；其记录与真正丢ACK的202分开保留。接收方对丢ACK不猜测失败，沿持久事件及回执核对，没有新增财务动作。

首轮人确认链停在只读查询：调用漏了PMS必填kind，被API拒绝；SQL确认actions仍为空。插件的事件关联读取与联合脚本均补入COLLECTION和status=ALL，明确读取已匹配状态用于回读。新增请求参数回归后Python
23项通过；未修改PMS、Rust或已运行的接收二进制。原失败日志保留在joint/collection-first-read-failure.log，未创建动作或执行收款，随后沿同一账单继续。

### 首笔收款与 MATCHED 联合回读

实际联合版本：Rust receiver 为 `ed0bb6c`，Python 插件与联合脚本为
`7726a6f`，PMS 服务仍为 `357f9ce`。PMS 合并没有切换在用联合环境。

沿原模拟账单执行真实本地业务链，脚本退出0：

- 无当笔确认时拒绝执行且账单仍为 AVAILABLE。
- 可信模拟 Person 对具体订单、流水、金额自然确认后执行 RECORD_COLLECTION。
- 测试故意丢弃 Confirm 响应，保留 UNKNOWN，随后沿原执行键恢复为 completed。
- 实际回读订单及流水 MATCHED；再次执行被拒绝。

发送方独立 SQL 核对仅一条 COLLECTION，金额12000分，匹配来源 CONFIRMED。接收方仅有一个动作
`eddb06be-f04e-4da8-98da-ed32d775ee64`，关联原事项
`a11c8919-5677-4f8b-ad8a-178f04e92679`，PMS 业务回执为
`receipt_dbafe6d0-b255-4c25-a109-13fa4312aa43`。以上均为隔离环境模拟资料。

PMS 原 HTTPS sender 随后投递事务生成的 MATCHED（sequence=2），返回202及回执
`eb310e73-9e1c-4017-99ff-ee15d085a987`。本线独立 SQL 确认 inbox=2、work=1、action=1。

两个事件共用原事项，completed 动作未被覆盖；push 后 cursor 仍为1。真实 feed 补拉后 cursor=2、baseline=0，两条事件及回执不变，没有重复收款或新事项。

本地证据均位于忽略目录 joint：

- collection-one.log、collection-one-result.json；
- matched-push-receiver.json、matched-feed.log、matched-feed-receiver.json。

首笔结果文件不重跑、不覆盖。PG 专项通过不能替代双端联合验证。

### 第二笔与有界模拟负例

第二笔真实 PMS
DISCOVERED 为 sequence=3；先实际 feed，cursor=3、inbox=3、work=2，action 仍为首笔唯一 completed。之后原 HTTPS
sender 返回 duplicate/accepted，回执 `d2f6824d-4693-4efc-bd08-171c54d6edbd`
与 feed 一致，未创建收款。

第二笔投递期间18450代理进程退出，一次 TRANSPORT_UNCONFIRMED 未到接收端。发送方归档原 TLS 记录后恢复同一入口及临时 CA；API、receiver、数据库均未重启或重置。

发送方在忽略目录使用有界 replay 工具读取原持久 body，复用 sendClaim、HTTPS
transport 和签名。以下为内存单字段变异的模拟信封，不是 PMS 业务事务事件；不调用 finishDelivery，不改变原 accepted 状态，也不把工具分类当作订阅已暂停：

| 模拟负例 | 单字段变化                              | HTTP | 本线逐次 SQL 回读                      |
| -------- | --------------------------------------- | ---- | -------------------------------------- |
| 内容冲突 | 原 eventId/sequence，occurredAt 加1毫秒 | 409  | inbox3/work2/action1 completed/cursor3 |
| 来源越界 | sourceInstance 换为未配置来源           | 403  | 同上，全部回执不变                     |
| 物业越界 | propertyId 换为未配置物业               | 403  | 同上，全部回执不变                     |

证据均位于忽略目录 joint：

- second-feed-first.log、second-feed-first-receiver.json；
- second-duplicate-conflict-receiver.json；
- wrong-source-receiver.json、wrong-property-receiver.json。

错误作用域拒绝不等于两个合法作用域隔离已联合验收；后者仍未覆盖。

### 第三笔 MATCHED 先到

发送方通过正常 runtime 角色 Preview/Confirm 事务将第三笔模拟账单登记为 MATCHED，独立 SQL 核对12000分、唯一 COLLECTION、来源 CONFIRMED。

这是模拟人工接手事务，不是浏览器点击或真人确认验收；本线没有重复执行收款。

发送方真实 publish 物化 DISCOVERED sequence4 与 MATCHED
sequence5，使用未变异原 body 先投递5再投递4，两次均202。

本线逐次 SQL 回读两事件 work_item 均为空，总 inbox=5、work=2、action=1
completed。push 后 cursor 仍为3。

随后实际 feed 连续补齐4和5，cursor=5、baseline=0，未补建催办或收款。

- MATCHED 回执：`d481c7fc-2c75-452c-88bc-7099d564072d`。
- DISCOVERED 回执：`5212eb66-f408-4209-9a4f-ba52dc4a72ea`。
- 证据：joint/third-matched-first-receiver.json、third-discovered-late-receiver.json、third-ordered-feed.log、third-ordered-feed-receiver.json。

单次 replay 不修改发送队列确认状态。随后原 sender 正常投递第三笔两事件，均返回200
duplicate，复用上述回执。发送方最终独立 SQL/status 确认五事件全部 accepted、无 last_error。

三笔均12000分：首笔和第三笔 CONFIRMED 匹配，第二笔未匹配；PMS 全库收款事实仅首/第三笔两条、合计24000分。本线最终独立回读仍 inbox5/work2/action1
completed/cursor5。两边账本差异符合第三笔由 PMS 模拟人工接手登记、Agent
OS 不重登的业务边界。

最终证据为本线 joint/final-payment-receiver.json 与发送方 joint/final-status.json、final-sql.txt。已停止新增支付场景，保留环境；真实浏览器、真人、渠道和生产仍未验收。

发送方版本化联合报告为 Green PMS 提交 `ddb51daf8e6697cc721465b89f5c999de35a7703`，文件
`待开发项/PMS-AgentOS-支付事件联合验收-20260924.md`。该报告提交不改变联合业务源码357f9ce。

## 阶段4：本地申请回读与分派切片

004由总指挥预留。复用既有签名 Unix
Bridge，新分支显式本地启用且默认关闭；旧未启用路由保持。同 record 回调持续唤醒，不被旧 jobs 的 delivery_id 去重吞掉；新模式不执行旧制卡/直发脚本。

固定来源适配器实际 GET 指定本地模拟记录，许可字段形成内容与身份指纹，不读取附件或输出资料正文。

回调只携带记录引用，模型不能提供身份 hash、Person、批准或撤回事实。

404、权限或网络错误保留待回读。

权威回读后，独立 HOST_TOKEN 通过原 broker 原子保存 welcome_applications 投影及岸岸/四老师各一事项。

内部 revision 仅随内容变化增加，不将 last_modified_time、delivery_id 或 record_id 当修订号。

read_token 拒绝旧读响应；

丢 ACK 同 token 重交同内容返回 duplicate。

四老师运营事项不成为普通订单审批。

已完成/取消/执行中的事项不因来源变化自动重放；

明确撤回只停止未完成来源待办，不改 PMS 事实。

身份字段为固定许可姓名/昵称/手机号。内容变化保留已确认 Person；身份字段变化清本申请 Person、解除案例 application_id 并保留旧引用审计，不撤销全局账号身份或 PMS 入住人事实。

公共 `application_identity_basis(&mut tx, scope, application)`
在同事务核对有效绑定及来源归属。

它以当前申请字段重算已提交 completed_hash，其他入口改写申请后返回 None，防止旧身份摘要被误用。

不重复实现欢迎确认，snapshot/open_task 的精确身份失效由基础线接入本公共读取方法。

| 验证                                       | 本线整合欢迎前的实际结果                  |
| ------------------------------------------ | ----------------------------------------- |
| 申请 PostgreSQL 专项                       | 6 passed，含完整 Bridge/HTTP/broker/PG 链 |
| person_collaboration 回归                  | 88 passed / 0 ignored                     |
| PMS/申请 Python                            | 30 passed                                 |
| 四老师 Bridge 与迁移兼容测试               | 21 passed                                 |
| 原业务目录 harness                         | 8 passed                                  |
| no-default/all-features Clippy all-targets | 禁止警告通过                              |

完整申请链的六次实际 HTTP 读取覆盖重复记录回调、内容/身份变更、404保持待回读、恢复及明确撤回。最终同一申请 revision4、原两事项取消，旧脚本执行0；坏签名拒绝。模拟 HTTP 边界不等于真实飞书触发通过。

证据为 application-pg-tests.log、application-bridge-journey.log、application-person-regression.log、application-python-tests.log、application-clippy.log，均在忽略目录
`.local-workspace/anan/`。004已应用后不再改写；先前考虑新增 observed_* 列的方案已撤回，实际用 completed_hash 读取接口。

本切片尚未整合欢迎6c6548b，不将以上结果记作001→002→003→004集成验证。原申请到真实人员候选、可靠住宿关联后的欢迎调用、群可信交互及生产修订/撤回接线仍需后续验收。

身份依据补充回归通过：其他入口仅改 field_hash、valid 或 consent_active 时均返回 None；跨 scope、绑定停用或版本改变同样拒绝。

来源 A→B→A 不恢复被 B 清除的申请 Person 或案例关联。helper 的 Some 仅证明当前回读内容一致，不代替关联成立、许可或旧确认收据仍有效。

四老师新能力沿既有业务目录登记为默认关闭，仅同步原 builtin 与 operations
smoke 的目录数量断言14→15。未改检查入口、CI条件或生产权限门禁。

### 欢迎基础线首次整合

正常本地合入欢迎冻结6c6548b，提交daaaccf。唯一冲突为data-design
CHANGELOG同位置追加，保留003/004两项；共享mod/store、broker分支及业务目录保留双方。没有覆盖个人身份解析或改写已应用SQL。

独立新建本地PG17/52322，顺序应用001→002→003→004；SQLx账本全部success，逐项SHA384与源码一致，安装顺序已回读。

原52316支付环境及ed0接收进程未重启；冻结二进制已按原SHA256保留。整合后all-features共同PG100项通过，欢迎PG25项通过。日志为integration/person-tests.log、welcome-tests.log、migration-readback.json。

基础线精确身份快照切片仍在推进，以上为首次整合结果。

目录数量随动是现有业务测试预期更新，适用guardrails“沿用现有契约的业务实现和测试用例不因此变成修改CI的授权请求”。没有另一次用户CI批准，不将总指挥调度写成新增门禁授权；URL
allowlist提案仍未获批。

### 申请调用侧欢迎交接与测试进程隔离

成功来源回读后，从共同服务现存申请/案例/Person/PMS
occupant 关系及当前有效住宿投影派生独立
`welcome_event`，精确绑定 tenant/scope/case/application，再调用公共
`welcome_review_open_task`。不从支付或办理事项冒充，不修改基础线 snapshot/open_task；未配置或无可靠关联时保持等待，重复回读不重建事项，已有引用冲突拒绝覆盖，manual_hold 保留。

本切片申请 PostgreSQL
7 项通过，含真实签名 Bridge→HTTP→broker→PG 联测及新增欢迎交接。新增覆盖无关联、不完整人员关联、无运营配置、精确 payload、重复回读、人工暂停、身份链接撤销、冲突 payload 保留、跨范围、PMS 投影失效和来源撤回。Python
31 项通过，包含欢迎交接失败后必须重新读取来源再重试。日志为
`integration/application-welcome-final.log`、`integration/application-python.log`。

签名 Bridge 测试原先临时修改共享进程环境，已单独提交 `7249ce7`
改为当前测试二进制的独立子进程，原链路及断言保留，专项通过。首次新欢迎测试因夹具重复初始化租户失败，改为读取既有 fixture
operator 后通过。

默认并行共同回归实际为 100 passed / 1
failed，不能视为全绿：唯一失败仍是账户生命周期的完整 state 比较，`local_dialogue_available`
从 false 变 true。继续定位到基础线 `foundation_server::enable_test_http()`
首次初始化同一全局环境变量；无 `#[test]` 的 PMS 专用旅程未被本轮选择器调用。原失败日志
`integration/person-parallel-tests.log` 保留，基础线负责既有 HTTP
fixture 初始化修复，再承接稳定 SHA 做并行回归。没有删除字段断言、降低授权检查或改共享 CI/runner。

all-features 与 no-default-features 的 all-targets Clippy（`-D warnings`）均通过，日志为
`integration/application-welcome-clippy.log` 和
`integration/application-welcome-clippy-default.log`。

生产默认关闭、真实飞书和群消息未执行，原支付服务与数据库未重启或覆盖。

### 精确欢迎最终整合

调用侧冻结 `5a37c288877bb8c7650556cafd88387d0f594edc`，随后正常合入基础线
`ac43412f52c3f3d3845a544a3ca84806dc077dec`。最终 merge 为
`145f6fbddd179678c347ed533b4ae8408cc7e3bc`，双亲即这两个提交；代码无冲突，首次提交标题因 Conventional
Commits 类型无效被拒绝，使用允许的 `fix` 类型后正常提交，未跳过 hooks。

合并后 all-features 共同 PostgreSQL 按默认并行执行 **104 passed / 0 failed / 0
ignored**，包含签名 Bridge 子进程、完整账户状态断言和新申请欢迎交接；resident_welcome 为
**25 passed / 0 failed / 0
ignored**。两组共 129 项，子进程内部的同一 Bridge 测试不重复计数。日志为
`integration/precise-person-parallel.log` 和
`integration/precise-welcome-tests.log`。新增交接场景沿既有 catalog 登记，test:list 可发现，test:harness
8 项通过；未修改共享执行器或 CI 门禁。

基础线的精确快照现已纳入：普通内容变动保留仍有效认人依据、内容需重审；A→B→A 中途未重开也不恢复旧 receipt，摘要不变但真实关系失效同样撤销旧批准。003/004源码未变，不重复改写迁移。原支付联合验收不重跑，ed0接收进程和证据保留。

本次仍不等于真实人员候选、群内可信交互、真实飞书修改/撤回、真人或生产验收。原最终 apply
smoke 的 URL hash
allowlist 阻断不变，未获得新 CI 提案批准；不能把这两组定向回归称为全部仓库门禁全绿。整条 PMS 业务范围的申请事项与后续办理动作汇合、跨营业日缩住/正常退房/未来取消成功路径仍是独立待收口项，不能由欢迎接线通过代替。

### 来源汇合与剩余 PMS 成功路径

继续原授权的 §6/§7.2/T05/T06/T08/T11；本节更新上一切片的待收口项。

- 申请原事项可承接对话业务动作；同事项订房或同 quote 并发/换消息/重启复用原动作。UNKNOWN、执行中或已完成不能通过换 quote 偷建新单；确定 NOT_EXECUTED 后允许新修订动作。
- 当前调用人范围权限仍先校验，其他主体/会话不能借重复请求取得旧动作。
- 新 link 工具实际 GET 宿主指定订单，绑定来源修订、订单版本、操作者和会话后展示摘要。自然“确认关联”仅批准当前唯一提议，版本改变需重审，“取消关联”只取消自己的当前待确认关联。
- 模型不能传 readback、批准或原文；精确版本 UUID 句式仅供本地兼容，不要求员工复制内部 ID。
- 同事务锁定来源与目标，原申请/支付 inbox 保留，metadata 保存可靠归属并记一次审计。
- 后续阶段核对同订单，支付仍单独读 bill 和确认，不从代付款人、姓名、金额自动合并。已含独立动作的来源拒绝迁移。无新迁移，001—004及欢迎 snapshot 未修改。

两项新增 PG 场景首轮通过，扩入默认并行共同回归 **106 passed / 0 failed / 0
ignored**。覆盖申请修订、并发、重启、UNKNOWN、更换 quote 拒绝、确定未执行后的合法修订、撤权、旧订单版本确认失效、申请/支付来源关联、一次审计、跨订单拒绝和独立财务确认。日志
`integration/source-pg.log`、`integration/source-person-regression.log`；Python
32 项通过，日志 `integration/source-python.log`。

真实 PMS 全链最新 **1 passed / 0 ignored**，包含正式 Hermes
PluginContext/registry、可信宿主、实际 Unix broker、共同 PG 和实际 PMS
HTTP。PMS 源码为只读 `0254fbabdda0b76b56370248f2ad24e44e4e950a`，未跨仓修改。最终 Agent
OS PG17 使用独立 `52322`，PMS PG17 为测试锁保护的
`52319`；模拟人员、申请、时钟与 HTTP 人工接手不是浏览器点击或真人渠道验收。

| 成功路径              | PMS 回执                                                           | 实际订单与回读                                                                                                                  |
| --------------------- | ------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------- |
| 跨营业日 SHORTEN_STAY | `receipt_cb0872e9-6090-4de7-961a-4cbd00a8fce9`，EXECUTED/committed | `order_313a9088-f852-41d5-a536-121b7be75eb0`，9月23入住，缩至9月25离店；SQL仅1个缩住 amendment                                  |
| 到期 CHECK_OUT        | `receipt_b2ffe50a-89bd-42e2-bf78-3c32ca87ebca`，EXECUTED/committed | 同上订单，实际回读 CHECKED_OUT                                                                                                  |
| 未来 CANCEL_ORDER     | `receipt_098ae25a-c2e8-4b27-9cbd-f65f88d7c250`，EXECUTED/committed | `order_ea8a058c-1858-42cd-a53b-2708f4469fa7`，9月26—27独立住宿，模拟员工HTTP接手后CANCELLED；岸岸manual_completed，重复执行拒绝 |

全链同时验证同名住客的另次未来住宿保留不同订单/事项，以及来源申请关联原对话订单后继续收款。日志
`integration/pms-success-paths-verified.log`，SQL证据
`integration/pms-success-receipts.json`。原支付联合5事件/2事项/1完成动作/cursor5账本与原最终证据逐字段相等；回读证据
`integration/payment-preservation-check.json`，未重启ed0或重新投递。

失败证据全部保留：

1. 缩住服务时钟推至明天，但 SQL `qintopia_stage10_property_today` 按数据库当天，
   `pricing_revisions_stage10_date_matrix` 拒绝，真实回执 NOT_EXECUTED。
2. 将全链起点改昨天又触发改期 `stage11_date_change_state_matrix`
   拒绝。参照 PMS 原测试，改为独立昨天建单/入住，到数据库当天缩住；不改约束或订单状态。
3. 最初未来区间报价 422
   `PRICING_POLICY_UNCONFIGURED`；同原键恢复得到 NOT_EXECUTED，未建单。改用既有夹具可定价的9月26—27区间后成功；未改价规或放宽 API 校验。
4. 新一轮 auto 首先发现新增文档 MD013 行长，已分段修复；不调整 Markdown 门禁。

前期试验沿历史本地脚本向52316新增了独立模拟租户，违反本轮不向保留库新增测试写入的隔离要求。

原支付绑定/记录未变不等于该操作符合隔离约束；保留新增租户和日志，不清库。已把该脚本的 Agent
OS 目标切至52322，最终成功联测不向保留支付库写入。日期失败、报价诊断和 SQL 快照分别保留在
`integration/pms-success-paths-run.log`、
`pms-success-paths-clock-aligned.log`、`pms-success-paths-diagnostic.log`、
`pms-shortening-first-failure.json`、`pms-reschedule-clock-failure.json`，不覆盖原支付证据。

### T01—T14 分层核对

| 项目                       | 本地证据与状态                                                            | 真实渠道/生产边界或剩余本地限制                                                 |
| -------------------------- | ------------------------------------------------------------------------- | ------------------------------------------------------------------------------- |
| T01 对话独立办理           | 正式插件→broker→PMS查房/报价/建单/回读通过，无事件前置                    | 真实模型/专属Bot尚未验收                                                        |
| T02 缺信息/同名/多次住宿   | 无按名/金额自动合并；同名另次住宿不同订单/事项已通过                      | 真实自然对话澄清质量尚未验收                                                    |
| T03 身份/范围/伪造/撤权    | 当前精确授权、跨范围/伪造/父撤权PG与插件边界通过                          | 真实工作群成员及岗位配置未接线                                                  |
| T04 方案变化/过期/库存冲突 | 旧预览/确认失效与真实PMS拒绝保留，重审新方案                              | 不把原预览或缓存房态作为保证；真实并发客服流程未验收                            |
| T05 重复/重启/并发         | 来源事项/quote订房防重复及同阶段并发、重启PG通过                          | 不自动合并两次不同业务                                                          |
| T06 丢响应/UNKNOWN         | 真实PMS Confirm成功丢响应沿原键恢复；UNKNOWN换quote拒绝                   | 未确定结果不换键，不声称生产故障恢复已验收                                      |
| T07 多种付款               | 两笔独立当笔确认、部分收款、代付款来源关联；到账不入住                    | 同租户双合法作用域专项已通过；实际渠道另待验收                                  |
| T08 入住及住宿变更         | 实际改期/续住/换房、跨日缩住、到期退房、未来取消成功回读                  | 真实操作人/生产未验收                                                           |
| T09 暂停/取消/人工接手     | 暂停恢复新预览；真实HTTP模拟入住/取消接手后manual_completed不重放         | 历史精确核验、现金事实确认及CREATE_ORDER人工方案采纳已补齐；真人UI/渠道仍未验收 |
| T10 事件基线/乱序/补拉     | 原支付联合5事件全accepted、断点与push/feed先后顺序通过                    | 原结果不等于生产sender配置验收                                                  |
| T11 三来源可靠汇合         | 申请原事项防重订房；申请/支付明确关联同对话订单PG通过；申请+真实PMS链通过 | 不依据同人/同名/金额或单候选自动关联；真实飞书触发未验收                        |
| T12 注入/凭据/URL          | 模型字段白名单、可信宿主、固定loopback、脱敏、redirect拒绝通过            | 本地同UID隔离不等于生产强隔离                                                   |
| T13 群提醒/欢迎交接        | 可靠关系→独立welcome_event→公共确认，默认关闭；缺工作约定不启动催办       | 候选投影匹配、企微确认映射、二花旧入口复用尚未接入；真实发送另待验收            |
| T14 Runtime/Profile        | 正式Hermes SDK/插件加载及本地专用环境通过                                 | Livecool实际模型、专属Bot、生产配置、真实操作效果均未验收                       |

本轮完成上述来源汇合和三条成功路径；剩余本地限制与真实验收没有相互替代。保留 PR 草稿，不自动推送、远端合并、发布、部署或外发。

来源汇合阶段提交前补充：最后收紧精确句式的 proposal hash/消息时间后，专项 PG
2 项再次通过（`integration/source-pg-final.log`）。106 项共同回归与 Clippy 在该小修之前通过，不能当作小修后的全套结果。PR 正文检查和
`pr:doctor` 通过，尚无 upstream 且保持本地草稿。综合检查最新默认 Rust 851 passed / 2
ignored，sidecar smoke 仍在运行，后续结果另记，不提前认定总门禁通过。

### T09 最终本地收口

来源汇合已提交
`af38f1d0d3dc9829fa6ded4e9a9de8244da2b9cc`。本节更新此前 T09 本地缺口，不将来源、欢迎或模拟结果当作生产验收。总指挥依据既有 T09 授权确定具体实施路径，没有新增人员权限类别或 PMS 接口。

人工接手先保存实际订单版本和已有收款事实 ID；登记时间与 manual_handoff 在同一 SQL 更新中取得，基线读取与登记之间发生的人工记录不会被旧时间误认。重复接手保留第一次基线。人工接手后禁止通过暂停/恢复绕回自动执行，UNKNOWN/在途仍先沿原键恢复。

- 入住、退房、取消、改期、续住、缩住、换房：按接手之后的唯一 amendment 核验完整 payload、命令、时间及版本，且它仍须是当前最新订单版本。状态相同、旧历史、后来改动和多义记录均不能代替具体效果。
- 有流水收款：原流水、订单、方式、金额、币种及新 fact/command 一致，且未冲正或转出。无流水现金先展示具体事实，有权人自然确认后再次 GET，仅记录原等待人工完成；完整可信回报准确指明同笔事实则直接采用。
- CREATE_ORDER：模型可从授权订单查询中选候选，宿主实际 GET 并展示原方案、订单、必要住客辨识、房间/日期/渠道/合同金额及差异。有权人采纳后重新回读；旧版本、缺事实、越权、多提议和其他事项冲突拒绝。

  原完整可信回报已覆盖具体简单订单和差异时不重复确认，不要求员工手输 UUID。

- 审计区分 exact_pms_effect、human_collection_adoption 和 human_order_adoption。人工采纳保留原 preview 和版本、订单事实、确认人/时间/决定，原 result 仍为空，不伪造原执行回执，不自动确认身份、收款或到店。
- 人工采纳订房也可再明确关联有效支付/申请来源，继续原事项。新 PG 组合验证无原 PMS
  result/receipt 仍能合法关联，但缺采纳依据、错订单、当前撤权均拒绝；关联后 CREATE_ORDER 拒绝，收款仍独立等待人类批准。

最终验证（仅受影响范围）：

| 验证                                           | 实际结果                                       | 证据                                                         |
| ---------------------------------------------- | ---------------------------------------------- | ------------------------------------------------------------ |
| 业务 PG，all-features、默认并行                | 23 passed / 0 failed / 0 ignored               | `integration/t09-final-business-pg.log`                      |
| 最后补齐人工采纳后来源关联组合                 | 3 passed / 0 failed / 0 ignored                | `integration/t09-final-source-link-pg.log`                   |
| 官方 Hermes→Unix broker→共同 PG→实际 PMS HTTP  | 1 passed / 0 failed / 0 ignored                | `integration/t09-complete-pms-final.log`                     |
| Python 插件/宿主/客户端                        | 33 passed                                      | `integration/t09-final-python.log`                           |
| all-features / all-targets Clippy，-D warnings | 通过；最后仅扩展 PG 断言，Rust 编译/专项另通过 | `integration/t09-final-clippy.log`                           |
| 业务目录发现、harness                          | 新3场景可发现；8 passed                        | `integration/t09-catalog.log`、`integration/t09-harness.log` |

PG 子进程的单项回显已包含在23项中，不另外累计。实际 PMS 全链包含7类住宿人工接手、有流水收款、现金事实确认及人工采用不同住客方案的建单。模拟员工经实际 HTTP
Preview/Confirm 操作，不宣称浏览器点击或真人操作验收。PMS 源码仍为只读
`0254fbabdda0b76b56370248f2ad24e44e4e950a`。

最新实际回执示例：

- 现金事实：`receipt_db49fe3b-9368-43c2-a23a-e36b0c5de853`。
- 人工 CREATE_ORDER：`receipt_1016c42a-8ab8-4936-a7b3-f956819d69de`。这是 PMS 员工执行回执；岸岸原订房动作保存人工采纳依据，未将该回执冒充自己的执行结果。
- 缩住：`receipt_16e35da2-8ef3-4a24-9a4d-13d8b56fa5ba`。
- 正常退房：`receipt_87855b68-2ebb-4dbc-b92c-afda1345cb44`。

最小账本证据 `integration/t09-final-ledger.json`
保留动作、阶段、订单、回执或人工证据种类，不保存原始历史正文或凭据。

### 检查归属、失败与隔离

本次 `source-pr-auto-verified.log` 的最后 apply smoke 仍因
`database URL hash is not in the reviewed allowlist`
失败，随后 JSON 解析失败是其连带结果。未修改 allowlist 或共享 CI；既有测试指南已记录该阻断及未批准提案。

该次 auto 与 T09 初次源码写入存在混合窗口，不整体归为 af38f1d：

1. 已在首次 T09 写入之前完成并收取结果的默认 Rust 851/0/2、两项 sidecar
   smoke 保留原阶段归属。
2. all-features
   Clippy 及后续 PostgreSQL 各命令与源码写入/恢复窗口有交叠或无法精确确定读取时刻，统一只保留诊断证据，不将其106项/24项回显冒称某个稳定版本通过。
3. 22:46:33（Asia/Shanghai）已将 T09 差异保存为
   `integration/t09-pending.patch`、`business_manual.rs.pending`，恢复受检源码到 HEAD。22:49:26
   auto 结束后才重新应用差异继续实现。日志没有每条命令的精确起止时间，不补造时间；以上按工具结果顺序及文件时间记录。
4. 最终 T09 稳定业务源码另做上述23项业务 PG、Clippy 和实际链；最后只扩展人工采纳来源关联断言，再跑3项专项。不为旧阶段追求全绿而重复欢迎或其他无关重型层。

保留两次增量联测失败：

- `integration/t09-complete-pms.log`：现金输入缺 PMS 必填的收款人说明，Preview 返回400
  VALIDATION_ERROR，随后 handoff 被终态保护拒绝。修复测试输入的 note，并在 handoff 前断言 awaiting_confirmation；未改 PMS 校验。
- `integration/t09-complete-pms-verified.log`：现金已成功；测试误读人工建单回执的 result.id，实际契约是 result.orderId。修正测试字段后最终全链通过；无业务重试回放。

独立测试脚本保留在 `.local-workspace/anan/run-full-chain.sh`。固定 Agent PG52322、PMS
PG52319，解析 URL 并实际 SELECT 核对库名/端口。外部故意传52316且 ANAN_PREFLIGHT_ONLY=1 的检查仍使用52322/52319，在 reset 和锁库入口之前退出，无写入；证据
`integration/t09-readonly-preflight.log`。前期向52316新增模拟租户的隔离偏差继续保留，不清库或掩盖。

### T01—T14 最终状态边界

上表 T01—T14 的证据仍有效，T09 已按本节补齐可行本地路径。

T01/T04/T05/T06/T08/T09/T11/T12 已有对应本地业务、恢复和边界证据。

剩余验证按性质保留：

- T02：真实澄清质量；T03：真实工作群身份配置。
- T07：双合法作用域本地专项通过；实际渠道/物业联演另待验收。
- T10：生产 sender 配置和实际投递。
- T13：候选投影匹配、企微可信确认映射、二花旧入口复用尚未接入实现；真实发送另待验收。
- T14：Livecool 模型、专属 Bot 和 Profile。

此前双作用域本地验证缺口由下节专项更新；生产配置缺失不描述为本地代码失败。

保留 PR 本地草稿，未 push、创建远端 PR、合并、发布、部署或发送真实业务消息。本地模拟进程与资料为后续验收保留；001—004及欢迎 snapshot 不改写。

最终格式、Markdown、协作规则及 diff 检查已按修改范围收尾。PR 正文校验和 pr:doctor 通过；无 upstream 是保留本地草稿的预期状态。正常提交钩子结果保留在
`integration/t09-commit.log`。

### 双合法作用域隔离专项与 T13 依赖更正

沿已有授权，仅补测试、既有业务目录登记和交接报告，不修改业务源码、PMS 或 CI。任务专用可销毁 Agent
PG52322 的启动只读核验见 `integration/t07-preflight.log`；本轮不启动 PMS
HTTP、不动52316及原支付 sender/receiver。

场景在同一新模拟租户创建一栋、二栋两个有效的栋级作用域。

两位不同 Person 各有有效任命、精确 RECORD_COLLECTION 授权、可信 Gateway 及独立来源/物业绑定；两侧先通过真实 business_authorize，不能用无效 B 代替隔离正例。

两边使用相同事件 ID、bill
ID、订单标识、流水、12000分金额和消息 ID，并发接收成功后分别查询各自事件、创建独立事项和收款方案。

两侧的收款方案都进入 awaiting_confirmation，事件本身不批准执行。

随后双向验证来源/物业替换、绑定/事项替换及已合法的另一主体越界被拒或不可见。

对绑定、事件、checkpoint、动作、事项及事件审计逐字段快照比较，确保另一侧不变。

A、B 再分别正向接收 MATCHED，另一侧完整账本仍保持不变。

这验证共同服务的真实权限、事务和持久隔离；PMS 预览作为外部边界模拟，不声称实际 PMS 双物业 HTTP、真实渠道或生产联演通过。

首轮编译发现测试直接访问私有 Actor.person，已改用现有 verified_person 接口；失败日志
`integration/t07-two-valid-scopes.log` 保留，未放宽可见性或业务保护。

T13 依赖按基础线的
[尚未完成清单](2026-09-24-welcome-operations-local-implementation.md#尚未完成与下一步)
分开交接：

- **尚未接入实现**：飞书/PMS 候选资料投影与匹配；企微按钮/消息到welcome_confirmation 的可信映射与岸岸群呈现；二花全部旧入口复用已确认人员关联。
- **另待真实验收**：提醒、欢迎实际送达、照片/卡片与真实渠道交互，以及模型、Bot/Profile、生产配置及发布门禁。

上述未接入实现由原基础线负责，本业务切片只标明依赖，不能将它们一概称为“真实环境待验收”，也不把本地 broker/可靠案例交接通过当作已接通真实群确认。

专项结果 **1 passed / 0 failed / 0 ignored**，见
`integration/t07-two-valid-scopes-verified.log`。all-features/all-targets Clippy（-D
warnings）通过，日志 `integration/t07-clippy.log`。既有目录新增场景可发现，harness
8 项通过，日志
`integration/t07-catalog.log`、`integration/t07-harness.log`。本轮只增加边界测试，没有重跑已通过的 PMS 全链或无关重型层。

T13 提醒的“未配置不启动”不是实现完成的证明；阶段5/§7.3 的有效约定、执行、停止及受众校验仍需下一有界切片按实际代码审计，不能一概归为外部验收。

## 阶段5提醒与申请候选薄接线（2026-09-25）

本轮从 `ca21709` 干净工作树继续。

已读入口 README、路线图、变更路由、编程护栏、Sidecar 规则、PMS/Foundation 包及 §7.3/T13，审计证实原 PMS 只有“缺约定不催办”的说明，缺实际执行与持久停止；

现有群 worker 固定二花，不适合借身份发送岸岸提醒。

新增独立 business_reminders 与 reminder_host，复用当前规则版本、工作连接、主动受众、物业/群绑定、原 WorkItem.metadata 及 work_item_events。

主管当前有效规则决定目标、类别、时段、首次等待、重复间隔及升级；

无规则仅待办。

没有新增迁移、计时器、事实库或生产发送。

- 每次有界轮转100个事项；102事项测试证明后续事项可被扫描，不把每次只取前100条当完整消费者。
- 同一原事项合并缺信息/待收款/待到店；已可靠关联来源沿 canonical 事项，继承暂缓及最近发送时间；来源未知发送会阻止另发。没有可靠关联不按姓名或金额猜合并。
- 申请信息只由当前可信回读与004字段摘要核验；订单按实际当前合同额、净实收和入住状态，支付事件不能推断入住。完成停止对应提醒；暂停/人工接手/在途先保留等待。
- claim 与 validate 都重验规则、当前职责授权、精确读订单授权、主动受众、当前群绑定、事实与内容摘要。只有 validate 才一次性提交 UNKNOWN 发送边界，之后只能登记原 claim 的具体模拟回执，不能重发。
- 暂缓沿当前人员的客房办理授权和可信当次消息，记录人、期限和原证据，不改订单。模拟渠道固定 anan，无二花/default
  fallback。
- 申请候选薄接线保留同次规范化 HTTP
  values，004保存后才调用独立 HOST_TOKEN 的 welcome_source_projection，再调用原 reconcile_welcome。

候选不等于身份确认；

投影失败与源保存成功分开。

公共候选服务仍由欢迎基础线交付，尚未在本切片整合005或声称服务端PG端到端通过。

最终资料规则为新申请手机号必填、主要按手机号匹配，姓名昵称辅助；

历史不追补、不催补、不阻断旧业务。

取消中间证件方案；

无证件字段新增、不放通普通脱敏、不改004摘要。

回读不能把首次观察当新提交，来源表单必填另属提交端；

提醒不接受 phone 催补规则，避免把新要求追施历史。

### 本地证据与失败修复

证据根仍为忽略目录 `.local-workspace/anan/`。

| 范围                                         | 结果                            | 证据                                                         |
| -------------------------------------------- | ------------------------------- | ------------------------------------------------------------ |
| 只读库预检                                   | Agent52322、PMS52319；禁止52316 | `integration/t13-preflight-verified.log`                     |
| 提醒共同服务/PG                              | 5 passed / 0 failed / 0 ignored | `integration/t13-pg-isolated-final.log`                      |
| Python包回归与候选/适配契约                  | 41 passed                       | `integration/t13-python-second.log`                          |
| all-features/all-targets Clippy，-D warnings | 通过                            | `integration/t13-clippy-final.log`                           |
| 统一目录新增5场景、harness                   | 可发现；8 passed                | `integration/t13-catalog.log`、`integration/t13-harness.log` |

PG五项中已包含真实 Unix broker→Python 宿主/PMS
HTTP客户端→共同服务/PG→本地记录渠道，不另计一次。PMS HTTP和渠道均为模拟；

只产生1条模拟消息，settle丢失后读取原回执而不重发，实际模拟事实变为收齐且 CHECKED_IN 后持久 stopped。

其余测试覆盖两类合并、缺规则、暂缓及到期、申请修订/补齐、可靠来源关系、时段、并发唯一 claim、重启 UNKNOWN、错误回执、撤销受众/群及跨范围拒绝。

来源关系测试预置已经确认的关联，来源关联命令本身由前轮专测负责，不冒充此轮重新验证。

保留失败：初次直接执行无执行位的本地预检脚本返回126，改用 sh 后只读预检成功；

PG第一轮重复调用 bootstrap 导致租户主键冲突，第二轮规则职责未配置导致 scope_access_denied，均只修测试 fixture，不放宽授权。

第四轮完整宿主测试的模拟HTTP误写订单 query 路径，真实客户端原路径正确；

修外部模拟后通过。

见 t13-preflight.log、t13-pg-first/second/fourth.log、t13-broker-second.log。

已有广泛 auto 检查的 apply smoke URL hash
allowlist 阻断仍按前述记录处理，不改 CI 门禁。最终本地PR/综合检查结果在本节追加，未通过者不包装为通过。

### 仍未完成的层级

提醒已有本地执行，不再把它整体列为“未实现”；真实 Hermes
cron 安装、专属 Bot/Profile、群消息送达与模型自然交互仍未启用/验收。

本轮不涉及生产、SSH、真实外发、PMS源码、远端PR或合并。

欢迎共享候选/创建人员/群确认及旧入口接入仍待基础线稳定提交与统一整合；

申请客户端测试不能代替那部分服务端验收。

停用宿主即可停止扫描，保留所有事项、暂缓、尝试和 UNKNOWN，不删除重放。

最后自查修复了申请 Bridge 的恢复遗漏：过去顶层 accepted/duplicate 会直接清除唤醒，忽略已经保存后的候选/欢迎技术失败。

现在只对 projection_unconfirmed/handoff_unconfirmed 保留既有唤醒和退避，持久 followup_pending；

不把004源保存改报失败，不把无许可/缺可靠住宿关联/人工确认等待变成技术重试。

Bridge 原生18项通过（含两种失败后恢复、新唤醒不丢、旧job不执行），见
`integration/t13-bridge.log`。

本轮首次 check:pr:auto 在 Markdown 长行检查失败，未进入重型层；仅修新增文档分段，不修改检查配置。保留
`integration/t13-pr-auto.log`，随后复跑另列结果。

最终隔离自查将 broker 用例放入独立子进程，避免 HOST_TOKEN/Profile 开关污染 native 整组后续用例。

父测试按一项计数，子进程不重复计数。专项按默认并行复验；业务源码未因测试便利放宽。

第三次 auto 在 quick 层主动停止以修此测试隔离，未进入旧T07/T09/支付链；日志为
`integration/t13-pr-auto-verified.log`（文件名不表示通过）。

按协调要求不重复旧链，最终只完成 quick 层及本轮专项；既有 heavy/apply smoke
allowlist 阻断仍未解决，未修改任何CI门禁。

最终收口：`pnpm check:pr:quick` 通过，见 `integration/t13-pr-quick.log`。

稳定源码的专项为5 passed / 0 failed / 0
ignored，已含独立broker子进程，不另加1；最终all-features/all-targets Clippy通过。

本地 PR 正文与 doctor 通过，见
`integration/t13-pr-body.log`、`integration/t13-pr-doctor.log`；doctor提示未设upstream及提交前脏树，不是检查失败。

Bridge18项对应本提交的 run_application_one
followup_pending 分支和参数化恢复测试，既有退避、队列及旧job路径不变。没有新远端PR、push、合并、生产操作或005变更。
