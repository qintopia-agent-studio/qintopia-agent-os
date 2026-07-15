# Staging Runtime Prerequisite Observation

Date: 2026-07-16 Asia/Shanghai

## Current State

A read-only SSH observation checked whether the fixed staging runtime prerequisites for
Huabaosi image generation and QiWe image send are already present on `paxon-server`.

The checked server reported hostname `VM-0-4-ubuntu`. The two fixed staging paths are
still missing:

```text
missing=/etc/qintopia/message-sidecar-staging.env
missing=/home/ubuntu/qintopia-agent-os-staging-releases
```

No staging release binary was present under the fixed immutable staging release root,
because the root itself is absent. This means the owner-approved Huabaosi and QiWe
readiness smokes cannot yet pass on the server, even though the repository PRs provide
the guarded scripts, evidence checkers, and templates.

## Command Shape

The observation used `ssh -o BatchMode=yes paxon-server` and only executed fixed
read-only shell checks:

- print the remote hostname;
- check whether `/etc/qintopia/message-sidecar-staging.env` exists;
- check whether `/home/ubuntu/qintopia-agent-os-staging-releases` exists; and
- if the release root exists, list only matching `*/sidecar/qintopia-message-sidecar`
  paths.

The command did not read env file contents, print secrets, create directories, copy
files, install services, run sidecar binaries, connect to Postgres, call Huabaosi,
Feishu, QiWe, provider, or media endpoints, restart systemd, publish a Release, or send
externally.

## Impact

Real isolated staging remains blocked by runtime provisioning, not by the local fake
smoke path:

- no staging env file exists for the fixed env allowlist parser;
- no immutable staging release root exists for the digest-pinned sidecar binary;
- no owner-approved staging release SHA or packaged sidecar SHA-256 can be verified on
  that server yet;
- no server-side Huabaosi staging readiness pass can be retained; and
- no server-side QiWe staging preflight/upload/callback evidence can be retained.

Treating local smoke tests, fake-sidecar tests, or green PR CI as real staging would
hide this missing runtime boundary.

## Required Follow-Up

Before a real staging exercise, an owner-reviewed provisioning step must create and
protect the fixed staging inputs without committing secrets:

1. `/etc/qintopia/message-sidecar-staging.env` with only the reviewed staging key
   allowlist and no production database or production target group.
2. `/home/ubuntu/qintopia-agent-os-staging-releases/<40-hex-sha>/sidecar/qintopia-message-sidecar`
   from the reviewed staging-feature artifact.
3. Recorded owner-approved staging release SHA, sidecar SHA-256, staging database URL
   SHA-256, isolated target group allowlist, Huabaosi image request UUID, QiWe
   send-ready work item UUID, callback source, and rollback owner.
4. Read-only Huabaosi and QiWe staging readiness smokes, followed by the protected
   Huabaosi generation smoke, QiWe preflight/upload/callback phases, and cross-flow
   evidence checker.

## Production Boundary

This observation did not edit the server or enable staging/production. It did not add a
listener, service, timer, production feature build, Feishu write, provider/media call,
QiWe send, Release publish, or production activation.
