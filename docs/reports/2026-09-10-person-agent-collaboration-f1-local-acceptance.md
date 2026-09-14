# 人员与智能体协作 F1 本地验收

用户后续明确否定了该版界面的信息组织与职责表达。以下记录是当时的技术验证，不代表界面业务验收通过。后续修正见[界面纠偏验证](2026-09-10-person-collaboration-ui-correction.md)。

日期：2026-09-10。用户已允许恢复开发；本轮完成通用关系与授权的合成本地闭环。未接生产登录或旧训练/发送入口，未上线。没有提交、部署、真实人员导入或外部发送。

## 实现与文件

- `runtime/sidecar/src/person_collaboration/`：类型化命令、范围/管理授权链、持久服务、合成本地 HTTP 和配置页，含模块 README 和测试 fixtures。
- `runtime/postgres/migrations/202609100001_person_agent_collaboration.sql`：追加 tenant、范围/群绑定、岗位、任职、合作、授权及幂等命令表；Person、来源身份、会话与审计沿用已有结构。
- `runtime/sidecar/src/local_http.rs`：从已有欢迎本地服务提取共用的有界 HTTP 解析/响应，欢迎服务继续沿用原路由和 cookie 名。
- sidecar
  `main.rs`、`config.rs`、README：新增明确隔离的本地 CLI，不修改生产命令默认行为。
- 通用数据设计、数据设计索引/变更记录、共同契约 §5.3 和界面草图：同步恢复开发与 F1/后续能力边界。

UI 已验证新建关系、预览无落库、保存、刷新持久保留、修改职责与增加权限、撤销单项权限。页面使用合成人员搜索选择，展示人员—岗位—Agent—范围关系，不要求手填数字 ID。人人可提供信息的规则不变，不把普通接待做成岗位权限名单。

任期、代理和权限链均以服务端当前事实判断；管理权与执行权分开。组织管理、技术支持、本栋业务使用同一模型。上级授权或任职失效时，依赖它的授权不再有效；重新任职不恢复旧授权链。

## 验证证据

| 检查                                                     | 结果                                                                                     |
| -------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| sidecar 全特性回归                                       | 717 passed / 40 ignored；ignored 仍需各自显式测试环境                                    |
| 最终通用框架目标测试（含显式 Postgres）                  | 8 passed，涵盖 5 项纯逻辑与 3 组数据库/HTTP 场景                                         |
| 既有欢迎 Postgres 回归                                   | 2 passed，覆盖身份、Inbox、恢复、去重及作用域                                            |
| Clippy all-targets + all-features                        | 通过，警告视为错误                                                                       |
| Clippy all-targets + no-default-features                 | 通过，警告视为错误                                                                       |
| Rustfmt、页面 JavaScript 语法、secret/runtime-state 检查 | 通过                                                                                     |
| Anti-drift、runtime contract、registry 检查              | 用同版本隔离依赖执行原检查脚本，通过                                                     |
| 浏览器                                                   | 新建→保存→刷新→调整→撤销通过；重启后数据保留；无页面 JavaScript 错误                     |
| 响应式与标签                                             | 实测 CSS viewport 1600 和 487 像素均无水平溢出，未标注的输入项为 0；临时 viewport 已恢复 |

数据库仅使用已有本任务容器 `agentos-welcome-v1-test`，镜像
`pgvector/pgvector:pg18`，端口仅 loopback 55439，数据库为
`qintopia_test`。没有连接其他 PMS 或业务测试容器。fixture 使用随机且隔离的 tenant/namespace 和虚构人员，数据库迁移重放与重复初始化拒绝均已验证。

重点场景包括同名不同人、同人多关系、跨租户/跨栋/跨 Agent 拒绝、委派边界和子范围边界、代理到期、身份撤销/重绑、离任后复任、责任人冲突、同键异内容、并发版本冲突、审计仅一次、进程重连和 HTTP 会话/Origin/伪造操作者拒绝。

## 检查工具问题与处理

原仓库 `node_modules` 在 Node 24 中出现 `ERR_INVALID_PACKAGE_CONFIG`，以及 YAML `Parser`
/ `Lexer`
构造器加载异常。对应文件可读，但部分依赖读取迟滞；未确定底层文件系统或模块加载根因，不归因于本轮业务代码。

在 `/private/tmp/person-collaboration-checks`
安装与仓库版本一致的 YAML、AJV、Markdownlint 检查依赖，关闭安装脚本。临时 Node
loader 只将原脚本的 YAML/AJV
import 指向隔离副本，源码、检查规则和工作目录不变。未改仓库锁文件或 `node_modules`。

`pnpm check:pr:quick` 仍停在全仓格式检查，原因是本轮之前已存在的三份文件：

- `docs/plans/active/xiaoman-minimal-activity-reminder-workflow.md`
- `研究学习/秦托邦-AgentOS-协同架构与身份基础设施复习笔记.md`
- `研究学习/秦托邦-AgentOS-系统与数据关系图.md`

保留这些无关改动，不能将整个项目聚合检查记为通过。相关 Rust、隔离数据库和专项检查分别执行，上表不是全部生产验收。

## PMS 与后续能力

F1 未更改 PMS
DTO、事件或补偿查询契约，未修改 PMS 代码。沿用上一轮 PMS 回传兼容结论，未重新进行双程序联调；其余原待联调项仍在唯一共同契约和 PMS
handoff 中。

下一片为合作启动引导和知识待确认闭环。生产可信登录、正式管理员初始化、真实 runtime 版本核对、旧训练/检索/审核/发送入口接入、卡片渲染与附件适配、PMS 双程序联调和切换验收仍未完成。F1 权限查询不能被当成已接通旧工具的执行许可。

后续切换必须使各真实入口使用同一当前授权，不允许 UI 撤权后旧白名单继续放行；未知发送回执先核对。代码存在、合成本地通过与生产已启用严格分开。

## 本地交付与恢复

本轮修改文档的定向 Markdown/Prettier、文档链接和 `git diff --check`
检查通过。Markdown 界面草图的独立粗体行作为模拟 UI 标签，局部豁免 MD036，其他规则保留。

本地页面保留在
`http://127.0.0.1:18871/`，只展示合成状态，便于下一轮查看。启动方式与边界见[模块 README](../../runtime/sidecar/src/person_collaboration/README.md)。停止本地进程即可关闭入口；保留合成库和审计，不删除或重建其他任务数据。

原有欢迎代码、AGENTS.md、小满计划及研究笔记改动均保留。本轮没有生产回滚动作；未来回滚不得复活已撤权限或重发未知结果。
