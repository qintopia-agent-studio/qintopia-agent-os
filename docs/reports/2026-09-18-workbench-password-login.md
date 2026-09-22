# 工作台用户名密码本地验收

日期：2026-09-18。范围：人员协作模块的首期内部账号登录。状态：本地实现与本模块验收完成，仓库总检查仍有既有阻断，不构成生产启用、发布或外部发送授权。

## 基线与实现

已 fetch 核对 `origin/master=0806c2e`，PR #704 的人员工作台已在主线。当前目录切换至
`codex/workbench-password-login`，未创建 worktree。原有 4 份历史报告的状态说明及模块 README 的已确认方案均保留；原始差异另存本地。

追加 `202609180001_workbench_accounts.sql`，未修改已应用迁移或 SQLx
checksum。账号绑定已有 Person；Argon2id 密码、服务端不透明随机会话、持久限流、精确 Origin 检查、密码生命周期失效和当前业务授权重验均已接入。账号管理必须获得根范围
`default / organization / identity`
管理授权。固定合成操作者 HTTP 适配仅编译进历史测试，实际启动入口必须登录。

只使用新建 `agentos-workbench-login-20260918` 合成 PostgreSQL（loopback
55448），页面端口 18876。首次管理员通过 CLI 核验已有授权后初始化；口令随机生成并保存在忽略目录的 0600 文件，不写入 Git、日志或报告。历史 55439 数据库未修改。

## 实际验证

- `cargo check`、`cargo fmt --check` 通过。
- 默认 Rust 测试 850 passed / 2 ignored；全功能 865 passed / 66
  ignored。ignored 保留各自外部、平台或数据库门禁，不宣称全部已执行。
- `pnpm check:runtime` 通过：QiWe 326 测试中 1 项 Linux peer
  credentials 平台跳过；Sidecar 默认测试、格式、编译、operations
  control-plane 与 Xiaoman activity smoke 通过。
- 默认与全功能 `cargo clippy --all-targets -- -D warnings` 通过。
- 仓库测试框架 `person-collaboration`
  16 场景通过，0 失败、0 跳过。运行 ID：`20260918013212-d7012661f5`，Allure
  HTML 报告已生成。
- 新增 5 场景：正确/错误密码、未登录、伪造身份字段、CSRF、退出；开通不赋权、越权账号管理、改密、重置、停用；跨楼栋与任职撤销；持久限流、过期与身份变更；账号管理独立授权及其撤销。拒绝请求同时核对账号、配置版本、命令及授权快照未改变。
- 实际浏览器验证：1440×900 桌面、390×844 窄屏；正确/错误密码、退出、组织关系、配置任职与范围、基础台账及个人账号表单均已检查，页面无横向溢出。
- 一栋舍长只见一栋工作，配置入口拒绝编辑，账号管理不显示。未展示的上级岗位和触达配置明确标注权限边界，不伪称不存在。
- 改密/重置/停用的写入与失效由真实 HTTP/数据库测试证明；浏览器检查表单和导航。
- 测试框架自身的 `pnpm test:harness` 8 项通过。
- `pnpm format:check`、`pnpm lint:md`、`pnpm collaboration:check`、`pnpm pr:doctor`
  通过。PR body 在本地准备与验证；未创建、推送或合并 PR。

## 环境问题与处理

起初 PATH 无 Rust，安装官方 1.96.0 到忽略的本地工作目录，未更改全局 PATH。Docker
daemon 停止，启动既有 Colima 后新建独立合成容器。默认 pnpm
11 的依赖预检查失败，改用仓库固定的 pnpm
10.29.2。首次 16 场景虽然通过，但报告生成被 pnpm/Java
PATH 阻断，运行整体标为 broken，不将该次误报为完全通过。配置现有 OpenJDK
17 路径后重跑，得到上方通过记录。RTK 当前不可用，执行原生工具并保留日志。

首次 `pnpm check:pr:auto`
在新增 README 的 Markdown 长行检查失败，拆分段落后通过。后续总检查在既有
`tools/deploy/test-collect-release-deploy-results.mjs:117` 失败：找不到临时
`results.json`。根因是收集脚本末尾将 `process.argv[1]` 与 URL 的 `pathname`
直接比较；当前目录含空格，URL 含
`%20`，导致 CLI 主函数未执行。本轮未修改无关部署工具，故 `check:pr:auto`
仍为失败，需该工具后续改用 `fileURLToPath(import.meta.url)`
并保留带空格路径的回归。部署工具维护者负责跟进。

`check:runtime` 起初使用系统 Python
3.9，QiWe 的类型联合与事件循环测试报错。显式使用已有 Python
3.12 后整项通过，未修改 QiWe 源码或断言。默认 Clippy 曾发现旧合成 HTTP 适配仅被数据库测试使用；已收紧为
`cfg(all(test, feature = "postgres-integration-tests"))`，复验通过。上述均为本地检查；未部署、执行远端 CI 或连接生产。

本机工具路径存于忽略文件
`.local-workspace/workbench-login/env.sh`，需要重跑时先在仓库根执行
`source .local-workspace/workbench-login/env.sh`。主入口 `http://127.0.0.1:18876/`
已运行；重启执行
`bash runtime/sidecar/src/person_collaboration/run-login-local.sh`，不重复初始化。账号为
`demo-owner` 与 `demo-steward`，随机口令分别保存在忽略的 `demo-credentials.json` 与
`steward-credentials.json`，不设默认密码。本轮日志、界面截图和本地 PR body 均在
`.local-workspace/workbench-login/`；业务测试报告在
`.local-testing/runs/20260918013212-d7012661f5/allure-report/`。

## 边界与后续

生产 HTTPS、Secure
cookie、真实人员、生产初始化与发布运维需单独评审和验收。本次不连接生产库、不改真实人员权限，不部署、不合并 PR、不发送真实消息。欢迎执行和二花工具仍有各自旧授权入口，本次不声称已统一改造或即时撤权。停止本地进程即可关闭入口；保留追加表、旧授权历史与审计，不反向删除迁移。
