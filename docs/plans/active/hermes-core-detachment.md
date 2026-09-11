# Hermes 核心解耦迁移计划

状态：进行中

更新日期：2026-09-11

## 2026-09-11 执行记录（优先于下方历史阶段记录）

- SSH 连接已验证；03:45 UTC 根分区 `df -h /` 为可用 23G、使用率 60%。
- 已在本地隔离环境安装真实官方
  `v2026.9.7`（`2237be355906fbe6065ce1815711eee52b2d646e`），使用 Python
  3.12 和锁定的 messaging、wecom extras。
- 真实 CLI
  `--help`、QiWe/WeCom 类型导入及插件注册通过；注册检查禁止网络连接，不代表消息到达或生产配置迁移验收。
- runtime 测试 21 项通过（另含 6 个 subtest）。修复测试夹具只复制解释器、不生成
  `pyvenv.cfg` 导致独立 Python 无法找到标准库的问题。
- 修复构建器在 macOS 上错误标记 Linux 产物、未执行 CLI
  smoke 却记录通过、遗漏消息通道 extras 的问题；明确验证依赖哈希，并从 profile
  registry 生成配置契约计数。这些计数是静态契约，不是生产配置 parity 结果。
- 已生成并在无系统 Python 的独立容器中验证 `v2026.9.7` Linux
  amd64 自包含 artifact；首次 bootstrap 所需的第二个不同官方 release artifact 尚在准备。
- 独立发布管理器及 systemd/controller 接线已有本地代码，尚未生产部署；不能继续把“有代码/fixture 通过”写成“迁移已完成”。
- 生产尚未切换。上线仍需真实 Linux
  artifact 验证、配置副本 parity、现有本地行为替代验收和首次切换回滚验证。

## 目标与决策

执行一次性的“干净核心”迁移：

- Hermes 始终使用未经本地修改的官方上游代码；
- Qintopia 行为由版本化发布的插件、sidecar 和工作流承载；
- 对通用且确有必要的能力，通过上游 PR 贡献；
- 不把长期 fork 或自动重放补丁队列作为稳定方案；
- 保留所有现有 Hermes WeCom 配置及其当前启用/停用状态，不以当前使用量决定是否迁移；
- 把 WeCom 配置和仍有价值的本地行为迁移到新版本支持的边界，不能因为当前没有流量就直接删除；
- 同时保留并验证当前实际承载微信消息的 `qiwe-platform` 路径。

当前生产 checkout 不能直接升级。完成本计划后，Hermes 的常规更新必须通过同一套经过评审的 runner 流程执行，不再需要人工处理本地补丁、合并冲突或服务器热修改。

本计划的最小可观察结果是：七个 gateway 使用同一个干净的官方 Hermes 版本，现有 WeCom
profile 配置在新版本中继续按原状态生效，Qintopia 扩展仍从 `release/current`
加载，QiWe 消息链路正常，后续升级可以通过标准命令和自动化验收直接完成。

独立 core 发布目录、artifact/receipt 契约、事务切换、逆序回滚和 retention 的实现拆分见
[Hermes Core 独立发布管理器设计](hermes-core-release-manager.md)。

## 已确认的生产基线

2026-09-09 的只读生产盘点结果如下：

| 项目         | 当前生产状态                                      | 目标状态                                   |
| ------------ | ------------------------------------------------- | ------------------------------------------ |
| Hermes CLI   | `v0.15.1`                                         | `v0.21.1`                                  |
| 源码标识     | `v2026.5.29-688-gc76d035c1-dirty`                 | `v2026.9.7`                                |
| Commit       | `c76d035c1cefa4dc1ef7e83f11b4e413897ecf58`        | `2237be355906fbe6065ce1815711eee52b2d646e` |
| 与上游的差异 | ahead 1、behind 2465                              | 官方 `main` 上 ahead 0                     |
| Worktree     | 11 个已跟踪改动、9 个未跟踪项                     | 完全干净                                   |
| 配置版本     | 5 个 profile 为 23，2 个为 26                     | schema 41                                  |
| Python       | 3.11.15                                           | 受支持范围 `>=3.11,<3.14`                  |
| Gateway      | 7 个活动服务，共用一个 venv                       | 7 个服务使用同一已验收版本                 |
| 根分区空间   | 2026-09-10 只读复核：可用约 24.25 GiB，使用率 58% | 至少 5 GiB，建议 8-10 GiB                  |

七个 gateway profile 分别是 `default`、`erhua`、`guanerye`、`huabaosi`、
`silaoshi`、`wenyuange` 和 `xiaoman`。它们均使用同一个 Hermes
checkout 下的解释器，因此该 checkout 是整个运行集群的共同依赖，不能在原目录内直接试升级。

最新 Hermes 的自动配置迁移最低支持 schema
12，目标 schema 为 41。现有 profile 均高于最低版本，所以可以在隔离副本上自动迁移配置；这只证明配置具备迁移条件，不代表可以原地升级当前 dirty
checkout。

## WeCom 与 QiWe 的现状判断

2026-09-09 对现有 30 天 journal 的脱敏检查结果如下：

| Profile     | Hermes WeCom 配置 | `qiwe-platform` | 近 30 天运行证据                       | 处置             |
| ----------- | ----------------- | --------------- | -------------------------------------- | ---------------- |
| `default`   | 已启用            | 无              | 无成功连接或入站消息，有连接错误       | 保留并迁移兼容性 |
| `erhua`     | 已停用            | 已启用          | 有 QiWe 活动记录                       | 保留两套当前状态 |
| `guanerye`  | 已启用            | 无              | 无成功连接或入站消息，有连接错误       | 保留并迁移兼容性 |
| `huabaosi`  | 已启用            | 无              | 无成功连接或入站消息，有连接错误       | 保留并迁移兼容性 |
| `silaoshi`  | 已启用            | 无              | 无成功连接或入站消息，有连接错误       | 保留并迁移兼容性 |
| `wenyuange` | 已停用            | 无              | 两种通道均无活动                       | 保留停用状态     |
| `xiaoman`   | 已启用            | 无              | 无成功连接或入站消息，有连接或认证错误 | 保留并迁移兼容性 |

结论：Hermes 官方 WeCom 并非“没有配置”，五个 profile 仍会尝试启动；现有 30 天日志窗口内虽然没有成功连接、重连或接收消息的证据，但这只说明当前运行状态异常，不能作为删除配置或本地兼容行为的理由。

升级必须保留这些 profile 的当前启用/停用状态，并处理其配置和代码兼容性。

`erhua` 的 `qiwe-platform`
是当前有真实微信消息活动证据的路径。它属于 Qintopia 的 release-managed 集成，不得随着 Hermes
WeCom 补丁一起删除或改写。

本计划继续采用“将 Huabaosi WeCom 生产路由迁往受版本管理的边界”的方向。
[Huabaosi WeCom 迁移计划](huabaosi-wecom-migration.md)及服务器补丁快照是本次 WeCom 兼容迁移的输入和审计证据，不能在替代行为验收前删除。

## 方案评估

| 方案                           | 一次性成本 | 后续升级成本 | 冲突风险 | 结论           |
| ------------------------------ | ---------: | -----------: | -------: | -------------- |
| 干净核心 + 插件/sidecar/工作流 |         高 |           低 |       低 | 采用           |
| 长期维护 Qintopia Hermes fork  |         中 |           高 |       高 | 拒绝           |
| 每次升级自动重放补丁队列       |         低 |         中高 |       高 | 仅可作紧急过渡 |

fork 或补丁队列不会消除耦合，只会把人工冲突延后到每次更新。

目标 Hermes 版本已经提供平台插件、工具 hook、中间件、集群更新、配置迁移、快速快照和更新回执。

当前剩余的 Qintopia 行为都能放到 Hermes 核心之外；WeCom 配置必须保留，通用行为迁移到官方插件或上游，Qintopia 特有策略迁移到 sidecar、插件或工作流。

## 服务器本地改动的处置

不要在当前生产 checkout 中逐文件删除补丁，也不要用“删到 worktree 干净”的方式改造现有目录。应记录原 commit、diff 哈希及处置结论，在新建的干净 checkout 中完成切换；旧 checkout 在验收期内只作为不可变回滚目标。

| 现有行为或改动                                                | 目标处置                                                                     | 删除或退出门禁                                   |
| ------------------------------------------------------------- | ---------------------------------------------------------------------------- | ------------------------------------------------ |
| WeCom 目标解析与媒体发送                                      | 迁移到目标版本官方 WeCom 插件；缺失的通用能力提交上游                        | DM、群聊、图片、文档、语音和 caption replay 通过 |
| WeCom 重连与过期回复降级                                      | 迁移到官方 close/reconnect 和错误码处理；必要时保留经过评审的兼容层          | 断线与过期回复 canary 通过                       |
| WeCom 内部过程文本过滤                                        | Huabaosi 策略放入 release-managed sidecar；通用噪声抑制和脱敏交给官方能力    | 用户可见 replay 通过，过滤策略不丢失             |
| 审批命令隐藏                                                  | 危险业务动作迁到确定性工作流；若仍有通用需求再向上游提案                     | 非 WeCom 消费者验证无命令内容泄露                |
| WeCom `errcode=-1` 媒体重试                                   | 优先放在 QiWe sidecar；若官方 WeCom 仍需要，则向上游提交通用幂等重试         | 重试和未知发送结果测试防止重复发送               |
| Kanban workspace 媒体 allowlist                               | 使用上游媒体策略和 `kanban_complete` artifact                                | 现有附件与 artifact replay 通过                  |
| Webhook `script_action`                                       | 把 Silaoshi 的 900 秒动作迁到 release-managed 异步 job endpoint              | 成功、失败、超时、重复投递和通知测试通过         |
| Kanban 标题/正文 `rec...` 去重                                | 生产者提供稳定、显式的 `idempotency_key`，例如 `qintopia:poster:<record-id>` | 重复 replay 只创建一个 work item                 |
| Huabaosi 完成门禁                                             | 用 Qintopia 插件的 `pre_tool_call` 阻止不合规 `kanban_complete`              | 允许与阻止 fixture 均通过                        |
| 三个 `qintopia-tools` variant 直接导入 `hermes_cli.kanban_db` | 改为自有 bridge；短期可调用带 `--idempotency-key --json` 的用户级 Kanban CLI | Qintopia 包不再导入 Hermes 数据库内部模块        |

最新 Hermes 插件兼容性扫描对当前已盘点插件报告 0 个 deprecated-import 命中，但这不代表
`hermes_cli.kanban_db`
已成为稳定 API。2026 年 9 月移除兼容层后，该内部模块仍没有长期兼容承诺，因此直接导入仍是迁移阻塞项。

## 目标运行结构

```text
Qintopia release/current
  -> 插件、sidecar、工作流、profile bundle
  -> 干净的官方 Hermes checkout
  -> 七个 gateway service

微信消息
  -> release-managed QiWe platform 或 Hermes WeCom platform
  -> Qintopia sidecar / 插件 / 工作流

Hermes 官方 WeCom
  -> 按各 profile 原有启用/停用状态继续生效
```

Hermes 核心和 Qintopia
release 必须保持两个独立发布流：各自拥有版本、审批、验收证据和回滚目标。升级 Hermes 不应隐式发布 Qintopia，发布 Qintopia 也不应隐式更新 Hermes。

## 当前推进状态

截至 2026-09-10：

- 已完成生产 Hermes 版本、dirty
  checkout、七个 gateway、配置 schema、WeCom/QiWe 状态和磁盘空间的只读基线盘点；
- 已增加
  `check-hermes-core-readiness.sh`，用于检查官方 remote、干净 worktree、磁盘、解释器和七个服务；
- 已增加 `check-hermes-wecom-readiness.sh`
  及 fixture 测试，用于检查七个 profile 的 WeCom 启用状态、配置可解析性和必要凭据键存在性，全程不读取或输出值；
- 已建立 `runtime/hermes/profile-registry.yaml`，集中声明七个 profile、固定 user
  service、WeCom 预期启停状态和 Erhua `qiwe-platform`
  保留边界；后续 readiness、parity、切换和回滚均从该 registry 读取，不再维护多份 profile 列表；
- 已增加 `check-hermes-wecom-parity.sh`
  及 fixture 测试，在不输出任何值的前提下比较生产 baseline 与迁移副本的完整 WeCom 配置段和全部
  `WECOM_*` 绑定；配置位置在 `channel.wecom` 与 `platforms.wecom` 之间迁移不视为漂移；
- 已修正 Erhua profile overlay，使其保持生产基线中的 WeCom
  disabled 状态，而不是在后续 profile 激活时重新启用；
- 已重新计算历史 WeCom patch
  SHA-256，并通过显式审计更正使 inventory、manifest 和 README 一致；
- 本次只刷新了根分区容量；Hermes core dirty 状态、上游差异、profile
  parity 等完整生产 readiness 尚未重新采集，不能继续把历史结果当作当前事实；
- Hermes core artifact manifest、build/update receipt schema、validation
  summary 和 fail-closed verifier 已完成仓库门禁；deploy bundle 已自包含 YAML runtime；
- 已固定独立 root
  `/var/lib/qintopia-hermes-core`，并完成 root-owned 不可变 release、单一
  `lineage/active` generation、独占锁和无副作用 dry-run planner 的本地门禁；
- 已完成固定 root-owned ingress 到 `incoming/<commit>` 的双重 artifact 校验、原子落盘、
  `fsync` 和失败 quarantine fixture；
- 已完成固定 COS key 的 ingress 下载边界、流式安全解包、两个不同 clean
  release 的首次 bootstrap、immutable generation 和 `lineage/active`
  原子提交，并覆盖 rename、`fsync`、quarantine 与幂等重试的故障注入；
- ingress 已增加 600 秒下载 timeout、512 MiB archive 上限、1 GiB 解包总量、固定可信
  `coscli` 路径和最多两个失败 quarantine；tar 原始 header 预扫描会在 `tarfile`
  前拒绝 GNU sparse、异常 PAX/GNU 扩展头、错误 checksum 与截断；
- 已完成 repository-only 七服务事务协调器，保留每个 profile 原有的 active/inactive 与 enabled/disabled 状态；
- 失败时按 registry 逆序停止 candidate、恢复完整旧 lineage 和 enablement，再按 registry 顺序恢复原服务；
- HC-2 与 HC-3 完成仅限仓库代码和本地 fixture。尚未下载真实 clean
  artifact、尚未在生产 bootstrap，也未接入生产 runner、签名请求或 systemd；release-local
  runtime binding、HC-4 请求接线和 HC-5 staging
  replay 均未完成，因此当前仍不允许直接执行生产 Hermes 更新；
- 当前服务器 checkout、WeCom 补丁、七个 profile 的 WeCom 配置及启停状态均保持不变，Erhua 独立
  `qiwe-platform` 也不在 core 发布的修改范围内。

## 迁移阶段

### 阶段 0：容量、证据和回滚准备

1. 为 172 个 Agent OS release 目录和现有 Hermes 备份生成只读保留报告。
2. 保留 `current`、一个独立且验证过的 `previous`、最近的评审版本和必要审计证据。当前
   `current` 与 `previous` 指向同一 release，不能把它当成有效回滚点。
3. 由 owner 审批明确的保留与清理集合，再回收磁盘空间。开始安装依赖前至少保留 5
   GiB，目标为 8-10 GiB。
4. 记录 dirty Hermes 改动的脱敏清单、原 commit 和哈希，不复制 secret、真实消息或 runtime
   state。
5. 保持旧 checkout 不变，直到干净核心验收和回滚演练完成。

退出门禁：磁盘空间达标、存在独立回滚目标、补丁证据已评审。

### 阶段 1：保留 WeCom 配置并建立兼容清单

1. 以服务器当前 profile 为基线，保留 `default`、`guanerye`、`huabaosi`、`silaoshi` 和
   `xiaoman` 的 Hermes WeCom 启用状态，保留 `erhua` 和 `wenyuange` 的停用状态。
2. 保留每个 profile 的 WeCom 配置键、平台选择、目标解析规则、凭据引用方式、重连参数和媒体相关选项；只迁移配置结构，不把 secret 或真实目标写入 Git。
3. 在隔离副本中把 schema
   23/26 迁移到 41，并做字段级的非敏感语义对比，确认启用状态没有被迁移工具重置。对比使用
   `runtime/hermes/profile-registry.yaml`
   作为 profile 集合和预期启停状态的唯一来源；只输出 profile、状态、字段计数和脱敏指纹，不输出配置值或目标标识。
4. 对当前 WeCom 本地改动逐项归类为官方插件能力、通用上游能力或 Qintopia 特有策略，为每项指定替代实现和回滚点。
5. 在 staging 中验证启用 profile 能加载 WeCom 平台并按配置启动连接流程，停用 profile 不会启动 WeCom；是否当时有真实消息流量只作为观测指标，不作为保留门禁。

退出门禁：七个 profile 的启用/停用状态和配置语义与基线一致，WeCom 平台加载无配置解析错误，QiWe 路径不受影响。

### 阶段 2：迁移 WeCom 与其他核心行为到受支持边界

1. 按 [Huabaosi WeCom 迁移计划](huabaosi-wecom-migration.md)
   完成 WeCom 目标解析、媒体发送、过滤、重连和降级行为的兼容验证。通用能力优先使用官方插件或上游实现，Qintopia 特有策略放入 release-managed
   sidecar、插件或工作流。
2. 把 Silaoshi 的长时间 `script_action` 替换为 release-managed sidecar 或 job
   endpoint。新路径必须具备签名入口、受限 payload、基于 delivery
   ID 的持久幂等、异步执行、有限重试、成功/失败通知和脱敏审计记录。
3. 先执行 WeCom
   shadow 或受控 replay，再切换实现；只有消息分类、媒体、重连、过期回复、过滤和重复发送路径都通过后，才能删除对应 Hermes 本地补丁。
4. 先执行 Silaoshi
   webhook 的 shadow 或受控 replay，再切换入口；只有成功、失败和重复投递路径都通过后，才能退役旧 subscription。
5. 所有 Qintopia Kanban 生产者提供稳定的显式 `idempotency_key`。
6. 将 Huabaosi 完成门禁放入 Qintopia 插件 `pre_tool_call` hook。
7. 用自有 bridge 替换对 `hermes_cli.kanban_db` 的直接导入。
8. 切换前明确保留或排空活动中的 Hermes Kanban workspace。

退出门禁：WeCom 兼容 replay、Silaoshi
webhook、Kanban 幂等、完成门禁和 bridge 的定向测试全部通过，Qintopia 运行包不再依赖被移除的 Hermes 内部 API。

### 阶段 3：建立干净核心 staging

1. 创建独立的新 checkout 和 venv，固定到已批准的官方 tag 与 commit。不得在当前 dirty 生产 checkout 中运行旧 updater。
2. 仅将评审过的配置复制到隔离 staging home。不得把
   `.env`、session、日志、认证数据、cache 或状态数据库写入源码仓库。
3. 在副本上验证 schema 23/26 到 41 的迁移。
4. 对每个启用的 Qintopia 插件运行兼容性扫描和 plugin doctor。
5. 验证七个 profile 的 provider、model、cron、插件、WeCom 启用状态和非敏感配置声明没有丢失。
6. 对启用 WeCom 的 profile 做配置加载、平台初始化和连接错误分类 smoke；该 smoke 不发送真实消息，也不把凭据或目标标识写入输出。

退出门禁：staging
checkout 干净、配置迁移成功、插件兼容扫描为 0 命中、doctor 与离线 smoke 全部通过。

### 阶段 4：Canary 与七个 gateway 切换

1. 先验证一个低风险 profile，再把一个 gateway 切到新 checkout 作为 canary；其余六个 gateway 继续使用旧 checkout。
2. Canary 窗口内检查启动、provider、工具调用、cron、插件加载、更新回执和资源使用。
3. 单独验证启用 WeCom 的 canary
   profile：配置加载、平台初始化、连接/重连处理、消息分类和媒体降级行为均符合基线；没有实际流量时，以无副作用的 fixture 和连接观测替代真实发送。
4. 单独验证 Erhua 的 QiWe 入站与出站路径，确认它没有隐式依赖旧 WeCom 模块。
5. Canary 通过后分批切换其余 profile；每批都检查版本一致性、WeCom 配置一致性和定向 smoke。
6. 在所有 profile 验收完成前，旧 checkout 保持不可变并可由明确命令恢复。

退出门禁：七个 gateway 运行同一官方 commit，QiWe 链路通过，回滚演练能够恢复固定的旧核心和全部 gateway。

### 阶段 5：清理与持续升级自动化

1. 七个服务稳定运行干净核心并通过观察窗口后，只清理已经证明无引用的服务器本地补丁备份、旧 checkout 和嵌套仓库。
2. 保留所有仍被 profile 引用的 WeCom 凭据和配置；凭据只通过独立的轮换流程更新，不能因为当前没有流量而删除。
3. 在 deploy runner 中提供一个固定 Hermes 更新动作，执行下述持续升级契约并输出脱敏结果。
4. 直接 SSH 更新只保留为紧急路径，不作为日常操作方式。

退出门禁：清理目标均有无引用证据，runner 能完成 dry
run、正式更新、验收与自动回滚，操作手册不再要求修改 Hermes 源码。

## 持续升级契约

最终 runner 必须按以下顺序执行：

```text
只读核心 readiness
  -> hermes update --check
  -> hermes update --plan
  -> owner / release policy 审批
  -> hermes update --yes
  -> 校验最新 update receipt
  -> hermes doctor 和 hermes plugins compat --json
  -> 七个 profile smoke 和集群版本一致性检查
  -> 记录成功，或自动回滚到固定的 previous core
```

使用 updater 默认的 quick snapshot。日常更新不要传
`--backup`：目标 Hermes 版本中，该参数会同时创建 quick snapshot 和完整的 `HERMES_HOME`
压缩副本，在根分区空间受限时不适用。完整备份只能写入经过评审的外部存储，或在单独批准的维护窗口中执行。

runner 封装完成前，命令级流程为：

```bash
deploy/runner/check-hermes-core-readiness.sh
cd /home/ubuntu/.hermes/hermes-agent
hermes update --check
hermes update --plan
hermes update --yes
```

命令退出码为 0 仍不足以判定成功。`$HERMES_HOME/logs/update_receipts/latest.json`
必须报告 `outcome: success`；七个 gateway 必须都显示新的 code
identity；插件兼容性扫描必须为 0 命中；七个 profile
smoke 必须全部通过。runner 只保留版本、profile、步骤状态和脱敏失败分类。

## 回滚策略

- 在干净核心完成验收前，旧 checkout 只读保留，不能在其上继续开发或升级。
- `previous` 必须指向与 `current` 不同且实际启动验证过的核心版本。
- 任一批次失败时，只回滚该批次的 gateway；不得同时改变 QiWe、Qintopia
  release 或生产数据。
- WeCom 迁移阶段的回滚是恢复经过评审的对应 profile 配置和旧核心路径，不得通过删除配置来规避兼容问题。
- 干净核心切换失败时，恢复固定的旧解释器路径并重启受影响 gateway，然后运行七个 profile 的版本与服务 smoke。
- 只有回滚演练通过、观察窗口结束后，旧 checkout 和补丁备份才可进入删除审批。

lineage 的读取也属于回滚正确性边界：读取方必须先解析一次
`lineage/active`，再从该同一 generation 读取 `current`、`previous` 和
`rollback-reserve`，不得在活动指针切换期间分别读取三个顶层兼容 symlink。

## 验收标准

- Hermes remote 为官方源，branch 为 `main`，upstream 为 `origin/main`，worktree 干净，
  `origin/main..HEAD` 的 commit 数为 0。
- 七个 gateway 重启后使用同一官方核心，并报告已批准的 code identity。
- 所有 Qintopia 插件和 sidecar 都从 `/home/ubuntu/qintopia-agent-os-releases/current`
  加载。
- 七个配置均迁移到 schema 41，provider、cron、model、插件和 WeCom 设置没有丢失。
- 五个原本启用 Hermes WeCom 的 profile 在新版本中仍为 enabled，`erhua` 和 `wenyuange`
  保持原有 disabled 状态。
- 启用 WeCom 的 profile 均能加载官方 WeCom
  platform，并完成连接/错误分类 smoke；连接成功与否不改变配置保留结论。
- Erhua `qiwe-platform` 的入站、文本出站、媒体出站和错误降级 smoke 通过。
- WeCom DM、群聊、媒体、过期回复、重连、审批输出和用户可见过滤 replay 通过。
- `hermes plugins compat --json` 没有 deprecated import，plugin doctor 通过。
- Silaoshi webhook 的成功、失败、超时和重复投递测试通过。
- Kanban 显式幂等和 Huabaosi 完成门禁测试通过。
- 更新回执证明七个 gateway 均运行新版本。
- 回滚演练能够恢复固定的 previous core 和全部七个 gateway。
- 更新完成后根分区至少保留 5 GiB，建议 8-10 GiB。

## 风险与控制

| 风险                                     | 影响                       | 控制措施                                                                        |
| ---------------------------------------- | -------------------------- | ------------------------------------------------------------------------------- |
| 30 天日志未覆盖低频 WeCom 用户           | 迁移后出现未发现的行为差异 | 以现有配置为基线做 fixture/replay，启用 profile 分批 canary，保留配置和核心回滚 |
| QiWe 隐式依赖旧 Hermes 文件              | Erhua 微信消息中断         | staging 和 canary 中独立验证 QiWe，切换时不改 QiWe 配置或 release               |
| 七个 gateway 共用 venv                   | 单次错误影响全部 profile   | 新建隔离 checkout/venv，先单 profile canary，再分批切换                         |
| 根分区空间不足                           | 安装或回滚失败             | 阶段 0 先完成保留报告和审批清理，低于 5 GiB 时 fail closed                      |
| 配置跨多个 schema 迁移                   | 配置丢失或语义变化         | 只在副本迁移，对七个 profile 做字段级非敏感对比和 smoke                         |
| 直接依赖 Hermes 内部 API                 | 新版本启动或运行失败       | 切换前完成 bridge，兼容扫描之外增加实际调用测试                                 |
| `current` 与 `previous` 指向同一 release | 回滚名义存在但实际无效     | 建立不同版本且启动验证过的 previous，再允许切换                                 |

## 工作量估算

预计需要 2-3 个工程周，不含 owner 审批等待时间：

| 工作流                                       |   估算 |
| -------------------------------------------- | -----: |
| 容量清理、证据和回滚准备                     | 1-2 天 |
| WeCom 配置保留、兼容迁移、QiWe 验证和观察    | 3-5 天 |
| Webhook job 外置                             | 2-4 天 |
| Kanban 幂等、hook 和 bridge                  | 3-5 天 |
| Staging、canary、集群切换、runner 和回滚演练 | 3-5 天 |

每个阶段拆成可独立部署和回滚的变更。任何阶段都不能在首次启用替代路径的同一个变更中删除旧实现。

## 非目标

- 设计和准备期间不在当前生产 checkout 中热修改、删除文件、更新或重启服务。
- 不把生产 secret、原始消息、`.env`、session、日志、认证数据、cache 或状态数据库复制到 Git 或长期证据中。
- 不把 `git stash`、下游 fork 或补丁队列当作最终方案。
- 不把 Hermes 核心更新绑定到常规 Qintopia `release/current` 发布。
- 不因为 Hermes WeCom 当前没有实际流量就删除配置、凭据或本地兼容行为。
- 不在本计划中替换 QiWe 服务商、扩大 QiWe 权限或改变真实消息目标。
