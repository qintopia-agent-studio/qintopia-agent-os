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
