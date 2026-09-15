# 双程序合成联合验收入口

日期：2026-09-11。仅用于 B 的独立合成实例；不代表生产联调已通过。

构建固定快照的 sidecar：

```sh
cargo build --offline --manifest-path runtime/sidecar/Cargo.toml --features welcome-synthetic-driver
```

所有命令使用该快照的
`runtime/sidecar/target/debug/qintopia-message-sidecar`。由调用进程显式注入下列环境变量；不得保存密钥、数据库连接或原始业务内容到证据：

| 变量                                  | 要求                                                           |
| ------------------------------------- | -------------------------------------------------------------- |
| QINTOPIA_WELCOME_SYNTHETIC_ENABLE     | 1                                                              |
| QINTOPIA_WELCOME_LOCAL_DATABASE_URL   | 独立 literal loopback 的 qintopia_test，不带 query             |
| QINTOPIA_WELCOME_LOCAL_SOURCE         | synthetic- 前缀的独立来源                                      |
| QINTOPIA_WELCOME_LOCAL_PROPERTY       | 合成 property                                                  |
| QINTOPIA_WELCOME_SYNTHETIC_READ_ROOT  | 固定 `http://127.0.0.1:18443/`；只允许 literal loopback 根路径 |
| QINTOPIA_WELCOME_SYNTHETIC_READ_TOKEN | B 临时产生并环境注入，不放在参数或文件                         |

```sh
qintopia-message-sidecar welcome-synthetic init
qintopia-message-sidecar welcome-synthetic status
qintopia-message-sidecar welcome-synthetic pull
qintopia-message-sidecar welcome-synthetic consume
qintopia-message-sidecar welcome-synthetic rebuild
qintopia-message-sidecar welcome-synthetic scan
qintopia-message-sidecar welcome-synthetic refresh
qintopia-message-sidecar welcome-synthetic recover
```

`init` 在受限库运行现有迁移并注册一组 synthetic source/property 和合成审核身份。返回
`synthetic_operator_link`
供本地接收器使用；重复初始化拒绝，不覆盖或重新放开重建中的来源。 `pull` 一页；`consume`
一项；`scan` 一页并处理至多 100 条持久扫描项；`refresh`
至多 100 个已知订单。根据返回的 has_more/consumed/complete 重复单步，位置及 claim 来自真实数据库。
`rebuild` 从真实 GET feed 取得并核验 head 后建立新代次；`scan`
自动恢复代次。状态输出仅包含状态与计数，不包含游标、正文和业务标识。失败返回固定脱敏错误，先查 status，不盲目重跑 init/rebuild。

接收器另注入 `QINTOPIA_WELCOME_LOCAL_ENABLE=1`、初始化返回的
`QINTOPIA_WELCOME_LOCAL_OPERATOR_LINK`、临时
`QINTOPIA_WELCOME_LOCAL_SIGNING_KEY`（至少 32 bytes），运行：

```sh
qintopia-message-sidecar run-welcome-local --port 18872
```

签名 key ID 固定为 `local-synthetic`。PMS 可经它自己的 loopback
HTTPS 代理投递原始签名请求，Agent
OS 接收路径、HMAC 验证、Inbox、版本核验及补偿解析均使用正式实现。GET 测试传输与生产的差别仅为 literal
loopback
HTTP；生产 Client::new 仍强制 HTTPS，且不跟随重定向。不得将测试代理改成对外转发器。接收器和 driver 均无消息发送、制卡、上传或 PMS 写入口。admission_enabled 仅表示合成事件准入，绝不表示外部执行已开启。

冻结方式：独立目录的 BASELINE.json 包含源 HEAD、分支、逐文件字节数与 SHA256。B 核验全部文件并复制到自己的构建目录；不要在 A 活动目录运行或修改。v1 为先前接收器，v2 加入本入口。
