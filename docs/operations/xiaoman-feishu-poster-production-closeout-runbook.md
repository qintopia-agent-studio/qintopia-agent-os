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

Keep these values outside git, shell history, reports, logs, and chat:

- the successor production database URL and its SHA-256;
- the direct Feishu chat and requester ids used by the private policy;
- the Xiaoman Bot open id for the group canary;
- the exact direct plus canary-group chat ceiling;
- the exact requester plus reviewer user ceiling; and
- one private and one internal-collaboration policy document.

The database owner should create a successor credential while the previous credential
remains valid. Revoke the previous credential only after the same-SHA service reload and
database health proof succeed.

## 1. Publish Once

Merge the single closeout PR through the normal owner review and Release Please flow,
then manually publish its draft Release. Record the exact 40-character Release SHA. Wait
for the Release-triggered production deploy result to succeed before proceeding.

Do not publish an intermediate configuration-only or group-only Release.

## 2. Prepare Direct Configuration

Stream this schema from an owner-controlled secret source. Do not create a
world-readable JSON file or paste real values into a terminal command:

```json
{
  "schema_version": 1,
  "desired_state": "direct",
  "release_sha": "<released-commit-sha>",
  "database_url_sha256": "<approved-successor-database-url-sha256>",
  "database_url": "<successor-production-database-url>",
  "rotate_ingress_hmac": true
}
```

`direct` reuses the existing direct delivery chat/user ceiling. The entrypoint creates a
new dedicated ingress HMAC in memory, keeps it distinct from the callback key, writes it
to both fixed environments, removes any stale group-only Bot identity, keeps the group
switch at `0`, updates the Release binding, and updates every present production
database hash binding.

Run the no-write preview first:

```bash
secure-reviewed-direct-config-json | \
  sudo /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/apply-xiaoman-feishu-poster-production-config.py \
    --stdin
```

The preview must report `production_config_ready`, `desired_state=direct`,
`database_url_sha256_matched=true`, and zero external calls, database writes, and
service changes. It must not print the URL, HMAC, callback key, or raw ids.

Apply the same reviewed payload once:

```bash
secure-reviewed-direct-config-json | \
  sudo /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/apply-xiaoman-feishu-poster-production-config.py \
    --stdin --apply \
    --approval approved-production-xiaoman-feishu-config-v1
```

An exact retry is idempotent. A partial two-file replacement restores the first file; an
abrupt process stop leaves activation fail-closed until the same payload is retried.

## 3. Reload The Same SHA

Use the existing `Deploy Production` workflow with the published SHA for `commit_sha`,
`runtime_sha`, `deploy_bundle_sha`, and `release_sha`. Keep
`runtime_artifact_profile=huabaosi-production`, use the reviewed full system-service
restart target, and keep rollback-on-smoke-failure enabled.

Run a dry-run request first. After owner approval, run the identical same-SHA request
with `dry_run=false`. This reloads the successor database URL without publishing a new
Release. Verify the deploy result, release/current identity, and fixed service health,
then have the database owner revoke the previous credential.

## 4. Apply The Private Policy

The private policy must use the real direct chat from the deployment ceiling and no
configured reviewers:

```json
{
  "schema_version": 3,
  "policies": [
    {
      "platform": "feishu",
      "chat_id": "<private-chat-id>",
      "conversation_type": "direct",
      "audience_class": "private",
      "allowed_capabilities": ["poster_production_request", "poster_workflow_status"],
      "return_mode": "direct_chat",
      "initiation_rule": "direct_message",
      "status_visibility": "requester",
      "enabled": true,
      "reviewer_user_ids": []
    }
  ]
}
```

Apply it through the release-local minimal-environment wrapper:

```bash
secure-reviewed-private-policy-json | \
  sudo /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/apply-xiaoman-conversation-policies-production.py \
    --stdin --apply \
    --approval approved-production-xiaoman-conversation-policy-v3
```

The result may contain only counts, versions, and opaque hashes. It must not contain raw
chat/user ids or the database URL.

## 5. Activate And Accept Direct

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

## 6. Configure One Internal Group

The group configuration must state the full direct plus canary-group ceiling. It must
not add an external, activity, or member group:

```json
{
  "schema_version": 1,
  "desired_state": "group",
  "release_sha": "<same-released-commit-sha>",
  "database_url_sha256": "<same-approved-database-url-sha256>",
  "bot_open_id": "<xiaoman-bot-open-id>",
  "allowed_chat_ids": ["<private-chat-id>", "<internal-group-chat-id>"],
  "allowed_user_ids": ["<requester-user-id>", "<reviewer-user-id>"],
  "reviewer_user_ids": ["<requester-user-id>", "<reviewer-user-id>"]
}
```

Preview and apply it with the same configuration entrypoint and approval used in step 2.
This changes persistent configuration only; it does not reload Xiaoman or start the
group delivery timer.

Apply one `internal_collaboration` policy with `conversation_type=group`,
`return_mode=thread_reply`, `initiation_rule=explicit_bot_mention`,
`status_visibility=conversation_members`, and the reviewed reviewer ids through the same
policy wrapper from step 4.

Run the disabled-delivery observation before activation:

```bash
sudo env \
  QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_PRODUCTION_OBSERVATION_ENABLE=1 \
  QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_EXPECTED_STATE=enabled \
  QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_DELIVERY_EXPECTED_STATE=stopped \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/xiaoman-feishu-internal-group-production-observation-smoke.sh
```

## 7. Activate And Accept One Group

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

For full poster rollback, apply `desired_state=disabled`, then run the reviewed full
poster rollback. Preserve all policies, receipts, workflows, notifications, attempts,
reviews, and ambiguous outcomes. Never reroute a group result into a direct chat or the
group main timeline.

If the successor database credential must be abandoned, restore the previous URL only
while that credential remains valid, run the same-SHA system-service reload, prove
health, and then revoke the failed successor credential. Do not edit either environment
file manually.
