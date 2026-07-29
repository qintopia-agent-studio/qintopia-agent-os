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

Current verified evidence indicates different rollback paths after the dual-runtime
cutover:

- Server-local automatic rollback path:
  - The production host `previous` symlink currently resolves to the server-local
    fallback target `d083e5ccfce2d07048e07c0ceb8c052671f65911`.
  - This fallback is used automatically only after promotion has occurred, the promoted
    release fails smoke, and the deploy request sets `rollback_on_smoke_failure=true`.
  - It uses on-host release directories and is separate from the owner-triggered GitHub
    Actions rollback path.
- GitHub Actions rollback path:
  - Uses the signed deploy-request queue (`current.json` + request object in COS).
  - It must target a published, non-prerelease Release tag whose deploy bundle already
    implements the dual-runtime release model.
  - It requires the Huabaosi primary artifact, QiWe companion artifact, and deploy
    bundle for the exact target SHA before request assembly.
  - No owner-triggered dual-runtime rollback target is currently verified. The workflow
    keeps `v0.2.0` visible only as historical audit context and fails before artifact
    access, secret-bearing upload steps, or deploy-request creation.

Verified release/evidence points relevant to `v0.2.3`:

- `v0.2.0`: published release + historical paired COS assets verified, but its deploy
  bundle predates the dual-runtime model and is not a valid current rollback target.
- `v0.2.1`: deploy run evidence shows failure; pairing evidence is insufficient.
- `v0.2.2`: historical output exists in server context (`previous` currently resolves to
  `d083e5ccfce2d07048e07c0ceb8c052671f65911`), but it is only a server-local fallback
  target and is not a verified GitHub rollback candidate under current policy.

For any future rollback candidate, operator guidance is strict:

- the candidate must be a published, non-prerelease GitHub Release; and
- COS must contain the Huabaosi primary sidecar, QiWe companion sidecar, and deploy
  bundle assets for that SHA; and
- the target deploy bundle must implement and validate the same dual-runtime layout.

The workflow does not SSH to production and does not edit server files directly. While
no valid target exists, it fails closed in `Resolve rollback target` and cannot reach
COS validation or request upload. Re-enabling it requires one reviewed change that
updates the allowlisted tag and SHA, removes the explicit unavailable-target guard, and
proves all three target artifacts. The resulting request must keep
`runtime_artifact_profile=huabaosi-production`; QiWe remains a companion and is never a
global profile choice.

## Validation

```bash
pnpm deploy:rollback:check
pnpm check:light
```
