# PR #704 主线集成与验证

## 范围

将人员、组织、授权工作台和 resident welcome 本地基础 `6e6250ee` 与 master `adb29207`
集成，保留两个原始恢复提交。

原成果约 90 个文件，主要是七个数据库迁移、人员/组织/任职和触达授权、事件 Inbox、WorkItem、PMS 订单投影、欢迎发送恢复及本地工作台。本次接受的是合成环境基础；真实登录、渠道身份、Agent 消费和生产欢迎不在验收范围。

## 冲突与修复

- `AGENTS.md`：原 PR 全量精简与主线新增工程门禁冲突。恢复主线全文；
  [删减评估](2026-09-15-pr704-agents-review.md) 说明为何不直接提交原稿的独立规则 PR。
- Sidecar `main.rs`：同时保留主线 registry 测试模块与欢迎模块，保持条件编译归属。
- `operations.rs` 与 control-plane
  smoke：合并能力清单，保留 13 个主线能力并增加默认禁用、无允许调用方的欢迎能力，断言总数为 14。
- 数据设计 CHANGELOG：按日期保留双方全部设计登记记录。
- 群停用影响说明修正为仅撤销群范围绑定；任职、训练权限保留。新增回归证明群停用与 Agent 停用的差别、预览不写入以及恢复不复活历史授权。
- 新增仅测试编译的数据库入口，支持测试框架随机端口，显式要求测试开关并拒绝远端、错误库名及连接覆盖参数。生产和本地服务器的数据库门禁没有放宽。
- 11 个人员协作和 2 个欢迎 PostgreSQL 场景登记到业务测试清单，并加入 CI 和本地 PostgreSQL 检查，避免 ignored 测试未执行却报告全绿。
- 本地 PR 检查按配置的隔离数据库端口探测就绪，避免错误探测已有 5432 实例。

## 失败与处理

首次 `pnpm check:light` 在迁移反漂移检查失败：组织工作台迁移把 active
plan 写作设计文档路径。修正为包内
`docs/data-design/2026-09-11-organization-person-workbench.md` 后 `pnpm policy:check`
通过。这改变未发布迁移的 SQLx checksum；运行过原 PR 的合成数据库需要新建隔离实例，不修改
`_sqlx_migrations` 绕过校验。没有应用生产迁移。

首次 heavy 的普通 Rust 单测有 848 通过、2 失败：本地检查器把 PostgreSQL 专项的连接地址传进普通 CLI/fixture 单测，导致“无数据库配置”的断言失败，并错误查询未迁移数据库。

修复检查器，将普通测试环境与 PostgreSQL 专项环境隔离；两个失败用例定向重跑通过。

Chrome 插件能列出扩展，但会话命名及标签页操作报
`unsupported Codex auth method: apikey`。按用户要求只使用 Chrome 插件；没有完成本轮浏览器交互和溢出验收，不将早期页面读取或历史截图计为通过。后续需在插件认证可用的 Codex 会话完成本地工作台授权、群停用预览/确认和窄屏检查。

## 验证证据

- Rust 工具链固定为 1.96.0，Python 使用 Codex 随附运行时。
- `pnpm test:harness`：8/8 通过。
- `pnpm test:business -- --feature person-collaboration`：11/11 通过。
- `pnpm test:business`：29/29 场景/测试组通过，0 跳过；运行
  `20260915022305-b25a68e14d`，本地 Allure 报告已生成，隔离数据库由框架清理。
- `pnpm secrets:check`、`pnpm policy:check`：通过。
- `pnpm check:pr:heavy`：轻量检查、普通 Rust 850/850、编译边界、两组 Clippy、全功能 Rust
  865 通过（60 个专项 ignored）、原有 PostgreSQL 专项、新增人员协作 22/22 和欢迎 9/9 均通过。最后的 apply
  smoke 被既有数据库 URL 哈希白名单拒绝。
- 本地 apply smoke 限制：本次隔离数据库使用随机端口 32783，既有 Huabaosi
  staging 适配器只接受明确审核的 URL 哈希，不能据此放宽安全门禁。GitHub PostgreSQL
  job 使用已审核的标准 CI URL，继续由该 job 验证完整 apply smoke。
- `pnpm lint:md`、PR 相对 master 的 `git diff --check`、`pnpm pr:doctor`：通过。

## 回退与剩余边界

未合并、发布或部署；未访问服务器、发送真实消息或修改真实 PMS。生产欢迎继续默认关闭。必要时撤销集成修复提交；保留原恢复历史，不删除身份历史或反向修改生产 schema。浏览器验收及更新后 GitHub
CI/审查必须据实完成，未完成时保留 draft，不能宣称已可直接合并。

## 推送阻塞

本地合并修复提交 `241a2ea` 保留原 PR 和 master 两个父提交，工作区检查与完整
`.husky/pre-commit` 脚本通过。GitHub 拒绝 HTTPS 推送，原因是当前 OAuth App 缺少修改
`.github/workflows/ci.yml` 所需的 `workflow` scope。远端仍为
`6e6250ee`，不能宣称冲突已在远端解决，也没有新提交的 CI 结果。PR 描述已明确标注本地待推送状态。

负责人需通过 `gh auth refresh -h github.com -s workflow`
完成 GitHub 授权；之后重新核对远端分支，再正常推送本地 HEAD 到
`codex/org-person-workbench-a`。无需强推。独立新 PR 同样需要 workflow 权限，拆 PR 不能解除这一阻塞。

授权补齐后，已成功将 `ab513ce`
推送到原 PR 分支。上述推送阻塞已解除，远端 CI 与 Chrome 验收仍需分别确认。
