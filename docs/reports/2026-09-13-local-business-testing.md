# 本地业务测试接入与验收

## 已验证的业务结果

最终业务运行
`20260913053114-b34fd8dc4b`：16 个登记场景/测试组、319 个实际用例，全部通过、0 跳过，退出码 0。Allure 结果与机器摘要保存在该 run 目录，未加入版本库。

- 二花早报：正常、重复发送、未确认、响应断开和 cron 声明/wrapper 语法均通过。
- QiWe 5 组、Agent runner 1 组、MCP 契约 2 组、真实 PostgreSQL 集成 3 组均通过。
- `pnpm test:harness`：8/8，通过选择器、空测试、错误/跳过状态、环境隔离、超时、导入目录、报告附件和数据库归属拒绝检查。
- 子进程组超时探针：带 fork 子进程的命令在超时后有界返回，整个命令组被回收。
- 三次独立运行（包括失败运行）均记录 `downCode=0, volumeRemoved=true`；最终按 owner
  label 查询无残留容器/卷。
- Playwright 浏览器验证：Allure 中文总览、场景步骤及成功/失败附件可用；HTML 可解析。

## 接入过程中发现的问题

| 现象                           | 根因与修复                                                                                  |
| ------------------------------ | ------------------------------------------------------------------------------------------- |
| Docker 官方镜像下载 TLS 错误   | 本机网络路径故障，恢复官方镜像后继续；没有修改 daemon 或接入非官方 registry                 |
| migration 连接被关闭           | 初始化临时 socket 服务误判就绪，改为 TCP 实际查询健康检查                                   |
| QiWe 旧测试导入/fixture 找不到 | 旧入口依赖包目录，清单显式声明 pythonpath/cwd，保留原断言                                   |
| cron 新断言误判源文件权限      | 源文件按仓库规范为 0644，安装器赋予执行权限；本地检查文件与语法，安装权限由既有部署契约验证 |
| 初次格式与 Markdown 检查失败   | 修复格式与行长，重新执行检查，没有绕过 hooks                                                |

独立审查还补强了发送报告/持久化事件断言、Cargo 子进程组回收、Compose 清理失败判定和 bridge 显式 opt-in；复审未发现阻塞性正确性问题。

## 原生回归

`pnpm check:pr:auto`
已通过 light、QiWe 原生套件（303 条，1 条平台跳过）、Rust 默认测试（832 通过、2
ignored）、两个 smoke 及生产/预发布编译边界测试。随后 Clippy 在本机 Rust
1.98 报告了本次提取产生的多余引用，以及未修改文件中的新版 lint。多余引用已修正；剩余 Clippy/全 feature 测试使用 CI 固定的
`RUSTUP_TOOLCHAIN=1.96.0` 继续验证，不修改无关的
`conversation_ingress.rs`。最终结果待补充。

## PR 发布记录

首次创建 PR 时新 worktree 分支仍跟踪 `origin/master`，现有 `pr:create`
把“存在 upstream”误当成“同名远端分支已经存在”，未推送就调用 GitHub，返回 head
ref 不存在。通过 `git push -u origin HEAD`
创建同名远端分支并修正 upstream 后重试；未推送 master。后续创建隔离工作分支应使用
`--no-track`，并在 PR 前核对 upstream；无需修改测试框架。

## 生产边界与回退

没有部署、真实发送或生产数据库访问。生产发送入口仍保留原 gates、allowlist 和生产 HTTP
client；新 bridge 仅用于测试编译，默认 ignored，并要求本次本地数据库归属标记。如需回退此开发工具，可回退本 PR；没有 migration、数据修复或生产开关回退步骤。

当前不证明真实 Hermes 调度、跨 Agent MCP
transport、真实模型质量、图片/card 或用户送达。后续功能开发由对应协作者/Codex 按
`docs/testing/agent-guide.md` 搜索和补齐具体风险场景，不能通过增加清单标签来宣称覆盖。
