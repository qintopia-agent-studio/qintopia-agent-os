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
bundle 的签名和摘要。P 对应回执为失败，不能把对象存在或旧树指针当作成功部署证明；R 产物尚不存在。

2026-09-26 再次只读核对：生产 O 的 runner、poller、promoter、rollback 脚本与 Git 提交
`16e8d56b` 的对应文件 SHA256 完全相同。使用服务器既有 COS
SDK 在进程内流式读取对象、计算 SHA256，未把包落地或输出配置：

| 已有制品                 | COS 内容 SHA256                                                    | 校验表预期 SHA256                                                  | 结果 |
| ------------------------ | ------------------------------------------------------------------ | ------------------------------------------------------------------ | ---- |
| P 主 sidecar tar         | `dbd8d029ba9a2754e697bb35553cebf79fecec4032334a602458e3dbe8f4c03e` | `dbd8d029ba9a2754e697bb35553cebf79fecec4032334a602458e3dbe8f4c03e` | 相符 |
| P QiWe companion tar     | `5dafbf5962aef55b07a43bde9af69cb1dfe5f78b210b5b434099ede6a5c6f851` | `5dafbf5962aef55b07a43bde9af69cb1dfe5f78b210b5b434099ede6a5c6f851` | 相符 |
| O deploy bundle tar      | `66fd3807c6aa1f3a0943c13779d7a4035abb81b4fa3580f5acbd072bd53aca7d` | `66fd3807c6aa1f3a0943c13779d7a4035abb81b4fa3580f5acbd072bd53aca7d` | 相符 |
| v0.3.3 deploy bundle tar | `dbf6bf9b82e295c51fa07ae433f277c2f7d3f4400a04c1bede5c6a5065e05177` | `dbf6bf9b82e295c51fa07ae433f277c2f7d3f4400a04c1bede5c6a5065e05177` | 相符 |

P 两份 `artifact-manifest.json` 的 COS 摘要分别为
`e5a7aa0cd1ee7689117657d363f04846b6bf0b3e072ad652a77acee0bad24a61` 和
`a803ebd92ffd401870d5efc7f03eec8db58ca1689a4ebdbe35bcd5746ee4725e`，也与各自 COS
`SHA256SUMS` 相符。三份旧校验表均与已装树字节一致；O bundle manifest不列入其
`SHA256SUMS`，但 COS 与已装树摘要同为
`c19558ae9ade6fe4f711adcec36eb186fc2ee38fa830b39f3580d8bede15fa8a`。v0.3.3
bundle 内的 installer、smoke、poller、runner、runner
unit 与 systemd 渲染脚本均存在且模式符合打包预期。这只能证明现成 v0.3.3 包的文件事实，不能替代未来 R 包的核验。

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
   `succeeded`、`current=70e7984`、`previous=16e8d56b`、新 runner 白名单含岸岸。同步失败时核验自动回退回执与原指针；进程中断或结果未知时停机核对 O/P、O/O 或 T/O，不重放原请求。
3. 对修复 Release 的完整六目标元组另发**新的**正常请求，先 dry run 后 live，
   `commit_sha=runtime_sha=deploy_bundle_sha=release_sha=R`，scope 依发布契约，六目标仍为
   `qintopia-system-services,hermes-erhua,hermes-xiaoman,hermes-silaoshi,hermes-huabaosi,hermes-anan`。live 后应为
   `current=R`、`previous=70e7984`。岸岸排空须取得新鲜零工作快照，最多等待 600 秒；超时暂缓并取消本次排空，不强杀、不补跑。

过渡 dry run 尚未实际执行；第一步旧 runner 对未来 R
bundle 的完整离线模拟、最新可信成功回执绑定及 R 产物可用性仍须验证。因此以上是待审执行路径，不是就绪结论。
`legacy_runner_bootstrap` 既有约束不得放宽或改名借用。

### 旧 O 隔离模拟

用精确 O 提交归档到 `/tmp`，运行旧版
`test-deploy-runner-promotion.mjs`、`test-promote-release-tree.mjs`、
`test-release-systemd-rollback.mjs`，均通过。额外一次性 fixture 使用模拟 O/P 指针和测试密钥签名请求：六目标在调用 promoter 前报
`restart_targets contains unsupported or duplicate entries`；
`commit=O,runtime=P,bundle=模拟 R,release=T`
且只重启 system 的请求通过旧 runner 验证并进入模拟 promoter。此结果证明请求合同和旧 runner 白名单的边界，未验证未来 R 制品的安装、实际 systemd 单元或生产 smoke。

旧 O
promoter 的真实顺序是：在 staging 下载主/伴随运行制品和 bundle、组装 manifest、校验树；把完整 staging 目录移到 T；先写
`previous=O`，再写
`current=T`；之后旧 runner 才调用 T 的 installer、smoke 并写回执。对旧 promoter 在这三个边界注入
`SIGKILL`：

| 中断点         | T 目录 | current / previous | 实际含义                                                 |
| -------------- | ------ | ------------------ | -------------------------------------------------------- |
| T 目录移动前   | 不存在 | O / P              | 指针未变；核对状态后停止，不需要新指针恢复机制           |
| 写 previous 后 | 完整   | O / O              | 原脚本未写 current；旧 rollback primitive 拒绝相同两指针 |
| 写 current 后  | 完整   | T / O              | 指针完成，installer/smoke 及回执尚未确认                 |

未中断的同一 fixture 得到 T/O。旧 runner 现有 fixture 的 installer 失败及 smoke 失败会进入同步自动回退；`SIGKILL`
不运行错误处理、installer 补偿或结果写入。因此 O/O、T/O 是首次接管必须覆盖的中断状态；T 未落盘时不存在本次指针改变。固定 T 入口可在 O/O 与 T/O 状态从完整 T 树运行，但必须先核验其 bundle 和原签名过渡请求。

故障注入位于完整命令之间，尚未证明 Linux `ln -sfn`
在命令内部被中断时的状态。实施前要用隔离 Linux
fixture 覆盖该窗口；若出现缺失指针，须按实际证据补充固定恢复状态并重新评审。最新只读服务器复核仍为 O/P，T 目录不存在。

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
`recover_request_id`，并要求 current/previous 两侧各自恰选一种身份。普通回退恢复只引用未终结、已持久记录的原事务；首次 O→T 恢复可引用旧 O
runner 的原签名过渡请求及 T
manifest，并仅在实测允许的 O/O、T/O 状态成立。恢复目标由原请求、已验 manifest 和事务记录（若有）共同决定，不接受新请求另填目标 SHA。

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
`QINTOPIA_DEPLOY_RUNNER_BIN` 覆盖 runner 不能保证 poller 也来自 T；直接从 SSH 启动 T
poller 会绕开 service 的 systemd 沙箱。因此进程中断后的恢复必须有固定于不可变 T
bundle 的一次性入口，不能依赖 `current` 或普通 timer。

比较过直接把已下载并验签的请求文件交给 T
runner：这能固定请求身份，却绕过 poller 现有的 COS 结果上传、已消费记录和远端结果去重，至少还需增加一套结果归档路径。更小的增量是复用现有精确制品、poller、runner、COS 指针及
`deploy.lock`：在 T 已完整安装并核验时，由受审的 release 内固定 wrapper 调用
`systemd-run` 创建一次性 root system service， `ExecStart` 钉住 T 的 poller，环境中的
`QINTOPIA_DEPLOY_RUNNER_BIN` 钉住 T 的 runner；`COSCLI_PATH`
也应固定到已验的既有 coscli，避免 poller 的安装回退从旧 `current`
取脚本。单元属性逐项继承并核对已安装常规 service 的 `User`、`Group`、`StateDirectory`、
`WorkingDirectory`、`NoNewPrivileges`、`PrivateTmp`、`ProtectHome`、`ProtectSystem`、
`ReadWritePaths`
及固定 COS/state 环境；任何属性不等价、路径不属于已验摘要的 T 树，或有效属性不能核对，都停止。wrapper 不接收任意可执行路径、unit 参数或目标 SHA，只接受新签名恢复 request
ID。 **身份绑定必须发生在 poller 实际消费处**：新增只用于固定恢复入口的
`QINTOPIA_EXPECTED_DEPLOY_REQUEST_ID`。T poller 下载并解析 COS
pointer 后、探测结果或下载请求前，把 pointer 的 request
ID 与该环境值比较；不一致即非零退出，绝不调用 runner、写回执或归档请求。下载请求后，既有
`parsed_identity`
再与该 ID 核对；错配同样在调用 runner 前拒绝。普通 timer 未设置该变量，维持原行为。wrapper 的预检可以发现早期错误，但不能代替此消费点检查：GitHub 可在预检与 poller 下载之间改写 COS
pointer，runner 内的锁也要到稍后才取得。这样即使远端 pointer 随后更新，固定单元也只会执行已下载且与批准 ID 一致的签名请求。由原 poller 做请求/回执一次性处理，由 T
runner 做 HMAC、锁、CAS 与状态恢复。不安装常驻新 service，不热改原 unit。

一次性 systemd 单元仍是**新增生产执行入口**，需评审其生成命令、权限和运行记录。wrapper、poller 消费点校验、单元属性比对、实际 systemd 隔离及中断测试均尚未实现。

恢复请求发布前，操作者在批准的维护窗口停止普通 deploy
timer，并确认普通 service 已退出、无正在处理的请求、无其他持锁进程；停止状态和原 enabled/active 状态写入恢复记录。

再发布新的签名恢复请求并启动固定 T 一次性单元。恢复期间持续核对 timer 未重新启动；installer 会复制 runner
unit 并
`daemon-reload`，所以每次 installer 后也必须复核。如 timer 重新活动或普通 service 启动，停止恢复并保留原始证据，不让两个 poller 竞争同一 COS
pointer。恢复成功、签名回执持久化且指针/manifest/installer/smoke 均核对后，按预检记录恢复原 timer 状态，并确认下一次普通运行只处理它理解的新请求。T→O 后 O
runner 不理解岸岸及恢复协议，必须在无新的部署请求期间完成审查，不能把普通 timer 已恢复解释为未来六目标请求可用。

只有 T 树本身可信、原签名请求和当前指针匹配、目标树及 installer 均可核验时，才允许使用该入口。首次 O→T 的原请求由旧 O
runner 执行，不会写入新事务记录。在模拟出的 O/O，旧 `rollback-release.sh`
要求 current/previous 不同而拒绝；在 T/O，它能按 `--restore-previous-sha P`
回 O/P，但原请求中断后不能重放，需要 T
runner 受签名恢复请求驱动并重新确认 installer/smoke。因此恢复协议须限于本次已签名 O→T 原请求，核对 T
manifest 的完整元组、O 成功回执、O manifest 的
`previous_sha=P`、P 旧树和允许状态 O/O 或 T/O，然后以 CAS 恢复 O/P；不得把 P 的失败回执当成功身份。T 目录移动前中断时仍为 O/P，不启动恢复写入。

R/T→T/O→O/P 的后续回退只接受各自成功回执与已持久事务，不扩成任意历史 release 恢复。

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
workflow 输入和同一部署 schema 的固定 recovery 字段。后续回退只引用当前未决事务；首次 O→T 只引用已验签的原过渡请求及不可变 T/O/P 身份。

两者均走原生产审批、签名和 exact-CAS 校验，并由上述固定 T 入口消费。动作仅为恢复原指针和原单元，不允许任意 SHA、服务或命令。

恢复中再次失败则停止并保留固定回执，不循环回退。任何结果未知先只读核对本地事务状态、指针和签名回执；不把服务 active 或缺少回执推断为成功或失败。岸岸排空若在指针切换后的 smoke 中超时，原 helper 会取消自己创建的排空标记，不强杀也不重启岸岸；事务只尝试一次固定原状态恢复。恢复未确认成功时保留失败状态并停止，不把指针或服务单项状态解释为完整回退。

不得手移 `current`、覆盖单文件、热改服务器、跳过签名/制品/原门禁、改写岸岸 `v2026.9.21`
核心入口或真实 PMS 数据。Profile、模型、凭据、会话、记忆、任务与通道状态保留。部署源码不安装 PMS 插件，也不启用 Foundation/PMS/events 凭据与生产 flags；业务验收另行进行。

## CI 专项评审请求

本报告只提出一个相互依赖的专项方案，不要求分次批准。修改 workflow、checker、部署门禁前须负责人逐项批准，不把历史 bootstrap 扩大到岸岸。

当前候选范围为 12 个现有实现/测试文件、1 个新 wrapper，合计 13 个实现/测试文件；不新增 workflow、job、step 或第三方依赖。相较上一版，新增 1 个 poller 实现文件，因为预检与真正消费 COS
pointer 之间存在竞态。此数量是具体评审输入，尚未获批。

1. `.github/workflows/rollback-production.yml`：改 2 个原必填 tag 输入，新增 3 个可选请求 ID/恢复输入；改现有 resolve、产物核验、请求生成 3 个步骤。
2. `deploy/runner/qintopia-agent-os-deploy-runner`：回退身份校验、精确 previous 事务、首次 O→T 的 O/O 或 T/O 固定恢复；普通正向与同 SHA 路径不变。
3. `deploy/runner/rollback-release.sh`：现有 primitive 增加 O/O 的固定原状态恢复模式，凭原签名过渡请求、T/O/P
   manifest 与 exact-CAS，不接受任意指针。
4. `deploy/runner/deploy-request.schema.json`：增加 1 个固定 recovery 对象约束。
5. `tools/deploy/create-deploy-request.mjs`：按经审批的 recovery 输入构造并签名该对象。
6. `deploy/runner/wait-deploy-result.sh`：复用现有 HMAC 校验，增加历史请求/回执验签和固定身份输出模式；原等待模式不变。
7. `tools/deploy/check-deploy-runner.mjs`：普通目标集合与回退 workflow 一致；保护双身份、恢复和 poller 的 expected
   ID 合同。
8. `tools/deploy/test-deploy-runner-promotion.mjs`：混合元组、O/O→O/P、T/O→O/P、R→T→O、各中断点与失败恢复负例。
9. `tools/deploy/test-wait-deploy-result.mjs`：历史签名请求与回执匹配、篡改和缺失负例。
10. `deploy/runner/poll-deploy-requests.sh`：固定入口设置 expected request
    ID 时，在实际 COS pointer 解析后、任何执行前比较；普通 timer 行为不变。
11. `deploy/runner/run-fixed-release-recovery.sh`（新文件）：固定 T 一次性 systemd 入口，核对请求 ID、T 树摘要、普通 timer 和 service 状态及有效沙箱属性；无任意命令参数。
12. `tools/deploy/build-deploy-bundle.mjs`：将上述 wrapper 纳入 R
    bundle 的固定文件清单与摘要。
13. `tools/deploy/test-deploy-runner-poller.mjs`：测试预检后 COS
    pointer 被新请求覆盖时拒绝执行、固定 T
    poller/runner 来源、timer 竞争拒绝和一次性消费。

配套更新本报告及 `deploy/runner/README.md`、
`docs/operations/production-deploy-runner.md`
属文档，不计入上面的实现数量。恢复入口须先在隔离 systemd 环境中证实与原 service 的有效限制等价；若
`systemd-run`
无法满足，则本方案不具备生产可执行性，应重新评审入口，而非退化为 SSH 直调。

| 原则         | 回退普通目标一致性                                     | 精确前次混合版本回退                                                                                                                                                                                                                                      |
| ------------ | ------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1 实际问题   | 岸岸已在 runner/rules 中，正式回退选择器遗漏           | 过渡 release 无 tag 且运行/包/目录 SHA 不同，现入口无法回退                                                                                                                                                                                               |
| 2 数量       | 1 个目标选项、2 处原规则、1 条集合校验                 | 候选合计 12 个既有文件、1 个新 wrapper；新增 3 个输入、1 个 recovery 对象规则、1 个历史验签模式、1 个精确 previous 事务、1 条 poller expected ID 规则和 1 个固定一次性执行入口；修改 2 个输入必填属性及 3 个既有步骤；新增 0 workflow/job/step/第三方依赖 |
| 3 依据       | 当前选项/展开缺 `hermes-anan`，而 schema/runner 已接受 | 当前 manifest 混合版本，原 rollback workflow 将所有 SHA 固定为 tag                                                                                                                                                                                        |
| 4 复杂度     | 复用 `allowed_targets - independent_targets`           | 复用现有签名协议、COS 校验、谱系字段、poller 和 runner lock；仅中断恢复使用固定 T 的一次性 systemd 单元                                                                                                                                                   |
| 5 现有 CI    | 不检查回退选项是否等于普通目标集合                     | 业务代码无法改变回退请求构造与正式入口                                                                                                                                                                                                                    |
| 6 最小范围   | 只补普通集合一致性                                     | 仅接受已成功部署且恰为服务器前次的完整固定元组；普通同 SHA 校验与正向发布不变                                                                                                                                                                             |
| 7 无单项特例 | 集合规则不写岸岸专用豁免                               | 按精确前次 release 元组通用处理，不加岸岸专属放行                                                                                                                                                                                                         |
| 8 部署相关   | 仅部署回退目标                                         | 仅不可变 release 回退                                                                                                                                                                                                                                     |
| 9 可复用     | 后续普通目标变更自动发现遗漏                           | 后续合法混合版本前次 release 也可精确回退                                                                                                                                                                                                                 |

原两文件提案只覆盖岸岸选项，不能以其完成作为上线条件。固定受审 bundle 过渡本身可复用现有正常
`workflow_dispatch`。既有 `rollback-release.sh --restore-previous-sha`
能处理完整原状态下的精确 T→O 指针切换，却无法在已写 current、尚未写 previous 的状态启动，也不会确认 installer、smoke 和回执。这就是新增专属事务、持久阶段和固定入口的理由。

若不随 bundle 部署这些回退能力，最终 R 的可执行回退仍缺失。

因此“需要 R”基于**可手动回到前次混合 T 且可继续回到 O**
的验收条件，不表示 R 已构建或获批。负责人确认上述具体范围后才实施；生产申请仍另行授权。

## 最终建议

以本次 O/P→T/O→R/T 及其返回路径为边界，先审 13 文件候选并完成 R 的隔离测试和不可变制品，再申请生产 dry
run。正常首次接管由旧 O runner 消费新的单 system 请求，正常修复部署由 T
runner 消费新的六目标请求；R→T 用 R/T 的 exact-previous 回退，T→O 用 T/O 的 exact-previous 回退。

每次 live 只在精确签名回执、指针、manifest、installer 和 smoke 一致后进入下一阶段。

首次接管硬中断时，T 未落盘且指针 O/P 则只读核对并停止；T 已落盘而指针 O/O 或 T/O，由固定 T 一次性入口消费**另一条**审批、签名且绑定 request
ID 的恢复请求，CAS 回到 O/P，运行 O
installer 与 smoke 并签名回执。后续回退硬中断也用同一固定 T 入口处理已记录事务。任何新 pointer 错配、目标树不可信、CAS 冲突或恢复失败都停止，不能重放原请求。此入口和恢复事务尚未实现，当前部署状态仍是 O/P；不应将本报告视为上线许可。

## 验证和交接

本报告变更仅文档。工作分支为 `codex/v033-runner-takeover-plan`，以
`62f403e3c5e615c10b6a0577954626664fcb22db` 为基点；此前文档提交为
`7b9578b`、`7c8d516`，HEAD 已在基点之后。此前远端 `master` 的只读查询返回该基点 SHA。PR
721、722 和 release PR 718 均已合并，对应原 CI
check 成功；PR-Agent 留有自动评论，`reviews=[]`，不能视为人工批准。v0.3.3 发布 run
`36206145717` 的构建成功、request-deploy 失败。

当前离线证据：`node tools/deploy/test-release-systemd-rollback.mjs`
通过，实际在临时目录运行既有 primitive，证明 `--restore-previous-sha` 更新两指针、保留
`rollback-from` 并调用旧树 installer。 `node tools/deploy/test-wait-deploy-result.mjs`
通过，覆盖当前回执验签等待合同。 `node tools/deploy/test-deploy-runner-promotion.mjs`
通过，覆盖当前普通推广失败的回退分支，未覆盖本提案的混合 previous 事务。另以精确 O 归档运行旧版三项隔离测试，加入签名请求及旧 promoter 硬中断模拟；结果见上文。

旧 runner 的白名单静态核对缺岸岸，新 runner 白名单与 smoke 均有岸岸；旧 runner 没有本提案的事务恢复能力。这些证据不等于未来 R 混合过渡或新回退路径已通过模拟。

最终文档的 Prettier、Markdown lint 与 `git diff --check` 均通过。 `pnpm check:pr:quick`
已启动，经过格式、Markdown 和多个现有合同检查后，在无关的 Agent
runtime 管理 fixture 中因耗时中止；不能记为通过。

获批实施后须补齐并实际运行：

1. 用未来 R bundle 在旧 runner
   fixture 验证完整制品安装与单 system 过渡；用新 runner 验证六目标和 `hermes-anan`
   的排空成功/600 秒超时暂缓。
2. 对双 tag、tag/request ID 混合、双 request
   ID 各路径验证；缺失/双填、无效签名、错误 key id、错误 request
   ID、环境、状态、四 SHA、scope、targets、COS 摘要、服务器 exact
   previous 均须在任何指针写前拒绝。
3. 以可抛故障 fixture 覆盖首次 O/O、T/O 恢复，以及 R/T→T/O→O/P、
   `rollback-from`、两次指针写、installer、smoke、回执丢失及恢复失败；在 poller 下载 pointer 之前将其改成另一有效请求，证明 expected
   ID 错配在执行前拒绝。普通正向、同 SHA、双 tag 旧合同不能放宽，不能循环恢复或重放原请求。
4. 跑
   `pnpm deploy:runner:check`、`pnpm check:pr:auto`、格式/Markdown 检查，核对新 PR 的 head/base、所有 checks、CODEOWNERS 和 reviewer 建议。

生产预检需固定 SHA、原 manifest 四元组、COS 签名/摘要、当前和 previous 指针、O 的历史成功回执与 P 的旧树事实；过渡 T 成功后再固定 T 的成功回执。

还需核对岸岸核心入口及零工作排空条件。正式回退实现须先通过隔离模拟，再审过渡生产 dry
run；T 尚不存在时不能伪造正式 R→T 或 T→O 生产 dry-run 回执。没有可执行且经隔离验证的恢复路径就不做 live 过渡。T
live 成功后才有 T 的签名成功回执，可据此进行正式回退 dry run；通过后再考虑 R
live。每次 live 后核对新签名回执、指针、manifest、runner 来源和六目标 smoke。CI 通过、runner
active 或服务 active 均不表示岸岸业务上线。

当前未决：CI 专项批准、修复 SHA
`R`、O/O 与 T/O 固定恢复的隔离验证、固定 T 一次性 systemd 入口及 poller expected
ID 校验、后续回退的阶段事务验证、修复产物可用性，以及总指挥统一安排的生产 dry
run/live 与业务验收。
