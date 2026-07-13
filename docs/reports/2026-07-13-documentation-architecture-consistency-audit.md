# 文档与架构一致性审计

Date: 2026-07-13

Status: complete; documentation-only correction

## Scope

审计了仓库全部 Markdown 文件的格式和本地链接，并以架构概览、产品 PRD、当前路线图、工作流 contract、Rust
implementation、systemd render 和生产只读状态为事实基线。

本次不读取密钥、原始聊天、飞书 Base 内容或生产日志；不写数据库、不改 systemd、不发布 release。

## Confirmed Architecture

```text
event_signals
  -> xiaoman activity_promotion_request
  -> evidence_request + visual_asset_request
  -> evidence_summary + poster_brief
  -> human review
  -> image_generation_request (merged on master, provider disabled)

existing internal send preparation
  -> approved poster_brief
  -> group_message_request (awaiting_publish)
  -> human final confirmation
  -> internal send-ready audit (send_executed=false)
```

Postgres/AgentOS remains the fact source. Feishu is a human workbench and mirror. The
existing send-preparation path still references `poster_brief`; it does not yet require
a `generated_image`. Changing that dependency belongs to the later real-image adapter
phase, after image storage and human review are implemented.

## Production State Observed

- Production release is `v0.2.6` at `f8b02d7`.
- The Xiaoman signal, promotion-starter, evidence, visual, and send-request-starter
  timers are active.
- The recorded `v0.2.6` aggregate Xiaoman preflight passed without external calls.
- `#91` through `#93`, including the image request starter and image worker preview, are
  merged on `master` but are not in the currently deployed `v0.2.6` binary.
- No Huabaosi image-generation timer exists. The worker remains disabled by default and
  has no provider or media-storage adapter.

## Corrections Made

- Current activity-promotion documents now point to `runtime/sidecar`, not the old
  standalone checkout.
- The 2026-06-30 control-plane runbook is explicitly a historical baseline. Its current
  status addendum points to the release/current model and the production preflight
  record.
- Current workflow and registry text now distinguish the passed internal production
  observation from unimplemented external adapters.
- The deploy readiness check now validates that same passed-observation state instead of
  requiring an obsolete `pending` registry note.
- The active Aliang plan records both the historical OpenAI-compatible `gpt-image-2`
  evidence and the fact that the merged request boundary is not yet deployed.

## Validation

- `pnpm lint:md`
- local Markdown link resolution check
- `pnpm workflows:check`
- `pnpm registry:check`
- `pnpm policy:check`
- `git diff --check`

## Remaining Boundary

This audit does not approve real image generation, media upload, Feishu writeback, QiWe
sends, public publishing, or a new timer. Those remain separately reviewed production
changes with staging smoke and rollback evidence.
