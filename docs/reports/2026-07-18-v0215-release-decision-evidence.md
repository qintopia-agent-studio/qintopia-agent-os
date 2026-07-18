# v0.2.15 Release Decision Evidence

Date: 2026-07-18 Asia/Shanghai

## Summary

Release Please PR `#180`, `chore(master): release 0.2.15`, is ready for an explicit
owner release decision, but it is not a Xiaoman production-complete release.

The release candidate packages the release-current acceptance docs, the staging values
observation gate, the staging evidence CI coverage, and the Feishu-to-QiWe delivery
boundary notes needed before staging runtime provisioning. It still does not provision
server-local staging values, render `/etc/qintopia/message-sidecar-staging.env`, run
Huabaosi/QiWe staging evidence, enable production external timers, or prove one real
Xiaoman activity through QiWe group-send arrival.

## Release Candidate

```text
release_pr=#180
release_pr_title=chore(master): release 0.2.15
release_pr_head=1648a463e0b4617230aa952b65301cdb479a1102
release_pr_state=open
release_pr_mergeable=MERGEABLE
latest_published_release=v0.2.14
```

Included changes:

- `027ee63` `fix: package release acceptance docs`
- `caeddc3` `fix: add staging values observation gate (#181)`
- `a53e3b2` `ci: run staging evidence contract tests (#182)`
- `1706c56` `docs: record feishu qiwe delivery boundary (#183)`

## Validation Evidence

Manual Release Please validation was run against the current Release Please head:

```text
workflow_run=29633149916
head_sha=1648a463e0b4617230aa952b65301cdb479a1102
changes=success
check=success
pr_attached_status=Release Please validation pass
```

The Release Please PR had no submitted reviews, no ordinary comments other than
generated release content, and remained mergeable at the time of this record.

## Deploy Bundle Evidence

A local deploy-bundle build completed successfully from the current checkout:

```text
node tools/deploy/build-deploy-bundle.mjs
```

The generated manifest contains the release-current files required for the next staging
runtime provisioning decision:

```text
payload/deploy/sidecar/scripts/staging-runtime-values-observation-smoke.sh
payload/docs/operations/message-sidecar-staging-values.template.json
payload/docs/operations/release-acceptance-checklist.md
payload/docs/operations/staging-runtime-provisioning-runbook.md
```

This proves the source/bundle packaging boundary, not a production deployment.

## Server State

A read-only server observation still showed production on `v0.2.14`-era release content:

```text
production_current=/home/ubuntu/qintopia-agent-os-releases/d41768f82c8f1c6f67ae6171621e3d4deb4e5755
staging_release_root=present
staging_release=37fff8bf819f0df68825961203e7998b51a07c31
/etc/qintopia/message-sidecar-staging-values.json missing
/etc/qintopia/message-sidecar-staging.env missing
qintopia-agentos-huabaosi-image-generation-worker.timer enabled=disabled active=inactive
qintopia-agentos-huabaosi-feishu-artifact-mirror-worker.timer enabled=disabled active=inactive
qintopia-agentos-qiwe-image-send-worker.timer enabled=not-found active=inactive
```

No server-local values, env contents, database URLs, table ids, tokens, group ids, raw
activity records, callback payloads, or provider outputs were read or recorded.

## Classification

Classify `v0.2.15` as `infrastructure`.

It may unblock reviewed release-current acceptance and staging provisioning steps, but
it must not be described as a completed Xiaoman production workflow.

The following completion gates remain missing:

- Huabaosi staging final JPEG evidence;
- QiWe staging upload/callback/send evidence;
- Huabaosi/QiWe cross-flow hash evidence;
- QiWe production enablement PR;
- Huabaosi production image activation and Feishu mirror activation evidence; and
- one real Xiaoman activity through image generation, human approval, send-ready, QiWe
  group-send arrival, and sanitized retained evidence.

## Owner Decision Required

Before merging `#180`, the owner must explicitly decide how the generated draft GitHub
Release will be handled after merge:

- publish the draft Release as the reviewed infrastructure release; or
- intentionally delete/defer the draft Release and avoid treating the Release Please
  baseline as published.

Do not merge the Release Please PR as a background maintenance action.

## Next Steps After Release Decision

1. If the owner chooses to publish, merge `#180`, publish the resulting draft Release,
   and deploy it through the reviewed release-current path.
2. Confirm the production `current` symlink resolves to the published release SHA.
3. Confirm release-current contains the staging values observation smoke, renderer,
   template, and runbook.
4. Owner creates `/etc/qintopia/message-sidecar-staging-values.json` from the reviewed
   template shape with real server-local values.
5. Run values observation, renderer validation, owner-approved renderer apply, and
   unified staging readiness.
6. Only after readiness reports `ready_for_huabaosi_qiwe_staging_smokes`, run Huabaosi
   staging generation and proceed toward the separate Feishu-to-QiWe delivery
   implementation and QiWe staging evidence.

## Production Boundary

This record is evidence only. It did not merge a Release Please PR, publish a Release,
deploy to production, create or edit server-local staging values, render staging env,
enable timers, run apply workers, write Postgres or Feishu, call Huabaosi or QiWe,
process callbacks, or send externally.
