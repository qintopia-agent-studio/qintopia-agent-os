# Huabaosi Canary Environment Owner

Date: 2026-07-20 Asia/Shanghai

## Observed Evidence

The `v0.2.17` production release and sidecar binary matched the approved release and
binary hashes. The production Huabaosi provider timer was disabled before the one-shot
canary attempt. The canary then failed closed before Postgres, provider, Feishu, QiWe,
publish, or send actions with:

```text
Huabaosi production canary release boundary is invalid
```

A read-only metadata check showed the release tree and sidecar binary were root-owned
with the expected modes, while `/etc/qintopia/message-sidecar.env` was owned by `ubuntu`
with mode `0640`.

## Root Cause

`deploy/sidecar/scripts/server-deploy.sh` rendered the production sidecar environment
file and then set it to `ubuntu:ubuntu 0640`. That kept existing user-run observations
working, but it conflicted with the Huabaosi one-shot production canary boundary, which
requires the fixed production environment file to be root-owned before it parses
provider, Feishu, release, and database bindings.

The file should remain readable by the `ubuntu` service/operator group but must not be
writable by the sidecar runtime user.

## Resolution

Render `/etc/qintopia/message-sidecar.env` as `root:ubuntu 0640`. This keeps read access
for the existing `ubuntu`-scoped systemd services and release-local observations while
removing owner-write permission from the runtime user. The deploy contract now requires
that ownership and forbids reverting to `ubuntu:ubuntu`.

## Production Boundary

This change does not edit production configuration values, print secrets, deploy to
production, enable a timer, approve a brief, call the image provider, write Feishu,
create or approve an artifact, publish, call QiWe, or send.

After this change is merged, released, and deployed, rerun the release-local one-shot
canary with a newly selected pending `poster_brief` UUID and the approved release,
sidecar, and database hashes.
