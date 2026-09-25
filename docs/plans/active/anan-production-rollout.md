# 岸岸生产接入

日期：2026-09-25。基线：PR
721，`046d0583a200629f748f38f06dfeb19f53152b7c`。用户已明确要求按本方案推进。当前未部署，不把服务 active 当作业务验收。

## 目标与边界

首批交付真实企微会话中的 PMS 查询、方案、有效确认、执行回执及原键恢复。复用现有 Profile、模型、Bot、历史、共同 Person、授权、WorkItem 和 PMS
API。PMS 继续拥有订单、库存、住宿与资金事实；Agent
OS 不新建平行业务库或权限系统。已有有效明确交办可复用，不为普通订单额外增加主管审批。未知结果保持 UNKNOWN，按原键核查，禁止换键自动重试。

收款 feed、申请接入、提醒和欢迎保持关闭，独立验收后再启用；不补跑历史。定时触发继续使用 Hermes
cron，不增加调度框架或重复投递账本。上线授权不代替具体业务授权：真实消息及写入验收由用户发起明确业务事项。

## 已核实的生产基线

- 现有 `hermes-gateway-anan.service` 正在运行；工作目录为
  `/home/ubuntu/.hermes/profiles/anan`，核心解释器属于 `v2026.9.21`。
- Agent OS release 为 `16e8d56b98001579c6288ba13199b80d6d3dfc74`。
- PMS app/WeCom worker 为 v1.7.2，提交
  `c75c5470f0202fadcd2a497ec6ce7e87483943fd`；app 绑定 `127.0.0.1:4100`，生产入口为
  `pms.qintopia.cn`。
- Profile
  config/.env 存在，plugins 目录不存在。两文件未发现 PMS/Foundation 绑定键；其他注入来源尚待核查，不能断言整个进程没有这些配置。凭据和业务数据不带出服务器。

## PR 拆分与已批准的门禁范围

PR 721 先完成现有岸岸服务的受管发布归属，PMS 生产入口仍关闭：

- 增加一个真实 `hermes-anan` target，对应
  `hermes-gateway-anan.service`；同步既有 manifest、restart rules、请求 schema、runner
  allowlist、smoke 和测试。
- bundle 包含岸岸、PMS 与共同服务插件源码，不自动安装插件或覆盖 Profile。
- 未知路径继续拒绝，不增加 no_restart_paths 特例、GitHub job/workflow 或检查入口。
- 不加入核心 Profile
  registry：其安装器会改写核心入口，岸岸当前版本必须保留。普通服务发布归属与核心升级接管分开；后者须验证版本、兼容和回退后再进行。

该范围已获用户批准，依据是实际 Light
check 的 unmatched-path 失败和现存服务。测试沿用现有解析器、部署合同和模拟 systemd 测试，不引入通用未发布包机制。

后续 PR 实现实际生产接线：先验证已有隔离 worker，能复用则复用。PMS 写 Token 与可信宿主 Token 不得被模型 terminal/execute_code 读取或调用冒充宿主。环境变量改名、同 UID
socket 和提示词禁止都不算隔离；确有缺口时只增加最小私有执行边界。固定 PMS 生产目标、校验 TLS、限制超时，不允许任意 URL、重定向或自动重试。复用官方 WeCom 可信上下文与共同授权，人员/物业绑定从可信配置核实，不猜测。

## 验证、切换与回退

1. 运行解析器、Agent 管理、部署 runner/smoke 测试，再运行
   `pnpm check:pr:auto`。生产接线另测越权、重复确认、未知结果、重启恢复和共享数据库迁移兼容。
2. 检查最新提交 CI 与 reviewer，修复有效问题；CI 通过不等于允许自动合并或发布。
3. 部署前记录固定 SHA、校验清单、原发布指针和服务入口；服务器本地一致性备份。不覆盖 Profile 模型、通道、凭据、任务、会话或记忆，不热改源码。
4. 普通发布使用现有 runner 发布/回退流程，岸岸仍使用原核心入口。等待当前任务结束再重启；超时暂缓。记录共享组件影响，不能声称只影响岸岸。
5. 回退必须使用支持该 target 的已验证 runner 和先前不可变产物；旧 runner 不认识新 target 时不能直接提交回退请求。首次接管前先验证这一兼容条件。后续插件激活需单独保留旧入口并演练恢复；不回滚实时业务数据。
6. 用户发起真实查询及一笔已授权办理，核对 PMS 事实、事项与原会话回执。未提供合适业务事项时保留写入验收未完成，不自行虚构订单或发送测试消息。

## 当前状态

发布归属修复已推送 PR 721（`57786787`）。后续 PR 先补固定生产 HTTPS 传输，插件仍关闭。

只读核查：岸岸未配置 terminal backend；现有核心默认 local，启动环境未发现 PMS/Foundation
Token。

不能据此证明模型工具已隔离。凭据隔离、安装接线、生产验收和部署均未完成。实现时更新本页状态，不新增平行状态手册。
