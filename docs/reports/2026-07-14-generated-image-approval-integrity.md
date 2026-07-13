# Generated Image Approval Integrity

Date: 2026-07-14

## Observed Evidence

`operations-artifact-review-decision` locked an artifact by id and loaded only its work
item and review status before writing the requested decision. It did not distinguish a
Huabaosi `generated_image` from other artifacts or verify the image worker provenance,
URI, hash, PNG metadata, source refs, or `generated_image_created` event.

The disposable PostgreSQL smoke demonstrated the same weak contract: it manually
inserted a minimal pending `generated_image`, approved it with the generic command, and
then used it to unlock the Xiaoman send-request starter.

## Risk

A malformed, manually inserted, or partially persisted image row could be marked
`approved`. The review command would complete its `image_generation_request`, after
which the Xiaoman send-request starter could treat that record as a reviewed image. No
external send occurs in the current runtime, but this bypass would become production
critical before a real send adapter is enabled.

## Root Cause

Human review policy validated reviewer identity and decision shape but assumed artifact
creation had already enforced every type-specific invariant. The generated-image path
introduced stronger provenance and media requirements without adding a matching
approval-time gate.

## Resolution

- Keep the generic reviewer allowlist and decision validation.
- Before approving a `generated_image`, validate its image-request type/status, Huabaosi
  creator and worker marker, HTTPS URI, canonical sha256, PNG MIME/dimensions/byte size,
  risk labels, source brief/hash, prompt hash, and matching creation audit.
- Reject an incomplete image before updating either artifact or work item.
- Record a sanitized `denied_by_policy` event using the generated-image integrity
  policy.
- Keep rejected or changes-requested decisions available so humans can safely stop or
  return malformed artifacts.

## Validation Boundary

The local OrbStack Docker socket was not running during the final database check, and
the same-day attempts to fetch the CI-equivalent `pgvector/pgvector:pg16` image were
already recorded as Docker Hub `Bad Gateway` failures. No local database smoke ran for
this change; no production database is an acceptable substitute.

The guarded disposable PostgreSQL smoke must first prove a malformed pending image is
denied without state changes, then repair the fixture to the production artifact shape,
approve it, and prove exactly one downstream send request is created. No provider,
media, Feishu, QiWe, publish, or external send adapter is called.
