# Hermes Core 独立发布管理器设计

状态：artifact、runtime
binding、生产 controller 和请求接线已有本地实现；真实 Linux 构建与生产部署未完成

更新日期：2026-09-11

## 目标

为 Hermes 官方核心建立独立于 Qintopia `release/current`
的不可变发布流。完成后，常规 Hermes 更新只需要提交一个受评审的 `hermes-core-release`
请求；runner 自动完成下载、校验、配置迁移副本检查、七服务切换、smoke、更新回执验证和失败回滚。

本设计不修改或删除当前生产 checkout，不改变任何 WeCom 配置，不改变 Erhua
`qiwe-platform`，也不把 Hermes core 合并进 Qintopia 发布目录。

## 决策

- 新增独占 deploy scope `hermes-core-release`，固定 restart target
  `hermes-core`。请求不能携带任意路径、命令、服务名或 fault injection 参数。
- 复用现有 HMAC、TTL、COS、production environment approval、`flock`
  和脱敏结果上传边界，但使用独立的 Hermes core manager 和 rollback 脚本。
- CI 或受信构建器生成官方 Hermes core artifact；生产服务器只下载、校验和切换，不在 live
  checkout 上 `git pull`、解决冲突或构建未知输入。
- `runtime/hermes/profile-registry.yaml` 是七个 profile、user
  service、WeCom 状态和 Erhua QiWe 保留声明的唯一运行清单。
- 首个实现只支持全七服务事务切换。低风险 canary 在 staging 完成；生产部分 profile
  canary 需要 service-specific 指针设计，不能假装由一个全局 `current` 指针实现。
- 失败 candidate 进入 quarantine，自动清理永远不能删除
  `current`、`previous`、已验证 rollback reserve 或相关 receipt。

## 独立目录模型

```text
/var/lib/qintopia-hermes-core/
  releases/
    <official-commit>/
      core/
      artifact-manifest.json
      build-receipt.json
      update-receipt.json
      validation-summary.json
  incoming/
  quarantine/
  state/
    manager.lock
    transactions/
  lineage/
    active -> generations/generation-<lineage-fingerprint>
    generations/
      generation-<lineage-fingerprint>/
        lineage.json
        current -> ../../../releases/<official-commit>
        previous -> ../../../releases/<different-official-commit>
        rollback-reserve -> ../../../releases/<verified-official-commit>
  current -> lineage/active/current
  previous -> lineage/active/previous
  rollback-reserve -> lineage/active/rollback-reserve
```

整个 root、release 和 lineage generation 均由 `root:root` 管理，gateway
service 用户只能读取。release 目录必须由官方 tag 和完整 commit 绑定，内容为
`0555/0444`；私有 `incoming`、`quarantine` 和 `state` 目录为 `0700`。不能把 root 放在
`/home/ubuntu` 下，因为该用户可以替换一个看似 root-owned 的叶子目录并绕过信任边界。

`current` 与 `previous`
必须指向不同且已通过启动验证的 release。三个顶层兼容指针保持不变，真正的事务提交点只有
`lineage/active`：manager 先创建并验证完整的不可变 generation，再以同目录临时 symlink、原子 rename 和目录
`fsync` 一次切换整个 `(current, previous, rollback-reserve)`
元组。不得依次切换三个独立指针并把中间状态暴露给 gateway 或回滚器。

所有读取方必须先且只解析一次 `lineage/active`，再从解析得到的同一个 generation 读取
`current`、`previous` 和
`rollback-reserve`。不得分别经由三个顶层兼容指针读取角色，否则可能在一次活动 generation 切换前后混读出不存在的 lineage 元组。

## Artifact 与回执契约

Artifact 至少包含：

- 官方仓库标识、tag、完整 commit、源码或 artifact SHA-256；
- 构建环境、Python ABI、依赖锁身份和 Hermes CLI 版本；
- regular-file/type/owner/mode inventory，不允许 symlink、hardlink 或特殊文件；
- `build-receipt.json`，证明构建完成且 artifact identity 一致；
- `update-receipt.json`，至少包含
  `outcome=success`、目标 tag/commit、旧新版本和完成时间；
- 脱敏 `validation-summary.json`，只记录检查状态、profile 计数和固定错误分类。

生产 manager 必须重新计算 digest，并验证 manifest、build receipt、update
receipt 与请求中的精确 identity 一致。命令退出码为 0 但 receipt 缺失、过期、不匹配或非 success 时一律失败。

HC-1 的生产侧 verifier 仅使用 Node 标准库。HC-0 的 registry/readiness/parity 工具仍需 YAML
parser，因此 deploy bundle 固定携带无传递依赖的 `yaml@2.9.0`，并在隔离 bundle
fixture 中验证三个工具均可启动。仓库测试使用 Ajv 编译 artifact/receipt 与 lineage JSON
Schema，并对正常 artifact、内容篡改、manifest/receipt
identity 漂移、失败 receipt、敏感摘要、symlink、hardlink、额外文件和错误权限执行对抗性 fixture。verifier 要求调用方提供精确 manifest
digest；HC-4 完成前该 digest 尚未绑定签名请求，因此 HC-1 通过仍不构成 artifact 来源真实性或生产发布授权。

## 事务状态机

```text
request_validated
  -> artifact_verified
  -> candidate_staged
  -> wecom_readiness_passed
  -> wecom_parity_passed
  -> doctor_and_plugin_compat_passed
  -> original_lineage_recorded
  -> services_stopped
  -> current_switched
  -> services_started
  -> seven_profile_smoke_passed
  -> receipt_verified
  -> committed
```

每一步只写入固定 schema 的本机事务 journal。journal 只保存版本、commit、步骤状态、服务别名和固定错误码，不保存 env、配置值、原始 journal、消息、prompt、provider
payload 或真实目标 ID。

## 切换与回滚

1. 在锁内重新验证 request、artifact、磁盘、`current`、`previous` 和 expected lineage。
2. 从 registry 读取固定七服务及顺序，记录 unit hash、ExecStart、active/enabled 状态。
3. 停止七服务并确认 inactive；任一失败时不切换指针。
4. 原子更新 `previous` 和 `current`，执行 user daemon reload。
5. 按 registry 顺序启动并验证每个服务的 active 状态、Hermes code identity 和 profile
   smoke。
6. 验证 WeCom readiness/parity、plugin compat、doctor 和 update receipt 后提交事务。

任一步骤失败时：按已启动服务的逆序停止 candidate，恢复原始 `current` 与
`previous`，daemon
reload，按 registry 顺序恢复旧服务并运行相同 smoke。回滚失败必须记录整体 `failed` 且
`rollback.status=failed`，不能把“已尝试回滚”当作成功。

## 磁盘与保留策略

- 开始 staging 前根分区至少 5 GiB 可用，目标 8-10 GiB。
- 固定保护 `current`、`previous`、`rollback-reserve` 及它们的 manifest/receipt。
- 默认只保留最近两个额外成功 release 和最近两个失败 quarantine；实际删除需先生成脱敏保留计划，并在 owner 评审后执行。
- quarantine 仅移动已验证位于固定 root 下的 candidate；不接受环境变量、glob 或请求字段提供删除路径。
- 不默认执行 Hermes `--backup`。完整 `HERMES_HOME` 备份只能进入另行评审的外部存储。

## 实施工作包

| 工作包 | 状态         | 范围                                                                                    | 退出门禁                                      |
| ------ | ------------ | --------------------------------------------------------------------------------------- | --------------------------------------------- |
| HC-0   | 已完成       | 七 profile registry、WeCom readiness、字段/env parity、Erhua=false                      | fixture、runner contract 和 bundle 检查通过   |
| HC-1   | 已完成       | core artifact manifest、build/update receipt schema 与 verifier                         | 篡改、缺失、错误 identity 全部 fail closed    |
| HC-2   | 仓库内已完成 | 独立 root、ingress、candidate staging、generation lineage、bootstrap 和 dry-run manager | 原子落盘与崩溃恢复 fixture 通过；尚未接入生产 |
| HC-3   | 仓库内已完成 | 七服务事务切换、逆序回滚和 repository-local fault injection                             | 每个故障点均恢复原始指针和服务状态            |
| HC-4   | 待实施       | `hermes-core-release` 请求/schema/workflow/result 接线                                  | 任意 path/service/command/fault 字段均被拒绝  |
| HC-5   | 待实施       | 隔离 staging：schema 41、doctor、plugin compat、WeCom/QiWe replay                       | 七 profile 与两条微信链路的验收矩阵全部通过   |
| HC-6   | 待实施       | owner-reviewed 首次 clean-core cutover 与回滚演练                                       | 七服务同一官方 commit，previous 可实际恢复    |
| HC-7   | 待实施       | retention/quarantine 观察与后续一键更新 runbook                                         | 更新不再依赖服务器本地补丁或人工解决合并冲突  |

每个工作包独立 PR、独立验证。HC-5 未完成前不能删除旧 WeCom 补丁；HC-6 的回滚演练与观察窗口未完成前不能删除旧 checkout 或 patch
evidence。

HC-2 已完成以下仓库内增量：

- deploy bundle 固定 vendor `yaml@2.9.0`，并通过隔离运行测试消除生产 `node_modules`
  假设；
- `plan-hermes-core-release.mjs` 与 root wrapper 固定使用
  `/var/lib/qintopia-hermes-core`、`root:root` 权限和 `state/manager.lock`；
- `stage-hermes-core-release.mjs` 从固定 root-owned
  ingress 复制 candidate，复制前后均校验完整 artifact，使用排他创建、原子 rename 和目录
  `fsync` 落到
  `incoming/<commit>`；失败的本次临时树只移入固定 quarantine，不删除或改写已有 release；quarantine 失败和 rename 后 fsync 失败分别报告专用错误码，保留现场供后续恢复；
- planner 在锁内复验 candidate、完整活动 lineage、兼容指针、至少 5
  GiB 空间及所有受保护 artifact；
- dry-run
  fixture 证明完整目录树、release 指针和服务状态不变，并覆盖锁、权限、lineage 漂移、artifact 篡改、symlink、额外文件和空间不足等拒绝路径。
- `fetch-hermes-core-artifact.sh` 从固定 COS
  key 下载 archive 和公开 identity 文档，绑定 archive/source/manifest/identity
  digest，在固定 ingress 上使用真实 `flock`；生产拒绝 `COSCLI_PATH`
  覆盖，只接受固定 root-owned、父路径不可写的 `coscli`，下载使用 600 秒 timeout 和 512
  MiB
  `RLIMIT_FSIZE`，失败前先删除未信任 archive；失败 quarantine 最多保留两个，达到上限后 fail
  closed，不自动删除审计证据；
- 流式 extractor 将 archive 限制为 512 MiB、单成员 256 MiB、解包总量 1 GiB；在 `tarfile`
  读取成员前先扫描原始 tar header，拒绝错误 checksum、截断、GNU
  sparse 和畸形扩展，且同时限制 PAX/GNU 扩展头的单项、累计字节和数量，随后再拒绝路径穿越、重复路径、symlink、hardlink、特殊文件和成员数超限，并完整读取 gzip 流验证 CRC/footer；
- `bootstrap-hermes-core-root.mjs` 与固定 root wrapper 在外部锁内安装两个不同的 clean
  release，创建 immutable generation，以一次 `lineage/active` 切换提交初始
  `(current, previous, rollback-reserve)`，并覆盖 rename、目录
  `fsync`、quarantine和重试恢复的不确定态；bootstrap
  lock 使用排他创建，竞争者验证并锁定同一 inode，锁文件、core parent 和首次 quarantine
  parent 均执行补偿 `fsync`；
- `commit-hermes-core-lineage.mjs` 提供 HC-3 将调用的低层 generation
  commit 原语，拒绝 candidate 重用受保护角色，并覆盖 release、generation、active 和 quarantine 提交边界的故障注入；回滚切换前重新验证目标 generation 的三个角色及每个 release
  artifact 完整性；
- candidate staging 会在锁内识别、验证并隔离固定命名的遗留 `.staging-*`
  目录，未知 incoming 条目继续 fail
  closed；所有已可见但提交结果不确定的 ingress、candidate、release、generation、active 和 bootstrap
  root 重试路径都会先补偿 `fsync`。

HC-3 已完成 repository-only 事务协调器和 journal schema。它只读取一次七 profile
registry，记录 `LoadState`、`ActiveState`、`UnitFileState`
与 ExecStart 指纹；仅接受可精确恢复的 `loaded + active|inactive + enabled|disabled`
状态。候选只重启迁移前 active 的服务，原本 inactive 的服务保持 inactive；失败时按 registry 逆序停止 candidate，恢复原 lineage 和 enablement，再按 registry 顺序恢复原服务。启动失败、lineage
commit 不确定、journal commit 不确定、进程崩溃、合法 phase 篡改、pending
journal、artifact identity 漂移、原始 lineage
preflight 和 rollback 失败 fixture 已通过。固定生产 systemd controller、release-local
runtime
binding 和 HC-4 请求接线已有仓库实现和 fixture，但尚未完成真实生产部署，因此仍不是已经启用的生产升级入口。

HC-2 完成仅指仓库实现与本地 fixture 门禁已完成。尚未在生产创建
`/var/lib/qintopia-hermes-core`，尚未下载任何真实 clean
artifact，也尚未把这些命令部署到生产 runner 或 systemd。当前服务器 checkout、WeCom 补丁、七个 profile 的 WeCom 配置及启停状态都保持不变。HC-5
staging replay 通过前不得执行首次 clean-core cutover。

## 验收命令方向

仓库本地：

```bash
pnpm deploy:runner:check
pnpm runtime:hermes:check
pnpm agents:profile-bundles:check
pnpm check
```

生产只允许由 owner-reviewed
runner 调用固定动作。直接 SSH 只用于只读诊断或紧急回滚，不作为日常升级入口。

## 当前阻塞

- 2026-09-10 已通过管理员提供的连接信息完成只读 SSH 连通性和磁盘复核；当前连接信息不写入仓库，后续 SSH 操作须联系管理员获取最新授权信息；
- 根分区当前可用 `25,425,580 KiB`，约 `24.25 GiB`，使用率 `58%`，已满足 staging 最低 5
  GiB；此前约 1.8 GiB 的数字是过期基线，不再作为当前阻塞；
- 当前 artifact 仅携带 `core/`，`python_abi`
  和依赖锁仍是声明字段，未携带或绑定 release-local
  interpreter/site-packages；如果只切换 core 指针，服务仍可能运行旧共享
  `/home/ubuntu/.hermes/hermes-agent/venv`，这是接入生产前的 P0；
- HC-2 的安全 ingress、staging、bootstrap、lineage
  commit 和只读 planner 已完成仓库内实现与故障注入，但尚未使用真实 clean
  artifact 或写入生产 root；HC-3 事务内核已完成仓库实现与故障注入，固定 systemd
  controller、release-local runtime binding 及 HC-4、HC-5 尚未完成。HC-1 已能验证 core
  artifact 和 receipt，但预期 digest 尚未绑定签名请求，也没有七服务事务回滚链；
- WeCom replay、Erhua QiWe replay 和旧本地行为替代验收尚未完成。

因此本设计完成不等于批准生产升级，也不授权删除服务器上的 WeCom 配置或旧补丁。
