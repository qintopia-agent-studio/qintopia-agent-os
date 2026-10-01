# 舍长—二花安装准备与 C1 实现验证（2026-10-01）

## 当前结论

负责人已具体批准复审后的 C1
**本地实现与验证**，包括提交、推送和范围清楚的实现 PR。六机制文件已实现；不再将本阶段写成“等待 C1”或“未实施”。

真实 Linux arm64/systemd 255 的实际 Rust broker、SDK 与隔离 PostgreSQL 已验证：

- 专用非 root 账户、root:root0600 env、0750 parent/0660
  socket；正常 context 与空闲关闭。
- 缺 FD9 返回 75；完整 unit 的重复 `User=root`、额外 `ExecStartPre` 均拒绝激活。
- 活 broker 阻止 installer 覆盖 unit；固定 R
  rollback 在第一处指针修改前拒绝，进程不被停止。
- helper metadata/digest 漂移拒绝； 关闭后固定 R 可回退到没有 helper 的旧 T。

  旧 T 模拟 installer 返回 55， 实际脚本保留可判定的 current/previous/rollback-from； 不是 T 安装成功。

- 同一实际 binary 在错误非 root UID 下运行时，quiesce 返回 75，原进程仍在。
- 真实 SQL 锁阻塞已接受的 SDK 请求，实际 runner
  **35.821 秒返回 75**。原 PID/starttime/InvocationID/cgroup 仍在，unit
  disabled/deactivating/Result=timeout。原 runner 的 `quiesce-management-ui`
  失败分支实际写入 root:root0600、request-bound hold。
- 30 秒 `foundation_broker_drain_deferred` 可见。SQL 自然解锁、客户端
  `outcome_unknown`、broker 自然退出后， hold 和全部指针仍不变， 未继续安装、未自动重放或使用新请求键。

  最终 unit 保留 failed/Result=timeout， verify-closed 仍返回 75； 没有 reset-failed/强杀/删除 socket。

**`installation_ready=false`。**
这些是模拟 release 的本地行为证据，不是正式同版制品、目标 x86_64
systemd、完整生产 ACL、管理页面/本人业务验收或 C2 上线许可。没有合并、Release 发布、Artifacts
dispatch、生产安装、证书签发、启用或真实业务发送。

## 变更、批准与边界

| 范围      | 实现                                                                                               |
| --------- | -------------------------------------------------------------------------------------------------- |
| 新 helper | `deploy/runner/foundation-broker-lifecycle.sh`：prepare/activate/quiesce/verify-closed             |
| renderer  | `deploy/sidecar/scripts/render-systemd-units.sh`：独立 broker unit，安装默认关闭                   |
| installer | `deploy/runner/install-release-systemd-units.sh`：覆盖任何 unit 前只 verify-closed                 |
| bundle    | `tools/deploy/build-deploy-bundle.mjs`：沿既有 payload/摘要合同打包 helper                         |
| UI 组合   | `deploy/runner/management-ui-lifecycle.sh`：固定 R、metadata/digest 校验后组合关闭                 |
| rollback  | `deploy/runner/rollback-release.sh`：从自身固定 R 取 helper，首次 rollback-from 前只 verify-closed |

专用账户为 `qintopia-foundation-broker`，非登录；主组为二花 runner 的
`ubuntu`，运行时核对 UID/GID，不固定模拟 UID。env 为
`/etc/qintopia/foundation-broker.env`；socket 为
`/run/qintopia-foundation-erhua/broker.sock`。

prepare 不覆盖已有 env， activate 独立授权。

prepare/activate 沿既有 FD8→9/hold 合同； 关闭只检查继承 FD9， 不争抢父锁。

停止只提交一次，使用同一个 monotonic
35 秒预算； 只终止等待的 systemctl 客户端， 不强杀业务进程。

快照核对专用 UID/GID、精确 argv、可执行文件和原 cgroup 身份； 关闭核对 Result/Invocation/journal、原 PID/starttime/cgroup、相关 broker 进程与 socket。

完整 unit 必须与已验摘要的固定 R
renderer 模板全文一致，且无 drop-in/daemon-reload 漂移。数据库角色名校验不代表完整最小 ACL 已验收，后者仍属于既有 runtime
preflight/上线门禁。

普通测试沿四个既有文件补用例；真实 C1 用例复用
`test-deploy-runner-systemd-linux.mjs --management-ui-maintenance-systemd-producer`， 显式提供已准备的可丢弃 Lima/PG 夹具。

没有改共享框架、checker、workflow、ownership、target、schema、全局超时、主 runner/recovery， 亦没有新增工具栈。

## 真实来源与失败记录

模拟 R 为 `1111111111111111111111111111111111111111`；T 为 `222…`。实际运行 Rust 为固定
`cc7cecd5afaa00037b970b45b002526642d368e2` 的 arm64 debug/all-feature binary：SHA256
`ab0ff62a34ceee77b348beb1bec5c73ae1c3fcbec78ace896095d6edb782286e`。SDK SHA256
`7531137d5e63874770adb8852204589cf258a30703cf4030d70d2adf2969f056`。

它们包含本次所需的跨 UID/drain 合同， 但不冒充最终集成源或 production
feature 制品。每次 VM 同步只改本地模拟 release，执行文件与源码 digest 对照记录在新证据包。

证据基目录为本实施工作树的忽略目录
`.local-workspace/steward-c1-implementation-20261001/evidence/`：

| 日志                               | 实际结论                                                                                               |
| ---------------------------------- | ------------------------------------------------------------------------------------------------------ |
| `bootstrap-real-systemd.log`       | 初次 env 正则拒绝 SHA256 数字键，prepare 返回 75；未产生 env/account                                   |
| `resume-real-systemd.log`          | 修正数字键后真实 prepare/activate/context、账户和文件权限通过                                          |
| `real-idle-close.log`              | 空闲组合关闭通过；末尾首次缺 FD9 为 1，期望 75 的断言失败；**整份不记全过**                            |
| `real-c1-fault-cases-initial.log`  | 前十项身份、unit、活 broker、rollback/installer 门禁通过；末尾恢复 activate 返回 75，保留失败          |
| `real-c1-inflight-resume.log`      | 查明并修正 journal 选择后恢复激活；真实在途 35 秒、原 runner hold、晚完成不清 hold 的三项通过          |
| `native-stop-actual-exit-code.log` | 原生 stop exit0/71 秒、Result=timeout、进程残留，随后自然退出；仅为缺陷依据，不替代新 helper 35 秒测试 |

后一个 activate 失败原因：daemon-reload 的 unit 警告也带
`UNIT`，原查询取到警告而不是停止 job。helper 现按该 unit 的最后 start/stop
job 核对，不让任意警告抹掉可证的成功停止，也不让旧 stop 越过较新的 start/失败。修复限于同一个已批准 helper；新恢复实测通过。

真实 fault 用例分阶段运行， 没有在持久 hold/未知结果上从头重跑， 亦不称单次全矩阵全绿。

初次 bootstrap 原计划的身份 JSON 未写出； 随后明确补录源码→VM 执行文件→binary/SDK 的摘要， 不将原始失败日志改写为成功。

全部早期日志保留。

最终只读对应记录为
`real-c1-final-identity-and-diagnostic.json`：五个执行脚本的源码/VM摘要完全一致。helper 最终摘要
`c6f5a18c64bd34270acc0bbbea94df6a532601b9635ede6c34d04d4979a6fff5`。将最终源码仅还原 journal 查询即精确得到初始摘要
`469097af…`，已验证不是其他机制差分。记录还包含原始
`RuntimeError:final journal event not successful stop`
工具回执和真实 reload 警告摘录，以及在途 InvocationID 的最后状态；补证只读，没有释放 hold 或重跑 35 秒。

## 当前二花 core 与工具边界

2026-10-01 21:46:45 +08:00 的生产**只读**核对： `hermes-gateway-erhua.service`
active，MainPID=1457，解释器 Python3.12.14 Linux
x86_64，入口为现有 core 的 venv/python，工作目录为二花 Profile。core 路径解析到
`v2026.9.21`，磁盘 HEAD 为
`d337b736aa1e8ebecfab043842d13e4a2d2f48a3`。这交叉确认运行入口与磁盘来源，**不是读取进程内的最终工具表**。

白名单配置为 toolsets
`[qintopia, qiwe]`、environment_probe=false、dispatch_in_gateway=false、plugins
`[qiwe-platform, qintopia-tools]`；未选择 hermes-cli。现网 disabled_toolsets 比受管契约少
`cronjob`，存在 `qintopia-context`
MCP 配置。配置存在不等于工具可达或风险。按精确 core 的最终过滤/工具搜索/插件注册路径继续核对，未读取 env/token、未返回 raw
Profile，未启动真实模型/通道、未调用业务工具。

旧 `2237be355906fbe6065ce1815711eee52b2d646e` /
Python3.12 的八工具发现只是准备阶段历史 API 兼容切片；不得当成现网 core 或最终工具隔离证明。独立 UID/env 不可读也只证明一层。如有实际可读取 future
foundation
token/socket 或冒充可信宿主的路径，集中提出原受管配置内最小收紧及功能影响；不关闭全部 MCP、不升级七 Profile、不做岸岸/PMS 全量沙箱。

## 架构、正式制品与历史快照

原本地 QEMU/GCC x64 构建失败/停滞日志保留。随后独立构建负责人对固定
`993edcbfcec9daf21eafc7902186d890e7e7e352` 归档完成 primary/qiwe 两 production feature
release/locked 交叉构建：ELF64 x86-64 GNU，最高 GLIBC2.34；只读 QEMU help/入口/15
capabilities 通过。这是明确后续成功证据，但**不是 native x64
systemd、最终同版业务验收或正式三包**。没有合法 prebuilt builder 入口，不伪造 artifact
manifest；最终冻结源仍走原 Linux x64 GNU
builder，由总指挥协调正式制品。未触发 Artifacts；`upload_cos=false`
仍可能清理同名 GitHub 制品，不能擅自触发。

旧 `.local-workspace/steward-install-preparation-20261001/`
是冻结的准备交接包，保留其 861 文件、manifest SHA256
`78ffdeb7293a33f1e886a7f38cf8309e554563422a4fb93c328cb66b6a0003aa`。

其“尚未批准/未实施”、目标 x64 当时失败、三文档差分 quick 通过均为**历史快照**， 不是当前 C1 实现结论。

其外部文档已存 baseline； 不重写旧包破坏 digest， 也不以授权后正常文档变化报数据损坏。

准备阶段的 48 迁移 SHA384、1707 约束/245 FK、六锁函数 PUBLIC EXECUTE 撤销及模拟 context
ACL 核对继续有效，但不代表完整生产 schema/最小 UI/broker/owner 权限。Dashboard 候选保留 upstream_typecheck=failed/Vite
passed，不将其伪造全绿。公开管理入口 hostname mismatch 仍是既有待 C2 事项。

## 验证与剩余验收

定向 renderer、普通 installer/rollback、promotion 测试和语法检查按最终差分执行；综合检查必须按部署高风险差分走
`pnpm check:pr:auto` 的实际 tier，不套旧 docs-only
quick。本机 RTK 不在 PATH，实际用 pnpm，未改工具包装/全局配置。首次 pre-commit 因当前 PATH 缺 cargo 返回 127；使用现有
`$HOME/.cargo/bin` 的局部 PATH 后重验，不跳 hook、不改检查规则。Draft
PR 首个固定 head 可供并行审阅；远端/最终本地检查及工具补证将在同一个 PR 收口，不提前合并或发布。

上线前集中保留：

1. 最终 #733/#734/C1 同版、正式三包、native x86_64 systemd、完整 ACL/迁移与签名身份。
2. Chrome 与本人业务验收、真实可信目录/同配置读取/撤销、管理路由与精确证书、C2。
3. 进程内最终工具表未直接获取；精确 core 的隔离过滤探针及具体 MCP 能力核对仍需补证。
4. 错误 GID/argv/cgroup、表面 inactive/PID0 但真实残留的独立实测仍未全覆盖； 当前源码严格拒绝不等于所有负例已实测。

完整补偿/recovery/lost-ack/dry-run 继续复用原矩阵， 本次真实新增差异不替代原全部矩阵或最终集成验收。

生产回退只撤入口并恢复 reviewed
release，不回滚业务数据库；保留凭据、配置、会话、记忆、任务、通道/Profile
jobs/audit 与 UNKNOWN 原键。结果不明先查状态，不能自动补跑。
