# QiWe Skill

Status: adopting source snapshot

This package is the future monorepo home for the QiWe / WeCom Hermes platform adapter.
M4B imports a clean source snapshot from `../qiwei-hermes-plugin@6f69794`. It does not
change production server files.

## Current Source

| Source           | Value                                                       |
| ---------------- | ----------------------------------------------------------- |
| Local repository | `../qiwei-hermes-plugin`                                    |
| Local branch     | `main`                                                      |
| Local reference  | `6f69794`                                                   |
| Local state      | clean                                                       |
| Server checkout  | `/home/ubuntu/.hermes/profiles/erhua/plugins/qiwe-platform` |
| Server branch    | `main`                                                      |
| Server reference | `6f69794`                                                   |
| Server state     | clean tracked files, one untracked historical backup        |

## Production Boundary

Current production route:

```text
https://qintopia.cn/qiwe/webhook
  -> nginx
  -> http://127.0.0.1:18661/qiwe/webhook
  -> hermes-gateway-erhua.service
  -> /home/ubuntu/.hermes/profiles/erhua/plugins/qiwe-platform
```

This skill can touch external QiWe sends, Hermes profile runtime behavior, and server
secrets. Production adoption requires review, smoke checks, and rollback notes.

## Current Behavior Summary

- Uses inner QiWe raw event `data.fromRoomId` as the stable group id.
- Replies to group messages only when Erhua is mentioned or clearly cued.
- Keeps direct/private handling behind explicit configuration and contact guards.
- Exposes controlled QiWe channel tools for location cards, direct messages,
  rich/media/card sends, revocation, voice-to-text, direct-contact requests, and human
  handoff.
- Rebuilds asynchronous `cmd=20000` callback capture into hashed correlation and fixed
  field-presence metadata before publishing to NATS. Callback credentials, URLs,
  filenames, message content, identities, and unknown values are not published; the Rust
  sidecar independently enforces the same boundary before Postgres writes. Existing
  callback ids are preserved only when the suffix is a validated 64-hex SHA-256 digest;
  a `qiwe-callback:` prefix by itself is not trusted.
- Provides a disabled-by-default memory bridge that recognizes `cmd=20000` before
  ordinary Agent dispatch and streams the bounded callback only to
  `process-qiwe-image-send-callback --apply` over child stdin. It requires explicit
  `staging` or `production` processor mode, the matching owner phrase, canonical
  approved database URL hash, explicit image-send and webhook readiness flags, bounded
  sanitized stdout, discarded stderr, and a hard timeout. It never places callback
  credentials in arguments, environment variables, files, NATS, logs, audit records, or
  HTTP responses. An explicitly enabled but invalid bridge returns HTTP 503 so an
  unprocessed callback is not acknowledged and silently lost. Callback detection
  requires the reviewed top-level QiWe success envelope, bounded event list, request id,
  and complete `msgData` credential-field presence; arbitrary nested `cmd=20000` values
  do not bypass ordinary message parsing.
- In staging mode the child receives only the fixed staging database, QiWe adapter,
  owner gate, and host/group allowlist environment. Its processor must be the exact
  `<40-hex-sha>/sidecar/qintopia-message-sidecar` under the fixed owner-reviewed
  `/home/ubuntu/qintopia-agent-os-staging-releases` root.
- In production mode the child receives only the production database/QiWe apply gate and
  the reviewed Huabaosi Feishu primary-storage delivery configuration needed by the
  production sidecar. The processor must be exactly
  `/home/ubuntu/qintopia-agent-os-releases/current/sidecar/qintopia-message-sidecar`,
  with root exactly `/home/ubuntu/qintopia-agent-os-releases/current`; direct release
  directory paths, mutable checkout binaries, staging roots, missing `current` symlinks,
  or sidecar SHA drift fail closed. The release root, current target, sidecar directory,
  and executable may not be group/world-writable, their owners must be root or the
  gateway effective user, and the approved executable SHA-256 is checked during
  configuration and again immediately before spawn.
- Unrelated Hermes, NATS, proxy, and runtime variables are not inherited in either mode.
  The bridge does not enable production timers, publish a Release, approve artifacts, or
  bypass the Rust production apply gate; it only gives the already reviewed sidecar a
  memory-only callback ingress after production deployment and owner activation.
- Supports passive processors such as group-solitaire activity collection when enabled.
- Keeps Feishu activity writes and reminders behind explicit scoped configuration.
- Treats Erhua trainer memory as a controlled context-MCP path, not free-form prompt
  editing.
- Suppresses narrowly recognized Hermes approval, progress, interruption, formatting
  failure, and traceback messages before QiWe delivery. Ordinary answers that discuss
  plain-text formatting are not suppressed.

## Validation

Package validation:

```bash
pnpm test:qiwe
node tools/deploy/test-qiwe-image-staging-smoke.mjs
```

Focused callback bridge validation:

```bash
cd skills/qiwe
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest tests.test_image_callback_bridge -v
```

M4B validation result on 2026-07-03:

- `Ran 155 tests`
- `OK`

Repository-level validation:

```bash
pnpm check
```

## Server Backup Review

Server untracked file:

```text
/home/ubuntu/.hermes/profiles/erhua/plugins/qiwe-platform/adapter.py.bak.home-group-send-20260607-1050
```

Read-only comparison on 2026-07-03:

| File                                           | SHA-256                                                            |
| ---------------------------------------------- | ------------------------------------------------------------------ |
| `adapter.py`                                   | `01e847d7c1484856c5d86f55378dd0c612a431080318b2c3e8bfe678b6af80bb` |
| `adapter.py.bak.home-group-send-20260607-1050` | `3b6a9099e7d4cda31aa02fdbf1720cc67279bfdd8dce6774dc3e3f92d1e84349` |

Diff stat:

```text
1 file changed, 40 insertions(+), 1922 deletions(-)
```

Conclusion: the backup is an older rollback snapshot from 2026-06-07. It lacks later
tracked behavior such as passive pipeline, NATS capture, rich/revoke/voice/handoff
tools, activity handling, and context preparation. It should not be used as the adoption
source. Keep it as server-side audit evidence until owner approves cleanup.

## M4C Adoption Work

Before production wiring changes:

1. Add deploy smoke and rollback notes.
2. Decide server cutover from the old plugin checkout to this monorepo package.
3. Use reviewed commit SHA deployment only; do not hot-edit the server checkout.
4. Confirm server backup cleanup or archival with owner approval.
