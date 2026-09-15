# 人员与智能体协作 F1

Owner：Agent OS /
sidecar。风险级别：high（身份和授权）。本目录是现有 sidecar 包的子模块，沿用
`runtime/sidecar/manifest.yaml`；不是新的业务 workflow 或 Agent。当前只提供合成环境入口，无生产登录、runtime 接入或外部执行能力。

## 已实现

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

确认稿工作台独立体验入口：`http://127.0.0.1:18875/`。固定深色主题，横向导航为“组织关系 / 配置任职与范围 / 基础台账”。工作台本轮实现位于独立分支
`codex/org-agent-workbench-confirmed`，继承 A 冻结后端；原 A 入口继续保留。

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
