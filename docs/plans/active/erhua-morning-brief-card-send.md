# 二花早报卡片发送 — 实现方案

## 背景：现在的真实情况

- 昨天 #616 做完的是「**能把早报画成卡片图片**」（`morning_brief.py --render-image`，已验证可出图）。
- **没做的是「把卡片发到群里」**。今天早报还是发纯文字。
- 发图没有现成捷径——必须给早报接一条「发图链路」，**要改 Rust sidecar 并重新部署**。

## 目标

早报每天 08:10 发出**卡片图片**（而不是现在的纯文字），照小满日报已经跑通的发图链路，给早报接一条平行的。

## 小满日报发图链路（参考，已在生产跑通）

```text
worker 渲染日报图(Pillow)
  → sidecar: operations-daily-case-report-media-upload  (上传图到存储, 拿 artifact_uri)
  → 建发送 work_item (claim 可认领)
  → qiwe-image-send-worker (systemd timer, 已激活) 认领 → 上传 QiWe → 回调 → 发群
```

## 早报要新增的（照日报平行造一套）

### ① 早报 worker 渲染卡片（改 bash，低风险）

文件：`deploy/sidecar/scripts/erhua-morning-brief-worker.sh`

- 调 `morning_brief.py` 时加 `--render-image <path>`，让早报每天生成卡片图
- 现在没加，所以根本没画图

### ② 早报 media-upload（新增 Rust，参考日报造一个早报版）

- 参考 `operations.rs` 的 `daily_case_report_media_upload`，新增
  `erhua_morning_brief_media_upload`
- 把卡片图上传到存储（早报现走 feishu-base），返回 artifact_uri + content_hash 等
- 在 `config.rs` 注册新 sidecar 命令
  `operations-erhua-morning-brief-media-upload`（参考 1082 行日报的注册方式），`main.rs`
  加分支（参考 608 行）

### ③ image-send 认领规则加早报（改 Rust，**动发送核心，最需谨慎**）

文件：`runtime/sidecar/src/qiwe_image_send_state.rs` 的 claim SQL（约 190-203 行）

- 现在只认两种：`image_generation_request`（AI画图）、`daily_case_report_request`（日报）
- **新增第三条**：早报
  `erhua_morning_brief`（workflow_type=erhua_morning_brief + 对应 work_item_type/capability_key），让 qiwe-image-send-worker 能认领早报的图
- **原则：新增，不动现有日报/画图那两条**，把对生产的影响降到最低

### ④ 早报 worker 发送段改成发图（改 bash）

文件：`erhua-morning-brief-worker.sh`

- 现在是 `run-qiwe-text-send-worker`（发文字）
- 改成：media-upload 拿 artifact_uri → 建发图 work_item → 走 qiwe-image-send 链发图

## 改动清单（要动的文件）

| 文件                            | 改动                       | 语言        | 风险                 |
| ------------------------------- | -------------------------- | ----------- | -------------------- |
| `erhua-morning-brief-worker.sh` | 加渲染 + 发送段改发图      | bash        | 低                   |
| `operations.rs`                 | 新增早报 media-upload 函数 | Rust        | 中                   |
| `config.rs`                     | 注册新 sidecar 命令        | Rust        | 低                   |
| `main.rs`                       | 命令分支                   | Rust        | 低                   |
| `qiwe_image_send_state.rs`      | claim SQL 加早报认领规则   | Rust        | **高（动发送核心）** |
| 对应测试                        | 新增/更新单测              | Rust/Python | —                    |

## 工作量与验证

- 估计：几百行 Rust + bash 改动
- **每步验证**：Rust `cargo build` +
  `cargo test`（确认日报发图不受影响）→ 早报卡片渲染出图（已验证）→ 本地 dry-run 全链路 → 上生产前 dry-run
- **上生产**：重编 sidecar → deploy
  → 确认 qiwe-image-send 仍激活 → 第二天 08:10 看早报是否发卡片

## 风险点（重点盯）

1. **#③ 动发送核心 claim SQL**——只新增、不改现有逻辑，先全测试再上
2. 重编 sidecar 后要确认 qiwe-image-send worker 仍正常（日报发图不能被影响）
3. 卡片图内容边界：早报卡片不能带内部运营词（参考现有文字版的 fail-closed 拦截）

## 不做的事（避免范围蔓延）

- 不改早报卡片的内容/版式（昨天 #616 已定的卡片样式不变）
- 不动日报发图链路
- 不改早报 08:10 调度
