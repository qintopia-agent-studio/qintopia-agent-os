# Foundation Unix broker 提前断连修复

## 已确认的根因

在 `codex/welcome-identity-audit` 的f5d5696基线上，用真实Unix
socket和真实broker复现。平台为Darwin25.6.0
arm64，Tokio1.53.1。测试单线程在服务accept前同步connect并关闭客户端，没有依赖等待时长或探针重试来制造结果；惰性SQL连接池未访问数据库。

修复前独立平台探针与broker JoinHandle分别记录：

```text
platform=macos accept=ok
closed_peer_cred=Socket is not connected (os error 57)
kind=NotConnected; os_error=Some(57)
broker exited: Socket is not connected (os error 57)
io_kind=Some(NotConnected); os_error=Some(57)
```

因此此次复现不是listener.accept失败、EOF读取错误或响应写失败，而是已接受连接的peer_cred错误经
`?`
传播，结束了整个broker任务。后续请求无法得到响应。这是服务传输层缺陷；探针改为完整请求只能避免触发条件，不能代替修复。Linux平台的凭据保留行为可能不同，本次没有声称在Linux独立复现同一errno。

## 修改与边界

仅修改 `foundation_server.rs::broker`
的连接凭据处理：peer_cred失败时关闭当前连接、继续监听；凭据成功后仍必须匹配原socket
owner
UID。没有未知凭据放行，没有新的协议、并发模式、重试或权限。启动、目录、socket权限和listener.accept错误继续向上传播。

新增不依赖PG的真实Unix回归，用独立子进程隔离环境变量，并捕获broker
JoinHandle错误。空连接、半截请求和发送完整请求后不读取即断连，均须允许后续完整请求得到协议响应；错误token与正确token的错profile仍分别拒绝。既有PG宿主测试继续核验有权调用成功与模型token、profile、gateway拒绝。本次未改CI、allowlist、依赖、迁移或生产配置。

## 证据与验证

本线日志位于 `.local-workspace/welcome-integrated/`：

- `broker-disconnect-before.log`：修复前1项失败，捕获NotConnected/os57退出。
- `broker-disconnect-after.log`：同一真实Unix回归修复后1项通过，平台peer_cred仍报57，broker继续服务。
- `broker-disconnect-auth-final.log`：本线PG51847真实宿主认证1项通过；提前断连之后，授权宿主正常受理，模型token/错profile/错gateway拒绝。首次短名称配合exact产生0项，保留在auth-zero-selection日志，不计为通过。
- `broker-disconnect-clippy.log`：所有target/feature的严格Clippy通过。
- `broker-disconnect-pr-auto.log`：最终源码（含子进程完成标记）执行原工程入口，light、Rust格式/编译、默认与全特性严格Clippy通过；默认测试852通过/2忽略，全特性867通过/188忽略。两个标准smoke、两个适配器编译边界测试通过。PG逐项检查通过，人员协作135通过/0忽略、欢迎24通过/0忽略。各层覆盖有重叠，不相加作为独立用例总数。
- 新增Unix回归已包含在上述默认、全特性及人员协作检查中；子进程必须输出完成标记，零匹配不能被误算为成功。PG宿主认证也包含在人员协作135项中，不重复计数。
- 原auto最终退出码1：`operations-control-plane-apply-smoke.sh` 报
  `database URL hash is not in the reviewed allowlist`，随后空结果引起JSON解析错误。本线51847隔离库未在该部署工具的审核白名单内；这是本次实际检查结果，不以专项通过替代总体通过，也未修改CI、数据库白名单或旧库来绕过。
- `broker-disconnect-harness.log`：测试目录框架8项通过。Markdown、格式、差异空白及协作检查通过；本地pull_request事件中的完整正文检查通过（不是跳过），PR
  doctor通过。最终文档检查日志使用同目录的 `broker-disconnect-final-*` 前缀。

岸岸原始日志保持在其工作树 `.local-workspace/anan/integration/`：
`phone-joint-first.log` 为stay_contacts_incomplete，`phone-joint-second.log`
为ConnectionRefusedError/errno61；当时没有记录broker退出原因，所以这两份日志本身不能独立证明根因。本线确定性复现补齐了缺失的退出证据。

岸岸业务联合已另交17fdbd5，真实Client/HTTP/Unix/服务PG的模拟业务路径通过不与本次传输回归重复计数。本线没有改动其52322数据库、旧支付环境、用户预览或真实渠道。

## 恢复与剩余事项

回退本次源码提交会恢复原有提前断连退出风险；本次没有数据库或协议迁移需要回退。保留旧日志与当前隔离树，由岸岸任务将本提交整合到其17fdbd5联合基线，保留两边独立的测试登记，再按影响范围验证共同入口；本线不反向合并其分支。

部署smoke的数据库准入仍交总指挥在既有受支持的审核环境中验证，不在本修复中调整CI或白名单。本地通过不代表生产部署、真实模型或群发送通过；本次没有执行推送、远端PR、主线合并或生产操作。
