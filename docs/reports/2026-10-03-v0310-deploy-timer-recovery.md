# v0.3.10 部署恢复与轮询 timer 缺口

## 已确认事实

- v0.3.10 `afe3e81c7c5ac0d74332dfb5aaa79fa35ce06d83` 的构建、预检
  [37100756580](https://github.com/qintopia-agent-studio/qintopia-agent-os/actions/runs/37100756580)
  与正式部署
  [37101695669](https://github.com/qintopia-agent-studio/qintopia-agent-os/actions/runs/37101695669)
  成功，正式请求为 `deploy-20261003T060443Z-afe3e81c7c5a`。
- current、runtime、bundle 均为该 SHA，previous 为过渡目录
  `70e7984fab92ddab956009585212d0e9729767b5`；签名成功回执与指针一致，未回退。
- 24 个 Profile 配置摘要未变；七个 Profile
  Gateway 运行中。未发送真实测试消息，不据此认定业务回复已验收。二花早报 worker 的失败发生于此次部署之前，未补跑。

## 轮询缺口与最小修复

固定接管收尾在 hold 存在时恢复 timer，之后才释放 hold。生产 timer 使用
`OnBootSec=2min`、`OnUnitActiveSec=1min`。现场 timer 为 active/elapsed，无下一次触发，上次触发仍在前一天；新预检请求未被消费。

恢复依赖旧 service 激活时间的周期是机制推断，不能概括成“所有 hold 跳过都会耗尽周期”。完整旧现场状态尚未在真实隔离环境复现。

确认 hold 已不存在、无 claim、普通消费者已退出后，手动启动一次现有 poller，预检成功且 timer 恢复 waiting。正式部署后 14:05:56、14:07:01
CST 自然轮询成功。这只恢复现状，未修复源代码。

采用现有 timer 每分钟独立日历触发，替换依赖 service
active 的周期。保留启动触发、原 enable/disable 状态、hold、锁、签名、不可变制品与请求去重；不在收尾时主动重放请求。恢复脚本增加一次 start 的方案会耦合收尾与请求执行，因此不采用。

现有 Linux
finalization 用例使用 daily 替代 timer，遗漏生产周期；改为读取真实 timer，检查 hold 拒绝启动后及释放后仍有下一次触发，并等待一次自然触发。沿用现有用例，不新增 CI 入口或环境。

## 验证与剩余边界

同步已合并的 #735 后，真实 poller 回归、Linux 用例语法检查、格式检查与
`pnpm check:pr:auto` 通过。quick 与 heavy Rust 通过；默认 860 项、all-features
875 项通过，本地 PostgreSQL 未就绪而跳过，不称其已执行。

补充真实 systemd 验证时，只使用独立、随机命名的临时单元和
`/usr/bin/true`，不加载部署凭据，不连接 COS，不执行 poller，不操作真实请求或部署目录。系统管理器为 systemd
255；保留生产 timer 的 `OnBootSec=2min`、 `OnCalendar=minutely`、`Persistent=true`、默认
`AccuracySec=1min` 和 `RemainAfterElapse=true`。通过独立临时 hold 的
`ConditionPathExists` 模拟拒绝启动。

早期用户级对照没有复现旧现场的 active/elapsed；临时单元默认值、自动卸载和已有激活时间都会影响结果，因此这些尝试不记为旧故障复现通过。最终系统级检查验证新周期的恢复不变量，不是完整接管/回退验收。完整
`--takeover-finalize`
故障矩阵使用固定部署路径，未在生产执行；对应真实脚本的签名、锁、CAS、去重和失败保持 hold 已由本地 poller 回归验证（OS 边界模拟）。

系统级检查于 2026-10-03 15:48 CST 通过：hold 存在时 `ConditionResult=no`，
`ExecMainStartTimestampMonotonic=0`，下一次触发为 15:48；移除独立测试 hold 后没有手动启动服务，自然执行
`/usr/bin/true`
成功（`ConditionResult=yes`、`Result=success`、`ExecMainStatus=0`），且仍有 15:49 的后续触发。临时单元、测试 hold 和对应 Persistent
stamp 在收尾清理。正式 poller 保持 active/waiting、最近执行 success/0，未修改其 unit 或重启业务服务。

复核命令：用 `systemd-run --unit=<独立测试名> --uid=ubuntu --gid=ubuntu` 搭配
`--on-boot=2min --on-calendar=minutely --timer-property=Persistent=true` 和
`--timer-property=RemainAfterElapse=true` 创建只执行 `/usr/bin/true` 的临时单元。
`--property=ConditionPathExists=!<独立测试hold>`
只指向测试目录。检查上述字段，释放测试 hold 后等待自然触发，再停止自己的测试单元并删除其测试状态。不得替换成正式单元名、正式 hold 或 poller 命令。

不启动 VM，不热改线上 unit；修复经 PR、版本化制品交付。七 Profile 核心升级独立。原 COS 模拟连接波动与 Git
maintenance.lock 权限竞争没有因此修复。
