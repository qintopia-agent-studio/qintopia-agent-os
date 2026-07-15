# Xiaoman Production Path Audit

Date: 2026-07-15

## Current Execution Path

Read-only production inspection found this active path:

```text
Hermes Xiaoman gateway
  -> /home/ubuntu/.hermes/hermes-agent
  -> /home/ubuntu/.hermes/profiles/xiaoman
  -> release/current/skills/qintopia-tools/variants/xiaoman

Postgres event_signals
  -> Xiaoman signal timer
  -> activity request
  -> promotion starter
  -> evidence and visual timers
  -> approved poster brief
  -> image-generation request starter
  -> approved generated image gate
  -> send-request starter
  -> human final confirmation
  -> group send-ready internal audit
```

`release/current` resolves to `v0.2.9` commit
`7553f92b3205dc7e8632894212380630c139a111`. The Xiaoman `qintopia-tools` plugin is a
symlink into that immutable release and its active source matches the release copy. The
sidecar system service and all seven inspected internal worker services execute the same
immutable release binary.

The aggregate Xiaoman production preflight passed. Every inspected timer was loaded,
enabled, active, and waiting; each latest worker result was successful. No external
adapter, database test write, Feishu write, QiWe send, provider/media call, or profile
edit was performed by the audit.

## Repository Ownership

Already managed by this repository and the release flow:

- Xiaoman `qintopia-tools` plugin source;
- Rust sidecar workflow and Postgres migrations;
- signal, promotion, evidence, visual, image-request, send-request, and send-ready
  worker commands;
- systemd service/timer render and installation;
- production preflight, release assembly, restart selection, and rollback contracts.

Still runtime-local or outside this repository's release ownership:

- Xiaoman `SOUL.md`, `config.yaml`, `profile.yaml`, `webhook_subscriptions.json`,
  `channel_directory.json`, and `cron/jobs.json` are regular files in the live profile,
  not release-managed symlinks;
- Hermes core runs from the server checkout at `.hermes/hermes-agent`; its observed
  commit is `c76d035c1` with 19 dirty entries;
- `.env`, sessions, auth, memories, logs, cache, locks, webhook runtime state, and state
  databases must remain server-local and must never be copied into git.

## Completion Assessment

The AgentOS-only Xiaoman orchestration path is deployed and operational. It currently
stops at internal, human-gated send readiness: Huabaosi provider execution and QiWe
image delivery remain disabled and unscheduled. A read-only preflight cannot prove a
real activity traversed an empty queue, so full end-to-end business acceptance still
requires owner-approved staging evidence and then one reviewed production item.

Unified code management is also incomplete until the reviewed Xiaoman profile files are
rendered from this repository and activated through release/current. Hermes core should
be treated as a separate upstream/fork migration; copying its dirty server checkout into
this repository would mix runtime state and unreviewed patches.

## Next Actions

1. Migrate Xiaoman's non-secret profile behavior into a reviewed profile bundle with a
   live diff, release packaging, profile smoke, and rollback to the existing local
   files.
2. Extract and classify the Hermes core server patch stack in a separate inventory PR;
   adopt only reviewed patches against a pinned upstream source.
3. Run owner-approved Huabaosi image-generation staging and QiWe callback/send canaries
   with isolated storage and target allowlists.
4. After those gates pass, process one reviewed real Xiaoman activity in production and
   retain only sanitized counts and state transitions as evidence.

## Validation

- `QINTOPIA_XIAOMAN_ACTIVITY_PRODUCTION_PREFLIGHT_ENABLE=1` production aggregate smoke
- fixed `systemctl show` checks for Xiaoman gateway and seven internal worker paths
- release/current and plugin symlink resolution
- active plugin-to-release byte comparison
- Hermes core commit and dirty-entry count only; no patch contents copied

## Validation Remediation

The initial preflight-record rewrite removed fixed queue labels and exact decision text
required by the repository deploy checks. Direct validation caught the regression before
commit. The record was corrected to preserve the machine-required headings, labels,
Huabaosi dry-run command, and Pass/Hold text while recording unavailable aggregate queue
counts honestly.

## Production Boundary

This audit was read-only. It did not edit the server, restart services, source profile
content into git, write Postgres or Feishu, call QiWe, generate or upload media,
publish, or send externally.
