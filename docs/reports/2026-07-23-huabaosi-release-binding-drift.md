# Huabaosi Production Release Binding Drift

Date: 2026-07-23

## Observation

After `v0.2.25` deployed successfully, the release-local Huabaosi image production
observation reached the immutable sidecar preflight and failed closed with
`config_valid=false`. No provider, Postgres, Feishu, QiWe, timer, or write action ran.

The active `release/current` target and sidecar artifact were bound to commit
`52d2a23447a70c2c5754d56614e319a31eb86417`, while the persistent production environment
still held both release-bound variables for an older release. The ordinary systemd
renderer refreshed only `QINTOPIA_DEPLOYED_COMMIT_SHA`, so Huabaosi's second release
binding could remain stale after a valid promotion.

## Contract

Deployment rendering owns active release identity. Huabaosi image-generation preflight
and worker units must set all three release-bound variables to the target release SHA:

```text
QINTOPIA_DEPLOYED_COMMIT_SHA=<target release sha>
QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_RELEASE_SHA=<same target release sha>
QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA=<same target release sha>
```

Feishu-mirror-only units set the deployed and Feishu release variables and do not need
the image adapter release variable.

The read-only production observation must independently derive the SHA from the verified
immutable `release/current` target and pass all three release-bound values to its clean
child environment. It must also pass the persistent production image approval, database
hash, HTTP timeout, and media byte bound needed to validate the same configuration as
the installed systemd preflight. Stale release values in
`/etc/qintopia/message-sidecar.env` must not override the verified release identity.

## v0.2.26 Follow-up

Release `v0.2.26` deployed the first binding fix successfully. The installed systemd
preflight then reported `config_valid=true`, proving the unit-level release binding was
valid. The release-local read-only observation still reported `config_valid=false`
because its clean child environment omitted the production image approval, image
database hash, timeout, media bound, and image-specific release binding. No provider,
Postgres, Feishu, QiWe, timer mutation, or write action ran during this failed
observation.

The fix must not edit the persistent production environment, enable a timer, execute a
provider, write Postgres or Feishu, call QiWe, publish, or send.
