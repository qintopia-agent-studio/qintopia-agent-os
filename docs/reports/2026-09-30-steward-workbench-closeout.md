# 舍长与二花岗位入口：前端收尾

日期：2026-09-30。Owner：Agent OS
/ 工作台前端。分支：`codex/steward-workbench-closeout`。从 #731 的 `b450755`
接续，原分支与可审阅结果保留；本切片依赖 #731，不能重复交付其整套功能。

## 已确认目标与分工

负责人已确认首批是舍长与二花岗位协作。保留原组织配置及舍长的「与二花对话」「本栋知识」「合作约定」「事项与进展」四个标签。用户搜索选择已有可信人员、工作账号、群和已核实渠道，安排任职、楼栋、二花协作权限及本栋知识；保存后的修改和撤销由共同服务立即采用。预填仅为可编辑起点。

本任务只修改 `workbench*.js`
和本 UI 记录。共享 Rust 路由、授权、身份、知识及迁移由本体基础任务独占；最终候选接口和正式能力状态由其冻结。岸岸/PMS 不作为首批前置条件，不借用或放宽其
`anan/hospitality` 专有接口。群或渠道勾选不创建身份、不增加授权，展示名不作为身份依据。

## 实施前核查

- #731 四类业务候选已有服务端独立搜索分页，属于岸岸业务配置。组织任职、联系对象和群选择仍在浏览器搜索
  `/api/state` 所给数组，不能将其宣称为全目录搜索。
- 原 `state`
  查询没有 256 条截断，但其人员集合依赖登录命名空间及当前可见关系；完整可信来源的跨 Gateway 人员、工作账号与渠道选择需要基础方统一提供。任职 Person 和共用账号须保持不同主体。
- `personalRulePanel` 用 `local_dialogue_available`
  挡住正式知识/约定入口；需要独立的正式能力状态，不能通过前端绕过服务端授权。固定例句网页对话保留明确标识，真实二花接入另由基础方验收。
- 已有配置保存后回读、普通账号本栋视图和管理员导航；需要核对候选失败、权限撤销以及返回页面时的旧表单清除与重新核验，版本冲突仍保留草稿。

## 最小前端差分与验证计划

1. 按基础方冻结的接口绑定现有任职、群绑定和触达表单。服务端搜索覆盖完整可信目录；保留当前选择，翻页或搜索不偷偷清空已选值，不能手填内部 ID 或凭名称合并。旧选择仍由保存事务重新核验。
2. 以明确服务证据区分无权、来源未同步和搜索无匹配；无返回字段时不猜测来源状态。
3. 正式本栋知识和约定读取真实能力状态；沿用已有新增、编辑、停止与未来安排命令。普通舍长只办理本栋已获准事项，管理者配置与舍长办理分别核验；撤权后旧表单停止使用。
4. 使用必要的脚本语法、前端状态逻辑检查及指定 Codex
   Chrome 的本地模拟验收。复用 #731 未变部分的既有证据，不重复旧全量检查。验证与真实模型/渠道验收分别记录。

## 当前验证状态

指定 Codex Chrome 首次返回
`Browser is not available: chrome`；启动已有 Chrome 后已发现指定扩展，但创建验收标签返回
`Codex auth token is unavailable`。尚未开始本轮浏览器交互验收，待认证恢复。未使用 Playwright 或其他浏览器替代，未重复 #731 未变部分的旧模拟截图。

## Git 与生产边界

当前隔离树复用，未在根共享目录开发、reset 或覆盖未提交删除。原 common
Git 对象库存在缺失对象，普通 fetch 的对象恢复和传输错误已报总指挥；由其协调单写窗口后仅追加恢复必要远端对象。恢复不改旧工作分支、不删除 pack、不执行 gc/repack/prune，不以下载完成代替可达对象验证。

本轮不修改 CI，不写真实人员授权，不发送真实消息，不合并、发布或部署。前端通过不证明共同配置已被真实二花消费；联合验收需要基础方最终版本和实际执行入口证据。

## 2026-09-30 Git 仅追加恢复结果

共同目录为
`/Users/feather/Documents/Codex project/qintopia-agent-os/.git`。本任务在负责人协调的独占恢复窗口操作，使用命令级现有
`gh auth git-credential`，未保存认证 Token。

第一轮从已核实 `refs/heads/master` 和 `refs/heads/codex/anan-host-recovery` 执行 HTTPS
`fetch --refetch`；退出 1，错误为
`bad object refs/heads/codex/anan-work-account-access`。已恢复指定主线和 #730 完整闭包，但 #731 尚缺 5 个树对象，不能将这一轮记为全库恢复。

第二轮来源为精确核实的 `refs/heads/codex/collaboration-channel-selection`（`b450755`）及
`refs/pull/723/head`（`4bcb9a3`）。#723 的原提交祖先包含
`131e7b4`。两轮都禁用自动维护，不写目标 ref、FETCH_HEAD、commit-graph 或 tag，不 prune。第二轮 fetch 仍退出 1：

```text
fatal: bad object refs/heads/codex/management-ui-deploy-proposal
error: https://github.com/qintopia-agent-studio/qintopia-agent-os.git did not send all necessary objects
```

退出结果与对象恢复分别核验；第二轮后 `git cat-file -e <tip>^{commit}`、`^{tree}` 及
`git rev-list --objects --missing=print <tip>` 的实际结果为：

| 提交归属              | 精确提交                                   | 完整可达闭包     | 剩余缺失                                          |
| --------------------- | ------------------------------------------ | ---------------- | ------------------------------------------------- |
| 核实主线              | `74899d4191b810bc0cf5dc748c742c3b84aea0b5` | 是，14002 个对象 | 无                                                |
| #730                  | `417d1a7fc2074a1295f047f7fcda1b233ac057a6` | 是，14031 个对象 | 无                                                |
| #731 / 本 UI 开发起点 | `b450755cd7046e9fd55beaa3d0e1228aea63f1d6` | 是，14072 个对象 | 无                                                |
| #723                  | `4bcb9a323ee85bcb27881e4e892803d9d79a3317` | 是，14167 个对象 | 无                                                |
| 旧工作账号分支        | `131e7b4c7c472bb5bc28d2b8bd9af0c8d4f0ceac` | 是，14106 个对象 | 无                                                |
| 未发布基础线          | `3e0772724ab416311eab516ebf2a5ab5e2355b38` | 否；commit 可读  | 原根树 `de9557c50d44aa0d1799acc1d226503b6f9e7248` |

PR #731 原来缺失的 `8b1c304`、`2627fdb`、`8a83fea`、`ffd42b5`、`31e12dc`
五个对象均已实际恢复，类型全部为 tree。第二轮前后 HEAD、head_ref、全部 refs、status、index
SHA256、common-dir 全部一致。本 UI 分支仍从原 `b450755`
接续；未移动本地 master 或 origin/master，也未合入主线。

### 原树的索引副本恢复候选

只读核实基础线原提交根树为 `de9557c50d44aa0d1799acc1d226503b6f9e7248`。其实际 Git 目录为
`.git/worktrees/qintopia-agent-os4`，实际原索引路径是该目录的
`index`。原索引文件不存在，`ls-files --stage -z` 返回 0 条；`git status` 退出 128，报
`bad tree object HEAD`。该路径没有可复制的完整原索引，因此未复制索引、未运行
`write-tree`，未采用
`--missing-ok`，未生成替代 commit 或移动原 ref。复核原索引仍不存在，refs 校验和未变。原根树仍需基础方保全的已知来源；这个恢复候选不通过，不能宣称基础线已恢复。

### 其他历史 ref 的影响

以下 ref tip 仍缺失，保留原引用及本地文件，不扩大本次恢复：

- `codex/management-ui-deploy-proposal`：`d78739aa3cd55d260ff3cc2d95450957dc0d6c21`。
- `origin/codex/management-ui-deploy-proposal`：`7e36a13297c05edc9dfcc57008c990f8c635573e`。
- `codex/runner-takeover-recovery` 及对应 origin
  ref：`f975c63d2ff08927ff17f1a2d496d83a1ffcc2d6`。
- `codex/v033-runner-takeover-plan` 及对应 origin
  ref：`43e220fb457a23ae5bcc585ae30660671279adc2`。
- `refs/stash`：`6fe5452e4e6c37fdb244fcab1755f6c772fd2af9`。

它们仍可能使普通 fetch、遍历全部 refs 的完整性检查或历史恢复失败。此次闭包检查不等于全库
`fsck` 通过。当前 UI 基线与指定主线可按精确 SHA 使用；基础线 `3e07727`
仍不可视为完整基线。

忽略目录中保留 `git-recovery-20260930-round2-before.json`、`round2-after.json`、
`round2-reachable.json` 及 `git-recovery-20260930-foundation-index/`
的只读前检与后检证据。原第一轮快照原样保留；这些本地恢复材料不进入 Git。

## 共享接口 v1：已冻结部分与待对齐项

已读取基础方计划中的「2026-09-30 舍长与二花生产最小接入：共享接口 v1」。候选读取使用
`GET /api/workspace/candidates`
的 scope、kind、purpose、search、limit、after 和稳定 ref；响应采用 items、label、description、version、next_cursor。保存复用原
`/api/preview`、 `/api/save` 与 Command。群绑定的 `SetGroups.conversations`
使用候选 ref。

当前可独立准备任职人员、个人触达及群绑定的服务端搜索与翻页、保留已选值和明确读取失败。正式知识/约定能力字段、来源未同步/无权/无匹配的准确错误契约，以及最多 50 的分页边界仍待基础方补齐；`dialogue_available`
不能代替知识能力。当前 Audience 只有 people/groups，工作账号和已核实渠道的保存字段须随共同服务最终契约对齐，不能放入 Person 字段或自行添加未知字段。

## 第一阶段：记录校验与交付状态

本轮完成对象恢复与前端接口核查，只修改本页及报告索引，尚未修改工作台 JS 或创建新实现提交。
`git diff --exit-code b450755 -- runtime/sidecar/src/person_collaboration`
通过，#731 实现原样保留。

- 两份记录的 `pnpm exec prettier --check` 通过。
- `pnpm exec markdownlint-cli2`
  按仓库配置扫描 Markdown；首次发现本页行首 PR 编号触发 MD018，已改为显式 PR 前缀，修复后的复查通过。
- `git diff --check` 通过。
- 未运行实现测试或全仓工程 tier；本轮没有代码差分，不复跑 #731 未变实现的既有验证。
- 环境 PATH 未找到 `rtk`，记录校验使用已有
  `pnpm`。Chrome 验收仍受认证阻断，没有浏览器通过结论。

下一步从已验证 `b450755`
继续前端接入；正式能力、错误与账号/渠道命令契约待基础方补齐后对齐。基础线原树恢复及真实二花联合验收仍由基础方处理，本页不把这些待项写为完成。

## 原提交恢复：精确树证明增补

基础方提供原根树 payload 及原五文件的 blob 证明后，本任务独立计算 Git tree SHA，精确等于
`de9557c50d44aa0d1799acc1d226503b6f9e7248`。进一步发现原树引用的 runtime/tools 子树及
`tools/testing/catalog.json` 原 blob 也缺失，不能只写根树后宣称闭包完整。

从完整父提交 `beb6159b7dccea1492a01cfcd20b0f55ec66fa2c`
与证明中的五个精确 blob 重建树链。其中四个 blob 已在 Git；catalog 原文件的 blob
SHA 实际计算为证明中的
`5b8e01211dd7a3b4c72a679110092858ff2b1b0e`。所有生成树组成的根 payload 与基础方原 payload 逐字节相同，每个引用的对象均有可核验来源；随后仅通过
`hash-object -w -t blob/tree --stdin`
增补 1 个原 blob 和 8 个原树对象，没有修改原 commit。

`3e0772724ab416311eab516ebf2a5ab5e2355b38` 的 `^{tree}` 为原 `de9557c`，
`rev-list --objects --missing=print`
退出 0，14045 个可达对象无缺失。恢复前后本任务的 HEAD、head_ref、全部 refs、status、index
SHA256、common-dir 完全一致；基础工作区的 1484 个已核对路径文件状态也完全一致。其原 index 仍不存在，未重建或替换暂存区，不能将提交对象恢复称为整个工作区恢复。证明、副本 payload、重建说明及前后检查保留在忽略的
`.local-workspace/git-recovery-20260930-original-tree/`。

前述第二轮表格及索引候选说明保留为当时结果；本节为最新状态。基础原提交对象基线现在完整可用，其他历史坏 ref 保持原状，未执行全库修复。

## 冻结契约后的最小独立前端差分

已读取更新后的共享接口 v1：候选 limit 最大 50，200 无匹配、401 登录失效、403 无权及 503
`candidate_source_unavailable` 分开处理；`foundation_available`
独立控制正式知识/约定服务，修改权限只由 `context.permissions`
判断。基础方明确代码接线尚未完成，文档不是运行证据。

本切片在已有 `workbench-view.js`
提供共用候选选择器，接入任职人员、个人触达和群绑定；保留稳定ref、跨页已选值、最新搜索结果优先及可读标签。`workbench-organization.js`
按正式能力字段展示知识和约定，并清除授权失效的旧编辑表单；已有版本冲突草稿与不确定请求语义保留。错误说明写在现有
`workbench.js`；`workbench-steward.js`
的真实渠道提示与固定演示分别展示。只修改这些工作台 JS 及本记录/索引。工作账号与渠道选择的写入 DTO 尚未出现在现有 Audience 中，仍需共同服务给出准确字段，不将工作账号放入 people。

验证使用 Node 语法及必要状态模拟检查，按新差分检查格式；指定 Chrome 认证恢复后再做 UI 联合验收。本切片不改变共享服务、CI、真实授权、外发或生产运行状态。

## 前端差分的实际验证

已修改五份工作台 JS：

- view：共同候选选择器。
- organization：任职/联系人员及本栋知识权限。
- catalog：群绑定。
- steward：渠道及演示提示。
- 主 JS：错误状态。

旧组织页面、四标签、Command、preview/save 及原不确定结果回读保持同一入口。共享 Rust、SQL 和 CI 没有差分。

- 五份修改 JS 的 `node --check` 全部退出 0。
- 忽略目录中的 Node DOM 状态模拟检查共 18 项通过，覆盖如下行为：
  - 候选读取：50 项续页、独立搜索、跨页多选去重、同名标签更新、乱序响应及旧范围组件卸载。
  - 错误边界：403 清旧候选；503 保留筛选和已选值，允许重读；错误范围及 51 项响应拒绝；401 登录失效。
  - 服务能力：知识入口独立于真实网页聊天；关闭正式服务时不发起知识请求。
  - 权限重核：返回编辑标签保留有效草稿；200 权限拒绝及 403 清编辑器。
  - 草稿恢复：短暂读取失败后恢复保存按钮；版本冲突草稿不被标签刷新意外重新启用。
  - 回执边界：已取得保存回执后若回读失权，保留已保存的说明，同时清除旧编辑器。
  - 对话展示：既有渠道提示与原四标签；固定演示要求显式模式与本地开关。
- 检查直接执行实际 JS 函数，使用内存 DOM 和模拟服务响应；未使用 Playwright，不能作为浏览器、真实后端、二花模型或微信接入通过证据。辅助说明及未改标签容器使用最小测试替身，未验证排版。
- 状态检查发现短暂权限读取失败后提交按钮可能无法恢复，已保存仅由本次失败禁用的按钮集合，重新取得权限后恢复；版本冲突原来禁用的按钮不被恢复。修复后的 18 项复查通过。
- 结果与脚本保留在 `.local-workspace/steward-closeout-state-results.json` 和
  `test-steward-closeout.cjs`，不纳入生产包。

指定 Chrome 尚无新交互验收通过结果。候选后端路由尚未接线，真实目录、保存后由二花采用及联合权限验收待基础方版本；工作账号/渠道的保存 DTO 仍待准确共同契约，本前端差分未将其写入 people。本记录不是首批岗位协作已经上线的证明。

## 工程检查与当前交付边界

`pnpm check:pr:auto` 以精确 `b450755`
为比较基线，实际只检测本切片的 7 个文件。首次因本记录的两行长度超限退出；修复后再次执行，quick
tier 全部通过。该次 heavy tier 在系统 Python
3.9.6 的 QiWe 测试失败：5 个用例报主线程无当前事件循环，另有 1 个跳过。保留失败日志，没有把总自动门禁记为通过。

按已有测试指南核实并使用本机已安装的 Python 3.12.14 和 Rust
1.96.0，未安装新工具或修改环境配置。重跑 `pnpm check:runtime`：QiWe
326 项通过（其中 1 项跳过），Rust 格式与 cargo
check 通过；首次 Rust 单元检查为 857 项通过、1 项失败、3 项忽略，失败为
`foundation_socket_restarts_without_replacing_active_listener` 返回
`foundation_broker_active`。原测试使用其临时目录，不是已查证的真实服务冲突，本记录不推断失败原因。

该原用例独立重跑通过；随后原 `pnpm check:runtime`
在正确工具链下重跑退出 0，包含原 QiWe、格式、cargo check、全部默认 Rust 单元和 sidecar
smoke。没有改断言、跳过失败用例或修改共享 Rust。原全量自动门禁的失败记录保留；其后续专用 feature/Clippy/PG
tier 尚未运行，不称 `check:pr:auto` 整体通过。日志均在忽略的本地恢复/验证目录中保留。

`pnpm collaboration:check` 通过，`pnpm pr:doctor` 通过。PR 正文已在本地准备。

直接调用 `pnpm pr:check-body <file>`
在非 PR 事件中只返回 skipped，不能记为正文验证通过。随后将该正文放入本地模拟 PR 事件，调用原校验入口通过；doctor 带正文检查也通过。没有创建 PR。

PR #731 已通过 GitHub API 核实仍开放，head 仍是原 `b450755`。

当前保持独立后续分支，待统筹确认 PR 依赖/基线后提交远端审阅，避免把 #731 整套差异重复交付。

没有合并、发布、部署或真实外发；共享 Git 原提交恢复已完成，基础工作区原索引仍缺失。UI 联合验收、二花实际采用配置及工作账号/渠道保存 DTO 仍是未完成项，未以本地模拟代替。

本切片以 Conventional
Commit 作为本地交付。远端 PR 尚未创建，依赖/基线安排仍待统筹确认。本任务的对象增补步骤已全部结束，未扩大修复其余历史坏 ref；基础原索引恢复仍由统筹安排。

## 正确工具链下的自动门禁复核

前端切片已由正常提交 hook 验证并保存为
`d6a47637abb819d9e1f1386eab0fbf37ccb31535`，包含原定七个文件。首次提交因本记录的 Markdown 格式问题被拒绝；修正格式后提交成功，没有绕过 hook。

随后以精确 `b450755cd7046e9fd55beaa3d0e1228aea63f1d6` 为比较基线，使用已有 Python
3.12.14 与 Rust 1.96.0 执行原 `pnpm check:pr:auto`，退出 0；本次 quick 与 heavy
Rust 两层通过：

- QiWe 执行 326 项，1 项按原配置跳过。
- 默认 Rust 单元测试 858 项通过，3 项按原配置忽略；原两个 sidecar smoke 通过。
- QiWe staging 与 production 的两个专用编译边界用例分别通过。
- 无默认 feature 和全部 feature 两种 Clippy 配置均通过，保持 `-D warnings`。
- 全部 feature 的 Rust 单元测试 873 项通过，212 项按原配置忽略。

自动入口明确跳过 PostgreSQL 层：本机 5432 无监听服务，一次性 `qintopia_test`
未就绪。没有连接或写入其他任务的数据库；数据库集成层仍待一次性环境或后续 CI 验证，不将退出 0 描述为三层全部通过。

本次日志和退出码保留在忽略目录的 `steward-closeout-pr-auto-python312.log` 与
`steward-closeout-pr-auto-python312.exit`。此前 Python
3.9 和 socket 用例的失败证据保留，前节描述为各次运行的历史结果。

本次仅更新验证记录与本地 PR 正文，未修改工作台 JS、共享 Rust、SQL 或 CI。指定 Chrome、真实后端及二花联合验收，工作账号/渠道保存 DTO 与 PR 基线安排仍待后续完成。
