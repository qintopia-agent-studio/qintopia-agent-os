# Xiaoman Feishu Poster Production Closeout

Date: 2026-08-02

This runbook finishes trusted direct ingress and one internal-group poster canary with
one closeout PR and one immutable Release. Configuration, database credential rollover,
policy writes, direct activation, and group activation remain separate approval gates,
but none of them requires another build or Release.

## Completion Boundary

The closeout is complete only when all of these facts hold on the same Release SHA:

- direct and internal-group requests acknowledge within five seconds;
- one source message creates one workflow, brief, image request, and notification;
- the generated image returns to the correct direct chat or original group thread;
- one authorized review decision is persisted;
- service reloads do not duplicate generation or delivery;
- external, community, activity, and member groups receive no draft; and
- the workflow has zero `group_message_request`, `send_executed`, and
  `external_published` facts.

Publishing the Release or passing a configuration preflight alone does not satisfy this
boundary.

## Owner-Held Inputs

Keep raw identifiers and credentials outside git, shell history, reports, logs, and
chat. The reviewed rollover request contains only these non-secret bindings:

- one operation UUID, the exact Release SHA, and the successful same-SHA dry-run request
  id;
- SHA-256 digests of the rollover, configuration, and policy entrypoints in that
  Release;
- the current production database URL SHA-256;
- opaque hashes of the database role, direct Feishu conversation, and requester.

The later group-only configuration remains in the owner-controlled secret source and
contains:

- the Xiaoman Bot open id for the group canary;
- the exact direct plus canary-group chat ceiling;
- the exact requester plus reviewer user ceiling; and
- one private and one internal-collaboration policy document.

The release-owned state machine generates the successor password in memory and stores
the old and successor URLs only in its root-owned mode `0600` recovery state under
`/var/lib/qintopia-xiaoman-db-password-rollover`. PostgreSQL invalidates the previous
password during `prepare`; recovery therefore uses the persistent state and never
depends on the previous password remaining valid after that point.

## 1. Publish Once

Merge the single closeout PR through the normal owner review and Release Please flow,
then manually publish its draft Release. Record the exact 40-character Release SHA. Wait
for the Release-triggered production deploy result to succeed before proceeding.

Do not publish an intermediate configuration-only or group-only Release.

Before configuration, the promoted Release must pass the same boundary in the deploy
runner and all three Xiaoman protected entrypoints: the Release root, `sidecar/`, and
sidecar binary are owned by the privileged deploy identity, required paths are not
symlinks, and no protected path is group- or world-writable. Owner-writable `root:root`
directories are valid because same-SHA metadata repair remains owned by the deploy
runner.

## 2. Preflight The Same-SHA Reload

Before changing the database password, copy the immutable identity from the successful
Release-triggered deploy result. For this Release it must contain all of these exact
values:

- the published SHA for `commit_sha`, `runtime_sha`, `deploy_bundle_sha`, and
  `release_sha`;
- `runtime_artifact_profile=huabaosi-production`;
- `release_scope=sidecar-runtime,deploy-bundle,hermes-plugins`;
- `restart_targets=qintopia-system-services,hermes-erhua`; and
- `rollback_on_smoke_failure=true`.

Do not use the manual workflow defaults or reconstruct this identity from memory. Run
one `Deploy Production` workflow dispatch with that exact identity and `dry_run=true`.
The result must confirm the existing immutable Release, complete scope, dual restart
target set, and successful dry-run checks without promotion or restart. Retain the
processed dry-run request id, reviewed request, and owner approval for its identical
`dry_run=false` dispatch, but do not submit the live request yet. The protected rollover
will read that exact request and result from the deploy runner's root-owned evidence
store before it creates recovery state or changes PostgreSQL.

Stop before password rotation if the dry-run identity differs from the existing manifest
or any check fails.

## 3. Prepare The Protected Rollover

Stream this exact schema from the owner-controlled evidence source. Do not create a
world-readable file or paste raw database, chat, user, or role values into it:

```json
{
  "schema_version": 1,
  "operation_id": "<owner-approved-uuid>",
  "release_sha": "<released-commit-sha>",
  "dry_run_request_id": "<successful-same-sha-dry-run-request-id>",
  "rollover_script_sha256": "<release-rollover-script-sha256>",
  "config_script_sha256": "<release-config-script-sha256>",
  "policy_script_sha256": "<release-policy-script-sha256>",
  "old_database_url_sha256": "<approved-current-database-url-sha256>",
  "role_ref": "sha256:<approved-role-ref>",
  "conversation_ref": "sha256:<approved-direct-conversation-ref>",
  "actor_ref": "sha256:<approved-requester-ref>"
}
```

Run `prepare` once with the exact owner approval:

```bash
secure-reviewed-rollover-json | \
  sudo /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/xiaoman-shared-db-password-rollover-production.py \
    --stdin --command prepare \
    --approval approved-production-xiaoman-shared-db-password-rollover-v1
```

`prepare` validates the immutable Release and approved targets, previews the three-file
configuration transaction, generates and applies one successor password, reconciles an
unknown PostgreSQL commit result, and applies direct configuration with group fixed at
`0`. It updates `/etc/qintopia/message-sidecar.env`, the Xiaoman environment, and the
Erhua environment through one locked recoverable transaction. Every holder must contain
either the approved previous URL or the generated successor URL; a third value fails
closed. The result must reach `phase=direct_config_applied` with `reload_required=true`.
The command does not restart a service, call Feishu, enable a delivery timer, or send a
message.

## 4. Reload The Same SHA

Immediately submit the already reviewed Step 2 request with only `dry_run=false`
changed. Do not pause to create or review a new request after `prepare`; the old shared
password is no longer valid. This reloads the successor database URL without publishing
a new Release. After the deploy result succeeds, verify the runtime boundary:

```bash
secure-reviewed-rollover-json | \
  sudo /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/xiaoman-shared-db-password-rollover-production.py \
    --stdin --command verify-reload \
    --approval approved-production-xiaoman-shared-db-password-rollover-v1
```

The result must reach `phase=reload_verified`. It proves the fixed system services run
the successor URL and exact Release, and that no readable process environment retains
the previous URL. The successful dual-target deploy result separately proves the Erhua
gateway reload and health.

If the live deploy fails before promotion, keep the rollover state and resolve the
reported deploy stage before choosing `status` or `rollback`. If it fails after
promotion and the deploy runner automatically points `current` to `previous`, the
rollover entrypoint will correctly reject commands because the approved Release is no
longer current. Do not bypass that boundary or invoke the script from `/private/tmp`.
First use the reviewed deploy workflow to re-promote the exact approved SHA with the
identical immutable scope, profile, and restart targets. If the failing smoke would
otherwise roll it back again, the owner may set `rollback_on_smoke_failure=false` only
for this bounded recovery re-promotion. Once `current` again resolves to the approved
SHA, run `status` with the same rollover request and continue forward or use the
rollback path below.

## 5. Apply The Private Policy

Continue with the same approved request. The state machine derives the one private
policy from the already bound direct chat and does not accept another target:

```bash
secure-reviewed-rollover-json | \
  sudo /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/xiaoman-shared-db-password-rollover-production.py \
    --stdin --command apply-private-policy \
    --approval approved-production-xiaoman-shared-db-password-rollover-v1

secure-reviewed-rollover-json | \
  sudo /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/xiaoman-shared-db-password-rollover-production.py \
    --stdin --command forward-verify \
    --approval approved-production-xiaoman-shared-db-password-rollover-v1
```

The terminal result must be `password_rollover_forward_completed`, with a durable
sanitized receipt written before the secret recovery state is deleted. Retain its
`active_database_url_sha256` for the later group configuration. An exact retry returns
that receipt without rotating again. To recover after interruption, run
`--command status` with the same request: phases through `credential_rotated` resume
with `prepare`; `direct_config_applied` resumes with the dual reload and
`verify-reload`; `reload_verified` resumes with `apply-private-policy`; and
`private_policy_applied` resumes with `forward-verify`. Never execute the rejected
`/private/tmp/xiaoman-db-password-rollover-guardian.py` workaround.

## 6. Activate And Accept Direct

Start the installed direct preflight. It performs no network call and no database write:

```bash
sudo systemctl start qintopia-agentos-xiaoman-feishu-poster-preflight.service
```

After explicit owner approval, activate the direct path through the reviewed script:

```bash
sudo env \
  QINTOPIA_XIAOMAN_FEISHU_POSTER_PRODUCTION_ACTIVATION=approved-production-xiaoman-feishu-poster-return \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/activate-xiaoman-feishu-poster-production.sh
```

The owner then sends one new private poster request. Retain sanitized evidence for the
acknowledgement latency, one ingress receipt, one workflow, one generated image, one
delivered notification, one review decision, and zero publication facts. Do not reuse
the earlier failed source message.

Do not proceed to group configuration until direct acceptance passes.

## 7. Configure One Internal Group

The group configuration must state the full direct plus canary-group ceiling. It must
not add an external, activity, or member group:

```json
{
  "schema_version": 1,
  "desired_state": "group",
  "release_sha": "<same-released-commit-sha>",
  "database_url_sha256": "<terminal-active_database_url_sha256>",
  "bot_open_id": "<xiaoman-bot-open-id>",
  "allowed_chat_ids": ["<private-chat-id>", "<internal-group-chat-id>"],
  "allowed_user_ids": ["<requester-user-id>", "<reviewer-user-id>"],
  "reviewer_user_ids": ["<requester-user-id>", "<reviewer-user-id>"]
}
```

Preview and apply through the protected configuration transaction:

```bash
secure-reviewed-group-config-json | \
  sudo /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/apply-xiaoman-feishu-poster-production-config.py \
    --stdin

secure-reviewed-group-config-json | \
  sudo /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/apply-xiaoman-feishu-poster-production-config.py \
    --stdin --apply \
    --approval approved-production-xiaoman-feishu-config-v1
```

This changes persistent configuration only; it does not reload Xiaoman or start the
group delivery timer.

Apply one `internal_collaboration` policy with `conversation_type=group`,
`return_mode=thread_reply`, `initiation_rule=explicit_bot_mention`,
`status_visibility=conversation_members`, and the reviewed reviewer ids through the
policy wrapper:

```bash
secure-reviewed-internal-group-policy-json | \
  sudo /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/apply-xiaoman-conversation-policies-production.py \
    --stdin --apply \
    --approval approved-production-xiaoman-conversation-policy-v3
```

Run the disabled-delivery observation before activation:

```bash
sudo env \
  QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_PRODUCTION_OBSERVATION_ENABLE=1 \
  QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_EXPECTED_STATE=enabled \
  QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_DELIVERY_EXPECTED_STATE=stopped \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/xiaoman-feishu-internal-group-production-observation-smoke.sh
```

## 8. Activate And Accept One Group

After the separate owner approval, activate only the group timer:

```bash
sudo env \
  QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_PRODUCTION_ACTIVATION=approved-production-xiaoman-feishu-internal-group \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/activate-xiaoman-feishu-internal-group-production.sh
```

One human member sends a new explicit `@Xiaoman` poster request in the reviewed group.
Retain sanitized proof that the acknowledgement and image card return in the original
thread, an authorized reviewer decision persists, direct delivery remains active, and
every external group remains at zero messages.

## Rollback

For group-only rollback, apply the same Release with `desired_state=direct`, then run
the reviewed internal-group rollback. This keeps direct intake and delivery active.

Before the rollover reaches a terminal receipt, use the same approved request with
`--command rollback`. If PostgreSQL still authenticates only the previous credential and
configuration is disabled, the operation terminates as `password_rollover_aborted`. If
the password already rotated, rollback keeps the valid successor credential, applies the
disabled poster configuration, and returns `phase=rollback_config_applied`. Run the same
dual-target same-SHA reload, then finish with `--command rollback-verify`.

After a terminal forward receipt, use the ordinary protected `desired_state=disabled`
configuration and full poster rollback; do not start another password operation merely
to disable poster intake. Preserve all policies, receipts, workflows, notifications,
attempts, reviews, and ambiguous outcomes. Never restore the invalid previous URL,
manually edit an environment file, or reroute a group result into a direct chat or the
group main timeline.
