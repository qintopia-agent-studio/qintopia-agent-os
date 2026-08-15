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
published Release version. The operator supplies a bounded semantic `release_tag`
(`vX.Y.Z`) instead of a raw SHA. The workflow resolves and verifies the tag at run time;
no release tag or commit SHA is hardcoded into the workflow.

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
  - The target commit must be reachable from `origin/master`.
  - It requires the Huabaosi primary artifact, QiWe companion artifact, and deploy
    bundle for the exact target SHA before request assembly.

For any future rollback candidate, operator guidance is strict:

- the candidate must be a published, non-prerelease GitHub Release; and
- COS must contain the Huabaosi primary sidecar, QiWe companion sidecar, and deploy
  bundle assets for that SHA; and
- the target deploy bundle must implement and validate the same dual-runtime layout.

The workflow does not SSH to production and does not edit server files directly. An
invalid, unpublished, prerelease, off-`master`, or incomplete target fails before
deploy-request creation. The resulting request must keep
`runtime_artifact_profile=huabaosi-production`; QiWe remains a companion and is never a
global profile choice.

## Validation

```bash
pnpm deploy:rollback:check
pnpm check:light
```
