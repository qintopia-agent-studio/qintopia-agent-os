# Rollback

Rollback restores production to the previous approved release without editing server
source files.

The standard model is:

1. keep immutable release directories under `/home/ubuntu/qintopia-agent-os-releases/`;
2. keep `current` and `previous` symlinks;
3. promote or roll back by repointing symlinks through an approved runbook;
4. restart only affected services;
5. run smoke checks and record evidence.

## Boundaries

- Do not edit files directly under `.hermes`.
- Do not fetch or build source on the production server for routine rollback.
- Do not delete rollback material without an owner-approved retention plan.

## GitHub Workflow

Use the `Rollback Production` GitHub Actions workflow for owner-approved rollback to a
published Release version. The workflow provides a `release_tag` choice input so an
operator can select a known version instead of typing a SHA manually. GitHub Actions
does not support dynamically populated `workflow_dispatch` choices, so the selectable
version list is updated in `.github/workflows/rollback-production.yml` as part of
release operations.

Current verified evidence indicates different rollback paths after `v0.2.3`:

- Server-local automatic rollback path:
  - The production host `previous` symlink currently resolves to the server-local
    fallback target `d083e5ccfce2d07048e07c0ceb8c052671f65911`.
  - This fallback is used automatically only after promotion has occurred, the promoted
    release fails smoke, and the deploy request sets `rollback_on_smoke_failure=true`.
  - It uses on-host release directories and is separate from the owner-triggered GitHub
    Actions rollback path.
- GitHub Actions rollback path:
  - Uses the signed deploy-request queue (`current.json` + request object in COS).
  - It must target a published, non-prerelease Release tag and requires paired COS
    assets (`sidecar-runtime` and `deploy-bundle`) before request assembly.
  - This path is now limited to `v0.2.0` as the only currently verified candidate.

Verified release/evidence points relevant to `v0.2.3`:

- `v0.2.0`: published release + paired COS assets verified.
- `v0.2.1`: deploy run evidence shows failure; pairing evidence is insufficient.
- `v0.2.2`: historical output exists in server context (`previous` currently resolves to
  `d083e5ccfce2d07048e07c0ceb8c052671f65911`), but it is only a server-local fallback
  target and is not a verified GitHub rollback candidate under current policy.

For any future rollback candidate, operator guidance is strict:

- the candidate must be a published, non-prerelease GitHub Release; and
- COS must contain both `sidecar-runtime` and `deploy-bundle` assets for that SHA.

The workflow does not SSH to production and does not edit server files directly. It
resolves the selected published Release tag to its commit SHA, verifies both the sidecar
and deploy-bundle artifacts for that SHA from COS, creates the existing HMAC-signed
deploy request, uploads that request to COS, and lets the server deploy runner promote
the selected release through the same release/current path used by normal deploys. A
historical Release whose artifacts have already been pruned fails in GitHub Actions
before a rollback request is submitted. The workflow uses the `production` environment
gate, defaults to `dry_run: true`, and records the reviewed `runtime_artifact_profile`
explicitly in the rollback request instead of relying on the ordinary deploy default.
Under the current rollback audit, that profile is fixed to `huabaosi-production`; this
workflow must not switch to `qiwe-production` or accept a mixed production artifact.

## Validation

```bash
pnpm deploy:rollback:check
pnpm check:light
```
