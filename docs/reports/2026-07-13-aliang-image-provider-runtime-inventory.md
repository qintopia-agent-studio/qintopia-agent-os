# 阿靓历史图片 Provider 运行态核查

Date: 2026-07-13

Status: read-only evidence; no production behavior changed

## Why This Check Ran

阿靓曾经产出过图片海报。本次核查确认旧路径使用的 provider 和可复用边界，避免把历史 Hermes
prompt 当成没有实现依据的设计假设。

## Evidence

在生产服务器上对 `hermes-gateway-huabaosi.service` 做了只读核查：

- 服务处于 `active`，由 Hermes `huabaosi` profile 启动。
- 运行进程存在 `OPENAI_API_BASE`、`OPENAI_BASE_URL`、`OPENAI_API_KEY` 和
  `OPENAI_IMAGE_MODEL` 环境变量。只枚举了变量名，没有读取值或密钥。
- 脱敏分类确认 `OPENAI_IMAGE_MODEL` 配置为
  `gpt-image-2`；endpoint 是 HTTPS 的 OpenAI-compatible proxy，而不是本地生成器。
- profile 保留 `garden-gpt-image-2`
  prompt 工作目录、Image2 交付要求，以及 2026-06-25 的 Image2 工作目录和图片文件。未读取 prompt、图片内容、会话或日志。
- 仓库的旧 `qintopia-collab` 合同也要求正式/待审核海报使用
  `gpt-image-2 / Image2`，并将成品附件登记到飞书设计产出库。

这证明旧运行路径确实采用 OpenAI-compatible
`gpt-image-2`。它不能证明某一张历史图片的完整 request、费用、授权或存储保留策略，因此不把旧 workspace 当成新系统的事实源。

## Decision For Continued Development

新的 `huabaosi.generate_image_asset` adapter 以 OpenAI-compatible `gpt-image-2`
作为 provider/model 实现目标。旧 profile 的 endpoint 和凭据仍是运行态秘密；新 adapter 不得硬编码、复制或自动复用它们。

现有可复用的是 provider 协议和模型选择，不是现成的 Rust
adapter：仓库仍没有图片请求、下载、上传、媒体回读校验或 `generated_image`
artifact 落库实现。

## Remaining Gates

在调用真实 provider 前，owner 仍需在 PR 中确认：

1. 可供新 adapter 使用的 OpenAI-compatible account/endpoint、预算上限和地域。
2. 独立媒体存储、对象 ACL、受控下载域名、保留周期和删除责任人。旧飞书附件及 release/deploy
   COS 均不能作为该存储边界。
3. staging 素材、审核人、失败升级人、灰度停止条件和 rollback owner。

在这些决定完成前，图片 worker 继续保持 disabled/preview，不连旧 endpoint、不写飞书、不发送企微，也不启动 timer。

## Validation

- 只读 SSH：服务状态、启动命令、非敏感文件名、环境变量名和脱敏 provider/model 分类。
- 本地仓库搜索：未发现历史 provider adapter、媒体上传实现或独立图片存储实现。
- 未读取 `.env` 内容、API key、Base token、原始 prompt、会话、日志或图片内容。
