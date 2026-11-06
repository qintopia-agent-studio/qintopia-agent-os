# 二花仅有时刻的接龙日期补全

日期：2026-11-06。范围：QiWe 接龙时间解析、首次记录说明和现有回归套件。

## 现场与原因

负责人反馈“下午7.30”仍收到时间待确认回复。只读核对生产 current 和插件链接均为v0.3.11 的
`c71f68da1ff3e4472623be7c6f30a3aeb4598e95`，时间解析文件哈希与上次发布一致。注册表指定的二花 Gateway 正常运行，本机健康返回
`status=ok`。

本地模拟复现：点号已能识别为时刻，但正文没有“今天／今晚”等日期词，确定性解析在
`missing_date`
分支退出。上次测试全部带日期词，未覆盖仅写时段与时刻的日常接龙。通用回复又把缺日期描述为“活动日期或时刻还不明确”。

上线前按时刻和当天范围仅只读核对一条匹配记录：首次发起于北京时间 12:27:29，最后更新于 12:29:34，解析原因为
`missing_date`，提醒计划仍为
`needs_confirmation`。仅保留时间和状态摘要，未导出原始群消息或身份。

## 已确认规则与实施范围

负责人明确选择：没有指定日期、只有明确时刻时，按接龙首次发起当天处理，并在回复中写明日期。

- `下午7.30`、`下午七点半`、`19:30` 等单一明确时刻使用首次发起日及配置时区。
- 保留原文日期事实为空，记录日期来自首次发起日；回复显示完整日期时刻并说明补全依据。
- 正文的明确日期优先；模型漏摘日期不能变成默认当天。模糊时段、无法识别的日期、多个日期或场次仍需确认。
- 缺少事件和接龙时间戳时不使用机器当前时间补日期；跨日接龙仍沿用原始发起日。
- 既有过期和提醒窗口规则不变，不自动补发历史活动，不开启新的发送通道。

修改限定于 `skills/qiwe`
解析与测试及本文档，不改 CI、定时任务、发送开关、凭据或 Profile。先运行现有提醒生命周期与 QiWe 全套，再运行
`pnpm check:pr:auto`；测试仅使用模拟模型响应和渠道。

## 验证与交付

已实现上述规则。提醒生命周期新增 14 个测试方法，既有模型上下文测试改为逐字提取，并断言完整解析状态。初始红测准确复现缺日期问题；审查又补齐多候选时刻、被模型漏摘的日期限定和标签冒号日期保全年份。

- 提醒生命周期 46 项及模型上下文 1 项，共 47 项定向测试通过。
- `pnpm test:qiwe`：349 项，348 项通过，1 项 Linux 专属检查在 macOS 跳过。
- 本次文件 `git diff --check` 与 Markdown 格式化通过。
- `pnpm check:pr:auto`：quick tier 通过，包含现有部署、协作和权限契约检查。
- `pnpm pr:doctor`、`pnpm pr:check-body`：通过。

Python 使用已安装的 3.12；RTK 不在 PATH，沿用原生 pnpm。没有调用真实模型或发送群消息。所有回归均在现有
`qiwe/activity-reminder-lifecycle` 与 parser 目录覆盖，无需改 CI 或测试运行器。

生产已使用审阅后的完整不可变制品上线 v0.3.12，并按 QiWe
README 完成排空、一致备份、明确重启与版本核验。回滚须保留活动和送达状态，不用旧台账覆盖新结果；具体结果见下文。

## 远端恢复制品检查

修复 PR [#753](https://github.com/qintopia-agent-studio/qintopia-agent-os/pull/753)
初始 head 为 `42f49da110f83d69d00b9f91a7515b9d664f5fe2`。
[Recovery run 42167209197](https://github.com/qintopia-agent-studio/qintopia-agent-os/actions/runs/42167209197)
的第一次尝试在官方 dashboard 模拟 Profile HTTP 探针失败：API 返回 500，尚未执行 packaged
QiWe 导入步骤。独立临时 Profile 禁止外网，不加载本次 solitaire 修改。

上次 PR 751 的
[同类检查](https://github.com/qintopia-agent-studio/qintopia-agent-os/actions/runs/42055600766)
通过；两次官方 Core 均固定为
`2237be355906fbe6065ce1815711eee52b2d646e`，相关 workflow、构建脚本和探针无变更。

日志未标记失败的具体路由，临时子进程日志未保留，因此 HTTP
500 根因未确认。不能将其归因于日期修复，也不能据此宣称环境已恢复。

通过现有入口仅重跑失败 job，保持全部检查及原失败证据，不改 CI。相同 head 的第二次尝试
`127728115511` 全部通过，包括原 HTTP 探针、packaged
QiWe 导入和候选制品上传。这次重跑通过不能解释第一次 HTTP 500 的根因。QiWe
README 已明确这类失败的定位边界。上述重跑时尚未执行上线变更或真实群消息重放。最终 head 的独立检查结果见下节，原失败记录保留。

## 最终检查与正式发布

修复 PR #753 的最终 source head 为
`0dfb50e743b3f4c887905fd3557f7033251762b3`，合并提交为
`19a5e24d2ef42ced9f5b8c4ed07996897e95aef8`。

- [CI run 42168419936](https://github.com/qintopia-agent-studio/qintopia-agent-os/actions/runs/42168419936)
  通过；Runtime、Light 等适用检查均通过。
- [业务测试 run 42168415076](https://github.com/qintopia-agent-studio/qintopia-agent-os/actions/runs/42168415076)
  通过：179 个场景、658 项测试、0 跳过。
- [Recovery run 42168415357](https://github.com/qintopia-agent-studio/qintopia-agent-os/actions/runs/42168415357)
  第一次尝试通过，包括恢复制品验证。该结果不解释前一 head 的首次 HTTP 500 根因。

Release Please PR
[#754](https://github.com/qintopia-agent-studio/qintopia-agent-os/pull/754) 在精确 head
`079967cf62b849d4c261474285da6e6d6521fa1c` 上完成发布检查：

- [CI run 42170382666](https://github.com/qintopia-agent-studio/qintopia-agent-os/actions/runs/42170382666)
  的 check、Rust、PostgreSQL 和 Release Please
  validation 均成功；后两类重检查没有以跳过代替通过。
- [PR Agent run 42170395633](https://github.com/qintopia-agent-studio/qintopia-agent-os/actions/runs/42170395633)
  认证校验通过，按 Release Please 规则返回 no-review。

合并后正式 release SHA 为 `9c77a51da30211c7183243e37a7225b86bddd122`。
[v0.3.12](https://github.com/qintopia-agent-studio/qintopia-agent-os/releases/tag/v0.3.12)
于 `2026-11-06T05:58:24Z` 发布。
[Deploy Production run 42171539570](https://github.com/qintopia-agent-studio/qintopia-agent-os/actions/runs/42171539570)
构建及模拟部署成功，request `deploy-20261106T061313Z-9c77a51da302` 为
`dry_run_succeeded`。

## 生产切换与恢复结果

切换前使用官方 reversible
drain，取得两次新鲜零在途观测，并独立核对持久在途状态。14:19:34 正常停止二花：退出码为零，MainPID、ControlPID 为零，原 cgroup 为空，无发送中提醒和待完成事务。256 条活动、154 条提醒完成服务器私有一致备份；文件集合、归档与前后摘要一致，
`abnormal_stop=false`。原提醒状态为 sent 145、expired 7、failed
2。备份和记录内容留在服务器，不进入 Git。

[正式 Deploy Production run 42173811323](https://github.com/qintopia-agent-studio/qintopia-agent-os/actions/runs/42173811323)
于北京时间 14:24:18 完成，request 为 `deploy-20261106T062325Z-9c77a51da302`，结果
`succeeded`、`dry_run=false`。commit/runtime/deploy_bundle/release 四个 SHA 均为
`9c77a51da30211c7183243e37a7225b86bddd122`；current 指向该完整 release，previous 为
`c71f68da1ff3e4472623be7c6f30a3aeb4598e95`。deploy-runner 检查通过，无回滚。scope 为
`sidecar-runtime,deploy-bundle,hermes-plugins`，启用 `rollback_on_smoke_failure=true`。

正式请求的 restart target 为
`qintopia-system-services`，因此随后明确调用新 release 自带的
`smoke-release.sh --restart-targets hermes-erhua`，脚本通过。首次调用前的路径断言误用了不存在的
`/home/ubuntu/qintopia-agent-os-current`，在重启前阻止执行；更正为注册路径
`/home/ubuntu/qintopia-agent-os-releases/current`
后重新核验并执行。该次本地操作错误没有改动服务器文件或重复部署，QiWe
README 已补充路径检查。

恢复检查全部通过后，仅清除本次自己创建的 drain marker：

- 新二花 PID `4044019`，Gateway 新鲜状态 `running`，本机 QiWe health `ok`。
- Profile 插件链接、release
  manifest，以及 adapter/parser/time-resolution 三份源码 SHA-256 与上线前从已审阅提交取得的独立摘要一致。
- 原 256 条活动和 154 条提醒保留，154 条终态提醒完整内容未变。
- 生产安装代码上的纯内存检查通过：5 项明确时间、3 项原始日期锚点、1 项缺少锚点、4 项待确认无计划及回复、1 项完整日期回复。
- Hermes core 保持 `v2026.9.21`，没有修改凭据、Profile 配置、发送门禁或其他任务。
- “小满·每日案例日报”仍为 `enabled=false`、`state=paused`；与切换前暂停状态一致。

本会话按负责人要求于 09:18:25 通过官方 cron
pause 操作暂停了该日报，发布没有恢复该任务。本地运维脚本 7 项模拟检查通过；文档分支的
`pnpm check` 完整链路通过，包括 QiWe 349 项（macOS 上 1 项 Linux 专属检查跳过）、Rust
860 项通过及 3 项已有忽略、两项 sidecar smoke。

## 单条历史记录修正与验收边界

负责人已单独授权将上述唯一记录修正为 11 月 6 日 19:30，并新增当天 19:00 群提醒。

该操作已在新版本恢复后通过现有台账锁和事务接口执行，仅更改这条活动的时间字段和提醒计划；旧提醒、参与人、来源及未知字段完整保留，没有重放消息、调用 Feishu 或立即发送群消息。

首次维护预检使用 5 秒状态阈值而阻断写入。随后只读确认同一 PID 正常运行、没有 drain、没有待完成事务，修复目录尚不存在；尚未发生台账写入。核对官方 Core 后确认默认心跳周期为 30 秒，状态过期阈值为 120 秒。维护脚本改用官方
`runtime_status_is_stale` 默认值，继续核验 PID 身份和存活；QiWe README 已补充这一边界。

维护脚本 10 项模拟检查通过。生产事务提交及锁内回读确认仅修改 1 条活动，新增 1 条提醒：开始时间为
`2026-11-06 19:30`，提醒到期为
`2026-11-06T11:00:00+00:00`（北京时间 19:00）。提醒当前 pending；原有 154 条提醒完整未变。修复前快照与提交回执仅保存服务器私有目录。

根据新进程白名单环境、固定 Profile 环境文件和官方配置加载器核对：提醒启用、非模拟、实际发送启用、目标群在允许范围内。没有修改任何门禁。安装版代码的纯内存检查确认 18:59:59 不到期、19:00 可领取、同一提醒不能重复领取、模拟送达后不再次到期。

随后独立只读回查确认活动已修正，新提醒存在且字段一致、状态 pending，无需事务协调或重试。

真实新接龙在群端的识别和回复仍待人工验收；计划创建不等于到点已发送或群端已收到。
