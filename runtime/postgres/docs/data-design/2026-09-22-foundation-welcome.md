# 欢迎流程消费共同基础

日期：2026-09-22。本设计实施合并交接 §8 的本地 D 切片。

复用 Person、来源身份、申请、住宿案例、WorkItem、Artifact、审批和逐部分 DeliveryAction。不建立第二个人员库、Inbox 或调度器。

## 权威与版本

`welcome_foundation_targets`
将已有逻辑目标固定绑定到共同授权 tenant/scope。绑定不授予任何权限。业务设置存于共同知识版本服务，固定 key 为
`resident_welcome`；正文沿用 Space
`business_definition_versions`。单次设置使用同一版本服务的 case_ref，优先于有效持续设置，不覆盖上层权力。

欢迎实际执行读取共同 `authorize_current`，维度固定为
`erhua / community_service / change_rules、review、publish`。旧 `welcome_grants`
不允许执行已接入共同基础的目标。

直接发送保存规则、内容和自主发布授权依据，不插入伪造的人工审批。先审和上层指定人确认只接受真实会话发起、版本匹配的批准；批准人的现行权限与授予依据在执行前重验。规则作者离任、许可撤回、内容改版和目标变化均使旧准备失效。

具体内容审核和上层发布确认分别保存为
`content_review`、`publish_confirmation`。先审模式由本栋规则作者以当前 `review`
权批准内容；上层指定确认人仅以当前自主 `publish`
权确认发布，不额外要求其持有内容审核权。两层都适用时必须分别满足，任一批准撤销或其依据变化都会阻止发送。每次批准调用只完成明确的一种决定。

## 持久化增量

既有审批和动作允许旧欢迎专用 grant 为空；共同基础动作必须有 `foundation_basis` 和
`artifact_id`，且不能同时持有旧发布授权。实际审批保存共同授权证据；直接发送不需要 approval_id。稳定 action
key 保持 `welcome-v1/case/phase/target/part`，不因改版、恢复或重试改变。

合成渲染结果只在 isolated synthetic 数据库的 `welcome_local_artifact_data`
中保存，用内容 hash 验证受控预览。申请附件适配沿用 upload
intent 和稳定 storage_ref；不写阿靓普通设计产出 Base。

`welcome_synthetic_effects`
是明确的外部边界测试适配器，按 action 去重并记录发送结果。unknown 不重发；只有回读既有 effect 或明确人工核对证据才能恢复。Agent 实际调用及结果使用已有
`work_item_events`，保留各 Agent 职责。

`.004`
将已有本地执行器可用性表的固定 Agent 集合从二花扩充到岸岸和阿靓。它不新建任务队列、启用 Profile 或提供生产执行许可。编排、渲染、审核传递和发送分别核验对应执行器；不可用时保留原任务和等待原因。

## 渲染与生产边界

旧 `generate_card_v10.py`
当前未在本地版本化源码找到。新增 Pillow 合成渲染器可证明真实图片解码和排版、长文案与无照片布局，不声称旧脚本迁移完成。固定脚本不接受网络 URL，不导入旧主程序，不上传、不发消息、不按姓名去重。

全部新入口保留 synthetic、literal-loopback、qintopia_test 门禁。未启用生产 Profile、真实消息、附件上传或 PMS 写入。回滚停止本地进程，保留审批、action、unknown 和审计，不删除记录、不重放历史积压。

## 验证

沿用 T01—T16 的既有测试，加共同授权与规则的 W01—W10、执行前撤权、版本变化、逐部分恢复、unknown 回读和跨实例重启。渲染测试调用实际 Pillow 脚本，外部上传及发送仅使用显式测试适配器。
