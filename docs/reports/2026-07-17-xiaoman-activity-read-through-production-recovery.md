# Xiaoman Activity Read-Through Production Recovery

Date: 2026-07-17 Server: `paxon-server` Scope: Xiaoman
`qintopia_xiaoman_activity_list_by_date` read-through for Feishu Base activity records.

## Summary

GitHub Release `v0.2.12` contained the Xiaoman activity read-through code from
`fix: xiaoman activity read-through records (#163)`, but `paxon-server` was still
serving the previous production release (`v0.2.11`). Xiaoman therefore continued to
return a worker-command preview instead of actual Feishu activity records.

After deploying the `v0.2.12` deploy bundle and Hermes plugin package, enabling the
Xiaoman read-through runtime flags, and tightening the release-local worker path
permissions, the same controlled tool returned real sanitized records.

## User-Visible Symptom

When asked "今天有什么活动？", Xiaoman answered that it had not obtained a reliable
activity list and suggested asking Liu Shan manually. That response was accurate for the
runtime state, but it meant the user-visible feature had not actually landed.

The tool had not read Feishu Base records in that session. It had returned an
`agentos_worker_command` preview requiring local execution.

## Timeline

- `v0.2.12` was published on GitHub at commit
  `53b893c7bf5ff7411f1ac314329a882169312442`.
- The published Release included #163, which added read-through output for
  `record_count`, `records`, and `summaries`.
- The automatic `Deploy Production` workflow for `v0.2.12` failed.
- `paxon-server` remained on release `a7c9d9cd06cabbf73c5826de816194fe41c691dc`
  (`v0.2.11`).
- Xiaoman's production `.env` was missing the read-through and Feishu Base runtime
  flags.
- The profile-local Xiaoman `qintopia-tools` plugin was not yet using the `v0.2.12`
  release variant.
- After a deploy-bundle / Hermes-plugin-only deployment, `paxon-server` switched
  `current` to `53b893c7bf5ff7411f1ac314329a882169312442`, while keeping the approved
  production sidecar runtime from `v0.2.11`.
- The first runtime check then failed with
  `xiaoman activity worker binary is not approved for read-through`.
- The release-local worker path was tightened to a non-writable root-owned boundary.
- Read-through validation then succeeded.

## Root Causes

### Release Was Published But Not Deployed

Publishing a GitHub Release created the version record, but production still depends on
the deploy runner consuming a signed deploy request from COS and switching
`/home/ubuntu/qintopia-agent-os-releases/current`.

The Release existed, but `paxon-server` was still on `v0.2.11`.

### Production Deploy Failed On The Deployed Runner's Old Sidecar Feature Allowlist

The `v0.2.12` production deploy failed because the server-side deploy runner was still
from `v0.2.11` when it tried to fetch the `v0.2.12` sidecar artifact. That older runner
allowed only:

```json
["huabaosi-production-adapter"]
```

The `v0.2.12` sidecar artifact was built with the newer approved production feature set:

```json
["huabaosi-production-adapter", "huabaosi-feishu-mirror-adapter"]
```

The old runner therefore rejected the new artifact with:

```text
artifact manifest Cargo features are not approved for production
```

That failure prevented the normal deploy from reaching the deploy-bundle and Hermes
plugin changes. The artifact itself was not missing the mirror feature; the deployed
fetch/validation script was stale.

### Runtime Flags Were Missing

Xiaoman read-through requires the production profile environment to opt in:

```text
QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE=1
QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE=1
```

Without those flags, the wrapper correctly returns a bounded worker command instead of
executing the read-through path.

### Worker Path Was Not Approved For Read-Through

Read-through intentionally rejects mutable or untrusted worker paths. The worker must be
an absolute release-local binary under:

```text
/home/ubuntu/qintopia-agent-os-releases/<40-hex-sha>/sidecar/qintopia-message-sidecar
```

It must not be reached through `current`, must not contain symlink path components, and
the protected path components must not be writable.

The first post-config runtime check failed because the current release-local path had
writable permissions and unexpected ownership on protected components.

## Resolution

The production recovery avoided hot-copying source files or building on the server.

1. Triggered a manual `Deploy Production` workflow for commit
   `53b893c7bf5ff7411f1ac314329a882169312442`.
2. Used scope `deploy-bundle,hermes-plugins`.
3. Kept the already-approved production runtime artifact:
   `a7c9d9cd06cabbf73c5826de816194fe41c691dc`.
4. Restarted only `hermes-xiaoman`.
5. Confirmed `paxon-server` `current` now points to
   `53b893c7bf5ff7411f1ac314329a882169312442`.
6. Confirmed Xiaoman's profile plugin is a symlink to:

   ```text
   /home/ubuntu/qintopia-agent-os-releases/53b893c7bf5ff7411f1ac314329a882169312442/skills/qintopia-tools/variants/xiaoman
   ```

7. Enabled the read-through runtime flags in Xiaoman's production `.env`.
8. Set `QINTOPIA_XIAOMAN_ACTIVITY_WORKER_BIN` to the canonical release-local worker
   path, not the `current` symlink.
9. Tightened protected path ownership and modes for the release root, release directory,
   sidecar directory, and worker binary.

## Verification

The controlled Xiaoman tool was executed on `paxon-server` for `2026-07-17` with
`table_role=activity_occurrence` and `timezone=Asia/Shanghai`.

Result:

```json
{
  "success": true,
  "read_through": true,
  "requires_local_execution": false,
  "record_count": 1
}
```

The returned payload included only wrapper-sanitized activity fields. The retained
evidence intentionally records the shape rather than production activity details or
participant names:

```json
{
  "summaries_count": 1,
  "records_count": 1,
  "fields_present": [
    "activity_title",
    "start_time",
    "gathering_point",
    "participant_count",
    "participant_names",
    "source_lead",
    "matching_status"
  ]
}
```

The same query for `table_role=activity_plan` returned:

```json
{
  "success": true,
  "read_through": true,
  "requires_local_execution": false,
  "record_count": 0
}
```

User-visible verification in WeCom also passed after the production recovery. When asked
"今天有什么活动？", Xiaoman answered with one concrete现场活动 record instead of a
worker-command preview or a manual fallback suggestion. The response included the
expected activity fields, but this report does not retain the activity content,
participant names, raw Feishu record values, or raw chat transcript.

## Why Code Could Be Correct While Production Still Failed

The code path was correct in `v0.2.12`, but the production feature depends on several
independent layers being true at the same time:

- the GitHub Release must exist;
- the deploy runner must successfully promote that release on `paxon-server`;
- Xiaoman's live Hermes profile must load the release plugin, not a stale local copy;
- the profile environment must enable read-through and Feishu Base mode;
- the worker binary must be the canonical release-local binary;
- release-local path permissions must satisfy the read-through trust check;
- Feishu Base credentials and table allowlists must already exist in the server-local
  profile environment.

The failure was therefore not a single code bug. It was an activation and deployment
closure problem: the code had shipped to GitHub, but production had not fully converged
to the required runtime state.

## Follow-Up

- When a Release changes production sidecar feature policy and deploy-runner validation
  in the same commit, deploy the `deploy-bundle` / runner validation first or run a
  dry-run that proves the current server runner can fetch the new sidecar artifact.
- Add a release acceptance check that verifies `paxon-server` `current` equals the
  target Release SHA before declaring a feature live.
- Add a Xiaoman read-through acceptance check after each relevant deployment:
  `read_through=true`, `requires_local_execution=false`, and sanitized
  `record_count/records/summaries` are returned.
- Keep the worker path canonical. Do not configure read-through with
  `/home/ubuntu/qintopia-agent-os-releases/current/...`.
- Treat a command-preview response as not executed. Xiaoman must not claim it has
  checked Feishu records unless read-through actually returned records or an explicit
  zero count.

## Sidecar Artifact Follow-Up Verification

After the deploy-bundle / Hermes-plugin recovery, the server-side runner was already on
the `v0.2.12` validation code. A production `Deploy Production` dry-run was then
submitted with:

```text
release_scope=sidecar-runtime,deploy-bundle,hermes-plugins
dry_run=true
runtime_sha=53b893c7bf5ff7411f1ac314329a882169312442
deploy_bundle_sha=53b893c7bf5ff7411f1ac314329a882169312442
```

Result: passed.

The `paxon-server` deploy runner downloaded the `v0.2.12` sidecar artifact from COS,
validated SHA256SUMS, accepted the production Cargo feature set, downloaded the deploy
bundle, and assembled the dry-run staging release without switching `current`.

This proves the feature allowlist is no longer blocking a full `sidecar-runtime` deploy
from the current server baseline.
