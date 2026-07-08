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

The workflow does not SSH to production and does not edit server files directly. It
resolves the selected published Release tag to its commit SHA, verifies both the sidecar
and deploy-bundle artifacts for that SHA from COS, creates the existing HMAC-signed
deploy request, uploads that request to COS, and lets the server deploy runner promote
the selected release through the same release/current path used by normal deploys. A
historical Release whose artifacts have already been pruned fails in GitHub Actions
before a rollback request is submitted. The workflow uses the `production` environment
gate and defaults to `dry_run: true`.

## Validation

```bash
pnpm deploy:rollback:check
pnpm check:light
```
