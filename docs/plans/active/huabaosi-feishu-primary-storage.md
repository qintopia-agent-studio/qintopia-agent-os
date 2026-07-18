# 阿靓图片飞书首存储计划

Status: owner storage decision confirmed; implementation and production canary remain

Scope: 将 Huabaosi 生成的最终 JPEG 直接写入“画报司 | 设计产出库”的“阿靓图片产物版本表”，再创建 AgentOS
`pending generated_image`

## Decision

生产图片 canary 不再依赖独立的 HTTP media upload/public
URL 服务。最终 JPEG 的首个持久化位置是飞书 Base 固定表的 `最终JPEG`
附件字段。Postgres/AgentOS 仍保存工作流、内容 identity、审核和审计事实；飞书负责附件保存、人工预览和协作。

这复用现有二花活动表写入模式：代码先通过固定 Base
API 按稳定业务键搜索，再创建或更新一条记录。飞书自动化只消费已写入记录，不接收 provider 原始响应，也不替代 AgentOS
worker。

```text
approved poster_brief
  -> gpt-image-2 provider PNG
  -> fixed PNG-to-JPEG transform
  -> Feishu media upload
  -> authenticated Feishu readback and byte/hash validation
  -> Base row upsert by AgentOS产物ID
  -> pending generated_image in Postgres
  -> optional Feishu notification/review automation
```

## Fixed Target

- Base: `画报司 | 设计产出库`
- table: `阿靓图片产物版本表`
- schema: `huabaosi-generated-image-v1`
- schema field: `Schema版本=huabaosi-generated-image-v1`
- idempotency field: `AgentOS产物ID`
- attachment field: `最终JPEG`

The existing `海报生产任务表/成品图` remains a business summary. It may be updated only
after a stable AgentOS workflow mapping exists; the canary writes the version table
only.

## Write Contract

1. Derive one stable artifact UUID from the image request and final JPEG identity before
   external I/O.
2. Authenticate with the fixed Huabaosi Feishu profile and validate exact Base/table
   allowlists plus the production release/database bindings.
3. Upload the exact bounded JPEG to the official Feishu media API.
4. Download the uploaded attachment through the authenticated Feishu API and require
   byte-for-byte, SHA-256, MD5, MIME, width, height, and byte-size parity.
5. Search the fixed version table by `AgentOS产物ID`; create on zero matches, update on
   one match, and fail closed on multiple matches. Every row must carry the exact fixed
   `Schema版本`; authenticated readback rejects missing or drifted versions.
6. Only after readback and row upsert succeed, create one `pending generated_image` and
   its sanitized creation audit in Postgres.

The worker must never print or persist Base token, table id, file token, app
credentials, raw Feishu responses, provider payloads, prompt text, or attachment URLs. A
provider, Feishu upload, readback, Base write, or Postgres persistence ambiguity is
terminal for the canary and must not auto-approve, publish, write the legacy task
summary, or call QiWe.

The first canary remains `pending` until an explicit manual review apply occurs. The
read-only `huabaosi-feishu-primary-storage-revalidate --artifact-id <uuid>` sidecar
entrypoint can prove the current Feishu row schema, workflow-root association, and
attachment still match AgentOS facts through authenticated readback, but it does not
approve, publish, write Postgres or Feishu, call QiWe, or send. Authenticated approval
consumption may approve a `feishu-base://` artifact only inside the explicit manual
review transaction after revalidation succeeds; Feishu field changes alone cannot cross
the approval gate.

Authenticated approval consumption is a separate boundary: an explicit manual `approved`
apply may revalidate first, then approve only if the transaction-locked Postgres
artifact still matches the in-memory revalidation evidence. A Feishu review field or
automation event alone remains insufficient. Rejection and changes-requested decisions
do not require external readback. QiWe intake may consume `feishu-base://` only through
the combined staging Feishu-to-QiWe bridge with same-byte temporary-storage readback;
production, default, and single-feature builds still reject that route until staging
evidence and a later production enablement PR are reviewed.

## Feishu Automation

The Base currently has no configured automation. After the first pending canary row is
verified, an owner-reviewed automation may use “添加新记录时” or “修改记录时” to notify
reviewers and reflect reviewed status. Automation must not decide approval from a field
change alone and must not trigger QiWe delivery or external publication.

## Production Gates

- immutable release binary compiled with the reviewed production and Feishu adapters;
- image generation and Feishu storage both explicitly enabled;
- exact owner approval phrases;
- deployed release SHA and production database URL hash bindings;
- exact Base/table allowlists and fixed schema version;
- fixed Huabaosi profile env path and official Feishu HTTPS API root;
- release-local no-network preflight before the timer is enabled;
- one-item-per-invocation timer and immediate rollback.

Release Please PR merge, GitHub Release publication, and production activation remain
manual owner actions. The first real row must remain `pending` until a human verifies
the final JPEG and sanitized worker evidence.
