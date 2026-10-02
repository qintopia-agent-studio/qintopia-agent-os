# v0.3.5 部署与首次接管阻断

## 已确认事实

- Release SHA：`891fac8a975e2d2fcd9446bf61a8e4914b471b5f`。
- 发布运行
  [36981039460](https://github.com/qintopia-agent-studio/qintopia-agent-os/actions/runs/36981039460)
  构建、上传通过；旧 runner 不支持 `hermes-anan`，六个目标没有重复，请求在提升前失败。
- 线上 O=`16e8d56b98001579c6288ba13199b80d6d3dfc74`，P=`83d694f2c3bc21fd78a73d25da3197379e2a14d5`。O 的原请求和成功回执在服务器内验签通过。P 的失败回执不构成成功部署证据。
- 过渡 dry-run
  [36983766568](https://github.com/qintopia-agent-studio/qintopia-agent-os/actions/runs/36983766568)
  通过，服务器请求为 `deploy-20261002T082551Z-16e8d56b9800`。已校验 P 主 runtime、QiWe
  companion 与新 bundle；没有切换指针。
- GitHub 构建包与服务器 COS 下载的 bundle tar SHA-256 一致：
  `511b2572e755bf359416079cb68afc2d2bfe7e8483a91b75ef87521e2226ba89`。
- 按固定入口 prepare 成功：原指针与部署 unit 留存服务器本地，旧 timer
  disabled/inactive，hold 已建立。

## 本次接管失败

live 运行
[36984257887](https://github.com/qintopia-agent-studio/qintopia-agent-os/actions/runs/36984257887)
生成 `deploy-20261002T083059Z-16e8d56b9800`，固定消费者退出 75 并保留 hold。

真实 systemd 沙箱中的 COS 回执探测尝试写 `/usr/local/bin/coscli.log`，只读文件系统拒绝。
`poll-deploy-requests.sh` 的 `cos_cp_probe`
保留日志用于区分缺失对象和查询失败，但仅切换工作目录并不能改变当前 COSCLI 的默认日志位置。只读对照探测使用可写临时
`--log-path` 后返回 `cos object not found`；不能将原查询失败算作回执不存在。

失败发生于请求下载之前：没有该请求的 pending、claim、processed、failed、result 或 journal；
`takeover-consumed`
已写。current/previous 仍为 O/P，T 尚未提升。核查时 Sidecar、二花与岸岸服务 active；这不是业务收发验收。GitHub 等待作业已请求取消；COS
live 请求并未因此撤销。

现有恢复 helper 只能从固定发布目录运行且要求 direction
journal；finalize 要求已成功消费。两者均不覆盖这个更早的失败窗口。不能删标记、清 hold 或重放请求。

## 已批准的修复范围

1. `deploy/runner/poll-deploy-requests.sh`：COS 探测显式指定私有临时日志目录，保留错误分类，不放宽沙箱权限。
2. `deploy/runner/run-fixed-takeover-request.sh`：补执行前失败的受控终止与重新准备入口。
3. 复用 `tools/deploy/test-deploy-runner-poller.mjs` 和
   `tools/deploy/test-deploy-runner-systemd-linux.mjs`。
4. 同步本报告及既有 runbook；实现通过 PR 和固定发布产物交付，不热改服务器。

终止必须核对同一请求、原 O/P、固定 unit 已结束且无残留、无 claim/journal/本地结果，以及远端确切缺失结果证据。

持有原锁顺序，保留原消费记录和终止审计。旧 COS 请求必须被可靠隔离后才能恢复轮询，不能只移除 hold。

任意证据冲突或不可读均继续封闭。新尝试使用新请求，不重放原请求。

测试覆盖只读安装目录、真实 COSCLI 错误行为、pre-claim 终止、未知结果拒绝及进程/指针/请求竞争。普通模拟不能替代真实 PID1 验证。

九项原则的具体结论：

1. 实际问题：COSCLI 日志路径错误与 pre-claim 恢复空窗。
2. 数量：两个机制文件、两个既有测试；零 workflow/job/依赖/检查门禁变动。
3. 依据：真实 systemd 日志与持久状态，无需猜测业务代码故障。
4. 复杂度：复用原锁、hold、固定身份和签名验证，不建第二套恢复协议。
5. 测试缺口：现有测试没有模拟安装目录日志行为及早期终止；业务代码无法修复消费者。
6. 最小范围：终止入口只覆盖已证明未执行的窗口，其他未知结果保持拒绝。
7. 不为岸岸单独放行，不放宽签名、产物或请求校验。
8. 不夹带业务配置或非部署任务。
9. 新增负例与早期终止保护可复用于后续固定接管故障。

用户随后明确要求“按照最佳实践进行修复”并继续推进，授权这两处机制及现有测试的修复。

消费标记已移到持久 claim 之后。launcher 串行锁防止并发绑定；准备中失败不能复用同一请求。新增
`retire-unstarted <id>` 保持 O/P、timer 与 hold，不操作服务或发布指针。

退役要求原请求验签通过且过期超过 5 分钟、固定 O/P、无 T、无 claim/journal/回执、无相关活进程。COS 必须返回固定结果键的 HTTP
404 + XML `NoSuchKey`；403、其他 404、网络错误和已有结果都拒绝。

旧格式标记原子归档，退役请求摘要保留在原 takeover 记录中；部分下载保留原文件与摘要。中断后可继续同一退役检查，但旧请求永远不能重新绑定。仅允许新签名请求沿原 consume 入口继续。

## 验证与限制

日志路径回归修复前出现只读目录错误，修复后通过。

现有 poller 测试增加实际 launcher 的本地边界测试，覆盖已领取、远端结果、验签、过期、指针漂移、残留进程及 cgroup。

另测旧格式归档中断、部分下载及新旧请求绑定。systemctl/COS 为替身，不冒充真实 systemd 验收。

本地 Ubuntu 下载两次网络 EOF，没有创建成功的 VM；用户要求不搭新环境，已停止该路线。代码未上线，原 hold 未清除；完整验证及 PR 状态以最终提交和 Checks 为准。

对本次日志故障另做了服务器只读探测：复用已安装 COSCLI，在临时 systemd unit 的
`ProtectSystem=strict`、`ProtectHome=read-only`、`PrivateTmp=yes`
下读取原结果键。显式临时日志路径后正常返回对象不存在，没有只读日志错误；unit 和临时文件随探测清理。没有执行部署请求、改变 hold 或发送业务消息。这证明实际 COSCLI 日志修复，不替代完整接管验收。

## v0.3.6 实际响应复核

v0.3.6=`5632f9f58dd8eaa1c2df0d7fa3eafe3aeb704641` 的
[构建运行](https://github.com/qintopia-agent-studio/qintopia-agent-os/actions/runs/36990114220)
已完成主程序、QiWe companion、控制台资源及部署包构建和 COS 上传。bundle tar SHA-256 为
`e5e13cf8f85fe0edc1fb762be863cf8720560f8fc3f44c84b2509cc17267ed8f`，GitHub 构建包与服务器独立 COS 下载一致。新版完整包暂存，v0.3.5 原包保留。

使用新版受审 launcher 退役旧请求时退出 75，未修改 takeover、hold 或发布指针。只读认证 GET 确认 COS 返回 HTTP
404、`Code=NoSuchKey`，字段为 `Code/Message/Resource/RequestId/TraceId`；`Resource`
精确等于请求对象路径，没有 `Key`。先前测试仅模拟
`Key`，遗漏真实服务响应。回退恢复 helper 存在同一假设。

本次最小修复限定两个 helper 的对象身份判断：接受精确匹配的 `Resource` 或旧格式
`Key`；若同时存在则两者必须一致，缺少身份、错误路径、其他错误码仍拒绝。

不修改 workflow、签名、锁、hold、回退状态机或发布权限。测试复用现有本地 HTTP 边界，并执行回退 helper 中原始 Python 响应处理代码，不重建测试环境。

修复继续通过 PR 和完整版本包交付；本次未退役旧请求，也未进行 live 接管。

## v0.3.7 接管实测与下一步范围

Release `5767e9c5b55f0b273bb045896b6b056a1146ff3a` 的构建和 COS 上传成功。部署包摘要
`37f83ed938c7ef1a63bbf40ddd1f77454b6c901f15432ff02cff0d2926086d6c`
在 GitHub 构建包与服务器独立下载之间一致，关键脚本与该 tag 源码一致。
[发布作业](https://github.com/qintopia-agent-studio/qintopia-agent-os/actions/runs/36993412196)
最终因 hold 下没有消费者而等待回执超时；不是构建或上传失败。

新版 `retire-unstarted` 已成功退役原
`deploy-20261002T083059Z-16e8d56b9800`，保留审计、旧标记归档和 hold。真实 COS `Resource`
响应兼容已得到验证。

[接管 dry-run](https://github.com/qintopia-agent-studio/qintopia-agent-os/actions/runs/36996466270)
使用新请求 `deploy-20261002T104009Z-16e8d56b9800`，旧 O
runner 在真实 systemd 沙箱中通过。回执验签、上传和 GitHub 等待成功；O/P 未变，T 不存在。旧回执没有顶层
`promoted_current`，应结合签名状态、请求和实际指针验证，不伪造该字段。

[新 live 请求](https://github.com/qintopia-agent-studio/qintopia-agent-os/actions/runs/36997427852)
`deploy-20261002T105028Z-16e8d56b9800`
已领取、产生 journal，并上传明确失败回执。失败阶段为
`quiesce-space-automation-runtime`，`promoted_current=false`，没有 Profile
activation 或 rollback。O/P 不变，T 未创建，hold 和消费标记保留。不能重放该请求，也不能使用 pre-claim 退役入口。

三个 Space automation
unit 均为 inactive；timer 和 worker 为 disabled。同等沙箱的真实脚本 trace 确认 disable/stop 通过，仅三个无条件
`reset-failed` 失败。单独复核得到
`Unit ... not loaded`：systemd 卸载空闲 unit 后，脚本误把非必需的失败状态清理当作停用失败。当前源码仍存在同一逻辑。复核只操作已证明 inactive 的这三个 unit，没有重放部署请求。

恢复边界仍不完整：现有 helper 只能从 T 不可变树执行，但该失败发生在 T 创建前；launcher 也不能闭合已领取、明确失败的首次请求。不得借目录伪造、删除标记、重新绑定原请求或给旧 runner 注入命令替身绕过。

负责人已确认继续修复以下四个机制文件及既有测试、runbook；不扩大 CI 或测试环境范围：

- `quiesce-space-automation-runtime.sh`：以实际停用状态为准，只对 failed
  unit 清理失败状态；保持活进程、未知状态拒绝和 PID/cgroup 验证。
- `run-fixed-takeover-request.sh` 与
  `poll-deploy-requests.sh`：固定接管使用完整、已验摘要的新 bundle 内 runner；保留原 O/P 验证、精确请求绑定、签名、锁和 journal。修正新源码却继续执行有缺陷的旧 O
  runner 无法解除阻断。
- `recover-release-lineage.sh`：允许经完整产物校验的暂存入口，窄化处理本地及远端签名回执均明确失败、失败阶段确定、O/P 未变且 T 不存在的首次请求。

  审计闭合后仅允许新签名请求。未知结果及其他 lineage 继续拒绝，hold 不手工解除。

这是四个机制文件及既有测试、runbook 的调整；不新增 workflow、job、依赖或测试环境，不改业务权限、不升级 Hermes 核心、不回退业务数据。

验收须覆盖 systemd 卸载 unit、失败回执闭合、未知结果拒绝、并发锁和实际新 runner 启动链。实施中；生产完整接管及后续发布仍待验证。

完整调用链复核发现 `install-release-systemd-units.sh` 在 unit 安装前后也重复无条件
`reset-failed`，会使第一阶段修复后在安装阶段再次失败。负责人已明确同意将此第五个部署机制文件及既有安装器测试纳入同根因修复；不新增 workflow、job、依赖或环境。

接管集成测试还要求从干净环境加载签名配置，避免依赖操作者 shell 的 export 状态。新版 poller 显式向验证子进程传递所需签名/COS 变量，launcher 从固定私有配置读取回执验证密钥，不改变签名协议。

### 本轮验证与剩余边界

服务器只读复核仍为 O/P、T 不存在、无 claim；failed 请求归档、原 invocation、hold 和本地回执上传摘要一致。

三个 Space automation
unit 均 inactive，两个 service 的 MainPID/ControlPID 为零且 ControlGroup 为空。

本轮没有切换线上代码或服务。

本地已通过：提升前停用、真实安装器、真实 poller/launcher/recovery、提升失败和回退测试。

回归覆盖空闲 unit 的 reset-failed 拒绝、failed 状态清理、残留 PID、未知 cgroup、无回执、错误签名、有效签名但失败阶段不符、已提升、后续 journal、证据漂移及旧请求禁重放。

失败闭合可在审计已落盘、消费标记未归档的中断点继续，不能覆盖新的请求绑定。

链路回归实际执行当前 runner 的验证、锁、journal、签名回执、上传核对及 finalize，然后执行完整六目标请求，得到 T/O→R/T 和成功回执。

systemd、COSCLI、制品提升、installer/smoke 的外部边界使用本地模拟；安装器与回退脚本另有真实脚本专项回归。

这不证明生产服务或岸岸业务闭环已通过。

既有 Linux PID1 fixture 已改为使用新 staged
runner 与完整校验入口，语法检查通过；本轮没有运行原生 PID1 故障矩阵，没有安装或下载 Ubuntu，也没有新增 CI 或测试环境。

上线必须使用审查后的完整版本包。

依次闭合本次明确失败请求、生成新签名接管请求、验证接管签名回执、再验证完整发布和 Profile 配置保留；未知状态保持 hold，不补跑。

生产完整路径成功前，本事故仍未关闭。

`pnpm check:pr:auto` 已通过 quick 与 heavy Rust：默认 860 项、all-features
875 项通过。本地专用 `qintopia_test` 不可用，PostgreSQL
tier 按现有规则跳过，交由 CI 验证。初轮文档格式/行长检查失败已修正，最终格式、Markdown、协作及部署契约检查通过。

### PR #740 Linux CI 的测试隔离遗漏

[Light check](https://github.com/qintopia-agent-studio/qintopia-agent-os/actions/runs/37003512760/job/110826406409)
在六目标发布的本地模拟中报
`consumer systemd invocation identity is not verified`。GitHub Linux runner 的宿主
`INVOCATION_ID`
被新增 fixture 继承，与模拟 systemctl 返回的固定调用身份不符。本地显式注入不同的
`INVOCATION_ID` 已复现同一断言失败。

修复仅限定既有测试环境：本地模拟固定自身 invocation，原生 Linux
fixture 的直接进程路径清除宿主 invocation；真正 systemd-run 路径仍使用 PID1 注入的身份。补充身份不一致必须拒绝的回归，不改生产签名/身份校验、workflow、检查入口或门禁。

该 PR 的 reviewer 未发现重大问题，但不能用该结论代替失败的 CI。v0.3.8 发布 PR 应等待此测试修复合并及最新检查通过，再由负责人合并并发布。
