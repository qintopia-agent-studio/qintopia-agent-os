# 正式管理 UI 部署接线提案

日期：2026-09-28。状态：**docs-only Draft
PR，新增部署实施范围待负责人逐项批准**。本文记录可审阅方案，不修改 CI、部署或业务源码，不批准合并、Release、证书申请或生产操作。

## 目标与依据

让已有授权的管理者通过独立 HTTPS
origin 的根路径使用现有统一工作台；R 中显式启用，T/P 中持续关闭。管理 UI 复用 #723 的账号、Person
grant、会话和
`workbench.html`，不建立第二套管理权限。本文以 #723 当前未冻结实现、已合入的 #726
runner 合同、现有 O→T→R 接管顺序、隔离 PostgreSQL 权限探针及 2026-09-28 脱敏只读观察为依据。源码和最终发布包仍须重新核对。

已读仓库入口、roadmap、change routing、programming guardrails、server change
policy、`deploy/runner/README.md`、`runtime/postgres/README.md`、`runtime/sidecar/AGENTS.md`、Nginx
README/manifest 与生产 runner 文档；候选代码和测试路径已逐一核存在性。两份本地工作方案仅作为起草证据，未纳入 Git。

当前 `qintopia.cn`
的根路径属于公开 COS 站点。在同一 origin 下加管理前缀会让公开页面脚本同源访问管理 API；Cookie
Path/SameSite 不能构成隔离。临时 loopback 加 SSH
forward 只能服务一次性初始化或技术验收，不能满足日常统一入口。因此采用独立的
`agentos.qintopia.cn`，不增加管理应用或权限模型。域名管理者已手动添加 A 记录；2026-09-28
12:25 CST，DNSPod 两个权威 NS 的直接 TCP 查询及 DNSPod、Google、Cloudflare
DoH 均返回同一源站 IPv4、TTL 600；未见 AAAA/CNAME。这只证明解析。Nginx
vhost、独立证书、UI
unit 和站点尚未安装。生产应用数据库经一次不输出凭据的只读查询返回 PostgreSQL
18.4；版本不能证明最终授权。

## 采用的最小接线

1. **入口与证书。** #723 固定生产 HTTPS Host/Origin、Secure
   cookie 和根路径的页面/API 合同，生产域名通过受限配置给定而非写死在二进制。先在既有受控 hold 下安装精确主机的 HTTP
   404/ACME 引导，申请独立 cert-name 和精确 SAN 证书并验证续期，再安装 HTTPS 根路径反代。Nginx 只代理固定 loopback
   UI 端口；upstream 不在时返回 503，不回落公开 COS。保留原 `qintopia.cn`
   站点、证书和全局 Certbot timer。
2. **运行身份。** 单独的无登录 shell、无补充组 `qintopia-management-ui`
   UID；固定 root-owned 0600 UI env 只含五键：`QINTOPIA_FOUNDATION_PRODUCTION_ENABLE`、
   `QINTOPIA_FOUNDATION_TENANT`、`QINTOPIA_FOUNDATION_IDENTITY_NAMESPACE`、
   `QINTOPIA_FOUNDATION_DATABASE_URL`、`QINTOPIA_COLLABORATION_PUBLIC_ORIGIN`。unit 只运行不可变 R 二进制，不继承主 sidecar
   env、PMS
   enable 或 LOCAL 标志。准备阶段须核实际 UID 对凭据、可执行文件及工具的读取边界，不能用不同 UID 的名义掩盖 root-equivalent 或既有主凭据读取能力。
3. **数据库。**
   UI 登录角色复用既有共享基础库，仅取得已可达 UI 路径所需的 CONNECT、schema
   USAGE 与列/表权限。身份保存需要四张 welcome 失效表的实际修改列；真实 host
   `raw_events` 仅需 SELECT，Person 草稿 CLI 的行锁不属于 HTTP UI。`/api/foundation/*`
   仍受 LOCAL 门禁，生产不设置 LOCAL。检查实际 tenant 谓词、跨 tenant 可见性和包括 PUBLIC/继承在内的有效权限；`NOBYPASSRLS`
   本身不构成隔离，也不预设新业务库或全局 RLS 迁移。
4. **固定锁入口。** #723 的版本化普通 SQLx 迁移创建五个 schema-qualified
   `SECURITY DEFINER` 函数：一个 `void` gateway SHARE 表锁、四个 `bool` 行锁；固定
   `search_path=pg_catalog,pg_temp`，无调用者提供的 SQL、表名或锁模式。布尔函数完整取得固定集合行锁后，未命中返回
   `false`，由调用方立即拒绝；函数不替业务层判授权。迁移事务内撤销 PUBLIC
   EXECUTE，不新增 UI 授权，也不创建或引用预置 owner，P 可照常执行 R
   bundle 迁移。`REVOKE ... FROM PUBLIC`
   本身不能证明 UI 不可调用：须核创建时适用的 default ACL、五函数实际 named
   ACL 与角色继承后的有效 EXECUTE；只要 UI 仍有有效 EXECUTE 或证据不明，就不能声称数据库侧已禁用，也不得激活 UI。

   正式启用前，管理员另行受审创建或核对非表 owner 的锁角色：
   `NOLOGIN NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOREPLICATION NOBYPASSRLS`，无成员。

   核五函数定义和默认/直接/继承 ACL，授它最小 schema/SELECT/行锁列 UPDATE，并按已核 PostgreSQL
   18.4 给它 gateway MAINTAIN，转移 owner 后显式保留原 runtime
   caller 及核准 UI 的 EXECUTE。UI 无 gateway UPDATE、owner membership、SET ROLE、schema
   CREATE 或替换函数权。转移如需临时 schema
   CREATE，完成后撤销。最终 #723 签名和列授权以冻结实现为准；以后迁移不能假定可直接
   `CREATE OR REPLACE` 已转 owner 函数。

5. **停止与恢复。** T 安装 R bundle 时只放置 disabled/stopped UI
   unit；缺 UID/env/证书/DB role 不得启动。R 的 `activate` 须自行非阻塞按 FD8
   `poller.lock`→FD9 `deploy.lock` 取锁，核当前指针为获批 R、无 hold 或 pending
   claim，并显式检查二进制身份、现有 tenant/管理员授权、账号 bootstrap、DB 有效权限、HTTPS
   Host/Origin/cookie 及浏览器验收。UI unit 固定
   `Restart=no`、SIGTERM、`TimeoutStopSec=35s`、`SendSIGKILL=no`。

   应用关闭 listener，在途请求最多等 30 秒；普通未开始写入的请求错误不导致停服。

   配置写开始后的 5 秒超时或 handler 错误，须以固定 UNKNOWN 错误非零退出。三条保存路径的 commit 结果不明也须同样退出，不能被普通确定拒绝的 JSON 响应吞掉。

6. **指针前停点。** 外部 root 维护直接调用 helper 的
   `prepare`、`install-http`、`issue-cert`、`install-https` 和 `retire-site`
   固定模式。每步须核本次 request-bound hold，并自行非阻塞按 FD8 `poller.lock`→FD9
   `deploy.lock` 取锁，持至动作完成；首次站点安装使用 T 已安装后、`finalize`
   前仍生效的 O→T hold。hold 持续阻断普通消费者，但不代替互斥锁。

   既有 runner/recovery 按原合同自行持锁，只调用 helper 的 `quiesce` 和
   `verify-closed`。helper 核继承的 FD9 与固定 `deploy.lock`
   设备/inode 相同，并在同一 open file description 上验证
   `flock -n 9`；不重开或等待该锁。父入口持 FD9 穿过所有指针写入，root
   helper 不在持锁时反向调用 runner/recovery，也不扩通用恢复协议。

   每次 `quiesce` 在 `disable --now`
   前保存精确 InvocationID、Result、ExecMainCode/Status、PID、NRestarts、二进制身份及该 invocation
   journal 中的固定错误码，不先 `reset-failed` 或重启。停止后复核**同一 invocation**
   的退出结果和错误码，覆盖 30 秒排空期间才出现的 commit
   UNKNOWN；再查 PID/cgroup/TCP 监听消失和 HTTPS
   503。任一异常、证据不明或残留，保留 hold 并拒绝指针写入；按原 operation_id、已有 receipt/审计及对象版本回读，明确决定恢复，不自动重放或重开。UI 无 Unix
   socket；PMS broker 的 socket 验收独立进行。

R→T 保留固定 HTTPS
vhost 和受限 DB 记录，但 UI 持续关闭。失败补偿即使恢复 R 指针，也不自动启用 UI。继续使用既有 O→T 单 system、T→R 六目标两步接管和单次 Publish 准备；不修改普通签名请求、COS 对象、回执或恢复状态机。

受限角色的可达面按最终 #723 head 复算，准备时至少核对以下正反例：

| UI 路径          | 允许的最小权限方向                                                                                                                                                                          | 明确禁止                                                     |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------ |
| 登录、会话、账号 | 读取 tenant/account/session/source/Person，写登录限制、会话、账号与审计；Person 行锁经固定函数                                                                                              | migration/owner 权限                                         |
| 组织、业务配置   | 只对既有组织、scope、role/duty、业务配置和 command 表作对应读写                                                                                                                             | `business_actions` 执行或消息插入                            |
| 身份预览与保存   | 读 source/gateway/Person 与 host `raw_events`；保存只写现有 source、会话、grants 和失效路径                                                                                                 | UI 直接 gateway UPDATE、MAINTAIN、Person 草稿 CLI 的额外行锁 |
| 欢迎关联失效     | 仅 `welcome_cases(version,manual_hold)`、`welcome_artifact_bindings(revoked_at)`、`welcome_actions(status,version)`、`work_items(status,claimed_by,locked_at,claim_expires_at)` 的列 UPDATE | welcome execution INSERT、全表 UPDATE                        |
| 本体、受众、决策 | 所需知识、目标和投影的 SELECT                                                                                                                                                               | `/api/foundation/*` 生产开放、其他 welcome/消息写入          |

列 UPDATE 仍允许该角色改任意可见行的这些列；应用 tenant 谓词、实际表授权与数据库信任边界须另验。

PG16 及以前的唯一备选是把 gateway 整表 UPDATE 限给不可登录锁 owner，仍不给 UI；已核生产 PG18.4，优先 MAINTAIN，迁移不加入版本状态机。

## 待批准的实施差分

下表是**下一轮候选**，不在本 PR 修改。相对当前 `origin/master`
已复核路径存在性：部署代码 8 条（改 5、新 3），测试 4 条（改 3、新 1），所属文档/manifest
4 条。#723 应用和迁移由其 owner 单独负责，不计入部署 8 条。

| 类型   | 精确路径                                                                                                                                                            | 目的                                                                                             |
| ------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| 新 3   | `runtime/nginx/templates/management-ui-http.conf.template`、`runtime/nginx/templates/management-ui-https.conf.template`、`deploy/runner/management-ui-lifecycle.sh` | 固定 HTTP→独立证书→HTTPS、503 与受控 root 准备；runner 不能越过 service 沙箱执行 root 站点动作。 |
| 改 2   | `deploy/sidecar/scripts/render-systemd-units.sh`、`deploy/runner/install-release-systemd-units.sh`                                                                  | 渲染/安装 disabled UI unit、固定 UID/env/不可变二进制与停止预算；T 不激活。                      |
| 改 2   | `deploy/runner/qintopia-agent-os-deploy-runner`、`deploy/runner/recover-release-lineage.sh`                                                                         | 保持原锁机制，在指针前调用 helper 的 quiesce/verify-closed，核同 invocation UNKNOWN 及关闭结果。 |
| 改 1   | `tools/deploy/build-deploy-bundle.mjs`                                                                                                                              | 将三条新增部署路径纳入显式 bundle sourceFiles。                                                  |
| 改测 3 | `tools/deploy/test-release-systemd-install.mjs`、`tools/deploy/test-deploy-runner-promotion.mjs`、`tools/deploy/test-deploy-runner-systemd-linux.mjs`               | 覆盖 disabled T 安装、继承锁、停机/UNKNOWN、补偿和恢复停点。                                     |
| 新测 1 | `runtime/nginx/tests/test-management-ui-route.mjs`                                                                                                                  | 定向验 HTTP 引导、HTTPS/503、错误 Host 和恢复；须明确执行，现有 Nginx 检查不会自动发现它。       |
| 所属 4 | `runtime/nginx/README.md`、`runtime/nginx/manifest.yaml`、`deploy/runner/README.md`、`docs/operations/production-deploy-runner.md`                                  | 记录站点所有权、一次性安装、授权、停止/回退及人工验收合同。                                      |

`promote-release.sh`、`rollback-release.sh`、`smoke-release.sh` 和 runner service
unit 复用现有入口，不在候选差分中。只有最终代码和 Linux/Nginx 定向验证显示具体缺口时，才重新审议路径数量；普通直接调用指针原语也不获得新授权。

## CI 九原则审阅

此 PR
**CI 接线 0**：不增加或修改 workflow、job、步骤、checker 入口/条件、依赖、配置或业务特例。未来实施仍要依据负责人规则另批具体范围。

1. **实际问题：** #723 提供 loopback
   UI，但现有 Release 没有正式 HTTPS 入口或指针前 UI 停止合同。
2. **数量：** 候选 8 部署代码、4 测试、4 所属文档/manifest；CI 变更 0。
3. **依据：** 现有 renderer/installer allowlist、runner
   quiesce、rollback 先写指针的顺序、recovery CAS、同 R
   bundle 的 T/R、Nginx/COS/DNS/TLS 只读观察及 #723 HTTP/UNKNOWN 行为。
4. **复杂度：**
   只设固定服务与单一固定动作 helper；复用现有锁、hold、请求和恢复入口，不增加 journal、队列或状态机。
5. **现有 CI：** 现有 runner fixture 可扩展；它尚未覆盖 UI
   unit/HTTPS/503。定向 Nginx 测试显式运行，不伪称现有 `runtime:nginx:check`
   或 CI 已覆盖。
6. **最小范围：** 同源前缀破坏隔离，临时 SSH
   forward 不满足日常使用；新管理应用、可配置任意 host/命令和新恢复协议无必要。固定独立 origin 加现有工作台足够。
7. **不设特例门禁：** 测试在既有部署测试内检查通用发布/回退行为，不加 UI 专属 CI
   job 或例外。
8. **部署相关：**
   DNS、TLS、unit 生命周期与指针前关闭直接决定 R/T 安全；PMS 业务排空仍由其现有流程负责。
9. **可复用性：**
   固定入口的锁继承、停机与关闭断言可服务后续受审管理服务；不开放任意单位、端口、路径或命令参数。

## 已有证据与待验

| 项目       | 当前证据                                                                                                                                                                                                                                   | 不能据此声称                                                                                  |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------- |
| DNS        | 用户已建 A；双权威 NS 与三个 DoH 答复一致，TTL 600，无观察到 AAAA/CNAME                                                                                                                                                                    | 证书、Nginx、服务或浏览器可用                                                                 |
| PostgreSQL | 生产应用库只读版本为 18.4；本地隔离 17.11 的真实受限角色验证：业务/消息越权 `42501`；临时单列 UPDATE 可使 Gateway/Person `FOR SHARE` 命中真实行，但 gateway SHARE 表锁仍 `42501`；临时 MAINTAIN 或整表 UPDATE 才能表锁；全部临时授权已撤销 | 五函数已安装、最终 ACL/tenant 隔离已验；四张 welcome UPDATE 探针影响 0 行，未证明真实状态转换 |
| 应用/迁移  | #723 owner 正在修正固定函数和 commit UNKNOWN；当前方案由其实施                                                                                                                                                                             | 最终迁移 SHA、SQLx 记账、P 执行 R bundle 的新增差分和真实受限 HTTP 全路径通过                 |
| 部署       | #726 runner 已在 `master`；现有 O/T/R 两步合同可复用                                                                                                                                                                                       | 本提案的 Linux systemd、Nginx、证书续期、503、真实浏览器或生产回退已执行                      |

冻结 #723 的新增迁移 SHA 是 P 新增差分/SQLx 兼容、全链验收和最终 R 制品身份的前置。

尚未冻结时，可在具体实施范围获批后独立编写模板、renderer 和 helper；该 SHA 不阻断这些代码的编写。

未变 NATS29 基线不重复。最终仍须以真实受限 UI
role 跑正反例和 tenant 边界，运行对应 Linux/systemd、Nginx、Certbot 与浏览器验收。任何 UNKNOWN 要回读原操作，不能因进程退出、自动重启或 503 就断言业务结果。正式生产动作另需具体对象、变量快照、通过条件、停点和受审回退；本报告不代替该批准。

## 本 PR 的验证

`pnpm exec prettier --check`、定向 Markdown lint 与暂存差分空白检查通过。
`pnpm check:pr:auto` 识别本 PR 仅两条文档路径并选择 quick
tier；其格式、Markdown 与已返回的前序契约用例通过，但在既有
`test-agent-runtime-management.mjs` 调用嵌套 deploy-runner 检查时，`spawnSync` 返回
`ETIMEDOUT`，所以整套 quick tier
**未通过**。超时原因未查明，本报告不把它记作应用或部署验收，也不据此修改 CI。首版
`7e36a132`
的远端 Light、汇总 check、changes 和 PR-Agent 已通过；本次文档修订须单独核对新 head 的适用检查，不能沿用旧 head 的结果。

## 本 PR 的边界

本 PR 仅增加本报告与报告索引。没有部署代码、测试、CI、迁移、凭据或生产写入；没有重建或叠加已 squash 的 #726 提交。

报告是下一轮 8+4+4 实施范围的审阅材料，负责人对普通 PR 的提交/推送授权不等于批准那些部署或 CI 修改。
