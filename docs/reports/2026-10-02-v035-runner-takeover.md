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
