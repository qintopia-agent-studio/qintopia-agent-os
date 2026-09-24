# Resident Welcome V1

Owner: PatrickLiveCool。状态：draft，本地开发；风险：high。

唯一业务与 PMS 协议为[共同契约](../../docs/plans/active/unified-person-welcome-v1-contract.md)。实现位于
[resident_welcome](../../runtime/sidecar/src/resident_welcome/)，数据设计见[版本化设计](../../runtime/postgres/docs/data-design/2026-09-09-resident-welcome-v1.md)。

## Responsibility

人员任职、智能体训练授权和协作职责属于整个 Agent
OS 的统一设置；本流程只使用这些设置，不要求用户为欢迎重新任命小管家或舍长。具体约束见共同契约第 5.3 节。

新接入目标通过 `welcome_foundation_targets` 绑定共同 tenant/scope，执行前消费
`collaboration_grants` 与有效 Space 规则版本；这些目标拒绝旧 `welcome_grants`
执行路径。旧未接入 fixture 保留原行为以回归历史接收和恢复契约。

客房领域通过结构化 WorkItem 协调，阿靓由 huabaosi 处理产物，二花面向社区/楼栋，四老师处理内部协作。

本批本地任务由岸岸 `anan` 编排，阿靓 `huabaosi` 渲染受控卡片，二花 `erhua`
传递具体版本审核并转发。内部可信上下文从持久 WorkItem 的固定 Agent、capability 和来源构造；模型不能自行填 Agent 身份。不恢复 call_agent，不启用生产 Profile。

本地 Runtime 通过固定 `scripts/local_agent_runtime.py` 子进程加载
`fixtures/agents/<agent>/welcome_runtime.py`
中各参与者的本地测试工具，调用其真实注册工具。Rust 锁定 WorkItem 后构造 stdin 可信任务上下文；工具的模型参数固定为空。

岸岸返回制卡请求和逐部分执行计划，阿靓运行 Pillow 并返回 PNG，二花返回绑定具体 Artifact/hash 的审核或转发指令。Rust 实际使用这些结果，并保留最终身份、权限、内容版本与发送门禁。

每次调用保存 call ID、请求及结果 hash、插件身份、版本和源码 hash。这是独立插件进程的
`local_scripted_agent_runtime` 验证，不等于真实 LLM、生产 Hermes
Profile 或真实渠道验收。原通用 `qintopia_person_task_status`
只查询共同基础分派任务；欢迎状态通过受控对话和对应 WorkItem/Runtime 结果查询。

小满不是前置条件。

复用既有 Person、WorkItem、Artifact 和 work_item_events；来源链接的 namespace、逐人案例、申请修订、有效同意、目标范围与人类审批决定资格。

群和内容部分独立恢复，不等一小时。已保存的 `succeeded` 或 `unknown`
状态优先回读，不因执行器临时不可用改报等待，也不重新发送；只有尚未完成的动作才等待执行器恢复。

通用 Operations MCP 仍不承担本流程的真实任务写入。

## Production Boundary

本地 Store 必须显式连接 literal-loopback 的
`qintopia_test`，不使用环境中的生产数据库配置。影子数据使用独立数据库；所有真实制卡、附件上传、PMS 写和发送硬关闭。

有 synthetic-only
loopback 接收器/工作台，以及逐作用域的单次消费、扫描和回读函数。本地工作台每 15 秒恢复一次条件汇合；没有安装生产调度、部署或切换。

已有生产流程继续运行。回滚先停新动作并核对 unknown，不删除业务记录或把积压重新准入。未完成的外部映射和旧渲染器不能用模拟成功替代生产验收。

## Acceptance Scenarios

共同契约 T01—T16 是验收总表。包测试覆盖签名、body 字节保留、JSON 边界、附件追加、自写回调和影子硬门禁；隔离 Postgres 测试覆盖 durable
accept、冲突回滚、claim fencing、逐人绑定、同意、审核、入群/入住汇合及稳定 action。

PMS 事务回滚/晚提交、真实名单身份转换、卡片视觉验收与真实 provider 回执仍需要各自权威实现及联调，不能由本仓 fixture 证明。

详细覆盖和未完成范围见[本地验收记录](../../docs/reports/2026-09-09-resident-welcome-v1-local-acceptance.md)。

## Local Workbench

这是本次住宿关联和内容审核的合成验证页面，不是“小管家/舍长任职与权限”的正式设置入口。用户当前无需在此录入真实人员；统一人员与职责设置尚待实现。

`run-welcome-local --port 18870`
只监听 127.0.0.1，拒绝非 synthetic 来源。它要求显式设置以下变量，不读取生产 `.env`
或 Hermes profile：

- `QINTOPIA_WELCOME_LOCAL_ENABLE=1`
- `QINTOPIA_WELCOME_LOCAL_DATABASE_URL`：隔离的 loopback `qintopia_test`
- `QINTOPIA_WELCOME_LOCAL_SOURCE`、`QINTOPIA_WELCOME_LOCAL_PROPERTY`
- `QINTOPIA_WELCOME_LOCAL_OPERATOR_LINK`：夹具中已确认的独立人工身份
- `QINTOPIA_WELCOME_LOCAL_SIGNING_KEY`：仅合成验收使用的独立签名材料

数据库应用迁移后，可载入 `fixtures/local-workbench.sql`
查看合成空态。工作台支持查找、首次确认、新建 Person、撤销、住宿/申请绑定和文案审核。图片未连接可验证预览时禁用图片审批；不存在后台真实发送按钮。

`Store::scan_once`、`consume_one`、`pull_once`、`refresh_orders`
接受固定读取适配器。扫描页与代次/检查点同事务提交；`finish_scan`
不自动重新启用来源。生产调度须补充容量、权限、准入与回滚验收，不能直接把本地入口升级为生产入口。

## Validation

推荐使用仓库框架自动创建和清理独立随机端口数据库：

```bash
pnpm test:setup
RUSTUP_TOOLCHAIN=1.96.0 pnpm test:business -- --feature resident-welcome
```

两项 PostgreSQL 场景同时接入 CI 和
`pnpm check:pr:postgres`，不是默认忽略后的普通单元测试结果。测试框架适配仅在 test 编译中启用；本地工作台仍使用显式合成配置，不回退到生产数据库。

```bash
RUST_MIN_STACK=33554432 cargo test --manifest-path runtime/sidecar/Cargo.toml resident_welcome
QINTOPIA_WELCOME_TEST_ENABLE=1 RUST_MIN_STACK=33554432 \
  cargo test --manifest-path runtime/sidecar/Cargo.toml \
  --features postgres-integration-tests resident_welcome_postgres -- --ignored
pnpm workflows:check
pnpm check:pr:heavy
git diff --check
```

数据库测试还需显式配置 `QINTOPIA_WELCOME_TEST_DATABASE_URL` 指向本次独立的loopback
`qintopia_test`；测试会应用版本化迁移并写合成数据。没有此配置不会尝试其他数据库。不要把真实数据导入测试库。

## 共同基础欢迎链路（2026-09-22）

舍长通过共同工作台的二花对话，在有效业务授权内约定这项具体工作，不设置独立欢迎分类或配置页。业务设置由共享知识版本服务保存：固定
`resident_welcome`
key，支持完整内容或仅文案、直接发送或先审、单次或持续、未来生效和到期。单次设置仅覆盖对应案例，不修改默认规则。上层指定人确认、个人展示许可、真实入住、目标成员关系仍在执行前检查。

直接发送保留有效规则和发布授权依据，不伪造人工批准。审核记录绑定具体 Artifact、hash、目标、规则和审核人的现行授权。每个目标的图片和文字分别持久去重；unknown 先从测试 provider 的持久记录回读，不能再次发送。

Pillow 合成渲染器位于 `scripts/render_synthetic_card.py`，显式配置
`QINTOPIA_WELCOME_RENDER_PYTHON`
为有 Pillow 的本地 Python 绝对路径。脚本只接收虚构姓名和受限简介，不接受网络 URL 或附件；不访问真实资料。旧
`generate_card_v10.py`
仍没有可复用的本地版本化源码，当前渲染是独立的合成排版验证，不能称为旧脚本迁移验收。样卡返回同一 Artifact，附件测试适配器固定登记申请 Base 的稳定引用，未连接阿靓普通设计产出 Base。

本地方法 `bootstrap_foundation_fixture(tenant)`
只在显式合成 tenant 创建两栋住宿与申请、来源身份、成员关系和本地执行器登记；它不授予岗位或权限。
`refresh_foundation_fixture`
是人工演示时对固定合成来源的回读，刷新观察时间，不创建 live 事件、不升级历史准入。常规源码路径保留 60 秒观察时效门禁。

本地对话验收可显式设置 `QINTOPIA_FOUNDATION_FIXTURE_OBSERVE=1`，与
`QINTOPIA_FOUNDATION_LOCAL_ENABLE=1` 一起启动既有的本地服务。默认关闭；只在
`Store::local`
完成隔离 loopback 数据库校验、127.0.0.1 监听成功后，每 20 秒调用上述合成回读。

每次核验 tenant 已初始化为 synthetic、目标非空且全属于本 tenant 的固定合成来源，拒绝混合来源。只更新已有来源和成员的观察时间，不重设订单、成员 current 状态、申请、准入或业务版本，也不触发欢迎发送。

这是伴随本地验收进程存活的开发夹具，不是业务调度或真实 PMS 接入。延迟不会补跑；任一次失败后停用观察器并输出固定错误码，不自动重试。停止本地进程即停止观测；重新启动时须再次显式保留开关。人工验收记录应注明来源仍为合成观测。

专项验证：

```bash
python3 -m unittest discover -s workflows/resident-welcome/tests -v
cargo test --manifest-path runtime/sidecar/Cargo.toml \
  --features postgres-integration-tests foundation_welcome -- --ignored
```

第二条需已有测试框架的隔离数据库环境、明确本地 Python 路径和 Pillow。真实模型理解、渠道人员身份、批准入口、申请附件权限及真实群送达仍待实测。

## 制卡能力归属与测试替身

四老师现有 Runtime 已有可用的欢迎制卡能力，正式迁移目标是阿靓。排版、字体和渲染实现由智能体能力包持有；Agent
OS 负责受控任务、输入许可、Artifact 版本及审核／发送回执，不接管具体设计逻辑。

当前 `render_synthetic_card.py`
是本地虚构资料测试替身，由阿靓的本地测试插件调用。它不代表四老师现有实现尚未完成，也不代表旧能力已经迁移。后续应先定位四老师现有源码与依赖，保留原效果和样例，再迁移到阿靓能力包；不因 CI 通过而启用或替换生产渲染器。

## 自动化链路与视觉验收分离

CI 的 Rust 欢迎测试在 test 构建中选择
`scripts/test_card_artifact.py`，Python 协议测试也显式选择同一夹具。它生成固定像素、绑定输入摘要的 PNG，不模拟中文排版。

仍实际验证插件进程、输入边界、产物版本、审批撤旧和失败恢复。共享测试框架无需欢迎专用环境变量、Pillow 或字库。

本地演示保持显式的 Pillow 解释器配置。可选视觉检查：在独立 Python 环境安装
`requirements-visual.txt` 后，运行
`python3 -m unittest discover -s workflows/resident-welcome/visual-tests -v`。这只是合成排版；四老师真实卡片需另行联调，不作为 CI 的网络依赖。

岸岸本地替身位于 `fixtures/agents/anan/`，其真实 Hermes
Runtime 已由负责人建立，但本 PR 不接管该 Profile 或服务部署。二花与阿靓的欢迎测试替身也归入
`fixtures/agents/`；二花真实人员工具入口仍在原插件能力目录。

合成欢迎的参与者声明位于
`fixtures/agents/welcome.yaml`，仅供现有 synthetic 来源链路核验固定任务协议，不从生产部署清单推断测试执行器是否可用。
