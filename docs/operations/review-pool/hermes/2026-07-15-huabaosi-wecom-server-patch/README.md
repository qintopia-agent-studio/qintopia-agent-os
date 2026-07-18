# Huabaosi WeCom Server Patch Snapshot

This package preserves the production Hermes WeCom patch as review evidence while the
owned behavior moves to the Rust sidecar. It is not a deployable Hermes fork or a source
for production hot patches.

## Source

- Repository: `NousResearch/hermes-agent`
- Server checkout: `/home/ubuntu/.hermes/hermes-agent`
- Upstream baseline: `9cbc37e25`
- Server-only commit: `c76d035c1`
- Observed: 2026-07-15
- Patch SHA-256: `bf60db1218d1a5690f93a999fb0e81bc6be1547c2f4da3f5b2397ed09658f572`

The patch is the exact diff from `9cbc37e25` through the observed WeCom worktree for:

- `gateway/platforms/wecom.py`
- `tests/gateway/test_wecom.py`
- `tests/gateway/test_text_batching.py`

The file hashes in `source-files.sha256` identify the complete observed files without
copying the old Hermes checkout into this repository.

## Disposition

| Behavior                                    | Disposition              | Owned target                                                               |
| ------------------------------------------- | ------------------------ | -------------------------------------------------------------------------- |
| Internal process filtering                  | adopt as contract        | `runtime/sidecar/src/huabaosi_wecom_policy.rs`                             |
| Busy and formatting fallback classification | adopt as contract        | Rust policy fixtures and narrow positive/negative tests                    |
| WebSocket close detection                   | separate upstream review | Not required for the Huabaosi policy migration                             |
| Media retry and expired reply fallback      | separate reliability PR  | Must prove idempotency and ambiguous-send behavior before adoption         |
| Shared Chinese approval/replacement copy    | reject                   | It hard-codes another Agent identity and conflicts with the included tests |
| Direct application to production checkout   | reject                   | Requires an immutable reviewed release and rollback                        |

The server patch does not recognize the two incident strings. The Rust policy does:

- `Interrupting current task`
- `Response formatting failed`

It also has a negative case proving that an inbound user request containing `plain text`
is not suppressed.

## Excluded From This Patch

The same server checkout had unrelated Kanban, webhook script-action, generic message
media, backup-file, and nested training-repository changes. They are classified in
`docs/reports/2026-07-15-hermes-core-server-patch-inventory.md` and must not enter this
migration PR.

## Production Boundary

- No server write, restart, profile mutation, database write, or external send.
- This directory is not included in the deploy bundle.
- A later independent PR must implement and validate the production routing switch.
- Rollback remains `hermes-gateway-huabaosi.service` until that cutover is approved.

## Validation

```bash
pnpm runtime:hermes:check
pnpm secrets:check
pnpm test:sidecar -- huabaosi_wecom_policy
git diff --check
```
