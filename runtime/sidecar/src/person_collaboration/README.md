# 人员与智能体协作 F1

Owner：Agent OS /
sidecar。风险级别：high（身份和授权）。本目录是现有 sidecar 包的子模块，沿用
`runtime/sidecar/manifest.yaml`；不是新的业务 workflow 或 Agent。当前只提供合成环境入口，无生产登录、runtime 接入或外部执行能力。

## 用户名密码本地入口（2026-09-18）

本轮已实现个人账号登录、退出、修改密码，以及有权管理员为已有 Person 开通、重置和停用账号。入口只使用合成人员与隔离 PostgreSQL；不是生产启用声明。

可信会话在服务端绑定稳定 person_id 和已核验来源身份。密码为 Argon2id
PHC，数据库仅存会话摘要；8 小时绝对到期，所有 POST 严格校验 Origin 和 JSON。每个请求及业务事务重验账号、会话、身份和现行授权。

密码重置、修改、账号停用撤销全部旧会话；撤销任职或授权后，无需退出即可收回后续访问和操作权。登录按用户名及整个本地入口限流，15 分钟分别最多 5
/
60 次尝试；改密最多 5 次。日志与审计不存密码、会话值或哈希。密码会话不会重放历史配置返回载荷，重复保存须重新读取当前状态核对，原提交不会重复执行。

账号管理单独要求根范围 `default / organization / identity`
的有效管理授权，组织台账权与登录本身都不够。开通账号不增加业务权限，不新建 Person。无公开注册、居民入口、微信、企业微信、短信找回或对外发送。

独立页面：`http://127.0.0.1:18876/`。沿用组织关系 / 配置任职与范围 / 基础台账；右上角“个人账号”提供改密与有权管理员的账号管理。普通任职只返回自己或授权范围内的连接，配置操作仍逐项检查管理权。空任职账号可登录、改密、退出，无默认业务权。

首次创建本任务专用空合成库并初始化（不要替换为业务库）：

```bash
docker run -d --name agentos-workbench-login-20260918 \
  -e POSTGRES_DB=qintopia_test -e POSTGRES_HOST_AUTH_METHOD=trust \
  -p 127.0.0.1:55448:5432 pgvector/pgvector:pg18
cargo build --locked --manifest-path runtime/sidecar/Cargo.toml
bash runtime/sidecar/src/person_collaboration/run-login-local.sh --init-fixture
```

容器或 tenant 已存在时不可重复初始化；后续启动去掉
`--init-fixture`。该端口仅 loopback，本地数据库不使用真实凭据。历史 `55439` 库保持原样。

首次管理员通过进程所有者 CLI 开通，无 HTTP 初始化接口。先从合成 fixture 的明确
`source_ref=fixture-person-0`
和本任务 tenant 查询已有 person_id，不按姓名匹配。CLI 仅允许空账号 tenant 中已具有组织身份管理授权的 Person，不创建任何授权。以下
`PERSON_UUID` 替换为核对后的 UUID；口令由标准输入传递，不放在命令参数或 Git：

```bash
export QINTOPIA_COLLABORATION_LOCAL_ENABLE=1
export QINTOPIA_COLLABORATION_LOCAL_DATABASE_URL=postgres://postgres@127.0.0.1:55448/qintopia_test
export QINTOPIA_COLLABORATION_LOCAL_TENANT=synthetic-collaboration-login-20260918
python3 -c 'import getpass,subprocess,sys; subprocess.run(sys.argv[1:],input=getpass.getpass("初始密码（12–128 字符）: ")+"\n",text=True,check=True)' \
  runtime/sidecar/target/debug/qintopia-message-sidecar \
  bootstrap-collaboration-account --person PERSON_UUID --username demo-owner
```

下述旧固定操作者说明保留为历史合成验收记录。当前 `run-collaboration-local`
一律走密码认证，`LOCAL_OPERATOR_LINK`
不再进入 HTTP 身份链；原合成 HTTP 适配只编译进测试。旧数据库使用前须正常追加迁移，不能改 SQLx
checksum。

设计与回滚：[账号数据设计](../../../postgres/docs/data-design/2026-09-18-workbench-accounts.md)。验证：[本轮登录验收记录](../../../../docs/reports/2026-09-18-workbench-password-login.md)。

## 已实现

### 当前集成状态（2026-09-18 核对）

本模块及本地欢迎流程已于 2026-09-15 通过
[PR #704](https://github.com/qintopia-agent-studio/qintopia-agent-os/pull/704) 合并到
`master`（合并提交
`171e227`）。后续从主线创建任务分支，不再依赖旧工作台分支或临时目录。历史验收报告中的分支、未提交和暂停合并描述仅代表报告当时状态。

人员、组织和权限不是一张表：`qintopia_identity.persons`
保存人员，`source_identity_links` 关联来源身份；`collaboration_positions`
保存组织岗位及上下级，`collaboration_roles` 和 `collaboration_duties`
定义岗位与职责，`collaboration_scopes` 定义范围， `collaboration_appointments`
关联人员、岗位、范围和任期，`agent_collaborations`
关联任职与智能体职责，`collaboration_grants`
保存具体动作、授权来源及确认条件。这些结构已有 SQL 迁移和真实数据库读写代码，历史合成数据库验收不等于生产数据库已迁移。

统一接入尚未完成：`Store::local` 和事务入口仍要求 synthetic 环境；欢迎执行路径仍读取
`welcome_grants`，不能宣称已全部消费通用
`collaboration_grants`。真实身份接入、业务执行前统一授权重验及生产数据库状态仍需分别验证。本次只核对源码和文档，没有重跑数据库测试或查询生产库。

- 复用 Person 与来源身份；会话只从当前合成 tenant 的已有清单选择。姓名搜索与具体人员选择分开，不用显示名作唯一键。
- 岗位、职责、范围分别维护。小管家、技术负责人等预置岗位可编辑、关联职责、停用；现有定义保留历史，只有明确未使用的台账或组织岗位草稿可删除。群绑定由组织管理授权控制。
- 管理权按可授予的 Agent、领域、动作和范围判断，与本人执行权分开。技术岗位不自动拥有业务权限。
- 每条连接明确人员、岗位、职责、智能体和工作范围。可调整、换人或结束单条连接；替换以同一事务结束旧连接、创建新连接并保留历史，不影响其他工作。最后一条连接结束时关闭其空任职。
- 职责是可复用的业务定义；权限逐项设为可自主决定、需指定人员确认、未授予。确认人须为其他自然人，在同职责、智能体和范围拥有该项有效自主权限；到期、撤权或身份失效后不放行。
- 配置版本冲突拒绝覆盖；同一命令重复保存返回原结果。成功变更与审计原子提交；预览事务回滚，不产生持久任务。
- 有效权限和负责人查询检查任期、身份、范围和全部管理授权来源。负责人不唯一时返回 conflict，不按姓名猜选。
- 本地 UI 提供搜索选择、关系总览、变更预览及真实持久保存；明确标记真实入口尚未接入。
- 主入口为“组织关系 / 配置任职与范围 / 基础台账”。组织岗位连接上级、岗位定义与范围；岗位可空缺。台账分类维护人员、智能体、群、岗位、职责和范围；任职配置与定义维护分开。
- 人员台账引用现有 Person，可搜索同名候选或登记待核验人员；待核验记录不创建已确认渠道身份，不可任命。智能体登记与 runtime 创建、运行接入状态分开；群从当前可访问清单选择，不创建或解散外部群。
- 停用人员、智能体、组织岗位先做事务影响预览，再原子结束相关连接；结束任职即时收权。恢复台账不恢复旧任职、授权、联系设置、群绑定或任务。组织岗位有下级时，须先调整其归属。
- 每条连接分别设置具体群、具体人、本范围在住/过往/全部动态选择器，回复、主动联系、确认人与信息可见性。公开咨询可以接待未登记的人，仅限公开信息；动态人员实际解析留给 PMS 消费链路。
- 联系配置保存其来源管理授权，执行查询重验当前授权链、确认人、目标绑定与台账状态。主动联系还需要 publish 授权，选择“全部”不意味着全通讯录或任意群发。
- 新台账、组织岗位及联系配置在命令历史保留前后快照；兼容旧记录，不编造旧字段快照。
- 权限提供解释、例子与边界；岗位关联职责限制可选权限。修改岗位或职责方案不自动增加存量授权；移除正在使用的职责或权限类别会拒绝并提示先处理连接。
- `/api/decision`
  解释某条连接的当前权限；它不生成批准或发送凭据。选择确认人仅配置确认条件，不代表该人已批准任何内容。
- `/api/contact-decision`
  仅解释当前配置。公开咨询、私聊对象和主动联系分别判断，不提供可重放的发送票据。

普通信息提供是开放接待的一部分，不需要授予岗位权限，页面没有“谁才可以提供信息”的名单。F1 尚未实现聊天接待或信息采集入口，也不把一项查询结果当可重放的发送许可。

## 本地启动

### 已确认的首期登录方案（实施前记录）

本节保留原始已确认范围与当时状态；本轮实现及验证见上方新入口和验收记录。

用户已选择用户名密码登录。首期仅面向舍长和内部管理人员，不涉及普通居民、公开注册或微信/企业微信扫码接入。

- 管理员为已有 Person 开通个人账号，账号绑定稳定的
  `person_id`；不另建人员库，不按昵称自动关联。开账号不自动授予业务权限。
- 登录后由后端根据当前有效身份、任职、职责、范围与授权决定可见内容和可执行动作；前端隐藏入口不能替代后端检查。
- 使用成熟密码哈希实现，禁止明文存储；具备登录限流、安全会话、退出和管理员重置密码能力。重置密码或停用账号使旧会话失效；业务撤权须在后续访问及执行时生效。
- 不建设短信找回、多平台账号中心或居民自助注册。以后若增加微信，仅增加账号认证入口，复用同一 Person 与业务授权。
- 实现需将可信账号会话接入现有授权服务，不能把当前固定合成操作者直接作为真实用户，也不能简单移除 synthetic 门禁。

上述是已确认的实施范围，不是功能完成声明。当前仍使用下述本地合成入口；后续必须验证错误密码、未登录、跨范围访问、账号停用、密码重置及任职撤销后的拒绝行为，并证明拒绝操作没有写入副作用。

确认稿工作台独立体验入口：`http://127.0.0.1:18875/`。固定深色主题，横向导航为“组织关系 / 配置任职与范围 / 基础台账”。实现已集成到主线；下方原 A 入口说明保留用于历史合成场景复验，不要求恢复旧分支。

本轮首次初始化已完成，后续从本工作区启动：

```bash
cargo build --manifest-path runtime/sidecar/Cargo.toml
bash runtime/sidecar/src/person_collaboration/run-local.sh
```

辅助脚本固定端口 `18875`、既有 loopback 合成库与独立
`synthetic-collaboration-confirmed-20260911`
tenant，且清除其他身份覆盖。仅首次初始化空 tenant 时才显式传
`--init-fixture`；已有数据不能重复初始化。

任职配置通过 `configure_work` 将一项协作的任职、三态权限和触达保存为一个事务。已有
`assign`、`set_audience`
等兼容接口保留。触达校验失败会回滚换任与撤权；预览回滚、版本锁、幂等保存和前后审计仍由同一 Store 执行，无新增迁移。同一岗位多项协作分别选择，整项离任通过“结束整项任职”即时收回全部关联权限。

台账主入口为智能体、群、人员、岗位四类。岗位定义、职责和范围在岗位详情中维护，群绑定在群详情中维护。新人员保持待核验，恢复台账不恢复旧授权。

本轮证据见[确认稿工作台验收记录](../../../../docs/reports/2026-09-11-organization-agent-workbench-confirmed.md)。

前提：Rust 1.96、已安装依赖、明确隔离的 `qintopia_test`。本任务验证使用现有
`agentos-welcome-v1-test` 合成容器，端口仅
`127.0.0.1:55439`。不要换成其他业务库、生产镜像数据或生产连接。

```bash
QINTOPIA_COLLABORATION_LOCAL_ENABLE=1 \
QINTOPIA_COLLABORATION_LOCAL_DATABASE_URL=postgres://postgres@127.0.0.1:55439/qintopia_test \
cargo run --manifest-path runtime/sidecar/Cargo.toml -- run-collaboration-local --init-fixture
```

`--init-fixture`
明确执行本地版本化迁移并初始化虚构资料，只能成功一次；重复初始化拒绝且不覆盖数据。之后启动去掉此参数。打开
`http://127.0.0.1:18871/`。本地进程固定合成操作者，会话绑定身份版本，浏览器 JSON 不能提供或切换操作者。

A 独立体验实例使用端口 `18873`，tenant 为 `synthetic-collaboration-a-20260911`。通过
`QINTOPIA_COLLABORATION_LOCAL_TENANT` 显式选择合成命名空间，仍必须是
`synthetic-collaboration-` 前缀。首次 `--init-fixture`
生成秦托邦岗位结构、社区服务管理委托和两栋独立二花连接，全部为虚构验收案例，不是真实任命。已有 tenant 不重复初始化。服务进程可通过
`QINTOPIA_COLLABORATION_LOCAL_OPERATOR_LINK`
固定另一已核验合成身份进行越权验收；它不绕过身份、管理授权或命名空间检查，也没有 HTTP 切换操作者入口。不能作为生产登录方案。

进程只绑定 loopback；请求验证 Host、会话 cookie、Origin、JSON 类型和长度，拒绝重复 HTTP 头与编码请求体。没有外部客户端、真实制卡/上传/发送或 PMS 写入口。端口和合成数据库地址必须显式配置；没有生产环境变量回退。

## 验证

推荐通过仓库业务测试框架启动独立、随机端口的 Docker PostgreSQL：

```bash
pnpm test:setup
RUSTUP_TOOLCHAIN=1.96.0 pnpm test:business -- --feature person-collaboration
```

清单逐项执行数据库事务与 HTTP 场景，保留报告并清理本次数据库；不复用业务库。CI
PostgreSQL job 和 `pnpm check:pr:postgres`
也显式运行本模块的 ignored 数据库测试。测试专用适配器接受运行器注入的连接及唯一
`sslmode=disable`
参数，生产和本地工作台的数据库门禁不变。以下原生入口仍可用于手动建立的隔离
`qintopia_test`；端口可调整，不再绑定原作者机器。

```bash
RUST_MIN_STACK=33554432 cargo test --manifest-path runtime/sidecar/Cargo.toml person_collaboration

QINTOPIA_COLLABORATION_TEST_ENABLE=1 \
QINTOPIA_COLLABORATION_TEST_DATABASE_URL=postgres://postgres@127.0.0.1:55439/qintopia_test \
RUST_MIN_STACK=33554432 cargo test --manifest-path runtime/sidecar/Cargo.toml \
  --features postgres-integration-tests person_collaboration -- --include-ignored
```

`tests.rs` 与 `duty_store_tests.rs`
包含合成 fixtures、权限纯逻辑测试和显式隔离库测试，不需要真实资料。除预览、幂等、并发、审计、重连、身份和授权链外，覆盖三态权限、确认人撤权到期、岗位职责引用、模板不自动赋权和单连接替换。更广回归按根和 sidecar
`AGENTS.md` 执行。

A 设计与验收入口：
[数据设计](../../../postgres/docs/data-design/2026-09-11-organization-person-workbench.md)、
[本地验收记录](../../../../docs/reports/2026-09-11-organization-person-workbench-a-local-acceptance.md)。

## 后续接入与回滚

F1 未接真实 UI 登录、自然语言入口、合作启动 WorkItem、知识审核或旧训练/发送工具。初始化真实管理员必须另有可信登录与受控 runbook；不能沿用这里的合成操作者方式。Agent
registry 证明包已登记，不证明真实 Hermes 版本或工具已可运行；客房 Agent 不在清单时不可凭名称启用。

F2 实现引导和知识确认；F3 让实际工具入口在最终执行事务中重新验证当前权限、范围绑定版本和实际渠道上限。完整交接中的开放任务转交、旧审批失效与知识衍生处理须在这些消费者接入后验收。当前结束任职只影响本模块的当前授权判断，不能声称旧生产工具已同步撤权。

本地停止进程即可关闭入口，保留测试库及审计可复验。新增迁移仅加表及岗位职责列，无生产默认授权，不回滚删除历史。未来生产切换仍须单一授权权威、不可回退到宽松旧名单、未知发送先核对。

`.003`
不自动猜测旧关系的职责；旧无职责的关系显示待关联，业务权限判断拒绝。用户在本地明确选择职责后形成新连接，旧记录保留。首次测试管理员是受保护的初始化连接，不能从普通连接入口修改或撤销。

设计：[通用数据与入口衔接](../../../postgres/docs/data-design/2026-09-10-person-agent-collaboration-v1.md)。

PMS 只沿用[唯一共同契约](../../../../docs/plans/active/unified-person-welcome-v1-contract.md)，F1 未改接口。

## 未发布迁移的校正

PR #704 集成时将 `202609110001_organization_person_workbench.sql` 中的设计文档路径修正为
`docs/data-design/2026-09-11-organization-person-workbench.md`，以符合迁移登记规则。这会改变该未发布迁移的 SQLx
checksum。先前运行原始 PR 的合成测试数据库应新建隔离实例，不要修改 `_sqlx_migrations`
来绕过校验；本次没有在生产应用迁移。
