# v0.3.9 制品 manifest 重复路径阻断

## 证据与原因

Release `v0.3.9` 对应 `e24a9379319710601b0b7e2359e6ffc33bf6cc69`，发布构建
[37096818102](https://github.com/qintopia-agent-studio/qintopia-agent-os/actions/runs/37096818102)
成功，完整 bundle 已上传 COS。

GitHub 构建和服务器下载的 archive SHA256 均为
`91cfbe08fc04806f2676fb5964f84622f3a8255d42902b3149719d4fc688243f`；manifest SHA256 均为
`b38ef7d48571688e50ddc61e019f0eb09a05305b818d52a81104a192c800f266`。

完整包校验拒绝
`staged recovery bundle path is duplicated`。manifest 共 771 项、765 个唯一目标；六个重复项的内容、摘要、大小和来源完全一致，分别是：

- `runtime/hermes/cron/reviewed-cron-jobs.json`
- `runtime/hermes/cron/xiaoman/weekly-plan-confirmation.job.json`
- `runtime/hermes/cron/erhua/morning-brief.job.json`
- `runtime/hermes/scripts/qintopia-hermes-cron-wrapper.template.sh`
- `runtime/hermes/scripts/qintopia_xiaoman_weekly_plan_confirmation.sh`
- `runtime/hermes/scripts/qintopia_erhua_morning_brief.sh`

提交 `f5fbb29d2`
于 2026-08-10 把两个目录加入递归选择，保留了原单文件选择。构建器按两个列表直接复制并追加 manifest，产生重复条目。#744 新增的路径唯一性校验正确拒绝该产物，但其回归未检查真实构建产物的唯一性，遗漏了构建/消费合同的兼容缺口。

## 修复范围

沿用现有唯一目标路径合同：仓库来源文件按集合合并再复制；任何后续来源与已写目标冲突，构建即失败。既有实际构建测试检查 manifest 路径唯一、所有条目与磁盘摘要及大小一致，并覆盖上述六个真实重叠来源。

不放宽 staged
verifier，不修改服务器 manifest，不复用旧包来执行新逻辑。新增 workflow、job、依赖、schema 和检查入口均为零；只修改构建实现和已有测试用例。

## 生产边界与后续

服务器保留旧包为
`recovery/staged-v038-retained`。新包位于固定 staged 目录，未通过入口校验。

没有收尾旧失败请求、执行新 consume 或切换 O/P；hold、旧 journal/回执保留。接管前 24 个配置文件摘要无变化。

发布 action 在等待服务器 dry-run 回执时取消，避免隔离中的旧 poller 造成超时。取消不撤销 COS 请求，不证明 dry-run 通过；该请求不重放。

服务器已核对 `deploy-20261003T044500Z-e24a93793197`
为有效签名 dry-run，无本地消费记录。普通取消未结束 `always()`
job，最终仅强制取消 GitHub 等待；未强杀服务器进程。

后续须持锁核对指针、claim、消费者，换新完整产物。校验包和旧签名失败回执，有限收尾，再发新的签名接管请求。

保留 v0.3.9 tag 和 COS 制品，不覆盖同 SHA 产物。修复合并后，先使用现有 `Artifacts`
workflow 在受审 master 上仅构建并上传新 bundle，完成受控接管。

再由 Release
Please 生成下一正常版本；负责人合并并发布。不改现有 workflow 或 secrets 归属。#735 仍等实际部署恢复后推进。

## 验证

实际构建回归修复前准确检出六项重复，修复后通过；带 v0.3.9 真实控制台资源的完整包回归也通过。每项 manifest 的大小与 SHA256 均核对实际文件，并确认六个 Cron 来源仍存在。

修复后 765 个唯一 manifest 目标；764 个 payload 文件的摘要、大小、权限与 v0.3.9 去重后的完整内容一致，业务脚本未删减或修改。

本地综合检查曾在既有 COS 模拟的 HTTP 读取出现 connection
refused，独立复跑真实 poller 测试通过。这次未修改该测试的服务启动时序，不能把复跑成功当成已修复波动。

完整项目检查及远端审查结果以关联修复 PR 的最新 SHA 为准；制品级回归不能替代完整检查。生产恢复尚未通过。
