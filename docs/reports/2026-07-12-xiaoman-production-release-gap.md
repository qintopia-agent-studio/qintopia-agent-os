# Xiaoman Production Release Gap Record

Date: 2026-07-12

## Scope

Release `v0.2.4` successfully promoted the immutable production release directory for
the Xiaoman internal workflow. The owner-approved aggregate preflight then revealed that
the release payload could not execute the checked-in preflight contract.

## Findings And Resolutions

| Finding                                                                                                                                         | Classification            | Resolution                                                                                                                                             | Prevention                                                                                      |
| ----------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------- |
| The release bundle omitted the Xiaoman aggregate preflight and its observation scripts.                                                         | Deploy bundle defect      | Package the fixed preflight script set in every deploy bundle.                                                                                         | Deploy runner checks and bundle-content validation require each script.                         |
| The release runner promoted `current` and restarted base services but did not install rendered systemd units or enable internal AgentOS timers. | Release promotion defect  | Render units from the immutable release, install a fixed allowlist, reload systemd, and enable only internal AgentOS timers before the existing smoke. | An isolated installer test verifies release-local paths, commit metadata, and timer enablement. |
| The deployed sidecar unit still exposed an old `QINTOPIA_DEPLOYED_COMMIT_SHA` even though `current` pointed to the new release.                 | Deployment metadata drift | Reinstall the rendered sidecar unit during release promotion so the metadata matches the immutable release SHA.                                        | The installer test asserts the rendered metadata and binary path.                               |

## Safety Boundary

- The installer accepts only a promoted release SHA and uses only that release's
  reviewed renderer and fixed unit allowlist.
- It enables only AgentOS internal workflow timers. It does not enable Feishu writeback,
  QiWe sends, real evidence retrieval, production visual generation, or external
  adapters.
- The failed preflight attempt did not write Postgres, Feishu, QiWe, or external
  systems; the script was absent before it could run a worker command.

## Required Follow-Up

1. Merge and release the deploy-runner fix.
2. After its first release promotion, submit one approved `workflow_dispatch` request
   for the same SHA so the newly promoted runner can install the unit allowlist.
3. Confirm the new release deploy result succeeds and the Xiaoman timers are active.
4. Run `xiaoman-activity-production-preflight-smoke.sh` from the release directory.
5. Record sanitized counts and the pass/hold decision in the Xiaoman preflight record.
