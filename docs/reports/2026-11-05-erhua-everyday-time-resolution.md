# 二花接龙日常时间表达修复

日期：2026-11-05。范围：`skills/qiwe` 时间解析、首次记录回复及本地回归测试。

## 原因与修改

本地模拟正文同时包含“今晚7.30”和“今天一起学习”时，旧代码按文字统计相对日期，将“今天”和“今晚”判为多个日期；单独解析“今晚7.30”又将点号数字匹配为 7 月 30 日。失败后的回复仍称“时间二花也记下了”，造成已确认时间的误解。这是源码与本地复现结论，没有读取该条线上活动的解析记录。

- 有明确时段的 `今晚7.30`、`明晚 7.30`、`今天晚上7.30`
  及全角点写法按时刻解析。保留原文证据；裸 `7.30` 和 `2026.7.30`
  继续表示日期，不凭空补时刻。
- 相对日期按接龙首次发起时间及配置时区比较实际日期。同日表达可共存，不同日仍需确认；显式日期必须同时符合每一条原始相对日期约束。
- 待确认回复显示“接龙里的时间写的是「…」，具体开始时间还待确认”，并说明尚未安排提醒。只有已解析时间使用原有确认时间的回复；发送开关、允许群和模拟运行门禁不变。
- 在现有提醒生命周期套件增加 9 个测试方法，并登记业务目录
  `qiwe/activity-reminder-lifecycle`。没有修改 CI workflow、检查器或共享测试框架。

## 验证

- 新测试先在修复前出现预期失败。最终 QiWe 原生套件运行
  **335 项，334 项通过，1 项 Linux 专属检查在 macOS 跳过**。
- 提醒生命周期文件现有 32 个测试，覆盖模拟模型 completion
  → 真实活动服务与 JSON 台账 → 提醒计划和回复。
- 模拟锚点为 2026-09-17：19:30 开始，仅创建一条 19:00 提醒；跨日、不可信事实、无效时刻及日期后缀均不产生错误安排。
- `pnpm test:harness`：8 项通过；新增目录可由 `test:list --feature qiwe` 发现。
- `pnpm pr:doctor`：通过。
- 本次修改文件的 `git diff --check`、Prettier 和 Markdown 检查通过。
- `pnpm check:pr:auto`：在允许本机模拟端口的环境中通过 quick
  tier，包含部署工具、协作和权限契约检查。

本机普通 `python3` 和既有 `.local-testing/venv` 为 Python
3.9；原生检查使用已安装的 Python 3.12，通过无空格的临时 PATH 入口执行
`pnpm test:qiwe`。最初的解释器入口路径含空格，导致图片回调测试的 shebang 启动失败；改用无空格入口后，5 项失败全部消失，没有修改这些测试或业务代码。

RTK 不在当前 PATH 或常用安装位置，使用原生 pnpm 命令。

统一业务入口的运行 `20261105120219-ac2bdf9cc6` 保留为失败证据：它固定使用旧 Python
3.9，已有 3 个提醒 worker 测试因 asyncio 事件循环初始化失败；随后 Allure 因找不到 Java
Runtime 未生成 HTML 报告。对应业务代码已在 Python
3.12 原生套件通过，不能把本次统一入口报为通过。日志与摘要保留在
`.local-testing/runs/20261105120219-ac2bdf9cc6/`；未覆盖或替换已有虚拟环境。

`pnpm check:pr:auto` 首次选择 quick tier，运行到本地 NATS 模拟预检时，被沙箱禁止绑定
`127.0.0.1`
端口。随后在允许本机端口的环境复验通过；期间新增报告的一处 Markdown 长行已修正。最终日志保留在
`.local-workspace/erhua-time-20261105/check-pr-auto-final.log`。

## 交付与生产边界

本地修复保留在原工作区。发布分支 `codex/erhua-everyday-time-release` 基于最新主线
`72c5698f`，只转入本次改动，保留其他任务的工作区和暂存状态。验证使用模拟身份、正文、模型响应和渠道结果，没有真实模型调用、线上配置更改、真实消息发送或历史提醒重放。

修复通过 PR 751 合并，发布 PR
748 生成 v0.3.11。部署结果见后文；真实群提醒送达仍需新接龙验收，未自动重放旧接龙或补发过期提醒。

## 上线准备

负责人已于 2026-11-05 明确授权本次修复上线。只读核对确认生产 `current` 为
`afe3e81c7c5ac0d74332dfb5aaa79fa35ce06d83`（v0.3.10），二花插件链接指向该发布，Gateway 正常运行。

默认 SSH 身份认证失败后，负责人提供当前授权连接方式，后续只读连接成功；连接细节仅保留在私有记忆，不进入 Git。

发布分支仅含本修复及文档、测试。正常 Release
Please 和 CI 通过后发布正式版本；仓库现有发布开关为模拟部署，保留该开关，制品验证后使用既有正式部署入口提交准确版本的非模拟请求。

当前 `skills/qiwe/**`
的重启归属只含系统服务。沿用现有发布规则，不修改 CI；实际切换前停止二花 Gateway 并一致备份服务器私有接龙台账，切换后明确重启二花，验证新进程、插件版本、健康和非发送时间解析。

本次不替换 Profile、凭据、会话或记忆，不升级 Hermes
core，不启用新的业务开关。回滚目标为本次切换前的 v0.3.10；按固定 rollback 流程恢复完整制品，保留新的台账结果。

本段为上线准备记录，实际执行结果见下文。

## 发布验证与制品

本节执行时间为 2026-11-06（Asia/Shanghai）。修复 PR
[#751](https://github.com/qintopia-agent-studio/qintopia-agent-os/pull/751) head 为
`2b4ad22826becc82784dad7ded6f4826c9afde0e`，合并提交为
`72b1b325c221417706947d7a7477e49ecfccebff`。发布工作区 QiWe 全套和 `check:pr:auto` quick
tier 再次通过。

- [业务测试](https://github.com/qintopia-agent-studio/qintopia-agent-os/actions/runs/42055600978)
  通过。

  179 个场景通过，644 个测试执行，0 跳过；提醒生命周期 32 项通过。

- 两轮修复 CI 均通过；完整 Reviewer Guide、reviews、comments 和 inline
  threads 无待处理建议。
- Release Please
  [#748](https://github.com/qintopia-agent-studio/qintopia-agent-os/pull/748) head
  `4fde08976b231322baa1f797685a37f64584d58a` 仅修改版本 manifest 和 CHANGELOG。
  [手动发布验证](https://github.com/qintopia-agent-studio/qintopia-agent-os/actions/runs/42058610209)
  的 check、Rust quality baseline、PostgreSQL integration 和 Release Please
  validation 全部通过，authenticated PR-Agent no-review 验证通过。
- [v0.3.11](https://github.com/qintopia-agent-studio/qintopia-agent-os/releases/tag/v0.3.11)
  精确提交为 `c71f68da1ff3e4472623be7c6f30a3aeb4598e95`。
  [构建及模拟部署](https://github.com/qintopia-agent-studio/qintopia-agent-os/actions/runs/42060724607)
  成功；服务器请求 `deploy-20261105T161616Z-c71f68da1ff3` 返回
  `dry_run_succeeded`，current 仍为 v0.3.10。

Git SSH 在发布身份复查时发生认证失败，改用现有 GitHub
CLI 凭据通过 HTTPS 读取，未修改持久 Git 配置或搜索密钥。发布工作流独立核对标签与当前主线提交一致。

模拟部署会清理 staging，并不保留新版本安装目录；因此模拟结果以签名请求及服务器结果核验，正式切换后再检查安装 manifest 和文件哈希。

## 停服异常与一致备份

00:19:09 发起官方 drain，取得两次零活跃观测后停止二花。旧 Gateway 在 00:19:12 以退出码 1 结束，systemd 为
`failed`，初次脚本因此中止并保留自己的marker；没有据此重复发送、重放或立即解除维护。

后续只读核验确认
`MainPID=0`、`ControlPID=0`、cgroup 为空、current 仍为 v0.3.10。活跃 turn lease、QiWe
pending/attempting、cron pending/delivering 均为 0，无 pending activity
transaction，无 sending
reminder。255 条活动、154 条提醒可读；有效提醒状态为 sent=145、expired=7、failed=2。

固定时间范围的 journal 未返回可用 Python 异常栈，旧 Gateway 退出码 1 的根因尚未确认，不能将此次停服称为正常关闭。

确认没有残留写入者后，于 00:22:30 继续服务器私有目录中的完整台账 tar 备份；核对归档内所有普通文件哈希与停服状态一致。

归档 SHA-256：`979225f5519696c32a7cff10cbabcc0ba202e7b14af7699987e80113cdb3a533`。

原始台账、备份、会话及连接信息均未进入 Git。后续应在独立问题中调查旧 Core 的关闭退出码；本次未修改 Core 来掩盖异常。

QiWe
README 已补充官方 drain 的覆盖边界、异常停服的进程与台账核对，以及明确重启插件和按 owner 清理维护标记的要求。

## 正式切换与验收

正式请求使用与模拟相同的 SHA、scope 和 restart
targets，开启既有 smoke 失败回滚。执行入口为
[Deploy Production](https://github.com/qintopia-agent-studio/qintopia-agent-os/actions/runs/42063865314)。服务器请求
`deploy-20261105T162637Z-c71f68da1ff3` 返回
`succeeded`，workflow 同时成功。生产 current 为
`c71f68da1ff3e4472623be7c6f30a3aeb4598e95`，previous 为
`afe3e81c7c5ac0d74332dfb5aaa79fa35ce06d83`。scope 为
`sidecar-runtime,deploy-bundle,hermes-plugins`，restart targets 为
`qintopia-system-services`，未更改仓库发布开关或 CI。

随后运行新发布内的
`smoke-release.sh --restart-targets hermes-erhua`，明确重启二花。新 PID 为
`2572894`，与停服前不同；插件链接指向精确的新发布，以下文件 SHA-256 与正式标签一致：

- `skills/qiwe/adapter.py`：
  `43a9459896698e753827c4cd345955b7c6d28a56ea94e678602f42fb782e5f01`
- `skills/qiwe/solitaire/time_resolution.py`：
  `5e206022e2324f8a5fcf8cab89e9fe7afa2582055887e1dea6b9441a84ef8eb8`

服务器上 8 项非发送检查通过：今晚/明晚点号时刻、全角点、同日词与分离事实、裸月日、真实日期冲突、待确认回复和已安排回复。模拟锚点为2026-11-06；“今晚7.30”返回当日 19:30。“待确认”分支不再声称时间已确认。这些是安装版本的纯解析及回复构造检查，没有建立真实活动或发群消息。

本机 QiWe `/health` 返回
`status=ok`；只清除了与本次完整 marker 匹配的排空请求，随后 fresh gateway 状态为
`running`，维护标记不存在。旧备份中的 255 条活动和 154 条提醒仍存在，既有提醒的 sent/delivery_state/status 全部保留，没有恢复旧台账覆盖新结果。主 sidecar、QiWe
companion、deploy bundle 的安装 manifest 均绑定同一正式 SHA。

两种实际生产二进制的 SHA-256 均与各自 artifact manifest 匹配：

- `huabaosi-production`：
  `034d195990e8acfa2fef7601207cb1f28cc042fd44da142c8332bc1807b7c3ee`
- `qiwe-production`： `f78444898e849497f88610098428e65573910829884468e185f33cf50773a4e5`

00:32:56 的复查仍为相同新 PID、`gateway_state=running`，本机健康正常。

回滚边界：若需回滚，先重新排空并确认没有在途事务，再通过既有 `Rollback Production`
入口指定 expected current `v0.3.11` 和 previous
`v0.3.10`；恢复完整不可变制品后明确重启二花。保持台账和送达状态，禁止覆盖旧备份或重放未知发送。

已实现、已验证和已上线均有上述证据。真实新接龙的完整模型解析、群内确认以及到时提醒送达尚未人工验收；不得将纯函数和健康检查写成真实群消息端到端成功。
