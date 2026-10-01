# 舍长页面上线前集成与审阅

日期：2026-10-01。范围：工作台前端及所属记录。正式分支：`codex/steward-workbench-closeout`。

## 目标、最小差分与工程量

负责人授权执行到上线前。完整目标是在线 UI 搜索选择可信人员、工作账号、群及已核实渠道，保存岗位、楼栋、知识和权限，二花采用同一版本，并可修改、停止或撤销。既有组织框架和舍长四标签保留。

沿用候选查询、既有 Command、版本、操作回执与正式知识服务。必要差分仅为账号及渠道选择与基础方准确 DTO 的接线，以及联合验证发现的页面问题。现有
`Audience` 只有 people/groups，未定字段不猜测；不把工作账号塞进 people。

有效工程量粗估：接口核对、前端收尾、未审文件核查和实际后端的离线联合检查约 2–4 小时。指定 Chrome 恢复后页面联合验证另约 1–2 小时。依赖等待不计入有效工程量。

当前等待基础方的渠道范围安全修正、账号/渠道通用保存 DTO，以及 Codex Chrome 认证恢复。
`scope_communication` 是 `anan/hospitality`
专属，不用于普通舍长保存，不给舍长附加 PMS 权限。

## 精确基线与 PR 安排

- 页面原提交：`d6a47637abb819d9e1f1386eab0fbf37ccb31535`。
- 页面记录提交：`ad8660a8b60c93e0ad0d97938793b303e3e14c68`。
- PR #731：`b450755cd7046e9fd55beaa3d0e1228aea63f1d6`，分支
  `codex/collaboration-channel-selection`。
- PR #733：`5eb59cb3c376e8ea9acf85521e2487e6c21c8b2d`，分支
  `codex/steward-production-minimal`。

本次 GitHub 读取确认两条 PR 开放。两者与页面的合并预检只在 `docs/reports/README.md`
冲突；共同 Rust、认证测试与测试目录可自动合并。完整输出保留在忽略目录，不覆盖基础方文件。

临时集成分支在现有工作树内准备，保留双方父提交，仅解决报告索引冲突。该版本用于实际共享服务和数据库检查。正式页面 PR 只包含原 UI 切片及必要接口收尾，使用 #731 分支作为清楚的 stacked
base；不将 #731 或 #733 的共同 Rust/SQL 再提交为页面新差分。主线顺序由总指挥协调为 #731
→ #733 → UI。

现行 PR 创建脚本硬编码 master，不能直接表达 stacked
base。正文校验和 doctor 仍复用原入口，按本轮明确授权的 stacked
base 创建页面 PR；不修改 PR 工具、CI 或部署机制。

总指挥另已核实 master ruleset 要求 strict required checks、check/PR-Agent 和 resolved
review threads。传统 protection
404 不能解释为无门禁。最终同步 master 和替代 head 的检查由总指挥依次协调，不用 admin
bypass。

## 指定 Chrome 能力核实

本次先得到
`Browser is not available: chrome`。启动已有 Chrome 后，插件库存已识别 Chrome 扩展。通过同一指定插件创建
`about:blank` 标签仍立即得到 `Codex auth token is unavailable`。

本次证据说明当前认证阻断，不能推断程序或扩展损坏。未反复重启、改系统设置或换用 Playwright/其他浏览器。页面联合验收未完成，DOM 状态模拟及 HTTP 检查均不代替浏览器验收。探测结果保留在本地忽略目录。

## 审阅与验证安排

完整读取 #731/#733 的 Reviewer Guide、reviews、conversation 和 inline
comments。#731 本轮没有 inline comments；Reviewer
Guide 未覆盖的文件由本任务补针对审阅。#733 当前渠道查询存在跨范围 gateway 信息披露意见，基础方负责修正；本任务不并行修改共享服务。

必要验证使用一次性本机 PostgreSQL、实际共享服务和原生测试。覆盖搜索分页、保存回读、修改/撤权、版本冲突、来源不可用及跨栋拒绝。真实 Hermes 独立进程验证由基础方负责，页面记录其精确版本与交接结果。未受新差分影响的已有检查直接复用，不把模拟身份或渠道验证描述为线上送达。

本次不合并、发布、部署、改真实授权或外发。旧 stash 和历史坏 ref 不作为本批新增前置。

## 第一版实际集成结果

本地临时分支 `codex/steward-prelaunch-integration-20261001` 的提交为
`5ecc0eabc62b59118fc4c76efbf3c600c357f5e1`，父提交精确为
`0ba09f429586d7e8f2752558bc6839e498a16b35` 与
`5eb59cb3c376e8ea9acf85521e2487e6c21c8b2d`。普通合并只人工保留双方报告索引条目，没有人工改共同 Rust、SQL、认证测试或测试目录。#733 的主线祖先还带入已合入的部署接线内容；本任务未新编写或发布这些改动。

本任务新建一次性 PostgreSQL 18.6，地址为本机 loopback 55483 的
`qintopia_test`，数据位于忽略目录。没有复用其他任务的验收库或生产资料。

- `steward_production_minimal_tests`：4/4 通过，0 忽略。
- 完整 `person_collaboration`，含原标记忽略的数据库用例：169/169 通过，0 忽略。
- 命令均用
  `--locked --features postgres-integration-tests --include-ignored --test-threads=1`，退出 0。

定向检查实际执行共享服务、认证 HTTP、305 个目录候选分页、人员/工作账号/渠道区分、权限撤销、错误状态，以及同一二花可信工具对两栋知识版本的读取、修改和停止。完整套件还覆盖 #731 的受限身份锁、既有业务接口和保存回执。日志及退出码保留在本地忽略目录，没有改断言或跳过失败。

这些结果是合并后的实际后端配合模拟身份和请求的证据，不是指定 Chrome 页面交互、真实 Hermes 模型或线上渠道验收。它们也不处置 #733 已知的跨范围渠道意见；待基础方修正后对实际替代 head 复核新增与受影响场景。

## PR #731 未覆盖文件的针对审阅

审阅基线为 `8173e53795062d4df53fc78f5099325b2e37ce59`，head 为原
`b450755`。本任务补读 Reviewer
Guide 列出的全部 11 个文件差分，目前没有发现新增阻断问题：

- `workbench-view.js` 与
  `workbench-catalog.js`：搜索保留选择、空结果、同名稳定引用及旧列表范围；它们是已给数组内的搜索，不作为全可信目录证据。新页面候选服务另用 workspace 接口。
- `tools/testing/catalog.json`：四个新增场景的路径、实际函数、完整 filter、数据库 feature 与 ignored 参数对应；描述明确模拟来源，不增加 CI 检查入口或新共享框架。
- 两份 `20260929000*`
  SQL：空配置不授予发送权；固定锁函数限定租户、范围、链接与 Person，锁定当前行，使用完全限定表名和固定 search_path，撤销 PUBLIC
  EXECUTE；没有创建生产角色或授予生产 ACL。
- 两份 scope communication / person
  lock 数据设计：与上述存储和锁契约一致，保留版本失效、并发和回滚边界。
- 岸岸 rollout、原本地验证报告、人员协作 README 与测试指南：明确专属业务权限、模拟与真实验收区别，保留原总检查失败和浏览器阻断，不把独立通过回填成总门禁通过。

两条 PR 的共同文件衔接已核对：store 模块同时保留 scope communication 与 workspace
candidates，认证测试、人员协作 README 与测试目录自动合并；唯一报告索引冲突保留了双方条目。本次完整数据库套件没有发现合并造成的认证、迁移或旧业务回归。

PR #733 的当前 Reviewer Guide 在 `5eb59cb`
仍列跨范围 gateway 候选信息披露为 ACTIVE，基础方负责修正。没有未读 inline
threads；不能把无 inline comments、既有 CI success 或本地数据库通过当作该意见已关闭。
