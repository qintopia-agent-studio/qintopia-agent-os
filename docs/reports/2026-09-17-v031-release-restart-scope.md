# v0.3.1 release restart scope failure

## Confirmed evidence

[Deploy run 35175144351](https://github.com/qintopia-agent-studio/qintopia-agent-os/actions/runs/35175144351)
built artifacts successfully but failed in **Resolve release or manual deploy inputs**:
`restart_targets contains unsupported entry: hermes-core` (exit 2). Upload, request and
result-wait steps were skipped. This was not a lost receipt.

The September 17 read-only server inspection found current release
`16e8d56b98001579c6288ba13199b80d6d3dfc74`, with runtime
`83d694f2c3bc21fd78a73d25da3197379e2a14d5`. The v0.3.1 release directory
`7acfa70502df2ed9692bc1dea125f011eabd99ce` did not exist.

## Cause and correction

The impact rules include the independent Hermes core target, but the ordinary release
resolver emitted all impacted targets into a workflow that correctly excludes core
upgrades. Global target-enum checks did not test the scope boundary. The exact
historical per-target baseline that selected core was not reproduced and is not asserted
here.

Declare independent targets in the existing rules, partition ordinary release output,
and retain a visible independent-upgrade notice. Derive the workflow allowlist from the
same declaration. Reject ordinary requests containing core in schema and server checks.
Keep the dedicated core transaction and unknown-path rejection intact.

Validation covers release history containing core changes, ordinary and mixed impacts,
request scope rejection and existing core transaction fixtures. No production state,
credentials or release versions are changed by this repair.

## Rollout

After review and merge, prepare and publish a new release through the normal owner flow.
Do not reuse an expired signed request or treat rerunning the old tag's workflow as
installing this repair. Verify the new run reaches request/result handling and compare
the signed server result and installed release identities before claiming deployment.
See the [runner guide](../../deploy/runner/README.md#restart-target-resolution).
