# Agent OS 本地业务测试

这组文档规定 Agent
OS 的本地业务测试方式。它服务于两类使用者：不需要理解测试框架的协作方，以及负责定位、补充和运行测试的 Codex。先看
[Codex 操作指南](agent-guide.md)；需要编写场景时再看
[测试编写规范](authoring.md)，需要理解边界时看 [架构说明](architecture.md)。

## 先做什么

使用下面的命令。它们是仓库级稳定入口，具体实现和版本以 `package.json`
为准；调试时也可以直接调用 `node tools/testing/run.mjs <subcommand>`。

| 目的                     | 命令                                             |
| ------------------------ | ------------------------------------------------ |
| 检查本机依赖和环境       | `pnpm test:doctor`                               |
| 准备专用测试 Python 环境 | `pnpm test:setup`                                |
| 查看已登记业务和场景     | `pnpm test:list`                                 |
| 运行全部本地业务测试     | `pnpm test:business`                             |
| 运行某个业务的全部场景   | `pnpm test:business -- --feature <feature-id>`   |
| 运行单个场景             | `pnpm test:business -- --scenario <scenario-id>` |
| 打开最近一次 Allure 报告 | `pnpm test:report`                               |
| 打开指定运行的报告       | `pnpm test:report -- --run <run-id>`             |
| 检查清单和运行器自身     | `pnpm test:harness`                              |

当前支持 macOS 和 Linux（需要 Bash/POSIX 进程组）。Windows 请在 WSL2 中使用。本机先准备 Node.js
24、pnpm 10、Python 3.12+、Rust 1.96、Docker（含 Compose，daemon 已启动）和 Java
17+。Java 只运行现成 Allure 报告，不增加 Java 业务代码。先执行
`pnpm install --frozen-lockfile`，然后执行 `pnpm test:setup` 和
`pnpm test:doctor`。全部业务测试默认一次选择所有已登记场景；一个场景失败不会阻止其他独立场景运行，命令最后仍会以非零状态退出。运行资料写入
`.local-testing/runs/<run-id>/`，不会写入 Git 跟踪目录。

## 这套测试验证什么

业务场景执行真实的业务代码，数据库使用本地 Docker
PostgreSQL。模型 completion 和外部平台使用合成输入或本地假服务。

v1 默认只准备 PostgreSQL。NATS、MCP
transport 与真实模型评估是后续专项场景，当前不自动启动。场景支持
`pytest`、`unittest`、`cargo`、`node` 四种执行器。

基础设施依赖锁定为 pytest `8.4.2`、`allure-pytest` `2.16.0`、Allure CLI `2.43.0` 和 Java
17。数据库测试通过 `QINTOPIA_SIDECAR_DATABASE_URL` 指向本次本地实例，运行资料目录通过
`QINTOPIA_TEST_RUN_DIR` 传给子进程；两者都由运行器生成和保护，不能指向线上资源。

`operations-control-plane-apply-smoke.sh`
支持显式指定的随机本地 PostgreSQL 端口。脚本在执行任何数据库命令前，要求 literal-loopback、显式非特权端口和
`qintopia_test`，拒绝连接覆盖参数；仅允许
`sslmode=disable`。脚本将确切连接串的 SHA-256 传给双测试 feature 的 Huabaosi 分支，该分支再次核对地址和 hash。

这项绑定只在 `postgres-integration-tests`、`huabaosi-staging-adapter`
和 apply-smoke 显式开关同时存在时有效。普通 staging 和生产仍使用原审批及数据库边界；不把随机测试连接加入 reviewed
allowlist。子命令失败会立即终止 smoke，不继续解析空 JSON。

本地确定性测试不需要生产凭证，也不向真实 channel 发送消息。报告中的“发送成功”只表示模拟 channel 收到了符合契约的请求并返回了脚本响应，不表示真实用户已收到消息。真实模型质量、平台权限和实际送达属于单独的评估或受控线上验证。

框架分为四层：包内单元/契约测试、跨模块业务场景、进程和传输专项、真实模型评估。现有 Rust
`cargo test`、Python `unittest`
和 Node 检查继续保留；本地业务套件负责把长链路放在一个可重复入口中，不要求迁移所有旧测试。

## Codex 如何使用

用户可以直接说“测试二花早报是否会重复发送”或“运行全部业务测试”。Codex 必须先按
[Codex 操作指南](agent-guide.md)
查找功能、阅读已有测试并判断覆盖情况，再选择已有测试或补充测试。找到一个同名测试不等于本次行为已经覆盖。

测试清单是检索入口，不是覆盖事实的替代物。测试实现、fixture 和运行结果才是最终依据。新增业务场景要登记到
`tools/testing/catalog.json`，让定向和全量命令都能发现它。

## 结果怎么看

Allure 报告按业务和场景展示状态、步骤、耗时、预期/实际结果及合成证据。状态含义如下：

| 状态          | 含义                                              |
| ------------- | ------------------------------------------------- |
| `passed`      | 断言和必要步骤均通过                              |
| `failed`      | 业务断言失败                                      |
| `broken`      | 测试运行器、子进程或环境错误                      |
| `skipped`     | 明确未执行；必须场景被跳过时不能宣称全量通过      |
| `interrupted` | 运行被中断；未执行场景在 Allure 中显示为 `broken` |

每次运行使用独立的结果目录。报告之外还会保留机器可读摘要和经过筛选的调用轨迹，便于 Codex 根据“哪一步、预期是什么、实际是什么”解释失败。

## 当前案例边界

第一条参考案例是二花早报的**文本发布链**：合成输入 → 真实早报业务逻辑 → 本地状态/审核/确认 → 文本发送 worker
→ 模拟 QiWe 请求。它不覆盖卡片或图片上传路径，也不把真实 Hermes
08:10 定时器当作本地场景已经执行。它默认使用模拟 channel，因此不能证明真实 channel 的权限或送达。

新增场景时必须明确哪些代码真实执行、哪些外部边界被模拟，以及哪些能力仍未覆盖。详细字段和断言规则见
[测试编写规范](authoring.md)。

人员与欢迎业务的 PostgreSQL 场景覆盖授权生命周期、身份与记忆、知识确认、持久欢迎流程及恢复边界。
`person-foundation` 另登记二花可信工具 SDK、岸岸独立注册和真实合成 PNG 渲染。可通过
`pnpm test:business -- --feature person-collaboration` 或
`pnpm test:business -- --feature resident-welcome` 定向运行；SDK 和渲染器可运行
`pnpm test:business -- --feature person-foundation`。这不证明真实模型理解、真实身份接入或真实消息送达。

欢迎卡片渲染依赖 Pillow，由 `pnpm test:setup`
安装到专用测试环境。运行器将该环境的 Python 路径通过 `QINTOPIA_WELCOME_RENDER_PYTHON`
注入子进程，不继承外部同名变量。本地仍须安装可用字体；macOS 使用系统中文字体，Linux 推荐 Noto
CJK。

## 本地可视化

执行时 Codex/终端显示
`[当前序号/总场景数] 业务 · 场景`，每个场景结束立即显示状态和数量。执行完成后运行
`pnpm test:report`，Allure 服务只监听 `127.0.0.1`；按 Ctrl-C 关闭。Allure
2 是运行后报告，不是实时测试控制台；不额外开发网页或报告后端。

- Overview：本次通过、失败、异常、跳过比例和总耗时。
- Behaviors / Suites：按业务展开场景，筛选失败项；左下角语言菜单可切换中文。
- 场景详情：早报案例展示实际步骤、断言失败和 JSON/请求附件。
- 旧测试以测试组展示，原生用例数量和原始输出在步骤/附件中；报告组数不等于原生用例数。

`pnpm test:business` 表示当前清单中全部已登记场景/测试组，不代表仓库所有业务已有覆盖。
`pnpm test:list` 同时列出缺口。协作方参考
[早报案例说明](../../workflows/erhua-morning-brief/tests/business/README.md)。

Docker 镜像下载失败时先检查本机 Docker 网络，恢复后重跑；运行器不会改 daemon 配置。每次运行使用随机端口和独立卷，场景间核对数据库归属后重新执行 migration；首轮 Rust 编译较慢，后续复用编译缓存。

若业务测试已通过而 Allure 报告提示找不到 Java，先核对已安装 JDK 17 的路径，并将
`JAVA_HOME` 和该 JDK 的 `bin`
加入运行命令环境。不要重复安装已有 JDK，也不要把报告生成失败标成整次运行通过；保留原运行状态，可用其已有
`allure-results` 重新生成报告。

本机默认 Rust 与 CI 不同时，先执行 `rustup toolchain install 1.96.0`，再通过
`RUSTUP_TOOLCHAIN=1.96.0 pnpm test:business` 或
`RUSTUP_TOOLCHAIN=1.96.0 pnpm check:pr:auto`
对齐版本；运行器会保留这个显式选择。不要为了较新 Clippy 的额外 lint 修改无关业务代码。

交付记录：[已完成计划](../plans/completed/local-business-testing.md) ·
[验收结果与边界](../reports/2026-09-13-local-business-testing.md)。

## 含空格路径下的发布检查

2026-09-22 在基线 `0806c2e` 发现：`pnpm check:pr:auto` 的 deploy runner 检查可能在
`test-collect-release-deploy-results.mjs` 读取 `results.json`
时失败。收集器将文件路径与保留 URL 编码的 `pathname`
比较，含空格路径使 CLI 主程序未执行。用
`node tools/deploy/test-collect-release-deploy-results.mjs`
可定向复现；这不是生产部署结果。

修复应正确转换文件 URL，并补含空格路径回归；不要把子进程零退出码或无空格目录中的通过当作当前工作区全量通过。证据与待办见[验证记录](../reports/2026-09-22-community-business-model-validation.md)。

## Staging smoke 测试的路径隔离

`tools/deploy/test-qiwe-image-staging-smoke.mjs` 在每次调用独占的临时源码根创建
`sidecar/` 合成打包路径，并逐字复制当前 checkout 的 staging shell 和 evidence
checker。它不再修改共享 checkout 的
`sidecar/`。临时源码根仍包含空格；路径、文件类型、属主、只读权限、内容哈希及执行前重新核验继续由未修改的 staging
shell 检查。

若 upload 或 callback 报路径可写，保留原始错误和当时具体路径的 mode、uid、时间戳。不要改仓库或系统目录权限来绕过检查，也不要把定向通过记作完整广验通过。原共享夹具中的权限变化来源尚未确定；隔离修复消除了共享路径耦合，不证明原失败由并发导致。详见[原始失败与隔离修复记录](../reports/2026-09-22-community-business-model-validation.md#staging-smoke-共享夹具隔离修复)。
