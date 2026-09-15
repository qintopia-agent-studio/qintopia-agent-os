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

## Space Configuration And Group Isolation

The three configuration tools, `qintopia_space_change_prepare`,
`qintopia_space_change_confirm`, and `qintopia_space_change_status`, remain
independently available through their trusted-session and administrator-confirmation
boundary. They do not replace or unregister the existing QiWe tools or Erhua
`qintopia-tools` capabilities.

For an ordinary QiWe group turn, setting `QIWE_SPACE_TURN_POLICY_ENFORCEMENT_ENABLED=1`
makes the adapter load identity, knowledge scope, and effective capabilities from the
active policy for the exact current group, then authorize every governed capability
again immediately before invocation. Both operations resolve the current Space and
speaker from the authenticated persisted message receipt; neither accepts model-supplied
room, actor, or destination ids. A missing policy, timeout, malformed response, or
unauthorized capability fails closed. `QIWE_SPACE_TURN_POLICY_TIMEOUT_SECONDS` defaults
to `0.4` seconds.

The switch governs ordinary group turns only. An explicitly authenticated QiWe direct
session keeps the existing direct-tool behavior without projecting a group policy, but
the gateway platform, conversation type, chat, speaker, and message fields must all be
present. Direct tools remain bound to the current direct conversation and speaker; a
direct turn cannot select a group target or another user.

The enforcement switch defaults to `0` for the one-time reviewed rollout. Ordinary
capabilities remain registered, but a governed call is effective only when the current
Space policy grants it, no active revocation subtracts that grant, and the matching
global capability registry row is enabled. The three configuration tools retain their
separate review boundary so an authorized Space administrator can prepare, inspect, and
confirm policy changes even when ordinary business capabilities are empty. Quota
declarations remain validated but explicitly non-enforced in v1.

## Official Event Research Boundary

`QINTOPIA_SPACE_EVENT_RESEARCH_ENABLED` accepts only the exact value `1` and defaults to
disabled. When enabled, the default researcher starts the release-owned
`official_qiwe_research_worker.py` with an isolated Python mode, a fixed minimal
environment, closed inherited file descriptors, no stdin, discarded stderr, a bounded
stdout protocol, and a hard deadline. It passes only bounded depth/page counts. The
worker accepts no URL, query, headers, credentials, proxy settings, or executable path
from the group turn or process environment.

The worker starts only from the two repository-registered Qiwe documentation pages,
follows only normalized `https://doc.qiweapi.com/doc-<number>` links without redirects,
and caps request count, page bytes, visible text, link fanout, crawl depth, page count,
runtime, and result bytes. Both worker and parent independently validate the output.
Retrieved text is always framed as untrusted reference data for the planner; it can
provide event facts but cannot provide instructions, destinations, credentials, tools,
or code.

Clearing the child environment materially reduces credential carriage, but a subprocess
under the same Unix UID is not a credential-isolation boundary: it may still be able to
read files available to that UID. Production research must remain disabled until the
worker runs as a dedicated OS identity or equivalent container with no access to Hermes,
Qiwe, NATS, database, deployment, or operator credential files. Enabling this switch on
the existing same-UID gateway is not production approval.

## Space Agent Completion Boundary

The default-disabled Space agent completion socket reuses the Hermes-owned `ctx.llm`
handle inside the QiWe adapter. It does not load provider configuration or credentials,
execute a capability, select a destination, or send a message. It starts and stops with
the adapter and accepts only a dedicated non-root runner whose Unix peer UID/GID and
bearer SHA-256 both match the reviewed configuration.

The newline-delimited, bounded JSON protocol accepts only
`operation=space_agent_turn_complete`, schema version 1, one work-item UUID, the bounded
goal/trigger/output contract, the broker-issued capability catalog, and at most 16
completed capability calls. Its response always has only `schema_version`, `accepted`,
and `decision`. An accepted decision is either a final output object or one capability
call containing a new UUID, an exact catalog key, and an input object. Authorization,
capability execution, receipts, and final output validation remain sidecar-owned.

Enablement requires `QIWE_SPACE_AGENT_COMPLETION_ENABLED=1`, the exact reviewed approval
phrase, an absolute socket path, the runner UID/GID, and only the runner bearer's
SHA-256. The plaintext bearer belongs solely in the isolated runner environment.

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
- Publishes ingress-authenticated durable system events only to the separate
  `qintopia.qiwe.raw.authenticated` subject. The producer uses a bounded auth file;
  credentials in the NATS URL are rejected. The sidecar ignores the envelope's
  `ingress_auth_verified` value and derives that fact only from the ACL-protected
  subject. Compatibility-mode or ordinary raw capture therefore cannot self-assert
  trust, drive Space event mappings, or count as real shadow evidence.
- Keeps ordinary message capture best-effort even when NATS is unavailable. The separate
  `QIWE_SYSTEM_EVENT_DURABLE_CAPTURE_ENABLED=1` gate applies only to authenticated
  system events: the whole `data[]` envelope has a fixed 1.5-second budget to receive a
  valid authenticated-raw-subject JetStream PubAck for every system event. Any timeout,
  NATS rejection, malformed acknowledgement, or partial batch returns a fixed HTTP 503
  without provider details so QiWe can retry. The gate is default-disabled and requires
  webhook authentication, a producer auth file, and NATS capture at adapter startup;
  `Nats-Msg-Id` and Postgres event ids retain replay idempotency. Production activation
  additionally requires anonymous publish denial and distinct producer/consumer subject
  ACL evidence; loopback binding alone is not an authentication boundary.
- Plans explicit requests such as "启用 welcome_new_members" as the bounded
  `definition_operation=activate` form. The public tool supplies only the stable
  automation key; the sidecar resolves and digest-binds the current group's latest
  shadow automation and exact dependencies, requires exact same-Space real-event
  evidence for event triggers, and rejects confirmation after any stream-head drift.
  Stored cron, timezone, event binding, and business input are never reconstructed by
  the model. `agent_turn` activation uses the dedicated authenticated broker contract;
  execution remains default-disabled until its isolated OS identity, socket group,
  bearer secret, model adapter, capability gates, and owner runtime approval are
  provisioned.
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
  and complete `msgData` core credential fields (`fileAesKey`, `fileId`, `fileMd5`, and
  `fileSize`). `filename`/`fileName` is optional and never becomes a fallback for the
  transaction-locked approved artifact filename; arbitrary nested `cmd=20000` values do
  not bypass ordinary message parsing.
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
- Promotes `public_source_check_required` answer context into explicit reply directives,
  including the Xiaohongshu search path. The raw answer-context JSON alone is not a
  reliable final-reply constraint.
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
