# v0.2.15 Release Decision Evidence

Date: 2026-07-18 Asia/Shanghai

## Summary

Release Please PR `#180`, `chore(master): release 0.2.15`, is ready for an explicit
owner release decision, but it is not a Xiaoman production-complete release.

The release candidate packages the release-current acceptance docs, the staging values
observation gate, the staging evidence CI coverage, Feishu-backed generated-image
approval/revalidation, the guarded Feishu-to-QiWe staging bridge, and the tightened
combined staging evidence gate plus stale staging artifact provisioning guardrails
needed before staging runtime provisioning. It still does not provision server-local
staging values, render `/etc/qintopia/message-sidecar-staging.env`, run Huabaosi/QiWe
staging evidence, enable production external timers, or prove one real Xiaoman activity
through QiWe group-send arrival.

## Release Candidate

```text
release_pr=#180
release_pr_title=chore(master): release 0.2.15
release_pr_head_authority=gh pr view 180 at release decision time
last_observed_release_pr_head=278be363e194249b3ccfe02dd9878596e3b3fed1
release_pr_state_requirement=open
release_pr_mergeable_requirement=MERGEABLE
latest_published_release=v0.2.14
```

Do not treat `last_observed_release_pr_head` as the merge-time head. Release Please
refreshes `#180` after any merged follow-up PR, including evidence-only follow-ups. The
owner must check the live PR head and PR-attached status immediately before a release
decision.

Included changes:

- `027ee63` `fix: package release acceptance docs`
- `caeddc3` `fix: add staging values observation gate (#181)`
- `a53e3b2` `ci: run staging evidence contract tests (#182)`
- `1706c56` `docs: record feishu qiwe delivery boundary (#183)`
- `51ab4b1` `docs: record v0215 release decision evidence (#184)`
- `7b0adc8` `fix: compile staging feishu primary storage (#185)`
- `e415c2d` `fix: add feishu primary storage revalidation (#186)`
- `3c07c31` `fix: harden user-facing failure handling`
- `ab2ed2a` `feat: approve revalidated feishu images`
- `1d02be4` `fix: tighten xiaoman staging evidence gate`
- `3493318` `feat: bridge feishu images to qiwe staging`
- `10b8820` `fix: prevent stale staging artifact provisioning`
- `bb5b920` `docs: refresh v0215 release evidence`

## Validation Evidence

Manual Release Please validation must pass on the exact current Release Please head. The
last observed successful validation was:

```text
workflow_run=29645591436
head_sha=278be363e194249b3ccfe02dd9878596e3b3fed1
changes=success
check=success
pr_attached_status=Release Please validation SUCCESS
```

If `gh pr view 180 --json headRefOid,statusCheckRollup` reports any head other than
`278be363e194249b3ccfe02dd9878596e3b3fed1`, rerun the manual validation from
`docs/operations/release-acceptance-checklist.md` and require the PR-attached
`Release Please validation` status to be `SUCCESS` on that replacement head.

The Release Please PR had no submitted reviews, no ordinary comments other than
generated release content, no review threads, and remained mergeable at the time of this
record.

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

The observed staging release root is historical. It proves a staging root can exist, but
it predates the merged Feishu-to-QiWe staging bridge and tightened combined evidence
gate. The real Huabaosi/QiWe staging exercise must use a newly reviewed staging-only
artifact built from the target release SHA and record that artifact's sidecar SHA-256
before provisioning `/etc/qintopia/message-sidecar-staging.env`.

This follow-up changes release-decision evidence and staging artifact guidance. If it
merges before `#180`, Release Please must update `#180`, and the manual validation above
must be rerun on the new Release Please head before any release decision.

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
   staging generation, QiWe staging upload/callback/send evidence, and the cross-flow
   hash checker.

## Production Boundary

This record is evidence only. It did not merge a Release Please PR, publish a Release,
deploy to production, create or edit server-local staging values, render staging env,
enable timers, run apply workers, write Postgres or Feishu, call Huabaosi or QiWe,
process callbacks, or send externally.
