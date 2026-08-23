# Task 6 — 二花·活动发起者招募定时广播（Hermes cron）

## 背景与根因

运营希望「小满产文案 → 二花发到居民主群」的每周活动发起者招募能稳定自动执行。之前靠在运营群（营造司）里让小满创建
**agent 模式 conversation timer** 来实现，存在两个根因问题：

1. **群路由绑错**：agent 模式 timer 的 `deliver: origin`
   会绑死到"建任务时所在的群"，所以在营造司群建的 timer 只发回营造司群（`17260808696`），到不了目标居民主群
   `10859791146538059`（秦托邦的小伙伴（新））。
2. **小满→二花 handoff 未实现**：`runtime/sidecar/src/xiaoman_activity.rs` 中
   `HANDOFF_TARGETS` 硬编码只认 `huabaosi`，且 handoff 仅映射
   `("visual_asset_request","huabaosi")`，没有任何 `(...,"erhua")`
   的 capability 映射。即"小满把活动交接给二花发群"这条能力在 sidecar 里从未实现，恢复二花
   `delegation` 工具集也无济于事（小满→二花交接走的是
   `agentos_worker_command`，不经 delegation）。

> 结论：把"恢复二花 delegation"或"Python 加 erhua 进 HANDOFF_TARGETS"当作修复是误判——真正通道是二花自己的受控发送流水线（与每日早报同款）。

## 方案

复用二花每日早报已验证可用的受控发送通道（`qintopia-message-sidecar` → artifact → review
→ work-item → confirm → `run-group-message-send-worker` →
`run-qiwe-text-send-worker`），把招募做成**二花 profile 下的 reviewed script
cron**，目标群固定为居民主群。

- 文案先用固定模板（可由 `QINTOPIA_ERHUA_ACTIVITY_RECRUITMENT_MESSAGE`
  覆盖），后续可增强为读取小满每周定制草稿。
- 招募复用二花主群发送身份（早报的 `QINTOPIA_ERHUA_MORNING_BRIEF_*`
  一系列 env），因为招募与早报发往同一居民主群、走同一受控边界。

## 文件清单

- `runtime/hermes/scripts/qintopia_erhua_activity_recruitment.sh` — Hermes cron wrapper
- `deploy/sidecar/scripts/erhua-activity-recruitment-worker.sh` — 实际发送逻辑
- `deploy/sidecar/scripts/apply-erhua-activity-recruitment-hermes-cron.sh`
  — 安装/启用 3 个 job
- `runtime/hermes/cron/reviewed-cron-jobs.json` — 登记 3 个 erhua 招募 cron（周六 12:00
  / 周六 21:00 / 周日 12:00）

## 部署

1. 评审合并本 PR。
2. 在服务器以 owner 审批设置
   `QINTOPIA_ERHUA_ACTIVITY_RECRUITMENT_HERMES_CRON=approved-production-erhua-activity-recruitment-hermes-cron`。
3. 运行 `apply-erhua-activity-recruitment-hermes-cron.sh --install` 再 `--enable`。

> 不改动 sidecar Rust、不扩大二花工具集、不触碰"招募不发群"的 fail-closed 安全边界。
