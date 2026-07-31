# 小满工作流现状报告

日期：2026-07-30

## 环境状态

- 仓库本地 HEAD：`v0.2.58` (`045bd2114114578b61c8be10510768ef2b563adb`)
- 生产 `release/current`：`v0.2.58` (`045bd211`)
- `/etc/qintopia/message-sidecar.env`：已切到 `045bd211`
- `/home/ubuntu/.hermes/profiles/xiaoman/.env`：`QINTOPIA_XIAOMAN_ACTIVITY_WORKER_BIN`
  已指向 `045bd211`
- `/home/ubuntu/.hermes/profiles/erhua/.env`：已切到 `045bd211`
- `/etc/qintopia/message-sidecar.env` 权限：已修复为 `root:ubuntu` `640`（Hermes
  MCP 可读）
- Hermes gateway：`xiaoman`/`erhua` 均 active，`qintopia-context` MCP 子进程已正常启动
- 文字预告生成验证：2026-08-01 瑜伽活动已成功生成 `next_day_preview`
  文案，等待刘珊确认后才会 handoff 给二花
- 历史遗留定时任务：已停止并禁用 `xiaoman-huabaosi-collab-delivery.timer` 和
  `xiaoman-huabaosi-handoff-consumer.timer`

## 目标架构

目标架构是：飞书多维表格作为活动与图片的事实源，小满读取后生成文字预告与提醒，经运营确认后由二花发群；海报 brief 由阿亮/IMG
2 生成图片存入飞书，确认后进入发群链路；活动结束后触发素材回填提醒，连续三次未回填则标记遗漏。

## 当前已完成的流程

已完成：小满可读取飞书 Base 活动记录并生成文字预告，图片生成 preflight 通过且 worker
timer 已启用，飞书图片表已有 approved 图片。未完成：二花企微发群未激活，海报端到端未闭环，素材回填提醒未实现，历史遗留定时任务已清理。

## 组件状态

| 组件                  | 状态   | 备注                                                                                                 |
| --------------------- | ------ | ---------------------------------------------------------------------------------------------------- |
| 小满读取飞书 Base     | 可用   | 2026-07-30 验证读取到 3 条 occurrence 记录                                                           |
| 文字预告生成          | 可用   | 2026-08-01 瑜伽活动已生成 `next_day_preview` 文案；需人工确认后才能发送                              |
| 二花企微发群          | 未激活 | `qiwe_image_send_production_observation_state=disabled`                                              |
| 图片生成 preflight    | 通过   | v0.2.58 已配置 provider                                                                              |
| 图片生成 worker timer | 已启用 | 等待触发                                                                                             |
| 飞书图片存储          | 有数据 | `huabaosi-generated-image-v1` 表已有 approved 图片                                                   |
| 飞书图片镜像 worker   | 未激活 | env 中 `QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED=1`，但 worker timer 仍是 disabled；需 owner 显式激活 |
| 素材回填提醒          | 未接入 | 未配置                                                                                               |
| 历史遗留定时任务      | 已清理 | 已停止并禁用 `xiaoman-huabaosi-collab-delivery.timer` 和 `xiaoman-huabaosi-handoff-consumer.timer`   |

## 未完成的卡点

1. 二花发群未激活：QiWe image-send 服务处于 disabled 状态，需 owner 决策激活。
2. 端到端未验证：从“活动录入 -> 小满生成预告 -> 刘珊确认 -> 二花发群 -> 企微到达”尚未跑一次真实流程。
3. 海报生成链路未闭环：preflight 通过，但缺少真实 brief 到生成到确认到发送的完整测试。
4. 素材回填提醒未实现：活动结束后自动提醒运营回填、连续三次未回填标记遗漏的逻辑未上线。
5. 飞书镜像未激活：env 中 mirror 开关已置 1，但对应 systemd
   timer 仍为 disabled，需 owner 按 guarded 流程激活。

## 下一步建议

1. 让刘珊在聊天中确认 2026-08-01 瑜伽活动预告（回复“发”），验证小满 -> 二花 handoff 链路。
2. 若文字 MVP 通过，再决策是否激活 QiWe image-send 进行图片端到端测试。
3. 按 guarded 流程激活 Feishu mirror worker timer，恢复 Feishu mirror 观察 smoke。
4. 按会议纪要推进素材回填提醒功能。
5. 确认 2026-08-01 瑜伽活动预告是否需要图片；若需要，先补齐负责人字段，再触发阿亮/IMG
   2 生成海报。
