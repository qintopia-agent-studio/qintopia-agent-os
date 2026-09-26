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

## 2026-09-25 受管发布修复

用户随后批准[生产接入方案](../plans/active/anan-production-rollout.md)。

先补现有岸岸服务的发布归属，再独立实现生产接线。

新增 hermes-anan target、请求及 smoke 合同，bundle 携带默认关闭的 PMS 源码。

不添加路径忽略，不新增 workflow/job；七 Profile 核心升级清单保持原状，避免改写岸岸核心入口。

历史 A 专项结论保留为当时状态，当前岸岸改为 managed，不表示 PMS 已启用。

解析器、Agent 检查及部署 runner 检查通过；模拟岸岸专属服务重启与失败传播测试通过。

Agent 管理 schema 与两个真实检查器的正反例均通过，bundle 构建与源码清单核对通过。

全量检查进行中。新增文档格式及本地 PATH 缺 sha256sum 已修正；不计为完整通过。

未修改生产配置或重启线上服务。安装、强凭据隔离和真实业务验收由后续 PR 完成。

### 后续 Light 检查暴露的模拟仓库路径问题

提交 `57786787` 的远端 Restart impact preview 已通过。

后续 Agent 管理测试将仓库复制到 Linux `/tmp`，嵌套 staging 权限检查拒绝其 1777 父目录。

改为仓库忽略的 `.local-workspace` 下创建并清理专属模拟仓库，保留所有父目录权限断言。

这是此前预览失败遮住的测试环境问题，不应放宽生产权限检查或跳过嵌套测试。

### business 检查的历史边界断言

主 CI 通过后，business 的岸岸模拟运行时用例仍断言生产中不能出现 hermes-anan。

这与获批接管现有真实服务的范围冲突。修正为验证模拟运行时不进入产物或启动链，同时继续验证岸岸未进入七 Profile 核心升级清单；不删除模拟隔离边界。

### 岸岸重启排空

新增目标不能直接 SIGTERM 中断业务。固定当前官方核心的可撤销 drain 协议，等待两次新鲜零工作快照后才重启；最多600秒，超时取消本次 drain 并暂缓，不强杀。

模拟覆盖在途任务、超时、失效快照、进程变化、已有/被替换 drain 及重启失败。未在生产请求 drain 或重启；这里只读核实官方接口。

### PR 722 凭据输入隔离

使用官方 v2026.9.21 在独立进程、空环境、虚构 Token 下验证：Skill 注册的自定义环境变量会经
`resolve_passthrough_env` 转发。三个 PMS/Foundation
Token 名称均可转发；未读取真实凭据，未启动工具容器或向业务系统发送消息。

因此仅配置远端执行不满足原隔离要求。PMS 插件增加 Profile 外私有文件输入，拒绝环境变量副本、软/硬链接、错误权限、重复 JSON 键和过大文件；失败不回退。本地模式保留，生产插件未启用。62 项 PMS 测试通过。此改动仅补凭据输入边界，实际工具不可达性、真实租户授权、安装和业务验收仍待完成。
