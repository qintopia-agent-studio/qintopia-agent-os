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
`OnBootSec=2min`、`OnUnitActiveSec=1min`；hold 导致 service 条件不满足，service 没有进入 active，而启动时间已过，后续周期没有起点。现场 timer 为 active/elapsed，无下一次触发，上次触发仍在前一天；新预检请求未被消费。

确认 hold 已不存在、无 claim、普通消费者已退出后，手动启动一次现有 poller，预检成功且 timer 恢复 waiting。正式部署后 14:05:56、14:07:01
CST 自然轮询成功。这只恢复现状，未修复源代码。

采用现有 timer 每分钟独立日历触发，替换依赖 service
active 的周期。保留启动触发、原 enable/disable 状态、hold、锁、签名、不可变制品与请求去重；不在收尾时主动重放请求。恢复脚本增加一次 start 的方案会耦合收尾与请求执行，因此不采用。

现有 Linux
finalization 用例使用 daily 替代 timer，遗漏生产周期；改为读取真实 timer，检查 hold 拒绝启动后及释放后仍有下一次触发。沿用现有用例，不新增 CI 入口或环境。

## 验证与剩余边界

真实 poller 回归与 Linux 用例语法检查通过；综合检查仍在执行。真实 Linux/systemd 验证未完成前不称已通过。

不启动 VM，不热改线上 unit；修复经 PR、版本化制品交付。七 Profile 核心升级独立。原 COS 模拟连接波动与 Git
maintenance.lock 权限竞争没有因此修复。
