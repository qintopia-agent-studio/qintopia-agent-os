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
