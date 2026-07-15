# Hermes Core Server Patch Inventory

Date: 2026-07-15

Status: read-only production inventory; Huabaosi WeCom patch extracted; no production
behavior changed

## Current State

The production checkout at `/home/ubuntu/.hermes/hermes-agent` is not a clean upstream
Hermes source:

- branch: `main`
- HEAD: `c76d035c1cefa4dc1ef7e83f11b4e413897ecf58`
- upstream: `NousResearch/hermes-agent` `origin/main`
- local relation to the server's fetched upstream ref: 1 commit ahead, 2465 commits
  behind
- worktree: 19 dirty entries

The 19 entries are not 19 services or 19 ready patches. They are 11 modified tracked
files, 7 backup files, and 1 nested untracked directory.

## Classification

| Source                                             | Classification | Decision                                                    |
| -------------------------------------------------- | -------------- | ----------------------------------------------------------- |
| `gateway/platforms/wecom.py` and two gateway tests | review-pool    | Extract exact patch; map incident policy to owned Rust code |
| `gateway/platforms/base.py` and its test           | deprecated     | Kanban workspace media-root change; do not adopt            |
| `gateway/platforms/webhook.py`                     | review-pool    | Script execution surface needs an independent security PR   |
| `hermes_cli/kanban_db.py` and Kanban tool/test     | deprecated     | Hermes Kanban is not the future orchestration backbone      |
| `tools/send_message_tool.py` and its test          | review-pool    | Generic WeCom media behavior needs a separate migration     |
| seven `*.bak-*` files                              | remove         | Server backups are runtime state, not source packages       |
| `tinker-atropos/`                                  | remove         | Nested training repository is unrelated to Agent OS runtime |

## Extracted WeCom Patch

The exact combined patch from upstream baseline `9cbc37e25` through the observed WeCom
worktree is stored at
`docs/operations/review-pool/hermes/2026-07-15-huabaosi-wecom-server-patch/hermes-wecom.patch`.
Its SHA-256 is:

```text
4a7f3d3c221cd85cb91318673fb30a52ea711a2b0b7ceb1ca0c7c39a932c9b06
```

The patch is intentionally review-only. It includes useful WeCom reliability ideas but
cannot be deployed as-is:

- Its internal-process filter does not recognize the incident's busy-ack or formatting
  fallback text.
- It hard-codes another Agent identity in shared WeCom copy.
- Its existing suppression tests conflict with the dirty replacement behavior.
- Its retry behavior does not prove idempotency or classify an uncertain external send.

The incident policy is already represented in
`runtime/sidecar/src/huabaosi_wecom_policy.rs` with bounded input, sanitized output,
narrow matching, and negative coverage for ordinary inbound `plain text` requests.

## Validation

Completed during read-only inventory:

- `git status --short`
- `git diff --stat`
- `git diff --name-status`
- `git diff --check`
- source file SHA-256 capture
- exact patch SHA-256 capture

Repository validation:

```bash
pnpm runtime:hermes:check
pnpm secrets:check
pnpm test:sidecar -- huabaosi_wecom_policy
git diff --check
```

The local `pnpm check:light` attempt was blocked because the configured package-manager
shim could not verify or fetch the signed `pnpm@10.29.2` release from the npm registry.
`pmOnFail=ignore` was not used. The fixed repository-local Node entrypoints for runtime,
secrets, inventory, policy, deploy contracts, registry, agents, deploy preflight, and
deploy-bundle construction were run directly; GitHub CI must still run the complete
repository check.

## CI Remediation

The first PR run failed restart-target resolution because non-Markdown files under
`runtime/hermes/review-pool/` were correctly treated as unmatched production-adjacent
paths. Adding a no-restart rule would itself change the deploy contract and schedule an
unnecessary system-service restart.

The review-only patch, manifest, and source hashes were therefore moved to
`docs/operations/review-pool/hermes/`. The owned runtime target remains the Rust policy,
while the raw server evidence now follows the repository's existing no-restart docs
boundary. Restart-target resolution must report no targets for this PR.

The latest Reviewer Guide also found that the first contract checker validated only the
`a/` side of each `diff --git` header. The checker now requires both `a/` and `b/` paths
to match the same ordered allowlist, with negative tests for either side escaping to an
unreviewed Hermes path.

## Remaining Boundary

- No production file was edited, cleaned, committed, or deleted.
- The extracted patch is excluded from release artifacts and is not production-ready.
- Webhook, Kanban, generic message-media, backup, and nested-repository entries remain
  separate dispositions; they must not be bundled into the Huabaosi WeCom migration.
- The next independent migration PR must own production routing, canary evidence,
  rollback, and a monitor window before Hermes can stop being the fallback.
