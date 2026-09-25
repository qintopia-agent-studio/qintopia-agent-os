# 岸岸生产接线核查与实施记录

日期：2026-09-25。当前 PR 为 #722，接手时远端 head 为
`d5a11ba5f979e1eeaed1d7cc256736dded4fd1ae`，base 为
`470c4505dff745a48d2967b884de25436da527c6`。本报告记录接手调查和候选实现，后续提交以该 PR 的当前 head 为准；不宣称工程收口或生产验收。

## 已核对的实际状态

完整 Linux 隔离模拟先出现两类失败：源码归档解包保留 macOS
UID/GID，在已降权容器内被拒，模拟编排改为不保留来源属主；之后 `production.require()`
正确拒绝浏览器配置。 `linux-isolation-full-second.log` 和
`linux-isolation-full-third.log` 保留后者原始失败。核查官方 `d337b736` 的
`browser_tool._browser_cfg` 与 `read_raw_config`
后确认：浏览器部分路径读取 Profile 原始配置，不应用受管覆盖。

候选安装方案因此须仅将已审阅的浏览器字段与受管策略对齐，保留其余 Profile 数据；插件预检同时检查两处、实际路由及容器，任何不一致均拒绝执行。

当前只调整一次性模拟 Profile，没有修改生产配置或官方核心。修复后的完整模拟结果见下节，不能以先前离线路由或容器文件系统验证代替。

- PR #721 已合并；#722 为 Draft。d5a 的替代 CI 运行 `36110743764`
  中 Light、Runtime、聚合 check 通过；Rust
  quality 与 PostgreSQL 根据该 head 的改动范围跳过。business 和建议审查通过。旧被替代取消的 run 不作为当前结果，本批 Rust/Python 增量不沿用这些旧 head 的检查结论。
- 经用户提供的 SSH 连接只读核对，岸岸服务 active，入口为官方 v2026.9.21 解释器，Profile 为
  `/home/ubuntu/.hermes/profiles/anan`。未安装 PMS 插件，未发现三种 Token 环境键或私有凭据文件键，terminal 默认 local。
- Agent OS current 为
  `16e8d56b98001579c6288ba13199b80d6d3dfc74`。七 Profile 核心 current 与岸岸入口独立，不将其当作岸岸生产版本。
- PMS 公开版本接口返回
  `{"version":"1.7.2"}`；接口不提供提交或 worker 版本，不据此证明主动投递功能已经部署。
- Docker 系统服务存在，但岸岸运行用户直接访问 daemon 失败。生产隔离未就绪，不把客户端存在或服务 active 视为可用。

## 验证与已知缺口

独立源码复核发现收款补拉与申请回读宿主入口在检查隔离前就打开私有文件。已将两处读取前置于共用
`production.require()`
之后，申请私有配置路径额外加入实际容器挂载核查。没有增加宿主豁免：这两个生产入口仍依赖同一受管岸岸运行条件；Bridge 条件不齐时保留待回读事项，不读取私有配置或连接外部服务。

修复后 PMS 包 92 项、Workflow 24 项通过，原始日志为
`pms-package-host-guard-verified.log` 与
`workflow-host-guard-final.log`。新增反例确认隔离失败时文件读取、客户端构造及连接均未发生；模拟元数据覆盖申请密钥藏在原本允许的凭据目录挂载内时必须拒绝。

完整 Linux 模拟也按更新源码重跑通过，见
`linux-isolation-host-guard-verified.log`：实际官方插件及
`production.require(private_paths=...)`
接线可运行，terminal、execute_code 和文件工具均不能读取第二份宿主私有文件，停止工具容器后拒绝且不回退宿主。首次新增探针错误假定存在
`/workspace` 绑定挂载，实际没有，原 `linux-isolation-host-guard-final.log`
的失败保留；最终改为真实私有文件可见性验证，挂载负例由独立模拟元数据场景覆盖。这仍是 UID
0、无真实凭据、无模型/业务渠道的隔离模拟；不证明生产启动时序、运行期配置保护或生产 Bridge 安装已经完成。

冻结候选的 `pnpm check:pr:auto` 已实际运行，原始日志为
`.local-workspace/anan/production/auto-current-frozen.log`。使用 Python
3.12、任务独占 Rust 工具链及
`127.0.0.1:55498/qintopia_test`；轻量检查、默认与全部特性 Rust 回归、严格 Clippy 及此前逐项数据库场景通过。

随后 `person_collaboration --include-ignored --test-threads=1`
为 143 项通过、3 项失败，总检查退出 1。失败为申请 Bridge、提醒 Python 宿主、欢迎联系人 Unix
broker 三个跨进程场景；欢迎子进程明确报
`stay_contacts_mode_required`。原因已定位为新增申请场景在同一进程设置七个环境键，影响后续模拟子进程。

按既有包内模式将该场景放入独立子进程，并要求退出成功且明确执行通过一项，防止 exact 选择器漂移后零项假通过；保持生产模式互斥，不修改共享框架或 CI。

格式化后重新运行完整 `person_collaboration`
组，146 项全部通过，原三项失败均恢复。权威原始日志为
`auto-current-final-formatted.log`；较早的 `auto-current-isolated.log`
是增加子进程项数检查前的通过记录。修复后
`cargo fmt --check`、带 PostgreSQL 特性的测试目标严格 Clippy 和差分检查通过。这是对失败组的完整重跑，不将其文件名或分段验证解释为整条
`check:pr:auto` 已退出成功。

该轮尚未到最终 apply-smoke，不能把本轮失败归因于历史 URL 白名单。新补登的 8 个已有 Rust 场景沿用业务清单与 exact 选择器；harness
8 项通过，名称全部匹配实际编译出的测试。官方核心及跨语言显式探针仍单独执行，不伪装成默认清单覆盖。

总指挥随后接续原检查尚未执行的两步，仍使用任务独占 `55498`：
`resident_welcome --include-ignored --test-threads=1` 为 24 项通过；
`operations-control-plane-apply-smoke.sh` 退出 1，首个错误为
`database URL hash is not in the reviewed allowlist`，其后的 JSON 解析错误是空输出的连带结果。日志分别为
`resident-welcome-current-final.log` 与
`apply-smoke-current-final.log`，准确命令和退出码保存于
`remaining-auto-results.json`。未改白名单或 CI、未使用其他任务的 5432 数据库；本地完整 auto 仍不能记为通过。远端须核对本批新提交在原 CI 固定隔离库上的实际结果，旧 head 的通过不覆盖本批代码。

PMS 包运行
`python3 -m unittest discover -s skills/pms-operations/tests -q`，主任务前一阶段保存日志为 71 项通过。申请/联系人切片完成后，岸岸任务回报最新包级 79 项及
`workflows/silaoshi-daily-ops/tests`
24 项通过；该片工具输出未另存原始日志，交接准确标明此限制。总指挥核对其 8 个文件哈希全部匹配。范围包含固定 HTTPS 客户端、私有凭据文件、候选生产模式、付款补拉、飞书只读回读、Bridge、逐人电话宿主及原确认恢复契约；不包含真实生产办理。

普通入站会话不再因 PMS 证据捕获暂时失败而被整体跳过；业务工具仍由共同服务逐条核验当前可信消息。另修正候选隔离策略遗漏官方实际
`cronjob_manage`
工具名的问题。插件移除旧“只支持人工发起”的客户端范围限制，未接线能力仍由 live
broker 拒绝；针对性模拟确认被拒的事件办理、关联和提醒暂缓不会触发 PMS 调用。

官方核心 `d337b736aa1e8ebecfab043842d13e4a2d2f48a3`
的实际 PluginManager 已在本地进程加载插件。通过官方 Docker
backend 运行隔离模拟：terminal、execute_code、文件工具读取不到虚构密钥及 broker
socket，Skill 环境声明不获得这些密钥，越界及软链接凭据挂载被拒绝，普通文件读写通过，Docker 停止后没有回退宿主执行。

证据位于工作树忽略目录
`.local-workspace/anan/isolation-docker-9/evidence.json`。模拟镜像是
`python@sha256:2f17fc044b579bab302c2e8054d3a686e2cb9a83de48e70534b94cd8ebbe06a9`，只用于该模拟，不是已选定的生产工具镜像。前期脚本调试失败记录保留；不称生产失败，也不隐去最终验证的范围限制。

追加源码核查修正了生产策略对官方媒体策略函数名的引用。候选策略仍需真实 Linux 安装条件、受管配置、有效工具全集与生产进程接线验证，不能以局部模拟代替完整隔离结论。

追加离线探针确认官方 `browser_cdp` 的 `DOM.setFileInputFiles`
在公开页面会把指定文件路径交给浏览器传输。探针替换传输为记录器，没有启动浏览器、读取文件或联网；它证明当前策略未阻断该路由，不证明实际凭据已泄漏。记录位于
`.local-workspace/anan/production/browser-boundary-probe.log`。生产浏览器的文件系统隔离仍是缺口，不能以私有文件名
`.env` 或 terminal Docker 结果覆盖。

浏览器候选检查现已接入 `production.require()`：受管配置固定 loopback
CDP 与不可变镜像，实际 Docker 容器需无宿主挂载、无额外环境变量，且唯一发布端口与声明一致；官方有效浏览器配置和缓存会话也须匹配。

未配置该边界时，生产调用在读取私有凭据前拒绝。定点 `test_production.py`
为 9 项通过，包含浏览器新增的配置、端口、挂载、镜像和改名密钥拒绝场景。

另以官方核心实际配置、会话和 CDP 路由运行离线模拟，Docker 元数据、HTTP
discovery 和 CDP 传输由模拟边界提供；运行时改道、旧宿主会话、停容器、异端口发现及发现失败均拒绝，未退回宿主。

原始输出在
`.local-workspace/anan/production/browser-routing-final.log`。未启动浏览器、未联网，也未验证真实浏览器文件系统或 Linux 全量生产条件，不能将此结果称为完整隔离通过。

后续真实容器模拟补齐了文件系统证据：固定
`chromedp/headless-shell@sha256:2d349b544a1ea6b5b5fd7c0fe99215ff662339c57407ee2e8c0a11af93516b04`
启动 Chromium，核对实际进程、Docker 元数据及 loopback
CDP 发现。容器无宿主挂载、根文件系统只读，宿主模拟私有文件、broker socket 和 Docker
socket 均不可见；停容器后策略拒绝。

首次因本机 Docker 的 `top -eo comm` 不支持而失败，原日志保留为
`browser-container-filesystem.log`；改为读取容器内 `/proc/1/comm` 后通过，见
`browser-container-filesystem-final.log`。本次没有页面自动操作、真实凭据或业务联网，不是 Codex
Chrome 页面验收；该镜像尚未定为生产制品。

生产策略另修复受管配置父目录可被替换的缺口：从根到叶检查 root 所有者、不可写父目录及真实文件类型，私有配置及所在目录不允许其他用户读取。

隔离 Linux 容器中的实际文件系统模拟通过，拒绝可写父目录、公开目录/文件、组可写配置、目录/文件符号链接、硬链接及非普通文件；记录为
`managed-policy-filesystem.log`。当前策略单元 9 项通过，记录为
`production-policy-current.log`。这些早期证据不代替完整 Linux Gateway 的联合验证。

后续 `linux-isolation-full-fourth.log` 退出 0：一次性 Linux
Gateway 进程实际发现并加载插件，在 production 开关、私有虚构凭据、真实 Unix
socket、官方 Docker 工具和独立 Chromium 容器条件下运行完整
`production.require()`。只写受管浏览器配置而不对齐 Profile 的反例被拒；对齐后通过。terminal、execute_code、文件工具和 Skill 挂载无法取得私有文件、broker、凭据环境或 Docker
socket；普通文件读写通过，停止工具容器后拒绝且无宿主回退。本次模拟 Gateway
UID 为 0，不替代生产 ubuntu 用户及其 daemon 权限安装验收；无模型、真实凭据、PMS 业务调用或渠道发送。镜像只用于模拟，未选定生产制品。相关源码快照记录在
`linux-isolation-source-*.json`；旧失败日志均保留。配置核验补充后的 PMS 默认套件
`pms-package-browser-alignment.log` 为 88 项通过。

### MCP 与插件加载的独立审阅

只读审阅发现官方 MCP stdio 不走 terminal Docker，native 配置、portable
plugin 和已注册 MCP 工具均可能保留宿主能力。生产只读清点为岸岸 native MCP
0 个、Profile 插件目录 0 项；全局两个插件未见 portable
MCP 声明文件。该清点没有加载插件、启动 MCP 或读取凭据，不能代替实际运行工具集合验收，安全摘要保留在
`mcp-readonly-inventory.json`。

候选 `production.py` 现检查合并配置、已加载 portable
MCP 和当前 Profile 加全局注册表；发现未经核验的 MCP 则在读取凭据前拒绝，不删除或改写任何配置。`pms-package-mcp-final.log`
为 89 项通过，`linux-isolation-mcp-final.log`
退出 0，覆盖宿主 MCP 配置和移除配置后仍有 MCP 工具注册的反例，以及完整 Linux 隔离正例。`workflow-current.log`
为 24 项通过。

审阅还确认官方插件 import/register 失败会记录错误并继续运行，缺失 hook 不会自动封锁工具。因此生产启动前仍须独立检查固定插件文件身份、加载成功及实际 hook/工具登记；该预检必须在真实凭据注入和通道启动前完成，不能仅依赖
`production.guard`。当前未安装该启动接线，本地模拟通过不关闭这项生产缺口。

后续独立复核还确认 `gateway/run_profile_reconcile.py`
的配置变化会自动连接 MCP；启动的宿主进程可能先于下一次 `require()`
接触文件。未注册工具的已启动进程也不在当前注册表内。因此 MCP 拒绝结果不是宿主进程级隔离，必须结合不可变配置、固定插件身份和实际 Gateway 启动顺序。只运行一次独立预检不能覆盖后续配置变化。

进一步只读复核记录了官方强制 reload 先卸载 hook、宿主 shell hooks 和存活 host
PID 恢复路径。受管空 MCP 字典不会删除用户新增项；同名插件也可能由后发现的来源覆盖。

这些源码事实要求生产启动方案覆盖配置和代码来源的持续约束，不能只加独立预检后宣告完成。当前专项启动器和 service
drop-in 仍是待验证草案，未实施、未获完整隔离结论。

回退核对另发现 `.github/workflows/rollback-production.yml` 未接受已存在的
`hermes-anan`，且“全部”展开也遗漏岸岸。两文件最小修复的九项评审已写入生产接入计划，已请求负责人确认；批准前保持 workflow/检查器原样，不能称已有可执行的完整回退路径。

### 官方企微未知回执阻断

对同一官方核心执行现有 `runtime/hermes/check_wecom_uncertain_send.py`，模拟结果为
`wecom_stale_request_clear=passed`、
`wecom_uncertain_send=blocked_duplicate_effect_possible`，退出码 1。模拟传输先接受被动回复、再丢失回执；官方实现随后主动发送，形成两次外部效果。没有使用生产凭据、连接真实渠道或发送真实消息。

源码核查发现公开 `send` 接受
`metadata.force_proactive_send`，但对已识别群聊仍使用被动回复；因此该参数尚不能证明群通知安全。继续针对公开入口验证单聊、群聊和无可用回复引用三种情况，不修改官方核心、不降低原探针断言，也不把单聊结果外推为群聊验收。

公开入口模拟已执行：普通单聊在回执丢失后发送两次；显式主动单聊只尝试一次并保留超时；已识别群聊即使设置上述参数仍发送两次；无可用回复引用的群聊拒绝发送。证据在忽略目录
`.local-workspace/anan/production/wecom-public-send-probe.json`。当前没有经验证、保持原群目标且只尝试一次的官方发送路径；不能自动换成私聊、其他 Bot 或渠道。

业务接收、Inbox 和 WorkItem 可继续推进；自动群通知在解决此兼容缺口前不能标记可上线。

只读核对最新官方 release `v2026.9.24`：其 `send/_send_inner`
源码片段与上述被验证版本完全一致。源码证据为
`.local-workspace/anan/production/wecom-upstream-comparison.json`；这不是新版本运行验收。已向负责人提出首批由岸岸私聊指定有权办理人的具体替代建议，或保留首批工作群要求；决定前不改变通知对象。

## 业务范围纠正与后续责任

负责人最新明确 PMS
worker 新收款主动通知以及申请、提醒、欢迎都要上线，取代接手提示中的阶段性关闭范围。已与原[支付交接](../plans/active/green-pms-payment-events-handoff.md)及[业务计划](../plans/active/anan-pms-event-integration.md)核对：PMS
→ Sidecar 签名接收、落盘及分派 → 岸岸沟通 → 当笔有权人确认 → PMS 办理和业务回执。

接手基线中的 Sidecar 付款路径只挂在显式模拟监听器；本批已增加独立生产 listener，并接入原 Run 的可选 broker 生命周期，默认关闭。持久唤醒及真实渠道发送尚未接通。

不能写成 PMS 直接向岸岸发聊天，也不能把接收 202 当作人已收到通知或收款已登记。

申请仍来自飞书统一 Bridge；提醒依有效规则；欢迎沿既有身份、住宿、阿靓制卡和二花审核传递流程。

共享 live 身份、授权、可选 broker 与运维配置入口由既有基础任务实现；主任务负责插件、隔离、整合与发布准备；既有岸岸任务已完成付款生产 listener、live
Inbox/WorkItem 元数据和私有文件补拉切片，公共 Run/broker 接线已由基础任务整合。

基础任务交付的 `backend-handoff.md`
为工具输出重建记录，没有原始 Cargo 日志；回报在其独占 55476 隔离 PostgreSQL 中验证 live 管理权限、未捕获消息拒绝、支付事项上下文、UNKNOWN 原键恢复、付款配置和 HTTP 接收，并通过 cargo
check、Clippy 和定点 rustfmt。总指挥逐项核对 12 个源码哈希，其中 11 个仍匹配；
`foundation_server.rs` 已进入下一片申请接线，不能沿用旧哈希结论。该模拟库已移除。

交接中两项非 ignored 测试曾误记为 `-- --ignored`
运行通过，任务已确认更正；总指挥随后核对 `foundation-broker-unit.log` 与
`live-broker-allowlist-unit.log`，正确补跑分别为 2 项和 1 项通过。旧名称的付款测试不代替当前 HTTP 测试覆盖。

申请 live 集成模拟的最终原始输出 `application-live-gate-final.log`
为 1 项通过，原申请模拟回归 `application-simulated-regression.log` 为 9 项通过。当前
`application-live-gate.log`
保留较早的失败，不能将该文件记为通过。手机号回归曾因新增 employee 条件误拒既有模拟工作账号而失败；修复后定点重跑
`welcome-contacts-broker-regression.log`
为 1 项通过。先前整组 8 通过、1 失败的日志保留，不把定点补跑写成新一轮整组 9 项通过。

总指挥审阅还发现原 Unix
broker 退出后残留 socket 会阻止下一次启动；基础任务已补归属、存活与独占校验、正常退出清理和崩溃残留恢复。总指挥核对
`foundation-socket-lifecycle-final.log` 和
`foundation-broker-restart-final.log`，各 1 项通过，覆盖残留恢复、活跃第二实例拒绝及实际 broker 任务取消后重启。

最终锁文件使用显式 `.truncate(false)`
后 Clippy 通过，原警告失败和第一次短名称 exact 过滤导致 0 项的情况均在交接注明。

总指挥逐项核对该片最终 12 个源码 SHA256 全部匹配，并核读最终 Clippy 原始日志；这不扩大为本批全部未提交源码的全量验证。原申请 9 项回归使用较早源码，未称为最终整组重跑。

申请切片沿原状态、CAS、私有投影和逐人手机号比较接 live 入口：基础任务唯一修改 Rust；岸岸任务已完成并冻结申请、Bridge、`stay_contacts_host.py`
及相关模拟验证。生产配置键已对齐，callback 只唤醒回读，不能指定任意源、撤回事实或业务权限。

普通增员 CLI 已补候选实现，复用 Person 草稿、可信来源观察和原身份确认交易。真实 HOST_TOKEN 的未知账号观察只生成
`wecom-host` pending 来源，标记
`ingress_auth_verified=false`，不冒充已鉴权 NATS 消息、不保存正文，不授予业务权限。
`payment-workitem-read-final.log` 与 `person-cli-final.log`
原始记录分别 1 项通过；同源码又在任务自己的隔离 PostgreSQL
55498 定点重跑，各 1 项通过，准确命令和 SHA256 已补入
`backend-handoff.md`。人员身份确认、任职与权限配置继续分别办理，撤销账号不会因再次观察复活。

保持人员身份确认与业务权限授予分别办理，不能伪造 UI
session 或以 SQL 自授权。首位管理员的具体身份和一次性引导契约仍待负责人确认。

已再次核对 Green PMS `origin/main` 的
`packages/db/src/integration-worker.ts`：目标路径固定为
`/api/v1/ingress/pms/events`，请求方法、路径、时间、delivery
ID 及正文摘要均参与签名；发送端核对 event ID、accepted/duplicate 与 receipt ID。Agent
OS 的接收侧先提交 Inbox/WorkItem 事务，再返回该回执。既有设计没有 PMS 直接调用岸岸模型的另一条主路径。

岸岸任务完成官方 Generic
webhook 只读调查。工具集配置会自动并入其他插件及 MCP，不能单靠路由列表声称只读。技术方案使用现有插件入站钩子关闭事件的网关控制权限，并在实际工具调用前只允许独立待办只读工具。

broker 对固定 binding、gateway、tenant 和原 WorkItem 另作范围校验。202 不等于回合完成或通知到人。Python 只读 webhook 切片已完成本地模拟：默认套件 88 项、显式官方 HTTP/hook
8 项通过。官方独立服务的 bare route 只接受缺省/default Profile；原 `profile: anan`
配置返回 404，已修为缺省 route 与插件真实 `anan`
核验。官方 handler 回归保留坏签名 401、旧 route
404、合法请求 202，以及 ContextVar 和工具硬拒绝验证；证据为
`workitem-official-http-hooks.log`。没有配置或调用生产 webhook，没有新增投递账本或以轮询代替主动推送。

后续 Sidecar 提交后发送已完成：保留公共 PMS ACK，随后固定 loopback、独立 HMAC
V2 密钥、仅版本与 WorkItem UUID 的正文及总计 2 秒上限；返回须为相同 delivery
ID 的 accepted/duplicate。 `payment-postcommit-wake-final.log`
为 1 项通过，覆盖签名、ACK、范围、非催办事件与失败边界。

同一测试启用 `workitem_broker_journey.py`
后，`payment-postcommit-official-joint-final.log` 为 1 项通过：实际 Rust sender
→ 官方 HTTP handler → 入站 hook/ContextVar → 原插件私有凭据加载和 Unix broker
→ 同一 WorkItem 六字段投影。使用独占隔离 PostgreSQL `127.0.0.1:55498/qintopia_test`
及固定官方核心
`d337b736aa1e8ebecfab043842d13e4a2d2f48a3`；准确命令和源码校验另在忽略交接中保留。

联合模拟明确替代安装与隔离前提，无模型、渠道发送或 PMS 业务写入；不能与独立 Linux 隔离结果合称生产全链验收。

持久待办恢复触发、实际通知及人类收件仍未完成，内部 HTTP 成功不更新这些业务状态。

Webhook 密钥不能放进可被 Skill 转发的环境变量，实际注入须使用受管私有配置；Hermes 运行日志与会话继续留在受限 Profile，不导出私人模型输出到工程交接。

没有新增任务或重复业务系统，未修改 CI 门禁。

宿主 cron 脚本与 browser_use
Python 的功能限定仍待负责人决定。首批人员和权限由负责人后续定稿；生产凭据、部署、真实发送及业务写入尚未执行。
