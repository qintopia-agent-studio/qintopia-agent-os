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

Deployment rendering owns active release identity. Huabaosi image-generation and
Feishu-mirror preflight and worker units must set both release-bound variables to the
target release SHA:

```text
QINTOPIA_DEPLOYED_COMMIT_SHA=<target release sha>
QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA=<same target release sha>
```

The read-only production observation must independently derive the SHA from the verified
immutable `release/current` target and pass those two values to its clean child
environment. Stale copies in `/etc/qintopia/message-sidecar.env` must not override the
verified release identity.

The fix must not edit the persistent production environment, enable a timer, execute a
provider, write Postgres or Feishu, call QiWe, publish, or send.
