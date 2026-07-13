# 阿靓真实图片生成 Adapter 计划

Status: internal request intake implemented; external adapter owner decision required

Scope: 阿靓（画报司 / `huabaosi`）受控图片生成，不含飞书写回、企微发送或对外发布

## Goal

把已经存在的内部视觉 brief 路径扩展为可审计的真实图片生成路径：

```text
approved poster_brief
  -> image_generation_request
  -> reviewed image provider adapter
  -> immutable generated_image artifact (pending review)
  -> human approval
```

生成成功不等于允许发布。任何企微发送、飞书写回、公开发布或群发仍沿用各自独立的人工确认和 adapter
allowlist。

## Current Baseline

- `huabaosi.create_visual_asset` 目前只生成内部 `poster_brief`，不调用图片模型。
- 小满活动推广路径需要先完成 `evidence_summary`，再生成 `poster_brief`。
- `run-xiaoman-activity-image-generation-starter-worker` 只会从已审核的 `poster_brief`
  创建幂等的 `image_generation_request`。
- `run-huabaosi-image-generation-worker` 目前只校验和预览请求；默认开关为
  `QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED=0`，因此不会请求 provider、上传媒体或创建
  `generated_image`。
- `qintopia_agent_os.artifacts` 已有
  `artifact_uri`、审核状态、来源引用、风险标签和审计事件。
- 已有 COS 工具只用于不可变发布构件和部署请求。不得复用发布构件 bucket/prefix 存储用户可见海报。
- 历史 `qintopia-collab` 文案提到 `gpt-image-2` /
  Image2；这只是历史输入，不能证明现有 API、模型 id、费用、密钥或存储方案已获批准。

## Required Owner Decisions

在实现或启用真实生成前，owner 必须在对应 PR 明确确认：

1. 图片 provider、API 版本、模型 id、费用上限和可用地域。
2. 产物存储服务、独立 bucket/prefix、对象 ACL、保留周期和删除责任人。
3. 可上传的参考素材范围，以及成员照片、肖像、私聊素材和版权材料的授权证明格式。
4. 第一批 staging/test group、审核人 allowlist、失败升级人和灰度停止条件。
5. 生产开关和 rollback owner。

未确认这些项时，worker 只能返回 `image_generation_disabled`、 `adapter_not_configured`
或 preview，不得尝试网络调用。

## Target Contract

新增一个专属 capability 和 worker，而不是扩展现有 `collaboration-worker` 执行外部调用：

| Object      | Target value                                                                          |
| ----------- | ------------------------------------------------------------------------------------- |
| Capability  | `huabaosi.generate_image_asset`                                                       |
| Work item   | `image_generation_request`                                                            |
| Requester   | `xiaoman` 或授权的人类 owner                                                          |
| Parent      | 已审核通过的 `poster_brief` 对应视觉 work item                                        |
| Input       | approved brief id/hash、已完成 evidence id/hash、平台规格、已授权参考素材引用         |
| Output      | `generated_image` artifact，初始 `review_status=pending`                              |
| Idempotency | `huabaosi_image:<approved-brief-id>:<specification>:<prompt-hash>`                    |
| Retry       | 仅可重试 provider 可恢复错误；每次尝试记录无敏感的 provider outcome 和 attempt number |

`generated_image` 必须保存：内容 hash、MIME
type、像素尺寸、字节大小、存储对象版本/不可变 URI、来源 brief/evidence
hashes、模型标识的非敏感摘要、风险标签和审核状态。不得保存 API
key、完整 prompt、私有原始素材、Base id、message id 或 provider 原始响应。

## Storage Boundary

生成图片必须写入独立的媒体存储边界，例如专用的媒体 bucket/prefix 和受控 HTTPS 下载域名。发布构件 COS 路径只服务 release/deploy，不能承载海报。

运行时只接收以下形式的配置，不提交真实值：

```text
QINTOPIA_HUABAOSI_IMAGE_PROVIDER
QINTOPIA_HUABAOSI_IMAGE_MODEL
QINTOPIA_HUABAOSI_IMAGE_API_BASE_URL
QINTOPIA_HUABAOSI_IMAGE_API_KEY
QINTOPIA_HUABAOSI_MEDIA_UPLOAD_ENDPOINT
QINTOPIA_HUABAOSI_MEDIA_PUBLIC_BASE_URL
QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS
QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED=0
```

启用开关默认 `0`。适配器只接受 allowlisted HTTPS media
host；上传前验证文件类型、尺寸、字节数和 hash，上传后再次读取/校验对象元数据。任何失败都不得留下
`approved` 或 `completed` 产物状态。

## Human Gates

1. 人工审核 `poster_brief` 后，才可创建 `image_generation_request`。
2. 图片生成后，`generated_image` 仍是
   `pending`，由审核人检查事实、视觉质量、素材授权和渠道规格。
3. 审核通过只代表该图片可被下游受控流程引用，不代表发送或发布。
4. 需要发送时，仍须经过现有 group-message final confirmation 与 send-ready 边界。

## Rollout And Rollback

- Phase 1: fixture/provider fake server，验证 prompt
  redaction、idempotency、媒体元数据和失败状态。
- Phase 2: owner-approved
  staging，使用隔离素材和测试媒体前缀；不连飞书、企微或发布 adapter。
- Phase
  3: 小范围生产生成，仅生成并进入人工审核队列；观察成功率、重复率、审核拒绝率和存储错误。
- Rollback: 设置 generation enabled 为
  `0`，禁用生成 timer，保留既有 artifact/audit 只读可查；不删除已审核产物或审计记录。

## Required Validation

- Rust unit tests：前置 brief/evidence/审核状态、prompt
  redaction、稳定 idempotency、错误分类、报告不泄露密钥或原始素材。
- Disposable PostgreSQL integration smoke：重复执行不创建重复 image
  request/artifact；失败不会将 work item 标记完成；生成后 artifact 保持 `pending`。
- Local fake provider/media
  server：校验请求 schema、超时、MIME/尺寸/hash、上传失败和回读失败。
- Protected staging smoke：只在显式 enable、配置 test provider 和 test media
  host 时调用外部服务；不发送、不写飞书、不发布。
- Production observation：检查 timer
  command、默认禁用开关、journal 脱敏、队列计数和 rollback 开关；不触发生成。

## Implemented Boundary

- 已加入 capability、迁移、starter、request preview 和受保护 PostgreSQL apply smoke。
- starter 的 idempotency key 包含 approved brief id、规格和脱敏 prompt
  hash；同一 brief 不会创建重复 request。
- apply smoke 固定 generation enabled 为 `0`，断言 request 保持 `queued` 且没有
  `generated_image` artifact。
- provider 调用、媒体上传、artifact 落库、重试、staging
  smoke 和 timer 均尚未实现，必须在 Required Owner Decisions 有明确 PR 记录后继续。

## Explicit Non-Goals

- 自动发布、企微群发、飞书写回。
- 将图片模型或存储密钥提交到 git。
- 将 legacy `qintopia-collab` 的 raw prompt handoff 视为新 AgentOS 接口。
- 使用 release/deploy COS artifacts 作为图片媒体库。
