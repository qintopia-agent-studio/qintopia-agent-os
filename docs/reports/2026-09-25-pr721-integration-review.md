# PR #721 集成检查与审查

## 范围与版本

唯一 Agent
OS 集成 PR：[#721](https://github.com/qintopia-agent-studio/qintopia-agent-os/pull/721)。首轮 head 为
`0414b23dc81a10c95641c31bd5b794181daa4063`，base 为
`b42db0d8b8388c65dcf62c1f1fd41a7325298c5e`，包含 132 个文件。后续新 head 必须重新读取 CI、完整 Reviewer
Guide、reviews、conversation 和 inline。

## Light check：新包生产归属阻断

[首轮 Light check](https://github.com/qintopia-agent-studio/qintopia-agent-os/actions/runs/36087118153/job/107921416429)
在 2026-09-25 02:39 UTC 的 Restart Impact preview 退出 1。解析出
`qintopia-system-services,hermes-erhua,hermes-silaoshi`，但新增 `skills/pms-operations/`
文件未匹配生产归属，最终报：

```text
Restart target resolution failed for unmatched production-adjacent files.
```

这是新发现的远端 Light 检查失败，与此前本地 apply-smoke 的 database URL hash
allowlist 失败是两个独立事项，不能混为一项。现有 A 专项仅批准 Agent 身份与受管 Profile 部署分离，不自动赋予新 skill 生产归属。保留 resolver、restart
rules、payload、生产 registry 和 CI，不以扩大通配规则或移除检查绕过。具体归属处置交总指挥协调。

本地原 apply-smoke 失败历史仍保留；当前 CI 的 PostgreSQL
integration 已有审核过的独立 Ubuntu/pgvector16 容器路径，在
`127.0.0.1:5432/qintopia_test` 运行同一 smoke。首轮精确 head `0414b23` 的 PostgreSQL
integration 已于 02:45 UTC SUCCESS，完整 job 日志末端明确
`operations control-plane apply smoke passed`；无需新增 allowlist。同 head Runtime
check 和 Rust quality
baseline 也已 SUCCESS；Light 及其聚合 check 仍失败；business 也已 SUCCESS，不称整个 PR 检查通过。

## 首轮 Reviewer Guide 处置

完整读取首轮 Reviewer Guide；它明确标记 partial
coverage，大多数文件因预算未覆盖。PR-Agent
SUCCESS 不是全面审阅或合并批准。该轮没有已提交 review 或 inline thread。

唯一建议为 `store/business_manual.rs:232` 普通 SELECT
EXISTS 可能允许同租户并发采纳同一订单。沿实际调用链核查后，该候选不成立，处置为不修改代码：

1. `store/business.rs` 的 `business_invoke` 在进入所有动作分支前调用 `self.begin()`。
2. `store.rs` 的 `Store::begin` 对当前 tenant 的 `collaboration_tenants` 行执行
   `SELECT ... FOR UPDATE`，获得锁后才读取时间、验证身份并继续业务。
3. `business_manual::adopt_order` 唯一调用使用同一个 `&mut tx`；冲突查询、提案保存和
   `manual_completed` 更新均在事务内，函数返回后由调用方提交，期间未释放租户锁。
4. 因此同 tenant 的竞争提案与确认串行执行，后续事务会看到前一事务提交的订单占用；不同 tenant 不在该冲突查询的业务范围内。已有锁足以处理本条建议，无需重复加锁。

## 归档与生产边界

受限本地归档包括三套模拟 PG（52322、52319、52316）的逐库逻辑备份、角色资料、配置、日志、文件 hash 和恢复说明。运行中的 PG
data 目录不作为可靠备份复制；排除 toolchain、venv 和可重建 receiver 二进制。备份状态不等于服务已停用。七个数据库已在独立 PG17/52325 完整恢复，
`pg_restore --exit-on-error`
均通过；三套业务库分别恢复126、85、124张表。隔离恢复演练服务已正常停止，原模拟库未改。

未部署、未发布、未合 Release
PR、未真实发送或写真实业务；模拟服务与集成工作树保留到必要验证及总指挥收尾通知。真实模型、渠道、部署、来源空号提交拦截仍待外部验收。
