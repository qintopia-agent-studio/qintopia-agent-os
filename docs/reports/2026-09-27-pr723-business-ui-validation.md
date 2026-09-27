# PR #723 物业业务授权入口与本地验证

日期：2026-09-27。范围：岸岸工作账号授权工作台的组织关系二级入口与管理范围元数据。仅本地模拟数据；未接入真实企微或 PMS，未发布或部署。

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
