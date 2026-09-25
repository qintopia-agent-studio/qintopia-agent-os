# 一次性 PostgreSQL 图像 apply 测试边界评审提案

日期：2026-09-24。审阅基线：`201b961`。状态：仅提案，未获实施批准。

本次只准备评审资料。不得据此修改固定 allowlist、CI、编译条件或部署门禁。依据：[CI change approval rule](../../engineering/programming-agent-guardrails.md#ci-change-approval-rule)。

## 结论与诊断更正

建议将下述通用测试契约调整提交负责人独立评审；现有门禁保持不变。固定端口的既有方式实际上可行，但本机该端口属于另一任务，不能占用或写入。因此本提案不是证明所有本地适配都不可能，也不把修改门禁当成阶段交付前提。

上一轮报告仅依据历史 staging 报告，误判两项哈希均不可用于本地测试。进一步核对
`image_generation.rs::ci_disposable_database_url_is_reviewed_explicitly`
并计算仓库公开的模拟 URL，证实：

```text
postgres://postgres:postgres@127.0.0.1:5432/qintopia_test
SHA256 = c6dc2730b2a3fdabf05d88e021340b748c5c5b5d06d8ec24b38feef387d39330
```

该值就是 allowlist 第一项。第二项 `be30c6fe…`
为历史获批 staging 哈希，见[历史记录](../../reports/2026-07-27-xiaoman-staging-database-hash-drift.md)。

历史文档对第一项的命名不替代当前代码和哈希计算证据；未查找任何 staging 凭据。

本机只读 `lsof` / `ps` 证明 `127.0.0.1:5432` 已由 PID 47613 的 PostgreSQL
17 占用，数据目录属于根工作区
`.local-workspace/foundation-batch1/pr720-reconcile-postgres`。本线不停止、重配、复用或写入该实例。若维护者另行提供无冲突的隔离宿主，使用上述固定 URL 可以沿用原契约；这种环境调度不需要本提案中的代码调整。

当前任务库端口为 52316，完整 URL 哈希不在 allowlist。换本任务用户名、口令或端口不能同时保持上述精确哈希与隔离目标。也不通过本机 5432 代理、DNS、连接参数或转发绕过 URL 约束。

## 九项审批依据

| 标准              | 结论、范围与依据                                                                                                                                           |
| ----------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1. 实际问题       | 通用测试 runner 允许一次性随机端口库，最后图像 apply smoke 却要求固定 URL 哈希；并行隔离实例无法完成该层。不是 PMS 业务失败。                              |
| 2. 改几个         | 拟修改 1 个 Rust 文件、2 个运行说明文件；0 个 job、0 个 CI 步骤、0 个依赖、0 个新 feature、0 个新环境变量、0 个 allowlist 条目。具体函数及测试数量见下文。 |
| 3. 依据           | 失败日志、当前校验调用顺序、默认 CI URL 正面测试、现有 runner 与通用校验函数，均列于下文。                                                                 |
| 4. 维护成本       | 不新建测试服务、配置注册表或专属测试入口；复用既有纯 URL 校验函数，避免复制第三套规则。                                                                    |
| 5. 可否环境适配   | 独立宿主的固定 5432 可行；当前主机已被其他任务占用，不能擅自调度。业务代码不能合法更改数据库边界，改预期或跳过失败也不能解决。                             |
| 6. 最小可用       | 仅为既有通用 disposable 模式选择受限 URL 校验，保留普通 staging 和 production 路径；不修改检查编排。                                                       |
| 7. 不加业务特例   | 不匹配岸岸、PMS、测试名称、迁移编号、端口或用户；所有相同双条件的 disposable apply 使用同一规则。                                                          |
| 8. 不混入生产部署 | 不增改发布 workflow、manifest、安装包或 runner 执行代码；测试 feature 仍不能作为生产制品。                                                                 |
| 9. 后续复用       | 任意同主机并行一次性 PG 实例均可使用；沿现有通用本地 URL 合约，不形成单项测试豁免。                                                                        |

## 当前失败证据与复用比较

- `.local-workspace/anan/heavy-remainder.log`：`pnpm check:pr:postgres` 最终退出 1。Rust
  PG 用例此前通过，包括 person_collaboration 77 项、resident_welcome 24 项。
- 失败命令：`deploy/sidecar/scripts/operations-control-plane-apply-smoke.sh`。主错误为
  `database URL hash is not in the reviewed allowlist`；JSONDecodeError 是次生错误。脚本此前已执行模拟写入，不能声称整个 smoke 在所有 I/O 前失败。
- `image_generation.rs::staging_apply_config` 调用
  `validate_staging_database_boundary`，后者先查固定哈希，再判断 disposable URL。
- `disposable_postgres_smoke_enabled` 已要求测试 feature 与显式开关，
  `validate_disposable_test_adapter_boundary` 已限制 provider/media 为 loopback。
- `deploy/runner/README.md` 的 disposable 规则明确包含 approved URL
  hash。移除测试分支对此哈希的依赖属于门禁语义变化，必须审批，不能称作普通环境修复。

| 已有实现                                                      | 可复用程度与选择                                                                                                                                |
| ------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| `person_collaboration::validate_local_database`               | 推荐直接复用；纯校验、无连接/写入，限制 postgres 协议、精确 loopback、精确库名、无 query/fragment。共享函数本身不修改。                         |
| `foundation_test_support::normalize`                          | 仅在 test + PG feature 中编译，CLI smoke 无法直接调用；还涉及环境选择与去掉 sslmode。不扩大其编译范围。                                         |
| image 模块测试内 `validate_postgres_integration_database_url` | 只校验 host/path，不拒绝 query/fragment，不能提升为通用安全边界。本提案不扩大使用它。                                                           |
| `tools/ci/run-local-pr-checks.mjs::isPostgresReady`           | JS 前置检查允许无 query 或单一 sslmode=disable；不能替代 Rust 在实际 apply 前的校验。保持原样。                                                 |
| `tools/testing/lib.mjs::startDatabase`                        | 通用 harness 生成随机端口并附 sslmode=disable。当前提案只接受无 query；显式传原生无 query URL 的 smoke 可用，不能声称直接支持所有 harness URL。 |

最小版本拒绝所有 query，包括
`sslmode=disable`，不为兼容 harness 暗中去掉参数。当前失败使用的 52316
URL 本身无 query，足以验证最小调整。将来若要支持该单一参数，须单独审阅规范化与实际连接同一 URL 的契约。

## 拟修改文件、函数与控制流

实施补丁限定 3 个文件；超过以下范围需重新说明，不能顺手重构：

1. `runtime/sidecar/src/image_generation.rs`：修改 3 个现有非测试函数：
   `validate_staging_database_boundary`、`validate_staging_database_boundary_with_allowlist`、
   `disposable_postgres_smoke_enabled`。修改 3 个既有测试函数，新增 2 个测试函数。
2. `deploy/runner/README.md`：仅更新既有 disposable 数据库边界条款，说明双条件及无 query。
3. `docs/testing/agent-guide.md`：更新本地失败处理及复测说明，保留历史失败链接。

不修改 `staging_apply_config` 的 owner approval、适配器校验调用；不修改
`production_apply_config` 或
`validate_production_database_boundary`。固定 allowlist 字节原样保留；本提案及验收报告后续只更新批准和验证记录。

建议控制流（伪代码，仅供评审）：

```text
validate_staging_database_boundary(url, requested_disposable):
  #[cfg(feature = "postgres-integration-tests")]
  if requested_disposable && disposable_postgres_smoke_enabled():
    return person_collaboration::validate_local_database(url)
  return validate_staging_database_boundary_with_allowlist(url, fixed_hashes)
```

测试允许分支必须使用 `#[cfg]` 编译隔离，不能只相信调用者传入 true。
`disposable_postgres_smoke_enabled` 在无 PG feature 时恒 false；有 feature 时要求
`QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE` 精确为
`1`，不 trim 接受空白变体。进入该调用的实际 CLI 还需编译
`huabaosi-staging-adapter`，并通过原 staging owner approval。

普通 helper 去掉 disposable 布尔参数，只保留原固定哈希与 staging 库名校验。因此非测试编译即使传入 true 或设置开关，也不允许
`qintopia_test`。调用点限定为当前 staging wrapper 和测试，批准后实施前再次核对全部引用。

复用的 URL 校验限定 `127.0.0.1` /
`[::1]`，不泛化到整个 127/8；允许独立合法端口与模拟用户名，不增加业务例外。校验原字符串、连接同一字符串。provider/media 仍需通过既有 loopback 校验；无自动读取生产配置或获取凭据动作。

## 验证矩阵与验收标准

拟修改既有 `staging_database_boundary_requires_reviewed_hash_allowlist`、
`disposable_database_exception_requires_exact_loopback_test_boundary` 和
`ci_disposable_database_url_is_reviewed_explicitly`，新增表驱动 URL 负面测试与编译/开关组合测试，各 1 个函数。总计修改 3、新增 2 个测试函数，多案例参数化。环境组合通过独立测试进程设置，避免并发修改环境影响其他测试。

| 条件/输入                                                             | 必须得到的结果                                             |
| --------------------------------------------------------------------- | ---------------------------------------------------------- |
| staging + PG feature + 开关1，随机端口127.0.0.1/qintopia_test，无参数 | 允许数据库边界；仍需 owner approval 与 loopback adapter    |
| 同条件，`[::1]`                                                       | 允许 URL 边界；不把 URL 测试称作实际 IPv6 服务验收         |
| 无 PG feature，即使开关1、调用方true                                  | 拒绝测试库，保持固定 staging 哈希要求                      |
| 有 PG feature，开关缺失、0、true、带空白1                             | 拒绝测试库                                                 |
| 缺 staging feature                                                    | staging apply 不可用；production 分支不借测试 feature 放行 |
| localhost、域名、远端IP、127.0.0.2、IPv4映射IPv6                      | 拒绝；不做 DNS 解析补救                                    |
| 非postgres协议、缺host、其他库名、额外路径、fragment                  | 拒绝                                                       |
| `?host=`、`hostaddr`、`port`、`dbname`、`service`、`options`          | 全部拒绝                                                   |
| `?sslmode=disable`、空query、重复参数、编码参数名、混合参数           | 全部拒绝，不在校验后偷偷去参数                             |
| 本地库正确但 provider/media 远端，或媒体白名单含远端                  | 既有 adapter 边界拒绝，不能发生外部调用                    |
| 未获 staging owner approval                                           | 拒绝，测试模式不代替批准标识                               |
| 普通 staging 已评审哈希/错误哈希                                      | 延续既有注入测试：正确通过、错误拒绝；不连接真实 staging   |
| production release/hash/name 错配，开关任意                           | 延续既有 production 拒绝行为                               |

编译与行为验证分别运行默认、staging-only、staging+PG、production-only 四组，每组通过独立子进程核对开关缺失与精确1；不得只跑 all-features 就宣称生产隔离成立。追加既有 no-default/all-features
warning-denied Clippy 和受影响测试。生产验证复用
`production_database_boundary_requires_hash_and_production_name`、现有 owner/release 边界测试及部署 feature
contract 检查。

注意：固定两项哈希属于 staging 校验。实际 production 使用独立的获批数据库哈希环境值、release
SHA 与生产库名验证；不能把 production 误称为走 staging 固定两项列表。无 PG
feature 的 staging 构建继续走旧固定列表，production 路径完全不改。

批准并实现后，仅在本任务隔离库补跑失败的完整 apply
smoke，保留新日志及退出码；若出现后续不同失败继续记录，不能因通过此处就宣称整个 tier 完成。随后根据实际改动运行适用 broader
tier；原 `check:pr:auto` 历史失败不改写为一次全绿。

## 影响、替代与回退

实际放宽点：双条件测试构建从“固定 URL 哈希”变为“复用通用受限本地库边界”。无法单凭 URL 证明该实例归当前任务；任务仍须核对实例所有权，禁止写别人的同名本地库。

保持数据库精确名称、literal-loopback、无覆盖参数和全部外部适配器限制。测试二进制不构成生产强隔离，严禁晋升生产 artifact。

无代码替代：在独立隔离宿主使用现有固定 CI
URL；或由拥有者协调释放固定端口后，再以任务自有新实例运行。当前不获准操作另一个任务实例，因此保留此备选待调度。

拒绝新增岸岸端口哈希、复制完整业务流程、跳过 smoke 或削弱断言的方案。推荐负责人评审通用小切片；若倾向零门禁变化，采用独立宿主方案，同样可接受。

回退为撤回批准后的这一个实施提交，恢复原函数和两处说明；无迁移、无生产配置变化。保留所有测试日志及实例数据，不在回退时删除证据或重置其他任务数据库。本次文档提交不是实施批准；总指挥负责汇总评审，负责人决定，开发线按具体批准范围执行。
