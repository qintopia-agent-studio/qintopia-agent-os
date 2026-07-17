# Xiaoman Production Completion Audit

Date: 2026-07-17 Asia/Shanghai

## Summary

Xiaoman is not production-complete. The internal AgentOS activity path is running in
production, but the external business loop required by
`docs/plans/active/xiaoman-production-completion-gate.md` remains incomplete.

The current production state is best classified as infrastructure / partial activation,
not a usable end-to-end Xiaoman activity-to-QiWe workflow.

## Release State

GitHub Release `v0.2.13` is still the latest published Release.

Release Please PR `#174`, `chore(master): release 0.2.14`, is open and has the required
manual `Release Please validation=SUCCESS` status. It has not been merged, and `v0.2.14`
has not been published.

The merged deploy-bundle fix from `#173` is therefore present on `origin/master`, but
not yet present in the latest published production Release.

## Server State

A read-only check on `paxon-server` observed:

```text
current=/home/ubuntu/qintopia-agent-os-releases/15576004af3ebd50412a0030f7f9f77580d9bf13
renderer=missing_or_not_executable
staging_values_file=missing
staging_env_file=missing
staging_artifact=8a04ab44cad0b60cbef499d7a58e0fb8fcac577be537d1418ec3649f38c4fa1f
```

The fixed staging sidecar artifact exists and still matches the reviewed SHA-256. The
current production release does not yet contain
`deploy/sidecar/scripts/render-staging-runtime-env.py`, because the bundle fix has not
been released and deployed.

## Staging Readiness

The unified staging runtime readiness evidence returned:

```json
{
  "action_status": "not_ready",
  "limitations": [
    "prerequisite_env_file_path_missing",
    "huabaosi_readiness_env_file_path_missing",
    "qiwe_readiness_env_file_path_missing"
  ],
  "packaged_sidecar_sha256": "8a04ab44cad0b60cbef499d7a58e0fb8fcac577be537d1418ec3649f38c4fa1f",
  "release_sha": "37fff8bf819f0df68825961203e7998b51a07c31",
  "success": false
}
```

This means Huabaosi/QiWe staging evidence is still blocked before any real provider,
Feishu, QiWe, Postgres write, or callback exercise can run.

## Production Timer State

The active production timers show that Xiaoman's internal AgentOS path is running:

```text
qintopia-agentos-xiaoman-activity-signal-worker.timer enabled=enabled active=active
qintopia-agentos-xiaoman-activity-promotion-starter-worker.timer enabled=enabled active=active
qintopia-agentos-operations-evidence-worker.timer enabled=enabled active=active
qintopia-agentos-operations-visual-worker.timer enabled=enabled active=active
qintopia-agentos-xiaoman-activity-image-generation-starter-worker.timer enabled=enabled active=active
qintopia-agentos-xiaoman-activity-send-request-starter-worker.timer enabled=enabled active=active
qintopia-agentos-operations-group-send-ready.timer enabled=enabled active=active
```

The external completion timers are not active:

```text
qintopia-agentos-huabaosi-image-generation-worker.timer enabled=disabled active=inactive
qintopia-agentos-huabaosi-feishu-artifact-mirror-worker.timer enabled=disabled active=inactive
qintopia-agentos-qiwe-image-send-worker.timer enabled=not-found active=inactive
```

## Production Preflight

The Xiaoman production preflight passed the internal observations:

- activity signal timer observation;
- activity promotion starter timer observation;
- operations evidence/visual timer observation;
- downstream evidence/visual preview;
- activity image-generation starter timer observation.

It then failed at the external image-generation boundary:

```text
Huabaosi provider timer must be active
```

This is consistent with the completion gate: Xiaoman is not complete until real Huabaosi
generation, Feishu review/mirror evidence, QiWe delivery, and one real activity are
proven.

## Completion Gate Audit

| Gate                                                        | Current evidence                               | Status               |
| ----------------------------------------------------------- | ---------------------------------------------- | -------------------- |
| Release Please validation on exact release PR head          | `#174` has `Release Please validation=SUCCESS` | ready but not merged |
| Fixed immutable staging sidecar                             | Artifact exists and SHA-256 matches            | passed               |
| Huabaosi staging final JPEG evidence                        | Staging env missing; readiness `not_ready`     | missing              |
| QiWe staging upload/callback/send evidence                  | Staging env missing; readiness `not_ready`     | missing              |
| Cross-flow JPEG hash match                                  | No retained Huabaosi/QiWe staging evidence yet | missing              |
| QiWe production enablement PR                               | Production QiWe image-send timer not found     | missing              |
| Huabaosi production generation and Feishu mirror activation | Both timers disabled; preflight fails          | missing              |
| One real Xiaoman activity through QiWe group-send arrival   | No production-complete retained evidence       | missing              |

## Required Next Steps

1. Make an owner release decision for `#174`: merge the Release Please PR and publish
   `v0.2.14`, or explicitly defer it.
2. After deployment, verify that `paxon-server` `current` contains the staging env
   renderer.
3. Provision the fixed server-local staging values and render
   `/etc/qintopia/message-sidecar-staging.env` through the reviewed renderer.
4. Rerun unified staging readiness until it returns
   `ready_for_huabaosi_qiwe_staging_smokes`.
5. Run Huabaosi staging generation, QiWe staging preflight/upload/callback, and the
   cross-flow evidence checker.
6. Only after retained staging evidence passes, add and review the separate QiWe
   production enablement PR.
7. Activate Huabaosi production generation and Feishu mirror through their guarded
   release-local scripts, then retain first-record evidence.
8. Process one real Xiaoman activity through image generation, human approval,
   send-ready, QiWe group-send arrival, and sanitized production evidence retention.

## Production Boundary

This audit was read-only. It did not merge a Release Please PR, publish a Release,
deploy to production, create or edit staging env files, enable timers, run apply
workers, write Postgres or Feishu, call a provider, process a QiWe callback, or send
externally.
