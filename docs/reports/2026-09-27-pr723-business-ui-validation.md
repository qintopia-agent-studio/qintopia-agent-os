# PR #723 物业业务授权入口与本地验证

日期：2026-09-27。范围：岸岸工作账号授权工作台的组织关系二级入口与管理范围元数据。仅本地模拟数据；未接入真实企微或 PMS，未发布或部署。

## 2026-09-28 正式网页应用接线修订

总指挥确认应用实现可先于生产 host 的实际分配，并将候选方案收敛为独立管理 HTTPS
origin、独占根路径。沿用现有统一组织工作台、密码会话与实时管理 grant；live
UI 在启动时读取并校验唯一 HTTPS public
origin，继续只监听 loopback。HTTP 入口按该配置核对精确 Host、写请求 Origin 与 JSON 类型；现有根路径页面、资产、导航和 API 保留。正式会话 cookie 在登录和清除时均带
`Secure; HttpOnly; SameSite=Strict; Path=/`。转发头不参与可信站点或授权判断，模拟 HTTP 根路径入口保留。

本地模拟配置仅使用 `https://admin.example.test` 和空闲 loopback 端口；部署候选端口
`127.0.0.1:18780` 不是硬编码值。公共 COS 页面与其子路径共享浏览器 origin，cookie
Path/SameSite 不能隔离同源页面脚本的 API 访问，因此本轮不实施公共站点 `/agent-os/`
子路径。最终独立 host、代理、受管 unit、生命周期及回退由部署 owner 单独核实和验证；若独立 origin 不可用，再另行讨论子路径。本 PR 不修改部署文件、CI、真实数据或生产状态。本节为实施前设计记录，后续验证结果须单独补记，不将本地模拟等同于正式站点可用。

受管 UI 的停止合同：SIGTERM 后关闭 listener，不再接收新配置；在途请求等待最多 30 秒，正常单请求处理上限 5 秒。

未进入配置写入的慢连接、解析或只读错误只关闭连接并继续服务；已进入配置写入的处理错误或超时以结果不明的非零状态退出，不重放原操作。停止等待超时且已有配置写入也不能视为排空。部署 owner 已确认
`Restart=on-failure` 会冲突，采用管理 UI `Restart=no`
最小提案；quiesce 在 stop 前保存精确
`InvocationID`、`Result`、`ExecMainCode`、`ExecMainStatus`、`MainPID`、`NRestarts`
和受限固定错误码，排空后复查同一 invocation。失败或不明保持 hold。

带 `operation_id`
的配置保存按原键与审计版本回读；账号、改密和退出原协议没有命令键，须按日志中的
`account_ref`、`person_ref` 或 `actor_ref`
核对账号、会话和审计状态。无法唯一判定时继续 hold。UNKNOWN 日志只含固定错误码和这些 UUID，不含凭据或私人表单。部署代码仍待单独批准；本轮不编辑 unit 或 runner。

### 首批配置路由矩阵（实施前核对）

| 已有工作台动作                                       | 前端 API                                                                      | 修订前 live 状态                            | 服务端实时授权与本轮处理                                                                                                                                    |
| ---------------------------------------------------- | ----------------------------------------------------------------------------- | ------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 登录、退出、改密；开通、重置、停用个人账号           | `/api/login`、`/api/logout`、`/api/password`、`/api/accounts`                 | 可达                                        | 会话绑定当前 Person；账号管理要求根范围 `default/organization/identity` 管理 grant。补固定 HTTPS origin 与安全 cookie。                                     |
| 物业绑定、工作账号登记或停用、具体操作授予或撤回     | `/api/business`、`/api/business/preview`、`/api/business/save`                | 可达                                        | `business_config` 事务内按当前 `anan/hospitality/read_business` 或 `execute_business` 管理 grant 及来源、账号、物业版本重验；业务 admin 不授配置权。        |
| 组织关系和管理范围读取，现有部分任职、撤权与结束动作 | `/api/state`、`/api/preview`、`/api/save`                                     | 部分可达                                    | Store 已重验 Person、当前管理 grant、版本与审计。只将现有 UI 的台账、岗位、职责、范围、生命周期与受众配置动作加入显式 live 白名单；不放行旧兼容或未知动作。 |
| 人员可信来源候选、确认与撤销                         | `/api/identities[?person]`、`/api/identities/preview`、`/api/identities/save` | 被 live 白名单阻断                          | Store 已要求密码会话与根 `identity` 管理 grant；live 网页确认另须与生产 CLI 一样核验企微宿主观测，撤销沿用原核验与审计。                                    |
| 组织详情与只读权限解释                               | `/api/ontology?scope=`、`/api/audience-preview`、`/api/decision`              | 被 live 白名单阻断                          | 只开放现有 UI 确实调用的路径，仍由 Store 对当前 scope、Person 与授权判断。                                                                                  |
| 基础台账导航                                         | 现有工作台 `ledger` 标签                                                      | live 被前端隐藏                             | 有全部活跃范围 `default/organization/manage` grant 者可见；页面本身不授予权限。                                                                             |
| 本地对话、规则、欢迎审核与业务执行                   | `/foundation`、`/api/foundation/*`                                            | 规则与执行受 LOCAL 门禁；页面部分为模拟展示 | 本轮不打开 LOCAL 门禁、固定话术、fixture、talk、欢迎审核或业务执行。规则编辑仍属后续独立接线，不作为本次首批配置成功证据。                                  |

人工新增人员台账会先创建待核验 Person 草稿，不产生可信来源身份、登录或业务授权；它与要求来源证明的生产 CLI 草稿入口不同。该影响已向总指挥单独报告。应用新增修改位置为
`local_server.rs`、`workbench.js`、`store/identity.rs`
及既有局部测试；无需新增 grant 类别或修改数据库迁移。

### 受限数据库角色复核（2026-09-28）

部署 owner 的独立 PostgreSQL 复现实验发现，仅有 `SELECT`
的管理 UI 角色不能执行现有身份确认事务的
`LOCK TABLE person_identity_gateways IN SHARE MODE`，也不能对实际行执行
`FOR SHARE`。前者冻结旧 writer 可能插入的 namespace 冲突行，后者保护当前来源、Gateway、范围及 Person；二者须分别解决。不得以移除表锁、仅加未被旧 writer 共同采用的 advisory
lock、或授予身份表整表 `UPDATE` 绕过。候选最小差分是经独立审查的静态 `SECURITY DEFINER`
锁入口，专属非登录 owner、固定 `search_path`、全限定表名、无动态 SQL，撤销
`PUBLIC EXECUTE` 并只授受限调用角色；表锁 owner 是否可仅持 `MAINTAIN`
取决于实际 PostgreSQL 主版本，须由部署 owner 只读核实。现有事务顺序和并发负例不变；行锁权限单独核对，不借 CLI 不可达路径扩大 UI 权限。

任何新函数与 grant 需要新增版本化迁移、角色接线与定向权限验证，方案审查前不实施 DDL。

同轮发现 `/api/ontology?scope=` 在查询命中当前 `rule` 行时会调用仅接受模拟 tenant 的
`foundation::authorize_current`，导致 live 只读页面失败；空规则数据不暴露问题。修订应复用本事务已加载的 Policy，执行与原函数等价的当前 grant/职责判定，保持
`/api/foundation/*` 的 LOCAL 门禁，补有真实规则行的 live 回归。密码 HTTP 入口的
`/api/foundation/card` 旧 GET 分支没有经过普通 foundation
dispatch 的 LOCAL 开关；live 路由须在认证后统一拒绝本地页面与全部
`/api/foundation/*`，不能只依赖 POST dispatch 的环境门禁。

#### 待复核的最小锁能力差分

固定函数仅接受可信 Store 传入的 tenant、来源 link UUID、Gateway
key 或工作账号 UUID，不接受表名、SQL、权限模式或任意条件；统一
`SECURITY DEFINER SET search_path = pg_catalog`、全限定表名、无动态 SQL。四个行锁函数完整执行原查询并按
`ROW_COUNT > 0` 返回
`boolean`；false 时应用立即拒绝，不继续读取可能新出现的未锁关联行。表锁函数返回
`void`。候选函数及原 SQL 对位如下：

| 固定函数签名                                                                             | 同事务固定锁操作                                                                                                                        | 应用调用点                                                                                                                       |
| ---------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| `qintopia_identity.management_ui_lock_actor(text, uuid) RETURNS boolean`                 | 由 tenant 的 live `identity_namespace` 和 link id 精确定位 `source_identity_links l`、`persons p`，`FOR SHARE OF l,p`                   | `Store::verify` 中 live、非 Gateway Person 路径，包括密码会话与受控首次账号 bootstrap；false 按原来源核验拒绝。                  |
| `qintopia_identity.management_ui_lock_gateway_registry() RETURNS void`                   | `LOCK TABLE qintopia_identity.person_identity_gateways IN SHARE MODE`                                                                   | `identity_ui_command` 原表锁位置、原 tenant 事务内；不替换旧 writer 并发保证。                                                   |
| `qintopia_identity.management_ui_lock_identity_candidate(text, uuid) RETURNS boolean`    | 与现有确认查询同一 tenant/link join，`FOR UPDATE OF l FOR SHARE OF g,s`；表 SHARE 后完整锁住所有匹配 Gateway，不能用 `EXISTS`/`LIMIT 1` | `identity_ui_command` 原行锁位置；false 按原 `identity_scope_unbound` 拒绝，随后仍按当前版本、namespace、宿主观测与 grant 校验。 |
| `qintopia_identity.management_ui_lock_business_source(text, text, uuid) RETURNS boolean` | 与 RegisterAccount 原查询同一 tenant/Gateway/link join，`FOR SHARE OF g,l,s`                                                            | `business_configure` 登记预览与保存的来源读取前；false 按原 `observed_work_account_required` 拒绝。                              |
| `qintopia_identity.management_ui_lock_business_account(text, uuid) RETURNS boolean`      | 与 GrantOperation 原查询同一 tenant/account join，`FOR SHARE OF w,g,l`                                                                  | `business_configure` 操作授权预览与保存的账号读取前；false 按原 `work_account_unavailable` 拒绝。                                |

这些函数只把既有锁语句移到受限 definer 下，不合并不同表锁/行锁，不代替后续普通 `SELECT`
对已锁实体当前数据的完整校验；保留现有 `begin()`
tenant 锁、锁顺序、操作标识与失败语义。对于 `business_property_bindings`
等网页角色本来获准更新的业务表，仍使用原行锁；`raw_events FOR SHARE`
属可信 CLI 来源草稿，不为网页新增第六个入口。

需新增 `runtime/postgres/migrations/` 的单一版本化追加迁移、配套
`runtime/postgres/docs/data-design/` 说明及 `schema_change_log`
记录。优先沿用现有 SQLx 迁移身份：在同一事务创建固定函数并立即撤销各函数的
`PUBLIC EXECUTE`，不在迁移中创建角色、引用尚未准备的角色、要求 `CREATEROLE`
或硬编码 PostgreSQL 版本专属的
`MAINTAIN`。迁移后、UI 启用前，由部署 owner 的受审管理员准备专用非登录、非 superuser 函数 owner，赋予其固定锁实际所需的权限，再转移函数 owner、保留原 runtime 身份的
`EXECUTE`、仅额外授 `qintopia_management_ui EXECUTE`。表锁 owner 是否可仅持 `MAINTAIN`
已由部署 owner 只读核实生产 PostgreSQL 18.4，后置非登录 owner 的 Gateway 表锁权限使用
`MAINTAIN`；版本不兼容时不得自动扩大 UI 权限。启用前预检必须验证精确函数 owner、`SECURITY DEFINER`、固定搜索路径、函数 ACL、`PUBLIC`
不可执行、UI 无 owner membership/`SET ROLE`、schema `CREATE`
或函数替换能力，并以真实角色运行非空行 HTTP 链。未完成后置准备和预检时 UI 保持关闭；不另起迁移器或改变现有发布流程。后续修改已转移 owner 的函数须另行审查迁移兼容性，不能默认
`CREATE OR REPLACE`。回退保留追加函数和审计数据，先关闭 UI 路由与进程；不从数据库删除既有授权。

验收须用专用隔离库的真实受限角色及非空行覆盖 bootstrap、登录/state、身份预览/保存、账号登记/停用、物业绑定、操作 grant/revoke、并发插入 namespace 与版本漂移；直接
`SELECT`/空表成功不算通过。冻结迁移后提供精确 SHA 与校验和给部署 owner 做基线差分，不重复无变化的 NATS/P29 验证。本段记录实施前方案；实际本地结果如下。

### 固定锁迁移与受限角色实测

- 新增 SQLx 迁移 `202609280001_management_ui_lock_capabilities.sql`，SHA-256 为
  `7ad5e650b1e1beb21b5ed791e631dc461da271c9371e94bedc911242a24a8b2a`。本任务隔离 PostgreSQL
  18.6 容器 `agentos-pr723-ui-20260928` 的 `qintopia_messages._sqlx_migrations`
  已登记第 46 条及 `schema_change_log` 的 `2026-09-28.001`；五函数均为
  `SECURITY DEFINER` 且固定 `search_path=pg_catalog,pg_temp`。
- 仅在该隔离库按部署候选矩阵创建 `qintopia_management_ui` 登录角色与独立
  `qintopia_management_ui_lock_owner`
  非登录角色。后者持必要 SELECT、列级 UPDATE 和 Gateway
  `MAINTAIN`；五函数转给该 owner，临时 schema
  CREATE 随即撤销。ACL 查询确认 UI 可执行五函数，`PUBLIC` 不可执行，UI 无
  `persons`/Gateway UPDATE、Gateway MAINTAIN 或身份 schema CREATE。
- 真实 UI 角色经现有 HTTPS
  handler 和非空行完成首次账号 bootstrap、登录、state/ontology/身份/业务读取、账号开通停用、物业绑定预览保存、共享账号登记停用、操作授予撤回、身份预览确认撤销。另一个连接在 UI 持有 Gateway
  registry 表锁期间插入同 namespace Gateway 得到 `55P03`；来源版本漂移的旧保存得到
  `identity_version_conflict`。UI 直接更新 Person/Gateway 行被数据库拒绝。
- 撤销前预置关联 welcome case、artifact binding、action 与 work
  item。撤销后 case 进入 hold，来源 link 与核验投影 link 各触发一次失效，case 版本由 1 到 3；artifact
  binding 失效，action 由 prepared 到 cancelled、版本由 1 到 2，work
  item 取消。无关 case 保持版本 1、未 hold；原身份命令回执可按 `operation_id`
  回读，已登录的被撤销 Person 会话失效。
- 在同一隔离库使用延迟约束触发器使实际 SQLx `COMMIT` 报错；应用输出固定
  `configuration_commit_outcome_unknown`
  与原操作 UUID。回读显示该次命令、审计未提交，tenant 版本未变化。触发器及函数已清除；此模拟只证明提交错误传播与按原键核查，不证明真实断连时事务必然回滚。普通确定性拒绝继续返回 HTTP 错误，断连/排空结果不明仍须停用并回读。

上述角色、数据、触发器均只在
`127.0.0.1:32778/qintopia_test`；未配置生产角色、origin、代理、unit 或页面。正式 host、浏览器交互、真实企微/PMS 与部署接线仍待对应 owner 另行验收。

该真实角色用例只有显式设置 `QINTOPIA_MANAGEMENT_UI_TEST_DATABASE_URL`
时运行；未配置角色的远端 CI 会输出
`restricted_management_ui_role_test_not_configured`，不构成角色链通过证据。

本修订的 `person_collaboration`
集成组在显式使用上述受限角色 URL 后串行 141/141 通过；`cargo fmt --check`、Markdown
lint、三处工作台脚本的 `node --check` 与 `git diff --check` 通过。新一轮
`pnpm check:pr:auto`
在 Light 阶段已通过格式、Markdown、registry、MCP、skills、inventory、CI 契约与已执行的 runtime/deploy 检查，但既有
`test-agent-runtime-management.mjs` 的 `check-deploy-runner.mjs`
子进程达到 600 秒超时，命令退出 1。该轮未进入后续 Rust tier 或 apply
smoke，不能记为总门禁通过；未更改 CI、部署检查或白名单。

新 head 的 PR-Agent partial 审阅再次质疑停用账号的旧 Actor 是否会直接通过
`business_authorize`。源码确认该入口与执行入口均在同一事务先调用
`verify`，其中工作账号路径要求账号处于 active 状态、账号/来源/网关版本一致且来源未撤销；之后才查具体操作授权。现有停用账号残留 grant 负例已改用旧 Actor 匹配的授权版本，并直接断言返回
`work_account_changed_or_revoked`。该用例在本任务隔离库以显式测试开关重跑后 1/1 通过。首次运行漏设
`QINTOPIA_COLLABORATION_TEST_ENABLE=1`，在数据库保护门禁处退出，未进入业务断言；修正环境后未修改业务代码。旧人工提案由后续动作读取时的
`actor_work_account_version` 检查挡住，旧提案 hash 不能批准新动作；PR-Agent 的
`d354592e732a`
评论不构成旧动作接续成功证据。该审阅跳过了大量文件，仍为 partial，不能替代完整人工验收。

远端 head `a306020` 的 CI run `36382816359`：Light、runtime 与 PostgreSQL
integration 通过，Rust quality 因新增 `auth_tests.rs`
的 HTTPS 请求辅助函数有 9 个参数而触发 Clippy
`too_many_arguments`，总检查失败。测试辅助函数现把四项请求头参数归到
`HttpsRequestHeaders`，未改变请求内容或应用逻辑。本地全目标全 feature
Clippy 通过；显式使用隔离库和受限 UI 角色 URL 的 `person_collaboration::auth_tests`
串行 10/10 通过。新 head 的远端 CI 结果另行核对。

## 实现与验证

- 工作台在「组织关系」按当前有效管理 grant 显示「物业业务授权」入口。关联岸岸客房职责的岗位可快捷进入；无岗位时仍可从管理范围进入。选中范围限定物业绑定、工作账号、候选和可授权操作。返回及保存后的焦点回到可见目标；读取到撤权后清除旧授权表单。
- 隔离 PostgreSQL
  `127.0.0.1:32777/qintopia_test`：岸岸业务集成串行运行 34/34 通过。共享账号负例确认仅持业务
  `admin` 角色时，配置状态不暴露管理范围，登记、停用、授予和撤回均被拒绝。
- 本机 Chrome 的 Playwright 会话：1440px 与 390px 的进入、返回焦点、ARIA 选中状态、无横向溢出与无脚本异常通过；模拟物业绑定完成预览、保存与焦点恢复。只读管理能力的模拟响应下，办理授权、跨能力撤权及停用按钮不出现。Codex
  Chrome 控制接口返回鉴权令牌不可用，未将其计作通过。

## 检查与环境边界

- 首轮 `pnpm check:pr:auto` 被新增测试的 `cargo fmt --check`
  拦截；排版后重跑通过 Rust 格式、Clippy、默认与全 feature 单元测试，以及前段 PostgreSQL 用例。
- 第二轮未显式设置 `QINTOPIA_SIDECAR_DATABASE_URL`，脚本回退到
  `127.0.0.1:5432`（归其他任务所有）。前段 PostgreSQL 用例误触该实例后，Space 配置测试失败；截断的自动检查输出未保留该用例的原始错误，不能把失败归因为已证实的产品回归。未停止、清理或进一步使用 5432；不能把该轮记为完整通过。
- 第三轮显式将 `QINTOPIA_SIDECAR_DATABASE_URL` 和
  `QINTOPIA_COLLABORATION_TEST_DATABASE_URL`
  指向本任务 32777 隔离实例。Light、Rust 格式与 Clippy、sidecar 单元测试（默认 856、全 feature
  871）、QiWe 326 项、业务 34 项、Person/Agent 147 项和欢迎 24 项集成均通过。
- 最后 `operations-control-plane-apply-smoke.sh`
  因 32777 数据库 URL 的哈希不在图像 staging 固定白名单内退出 1；后续 `JSONDecodeError`
  是没有成功 JSON 输出的次生错误。`pnpm check:pr:auto`
  总退出码为 1。未更改 CI 或白名单，不能把总检查记为通过。该 smoke 须在原受支持的部署验证环境完成。

## 新 head 审查修订

PR-Agent 对 `7570e566` 指出 `RegisterAccount` 没有限定 `shared`
网关。注册与候选查询已补该条件，业务账号解析、当前身份校验和新操作 grant 也拒绝普通
`employee` 网关的异常旧账号。隔离库新增负例通过；修订后整个 `person_collaboration`
集成组串行 134/134 通过。最终 PR 自动检查和远端 CI 以修订 head 为准。

PR-Agent 对 `9bafc8b9` 又指出旧 Person 动作接续和先按全租户截断配置列表的风险。

后续动作现显式保持原始 Person/工作账号主体类型。绑定、账号、候选和可见操作授权在 SQL 中先按当前管理范围或能力过滤，再应用 256 项上限。

模拟旧 Person 动作与 257 条不可见跨范围数据的 PostgreSQL 负例均通过；修订后整个
`person_collaboration`
集成组串行 135/135 通过。最终 PR 自动检查和远端 CI 仍以新 head 结果为准。

## 停用入口复核

对只具 `read_business`
管理能力的管理者，账号可能同时存在当前列表不可见的办理授权。配置状态新增只读
`can_disable`，按账号全部未撤销操作授权计算。只要其中一个操作超出该管理者能力，停用入口就隐藏。

撤销该授权后重新读取可恢复入口。后端停用命令继续在事务内重验全部授权，不以界面提示代替授权。

本次新增负例与完整 `person_collaboration` 集成组串行 150/150 通过；全目标全 feature
Clippy、Prettier、Markdown lint 与 `git diff --check` 通过。

本轮 `pnpm check:pr:auto` 的 Light、默认及全 feature Rust 测试、两套 Clippy、QiWe
326 项、Person/Agent 150 项和欢迎 24 项集成通过。末段
`operations-control-plane-apply-smoke.sh`
再次因本任务 32777 数据库 URL 哈希不在固定图像 staging 白名单内退出 1；后续
`JSONDecodeError` 是缺少 JSON 输出的连带错误。总门禁未通过；未修改 CI 或白名单。

## 指定浏览器验收状态

针对 `ef31256893e650bbb183207462d0043f03fa6451`，2026-09-27 通过 Codex 浏览器控制
`cua.getState()` 查询会话，返回
`Codex auth token is unavailable`；原生应用侧还报告 Mac 已锁定且无法自动解锁。因此没有取得可操作的 Codex
Chrome 会话，未按指定工具补验组织二级入口、首次空配置、有权但无岗位入口、范围隔离、预览保存、撤权清表单、`can_disable`
的各授权范围、焦点与键盘及窄屏实际表现。

上文 Playwright 结果仅为本机辅助记录，不能作为指定 Codex
Chrome 浏览器验收通过。待 Codex 浏览器鉴权与桌面解锁后，需在同一模拟版本逐项补验；未执行真实渠道或生产操作。

同一 Codex 浏览器入口再次执行 `cua.getState()`，仍返回
`Codex auth token is unavailable`，原生应用侧仍报告 Mac 已锁定；不再重试或改用其他浏览器工具代替指定验收。

## 5432 误触追溯

只读 `lsof`/`ps` 确认 `127.0.0.1:5432` 由 PID 47613 的本机 PostgreSQL
17.11 监听，数据目录为根工作区
`.local-workspace/foundation-batch1/pr720-reconcile-postgres`，归编号 720 的 PR 基础线，不是本任务 32777 容器。runner 未取得
`QINTOPIA_SIDECAR_DATABASE_URL` 时默认连接
`postgres://postgres:postgres@127.0.0.1:5432/qintopia_test`；仅设置协作测试 URL 不覆盖这一值。

原实例服务器日志 `.local-workspace/foundation-batch1/pr720-reconcile-postgres.log`
在 2026-09-27 12:53:20
CST 记录 Space 业务版本外键拒绝；基础线只读审计确认该结构符合测试预期的跨空间拒绝，不证明库损坏。

12:53:20.759 另有客户端在开放事务中断开。误触轮客户端输出未完整保留，不能还原所有精确 SQL 和事务终态。

runner 顺序表明该轮前段会执行独立 PostgreSQL 用例，然后到 Space 配置测试；源码中这些用例可运行
`db::run_migrations`，Space 用例还会无 tenant 条件地更新共享
`qintopia_agent_os.capabilities`，并以随机后缀在 `qintopia`
空间写模拟数据。故不能声称只用了独立 tenant/schema。

基础线的受限只读审计以时间和固定测试前缀识别出 2 个 `qintopia`
模拟空间、16 个 WorkItem、30 条事件等 cohort 残留。当前迁移表共 45 条，9 月 27 日没有新安装记录；共享能力项
`erhua.qiwe_text_template` 的更新时间为 12:53:20
CST，但事故前值未知，不能把当前启用状态全部归因于本轮。现有 PG16 及其他模拟实例备份都不是该 5432 实例事故前的恢复点。原客户端日志与聚合核查不足以证明全部跨表影响或精确回滚。

本任务没有再连接、写入、清理或停止 5432。`docs/testing/agent-guide.md`
已要求两个数据库 URL 均显式指向所持有的一次性库；本轮后续只使用本任务 32777。总指挥已将该实例与日志保留为编号 720 的 PR 历史证据，不再用于新验收，不删记录、不恢复、不停服。详细聚合证据保存在基础线 Git 忽略目录的
`pr720-20260927-5432-readonly-audit.md`，未复制原始人员资料或凭据到仓库。

## 审查与重复登记修复

`ef312568` 的 PR-Agent Reviewer
Guide 为 partial；`business_config.rs`、工作台和若干身份入口未覆盖，持久评论中的 9 条 finding 仍标
`ACTIVE`，不能当作全面批准。人工核读当前实现后：

- `43c77a2c3abc`/`9041993a8aca`：业务入口在同一事务先调用 `self.verify`，再使用
  `operation_for_actor_in`；旧 Actor 的来源、网关或账号漂移由前者拒绝。负例覆盖撤权及版本漂移。
- `518b07e5d1df`/`68772f4ede8f`：旧 Person 动作接续已按原主体类型拒绝；配置账号、绑定、候选及可见 grant 先限定当前范围或能力，再执行上限。旧动作和 257 条跨范围数据负例覆盖。
- `95d272ba963c`/`a5a46ba795aa`：提醒暂缓要求专用 `pms.reminder.snooze`
  授权；账号登记和解析限制 `shared` 网关，普通 `employee` 来源负例覆盖。
- `c141a9aadc7c`/`c2778cab7093`/`e5d92c59fd80`：流水旧动作、消息 evidence 及 work
  item 均比较账号版本。按既定恢复契约，旧版本冲突明确拒绝而非让新账号换键重放；相应版本负例覆盖。

导航和管理元数据人工核读：`/api/business` 先核验 Actor，再以现行 `Policy::manager` 派生
`manageable_scopes`；无岗位但有管理 grant 的范围仍可显示入口，只有业务角色或执行 grant 的账号没有入口。工作台按选中范围过滤绑定、账号和候选，保存后回读，撤权后清除旧范围与表单；`can_disable`
按全部未撤销授权计算且停用命令独立重验。以上是源码和集成测试核读，指定浏览器的实际交互仍未验收。

新发现：只读管理者可直接对在用账号重复
`RegisterAccount`，旧实现会增加版本并撤销其无权管理的办理 grant。隔离库负例先得到错误的成功返回，再加原子 upsert 条件拒绝在用账号；停用账号如有残留未撤销 grant 也拒绝重登记，登记不再代为撤权。原
`operation_id` 幂等返回、正常新登记和有权停用后重登记保留。

最终修订的定向红转绿、残留 grant 和网关漂移路径通过；完整 `person_collaboration`
串行 150/150、全目标全 feature Clippy、Markdown lint 通过。本地 `pnpm check:pr:auto`
的 Light、Rust、QiWe 326 项、Person/Agent 150 项与欢迎 24 项通过，末段 apply
smoke 仍因 32777
URL 哈希不在固定白名单退出 1。总门禁未通过；没有更改 CI、白名单或借用 5432。
