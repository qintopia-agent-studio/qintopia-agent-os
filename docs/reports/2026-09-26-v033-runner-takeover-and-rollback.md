# v0.3.3 Runner 首次接管与回退

日期：2026-09-26。状态：技术方案待负责人评审；本报告不批准生产请求。

## 依据与现状

已读 `README.md`、`AGENTS.md`、`docs/README.md`、当前 roadmap、change
routing、programming guardrails、server change policy、`deploy/runner/README.md`、
`docs/operations/production-deploy-runner.md`、相关 workflow、runner、schema、restart
rules、检查器与历史 bootstrap 报告。只读参考并行工作树的岸岸生产核查报告及
`anan-production-rollout.md`，不在本分支修改它们。生产仅用管理员给定 SSH 做脱敏只读核对。

`v0.3.3` 已发布，精确 SHA 为 `62f403e3c5e615c10b6a0577954626664fcb22db`。GitHub 部署 run
`36206145717` 的构建与产物上传/核验成功，签名请求 `deploy-20260926T005852Z-62f403e3c5e6`
已发送，等待回执步骤失败。旧 runner 在提升前拒绝六目标请求，固定错误为
`invalid deploy request: restart_targets contains unsupported or duplicate entries`。六项目标没有重复；旧 runner 只缺
`hermes-anan`。该请求不能重试，也不能删掉岸岸目标当作 v0.3.3 已部署。

2026-09-26 只读服务器复核：`current=16e8d56b98001579c6288ba13199b80d6d3dfc74`，
`previous=83d694f2c3bc21fd78a73d25da3197379e2a14d5`，目标 `62f403e`
目录不存在。当前 manifest 为 `release_sha=commit_sha=deploy_bundle_sha=16e8d56b`、
`runtime_sha=83d694f2`、`previous_sha=83d694f2`；实际主 sidecar 制品为当前获准的三项 Huabaosi 特征，QiWe
companion 存在。这是既有混合版本契约，不是漂移。当前 runner 与 smoke 均不含
`hermes-anan`。原回执记载未提升且无需回退。

## 首次接管路径

候选为两次**不同身份**的签名请求，不能重放失败请求。第一步只安装能认识新目标且具备精确前次树回退事务的受审修复 bundle；第二步才以完整六目标部署该修复 Release。由于 v0.3.3 不含此回退事务，不能让它作为第二步的最终
`current`，否则会再次失去可执行的手动回退入口。它的失败需由后续修复 Release 接替，不能声称 v0.3.3 原请求成功。下文以
`R` 表示批准后生成并审阅的修复提交/制品 SHA；在 `R`
固定前不得发生产请求。前提是负责人分别批准精确生产请求，并在第一步前核对最新可信成功回执、当前/前次指针、旧 runner、COS 中
`83d694f2` 主/伴随运行制品与 `R`
bundle 的签名和摘要。只读 COS 核对只证明 O/P 相关请求、回执及旧产物对象存在；旧产物的内容摘要尚未下载核验，R 产物尚不存在。P 对应回执为失败，不能把对象存在或旧树指针当作成功部署证明。

1. 固定过渡候选：`commit_sha=16e8d56b98001579c6288ba13199b80d6d3dfc74`，
   `runtime_sha=83d694f2c3bc21fd78a73d25da3197379e2a14d5`， `deploy_bundle_sha=R`，
   `release_sha=70e7984fab92ddab956009585212d0e9729767b5`（独立祖先提交，只作不可变过渡目录身份，服务器上目前不存在），
   `release_scope=[deploy-bundle]`，`restart_targets=[qintopia-system-services]`，
   `rollback_on_smoke_failure=true`。这是单独的 runner 接管事务，不是缩减 v0.3.3 的目标。不能标记为
   `legacy_runner_bootstrap`：该 Huabaosi 特征兼容模式要求最近可信成功回执的四个 SHA 相同，且已有批准不涵盖岸岸首次接管。
2. 用现有 `workflow_dispatch` 从 `master` 先发 `dry_run=true`，比对签名请求与
   `dry_run_succeeded` 回执的全部身份字段、目标、范围和
   `promoted_current=false`；任何不符即停。独立批准后用相同固定元组发新的
   `dry_run=false` 请求，要求
   `succeeded`、`current=70e7984`、`previous=16e8d56b`、新 runner 白名单含岸岸。失败时只接受经核验的自动回退回执与原指针；结果未知不得重放。
3. 对修复 Release 的完整六目标元组另发**新的**正常请求，先 dry run 后 live，
   `commit_sha=runtime_sha=deploy_bundle_sha=release_sha=R`，scope 依发布契约，六目标仍为
   `qintopia-system-services,hermes-erhua,hermes-xiaoman,hermes-silaoshi,hermes-huabaosi,hermes-anan`。live 后应为
   `current=R`、`previous=70e7984`。岸岸排空须取得新鲜零工作快照，最多等待 600 秒；超时暂缓并取消本次排空，不强杀、不补跑。

过渡 dry
run 尚未实际执行；第一步旧 runner 对新 bundle 的完整离线模拟、最新可信成功回执绑定及 COS 原产物可用性仍须验证。因此以上是待审执行路径，不是就绪结论。
`legacy_runner_bootstrap` 既有约束不得放宽或改名借用。

## 回退缺口与约束

现有 `.github/workflows/rollback-production.yml` 的选项、单目标规范化和
`all-hermes-and-system` 展开都缺
`hermes-anan`。另外回退 workflow 假定前次 release 的四个制品身份都等于发布 tag
SHA；上述过渡 release 的身份刻意混合，且 `70e7984`
不是发布 tag。修复 Release 成功后，选择一个旧 tag 既不匹配服务器的
`previous=70e7984`，也不能验证过渡产物。仅补岸岸选项不足以提供可执行回退。

首次接管前必须形成并模拟**精确前次过渡 release**
的正式签名回退入口：它从可信成功回执读取完整过渡元组，并与服务器
`current=R`、`previous=70e7984` 及 manifest `previous_sha` 双向核对；下载并验证
`83d694f2` 主/伴随 runtime 与 `R` bundle；使用原请求 schema 中的 `release_rollback`
精确谱系约束。现有 runner 对 `release_rollback` 还要求四个身份 SHA 都等于 previous
SHA；修复只能在此回退分支内改为核对 previous
manifest 的完整身份，普通请求仍保持原约束。现有 `promote-release.sh`
的同 SHA 复用会要求请求目标集合与过渡 manifest 的单目标集合相同；因此修复 runner 需在回退请求中校验已存在的不可变 previous 树及其精确元组后，调用既有
`rollback-release.sh`，并用回退请求的完整目标集进行排空和 smoke。这一分支只接受服务器的精确 previous，不重建或改写该树，也不放松普通同 SHA 复用。先 dry
run，后经独立批准 live。目标集包含岸岸，使用
**当前新 runner**；不得调用旧 runner 处理岸岸。若此入口未实现并验证，停止在过渡 dry
run，不进行 live 接管。回退过渡 release 到 `16e8d56b`
也须基于其精确 manifest 和受审路径，不能把服务 active 当作回退成功。

### 输入和可信身份

现有两个 tag 输入均为必填，这对未发布 tag 的 T 和现有 O 都不可用。建议将
`expected_current_release_tag` 与 `release_tag` 改为可选，增加
`expected_current_deploy_request_id`、`previous_deploy_request_id`
两个可选输入。每一侧必须恰好提供 tag 或请求 ID 一种；原有双 tag 路径继续要求两个已发布、非 prerelease、可从
`origin/master`
到达的不同 Release，原有产物校验保持适用。请求 ID 路径只接受 COS 中原签名请求及签名成功回执，校验 HMAC、key
id、
`request_id`、`environment=production`、`status=succeeded`、完整四 SHA、profile、scope、targets 和两者一致性。GitHub 日志、workflow
success 和未签名摘录不充当身份来源。workflow 仍用生产环境审批生成新的签名回退请求；服务器再次检查
`current`/`previous` 及 current manifest 的 `previous_sha`。

R→T 的输入是 current 的已发布修复 Release
tag 加 T 的成功请求 ID；T→O 的输入是 T 和 O 各自的成功请求 ID。O 当前 manifest 记录的请求 ID 为
`deploy-20260916T073656Z-16e8d56b9800`，P 的为
`deploy-20260916T062333Z-83d694f2c3bc`。只读 COS 内存核对已证明 O 的请求和回执签名有效，身份相符、状态
`succeeded`，并与 O manifest 的四 SHA 和 `previous_sha=P`
相符。P 的请求和回执同样验签有效、身份相符，但状态为
`failed`；它不能充当成功部署凭证。T→O 的 previous 身份使用 O 的成功回执，P 只作为要恢复的旧树指针，须验证其目录、manifest 和 installer。T 的请求 ID 要在过渡成功后从回执固定。

既有 `wait-deploy-result.sh`
已验证结果签名、请求 ID、环境与完整元组，但面向新请求等待并将结果全文输出。建议扩展其现有验证逻辑为只读历史验证/固定字段输出，另验证归档请求签名；工作流从 COS 固定键读取请求和回执。这样复用 HMAC 合同，不新建第二份解析器或把旧日志当证据。

恢复模式与正常回退模式互斥：提供 `recover_request_id` 时不得提供任一 current 或 previous
tag 或请求 ID；普通回退则不得提供
`recover_request_id`，并要求 current/previous 两侧各自恰选一种身份。恢复请求只引用未终结、已持久记录的原回退事务；恢复目标从该记录与已验签原请求决定，不接受新请求另填目标 SHA。

### 指针和 runner 来源

`O=16e8d56b`，`P=83d694f2`，`T=70e7984`，`R=待固定的修复 SHA`。 `rollback-release.sh`
默认把原 current 置为 previous，会导致新 current 的 manifest.previous_sha 与指针不一致。R→T 必须使用
`--expected-current-sha R --expected-previous-sha T --restore-previous-sha O`；T→O 必须使用
`--expected-current-sha T --expected-previous-sha O --restore-previous-sha P`。所有参数先由签名请求和不可变 manifest 交叉核对。

| 阶段        | current / previous | current manifest 的 previous_sha | 本次处理 runner / 后续 poller runner                      |
| ----------- | ------------------ | -------------------------------- | --------------------------------------------------------- |
| 原基线      | O / P              | P                                | O / O，均不支持岸岸                                       |
| 过渡 T 成功 | T / O              | O                                | O / T；T bundle 为 R，后续支持岸岸                        |
| 修复 R 成功 | R / T              | T                                | T / R，均支持岸岸                                         |
| R→T 成功    | T / O              | O                                | R / T，均支持岸岸                                         |
| T→O 成功    | O / P              | P                                | T / O；本次仍由 T 完成岸岸排空和 smoke，后续 O 不支持岸岸 |

`rollback-from` 由原 primitive 留作原 current 指针证据，不能单凭它声称服务恢复。

每次切换后的 installer 和 smoke 都必须成功，且回执签名、精确指针与 manifest 一致。P 的目录、manifest 或 installer 缺失时不得启动 T→O。

### 固定 T 恢复入口

普通 timer 的静态 service `ExecStart` 指向
`current/deploy/runner/poll-deploy-requests.sh`，poller 默认也从 `current`
选 runner。T→O 一旦切换 current，下一轮普通启动便加载不认识恢复协议的 O。仅用
`QINTOPIA_DEPLOY_RUNNER_BIN`
覆盖 runner 仍不够：O 的 poller 可能在签名请求抵达前拒绝新字段，且直接从 SSH 启动 T
poller 会绕开 service 的 systemd 沙箱。因此进程中断后的恢复必须有固定于不可变 T
bundle 的一次性入口，不能依赖 `current` 或普通 timer。

优先复用现有精确制品、poller、runner、COS 指针及
`deploy.lock`：在 T 已完整安装并核验时，由受审的 release 内固定 wrapper 调用
`systemd-run` 创建一次性 root system service， `ExecStart` 钉住 T 的 poller，环境中的
`QINTOPIA_DEPLOY_RUNNER_BIN`
钉住 T 的 runner。单元属性逐项继承并核对已安装常规 service 的
`User`、`Group`、`StateDirectory`、
`WorkingDirectory`、`NoNewPrivileges`、`PrivateTmp`、`ProtectHome`、`ProtectSystem`、
`ReadWritePaths`
及固定 COS/state 环境；任何属性不等价、路径不属于已验摘要的 T 树，或有效属性不能核对，都停止。wrapper 不接收任意可执行路径、unit 参数或目标 SHA；它仅接受新签名恢复 request
ID，核对本地未终结事务和 COS
pointer 恰好指向该 ID 后，由原 poller 做请求/回执一次性处理，由 T
runner 做 HMAC、锁、CAS 与状态恢复。不安装常驻新 service，不热改原 unit。该入口的 wrapper、单元属性比对、实际 systemd 隔离及中断测试属于待审新增实现，当前尚不存在。

恢复请求发布前，操作者在批准的维护窗口停止普通 deploy
timer，并确认普通 service 已退出、无正在处理的请求、无其他持锁进程；停止状态和原 enabled/active 状态写入恢复记录。

再发布新的签名恢复请求并启动固定 T 一次性单元。恢复期间持续核对 timer 未重新启动；installer 会复制 runner
unit 并
`daemon-reload`，所以每次 installer 后也必须复核。如 timer 重新活动或普通 service 启动，停止恢复并保留原始证据，不让两个 poller 竞争同一 COS
pointer。恢复成功、签名回执持久化且指针/manifest/installer/smoke 均核对后，按预检记录恢复原 timer 状态，并确认下一次普通运行只处理它理解的新请求。T→O 后 O
runner 不理解岸岸及恢复协议，必须在无新的部署请求期间完成审查，不能把普通 timer 已恢复解释为未来六目标请求可用。

只有 T 树本身可信、签名原请求和事务记录匹配、目标树及 installer 均可核验时，才允许使用该入口。O→T 首次接管若在 T 完整落盘前中断，固定 T 入口不可用；该段仍由旧 O
runner 执行，也没有新事务记录。须先用旧 runner 故障注入模拟各指针/installer/smoke 阶段，并证明可用现有受审精确制品恢复命令回到 O/P。

若既有命令不能覆盖某阶段，则在 R 中增加最小固定恢复能力并先审后执行 live。不能以本段 T→O 的恢复设计覆盖首次接管风险。

### 失败与中断

现有普通错误分支以新 current manifest 的 `previous_sha=original_current`
为前提，回到旧树后该前提不成立。精确 previous 回退要有专属事务分支，不能落入普通 promotion 错误分支。最小受审实现可在同一 root
runner lock 下持久记录
`request_id`、原指针、目标、restore-previous、签名请求摘要、已验 manifest/产物身份和阶段；每次阶段以临时文件、原子替换及文件和目录 fsync 落盘。恢复只接受原指针或此事务产生的有限状态，以指针、manifest、`rollback-from`、事务记录共同判定，不能从请求参数单独推断服务器状态。写
`rollback-from`、写 current、写 previous、installer、smoke、回执这六个阶段各自持久记录完成证据；每次指针替换前重新 CAS 核对预期指针及目录身份。

恢复原状态时先阻断普通 poller，再按事务记录恢复原 current/previous，复核原 current
manifest 的
`previous_sha`、运行原 installer 和完整目标 smoke，最后签名回执。CAS 冲突或结果未知均停，不猜测下一步。

| 中断/失败点                        | 可观察状态                                        | 恢复动作                                                                                                      |
| ---------------------------------- | ------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| 任何指针写前                       | R/T 或 T/O，目标不变；可能已排空系统服务          | 保留原指针，记录 failed；复核被停服务，不自动重发请求                                                         |
| `rollback-from` 后、current 写前   | 原指针及原 current 标记                           | 固定事务恢复例程核对后清理标记；不运行反向 rollback                                                           |
| current 写后、previous 写前        | R→T 时 T/T；T→O 时 O/O；有原 current 标记         | 停止新部署，固定恢复例程按 CAS 恢复原 current/previous，并 fsync；再装原 current 单元并 smoke                 |
| 两指针写后、installer/smoke 未确认 | 目标指针 T/O 或 O/P，原 current 标记仍在          | 固定恢复例程回到原 R/T 或 T/O，验证 installer/smoke；失败记 `rollback.failed`，保留证据并停止                 |
| installer/smoke 成功、回执缺失     | 目标指针、manifest、单元与 smoke 证据均须独立核对 | 查本地事务阶段和 COS 结果；若完成状态不明，停止、不重放，按新签名恢复请求选择恢复原状态或由负责人确认目标状态 |

同步失败可在原请求内执行一次上述恢复；进程中断后不能重放原请求。建议为未决事务增加一个可选
`recover_request_id`
workflow 输入和同一部署 schema 的固定 recovery 字段。它只能引用当前未决事务，走原生产审批、签名和 exact-CAS 校验，并由上述固定 T 入口消费。动作仅为恢复事务记录中的原指针和原单元，不允许任意 SHA、服务或命令。

恢复中再次失败则停止并保留固定回执，不循环回退。任何结果未知先只读核对本地事务状态、指针和签名回执；不把服务 active 或缺少回执推断为成功或失败。岸岸排空若在指针切换后的 smoke 中超时，原 helper 会取消自己创建的排空标记，不强杀也不重启岸岸；事务只尝试一次固定原状态恢复。恢复未确认成功时保留失败状态并停止，不把指针或服务单项状态解释为完整回退。

不得手移 `current`、覆盖单文件、热改服务器、跳过签名/制品/原门禁、改写岸岸 `v2026.9.21`
核心入口或真实 PMS 数据。Profile、模型、凭据、会话、记忆、任务与通道状态保留。部署源码不安装 PMS 插件，也不启用 Foundation/PMS/events 凭据与生产 flags；业务验收另行进行。

## CI 专项评审请求

本报告只提出一个相互依赖的专项方案，不要求分次批准。修改 workflow、checker、部署门禁前须负责人逐项批准，不把历史 bootstrap 扩大到岸岸。

当前候选范围为 11 个现有实现/测试文件、1 个新 wrapper，合计 12 个实现/测试文件；不新增 workflow、job、step 或第三方依赖。此数量是评审输入，尚未获批；首次 O→T 中断模拟若证明现有恢复命令不足，须说明所需增量并重新报审，不能沿用此数量实施。

1. `.github/workflows/rollback-production.yml`：改 2 个原必填 tag 输入，新增 3 个可选请求 ID/恢复输入；改现有 resolve、产物核验、请求生成 3 个步骤。
2. `deploy/runner/qintopia-agent-os-deploy-runner`：回退身份校验及精确 previous 事务/失败恢复；普通正向与同 SHA 路径不变。
3. `deploy/runner/rollback-release.sh`：现有 primitive 增加固定原状态恢复模式，只接受事务记录的 CAS 状态，不接受任意指针。
4. `deploy/runner/deploy-request.schema.json`：增加 1 个固定 recovery 对象约束。
5. `tools/deploy/create-deploy-request.mjs`：按经审批的 recovery 输入构造并签名该对象。
6. `deploy/runner/wait-deploy-result.sh`：复用现有 HMAC 校验，增加历史请求/回执验签和固定身份输出模式；原等待模式不变。
7. `tools/deploy/check-deploy-runner.mjs`：普通目标集合与回退 workflow 选项、单目标分支、全部展开一致；保护新双身份和 recovery 合同。
8. `tools/deploy/test-deploy-runner-promotion.mjs`：混合元组、R→T→O、各中断点、重复/漂移/失败恢复负例。
9. `tools/deploy/test-wait-deploy-result.mjs`：历史签名请求与回执匹配、篡改和缺失负例。
10. `deploy/runner/run-fixed-release-recovery.sh`（新文件）：固定 T 一次性 systemd 入口，核对请求 ID、T 树摘要、普通 timer 和 service 状态及有效沙箱属性；无任意命令参数。
11. `tools/deploy/build-deploy-bundle.mjs`：将上述 wrapper 纳入 R
    bundle 的固定文件清单与摘要。
12. `tools/deploy/test-deploy-runner-poller.mjs`：测试固定 T poller/runner 来源、COS
    pointer 身份、timer 竞争拒绝和一次性消费。

配套更新本报告及 `deploy/runner/README.md`、
`docs/operations/production-deploy-runner.md`
属文档，不计入上面的实现数量。恢复入口须先在隔离 systemd 环境中证实与原 service 的有效限制等价；若
`systemd-run`
无法满足，则本方案不具备生产可执行性，应重新评审入口，而非退化为 SSH 直调。

| 原则         | 回退普通目标一致性                                     | 精确前次混合版本回退                                                                                                                                                                                                  |
| ------------ | ------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1 实际问题   | 岸岸已在 runner/rules 中，正式回退选择器遗漏           | 过渡 release 无 tag 且运行/包/目录 SHA 不同，现入口无法回退                                                                                                                                                           |
| 2 数量       | 1 个目标选项、2 处原规则、1 条集合校验                 | 候选合计 11 个既有文件、1 个新 wrapper；新增 3 个输入、1 个 recovery 对象规则、1 个历史验签模式、1 个精确 previous 事务和 1 个固定恢复入口；修改 2 个输入必填属性及 3 个既有步骤；新增 0 workflow/job/step/第三方依赖 |
| 3 依据       | 当前选项/展开缺 `hermes-anan`，而 schema/runner 已接受 | 当前 manifest 混合版本，原 rollback workflow 将所有 SHA 固定为 tag                                                                                                                                                    |
| 4 复杂度     | 复用 `allowed_targets - independent_targets`           | 复用现有签名协议、COS 校验、谱系字段、poller 和 runner lock；仅中断恢复使用固定 T 的一次性 systemd 单元                                                                                                               |
| 5 现有 CI    | 不检查回退选项是否等于普通目标集合                     | 业务代码无法改变回退请求构造与正式入口                                                                                                                                                                                |
| 6 最小范围   | 只补普通集合一致性                                     | 仅接受已成功部署且恰为服务器前次的完整固定元组；普通同 SHA 校验与正向发布不变                                                                                                                                         |
| 7 无单项特例 | 集合规则不写岸岸专用豁免                               | 按精确前次 release 元组通用处理，不加岸岸专属放行                                                                                                                                                                     |
| 8 部署相关   | 仅部署回退目标                                         | 仅不可变 release 回退                                                                                                                                                                                                 |
| 9 可复用     | 后续普通目标变更自动发现遗漏                           | 后续合法混合版本前次 release 也可精确回退                                                                                                                                                                             |

原两文件提案只覆盖岸岸选项，不能以其完成作为上线条件。固定受审 bundle 过渡本身可复用现有正常
`workflow_dispatch`。既有 `rollback-release.sh --restore-previous-sha`
能处理完整原状态下的精确 T→O 指针切换，却无法在已写 current、尚未写 previous 的状态启动，也不会确认 installer、smoke 和回执。这就是新增专属事务、持久阶段和固定入口的理由。

若不随 bundle 部署这些回退能力，最终 R 的可执行回退仍缺失。

因此“需要 R”基于**可手动回到前次混合 T 且可继续回到 O**
的验收条件，不表示 R 已构建或获批。负责人确认上述具体范围后才实施；生产申请仍另行授权。

## 验证和交接

本报告变更仅文档。当前本地 HEAD/base 均为
`62f403e3c5e615c10b6a0577954626664fcb22db`，工作分支
`codex/v033-runner-takeover-plan`；远端 `master` 的只读查询也返回该 SHA。PR
721、722 和 release PR 718 均已合并，对应原 CI
check 成功；PR-Agent 留有自动评论，`reviews=[]`，不能视为人工批准。v0.3.3 发布 run
`36206145717` 的构建成功、request-deploy 失败。

当前离线证据：`node tools/deploy/test-release-systemd-rollback.mjs`
通过，实际在临时目录运行既有 primitive，证明 `--restore-previous-sha` 更新两指针、保留
`rollback-from` 并调用旧树 installer。 `node tools/deploy/test-wait-deploy-result.mjs`
通过，覆盖当前回执验签等待合同。 `node tools/deploy/test-deploy-runner-promotion.mjs`
通过，覆盖当前普通推广失败的回退分支，未覆盖本提案的混合 previous 事务。旧 runner 的白名单静态核对缺岸岸，新 runner 白名单与 smoke 均有岸岸；旧 runner 没有本提案的事务恢复能力。这些证据不等于混合过渡或新回退路径已通过模拟。

最终文档的 Prettier、Markdown lint 与 `git diff --check` 均通过。 `pnpm check:pr:quick`
已启动，经过格式、Markdown 和多个现有合同检查后，在无关的 Agent
runtime 管理 fixture 中因耗时中止；不能记为通过。

获批实施后须补齐并实际运行：

1. 用旧 runner 的隔离 fixture 验证原六目标失败、过渡单 system 目标通过；用新 runner 验证六目标和
   `hermes-anan` 的排空成功/600 秒超时暂缓。
2. 对双 tag、tag/request ID 混合、双 request
   ID 各路径验证；缺失/双填、无效签名、错误 key id、错误 request
   ID、环境、状态、四 SHA、scope、targets、COS 摘要、服务器 exact
   previous 均须在任何指针写前拒绝。
3. 以可抛故障 fixture 覆盖 R/T→T/O→O/P、`rollback-from`、两次指针写、installer、smoke、回执丢失及恢复失败；证明普通正向、同 SHA、双 tag 旧合同没有放宽，不会循环恢复或重放原请求。
4. 跑
   `pnpm deploy:runner:check`、`pnpm check:pr:auto`、格式/Markdown 检查，核对新 PR 的 head/base、所有 checks、CODEOWNERS 和 reviewer 建议。

生产预检需固定 SHA、原 manifest 四元组、COS 签名/摘要、当前和 previous 指针、两个历史成功回执、岸岸核心入口及零工作排空条件。先取得正式回退入口的 dry
run 和受审回执，再审过渡 dry
run；没有可执行回退就不做 live 过渡。每次 live 后核对新签名回执、指针、manifest、runner 来源和六目标 smoke。CI 通过、runner
active 或服务 active 均不表示岸岸业务上线。

当前未决：CI 专项批准、修复 SHA
`R`、旧 COS 产物内容摘要核验、O→T 旧 runner 中断恢复可达性、固定 T 一次性 systemd 入口及阶段事务的隔离验证、修复产物可用性，以及总指挥统一安排的生产 dry
run/live 与业务验收。
