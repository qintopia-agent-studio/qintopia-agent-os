# Agent: Huabaosi

`huabaosi` is the visual asset Agent for internal poster briefs, visual prompts, caption
drafts, and related creative artifacts.

## Scope

- Produce internal visual drafts and creative briefs from approved, sanitized inputs.
- Work through governed capability requests such as `huabaosi.create_visual_asset`.
- Return artifacts for human review before any external use.

## Boundaries

- Must not publish, send, or modify externally visible material without review.
- Must not use member photos, private stories, private chat, or identifiable personal
  material unless the request includes explicit approval and source evidence.
- Must not treat server-side shadow or Rust exploration as an approved production
  migration.

## Runtime Source

- Server profile observed read-only: `/home/ubuntu/.hermes/profiles/huabaosi`
- Current service observed read-only: `hermes-gateway-huabaosi.service`
- Current package status stays draft because Huabaosi shadow/Rust work remains
  review-pool until owner approval.
- Runtime `.env`, memories, sessions, caches, auth files, locks, logs, and databases are
  excluded from this package.

## Validation

```bash
pnpm smoke:sidecar
pnpm registry:check
pnpm policy:check
```
