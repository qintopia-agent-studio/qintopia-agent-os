# Space Automation Runtime Production Runbook

This runbook covers the fixed activation, observation, and rollback path for the generic
Space automation dispatcher and execution worker. The repository tests use temporary
artifacts and a fake `systemctl`; they perform no production action.

## Boundary

Release installation places these reviewed units under `/etc/systemd/system`, then
disables and stops them and verifies the final inactive state:

```text
qintopia-agentos-automation-dispatcher.service
qintopia-agentos-automation-dispatcher.timer
qintopia-agentos-space-automation-execution-worker.service
```

The dispatcher is the single minute-level Postgres scheduler. The execution worker is
bound to the immutable `qiwe-production` companion binary. Activation, observation, and
rollback requests accept only the fixed target names documented below. They cannot carry
a unit name, command, path, URL, credential, database address, Space ID, or environment
override.

The unit's final `/usr/bin/env` boundary binds both the deployed Release SHA and the
Release-local migrations directory. A stale `QINTOPIA_SIDECAR_MIGRATIONS_DIR` in the
persistent env file cannot redirect schema startup to a checkout. Every Release install
quiesces this runtime, so a newly deployed Release requires a fresh owner-approved
activation.

The production scripts always use:

```text
/etc/qintopia/message-sidecar.env
/etc/qintopia/nats/qiwe-adapter.json
/etc/qintopia/nats/message-sidecar.json
/etc/systemd/system
/home/ubuntu/qintopia-agent-os-releases/current
/usr/bin/systemctl
/usr/bin/sha256sum
nats://127.0.0.1:4222
qintopia.qiwe.raw.authenticated
```

## Owner Preparation

Before the first activation, complete the separately reviewed platform configuration
change and keep the values only in the existing root-managed sidecar env file. The
activation requires exactly these persistent declarations:

```text
QINTOPIA_SPACE_AUTOMATION_EXECUTION_ENABLED=1
QINTOPIA_SPACE_AUTOMATION_EXECUTION_APPROVAL=approved-production-space-automation-execution
QINTOPIA_SPACE_AUTOMATION_EXECUTION_DATABASE_URL_SHA256=<sha256-of-the-exact-database-url>
QINTOPIA_SPACE_AUTOMATION_QIWE_ALLOWED_HOSTS=manager.qiweapi.com
QINTOPIA_SPACE_AGENT_TURN_RUNTIME_READY=0
QIWE_SPACE_TURN_POLICY_ENFORCEMENT_ENABLED=1
QIWE_NATS_CAPTURE_ENABLED=1
QIWE_SYSTEM_EVENT_DURABLE_CAPTURE_ENABLED=1
QIWE_NATS_URL=nats://127.0.0.1:4222
QIWE_NATS_AUTH_FILE=/etc/qintopia/nats/qiwe-adapter.json
QIWE_NATS_AUTHENTICATED_RAW_SUBJECT=qintopia.qiwe.raw.authenticated
QINTOPIA_SIDECAR_NATS_URL=nats://127.0.0.1:4222
QINTOPIA_SIDECAR_NATS_AUTH_FILE=/etc/qintopia/nats/message-sidecar.json
QINTOPIA_SIDECAR_RAW_SUBJECT=qintopia.qiwe.raw
QINTOPIA_SIDECAR_AUTHENTICATED_RAW_SUBJECT=qintopia.qiwe.raw.authenticated
QINTOPIA_SIDECAR_MESSAGE_SUBJECT=qintopia.qiwe.message
QINTOPIA_SIDECAR_TRUST_AUTHENTICATED_RAW_SUBJECT=true
QINTOPIA_SIDECAR_NATS_STREAM=QINTOPIA_QIWE_MESSAGES
QINTOPIA_SIDECAR_CONSUMER=qintopia-message-sidecar
```

`QINTOPIA_SIDECAR_DATABASE_URL`, `QIWE_API_URL`, `QIWE_TOKEN`, and `QIWE_GUID` must also
already be present through the existing production secret-management path. Do not put
their values in GitHub inputs, task text, pull requests, logs, or evidence files.

This activation is deterministic-only. It installs neither the dedicated agent-turn
broker nor the isolated runner, so readiness must be exactly `0`. With that value, Space
configuration rejects active `agent_turn` automations, schedule and event dispatch do
not enqueue them, and the execution worker cannot create a stranded `space_agent_turn`
child. The manual `tools/agents/run-space-agent-turn.py --once` path remains available
for a separately owner-reviewed rehearsal after provisioning the broker, completion
socket, dedicated OS identity, private group, and bearer. A future production enablement
must add and observe that runtime in a separate owner-reviewed provisioning Release
before this runbook or binary may accept readiness `1`. The reserved approval phrase is
intentionally insufficient in this Release: an environment declaration cannot prove
broker availability, runner liveness, or the dedicated identity boundary.

Provision the two fixed NATS auth files through the same root-managed secret path. Each
file is a private regular JSON file with the exact schema
`{"version":1,"username":"...","password":"..."}`. The producer and consumer must be
different NATS users. Do not put credentials in a NATS URL. Configure the NATS subject
ACL so that:

- the Qiwe adapter producer can publish `qintopia.qiwe.raw.authenticated` and receive
  its JetStream acknowledgement;
- the sidecar consumer can subscribe to that subject and its private reply inboxes, can
  use only the fixed `QINTOPIA_QIWE_MESSAGES` / `qintopia-message-sidecar` JetStream
  info, create, next-message, and acknowledgement subjects needed by the runtime, but
  cannot publish to the trusted raw subject;
- an anonymous connection cannot publish to that subject.

Do not grant the consumer a broad trusted-subject publish permission or an unrestricted
`$JS.API.>` permission. Its fixed durable consumer must already use explicit ack and the
three exact filters `qintopia.qiwe.raw`, `qintopia.qiwe.message`, and
`qintopia.qiwe.raw.authenticated`. The preflight sends invalid, non-mutating requests to
the exact create and next-message API subjects to prove their ACL without creating a
consumer or pulling a real group event. It also publishes a synthetic impossible ack
subject to prove the narrow acknowledgement prefix without acknowledging a real event.

The release does not provision NATS users or secrets. That remains a separately
owner-reviewed platform change. Space runtime activation proves the live ACL with a
fixed release-local preflight; a declaration in the env file is not accepted as proof.
The preflight publishes only a bounded synthetic event with no Space, room, member, or
message data. It also verifies that the fixed stream covers the trusted subject and the
fixed durable consumer has the exact three filters before the synthetic publish. Its
output is a fixed pass/fail marker and never includes the NATS URL, subject traffic,
credential values, auth-file paths, or server diagnostics.

Before approval, verify all of the following:

- the authenticated Qiwe callback ingress is current and one real sanitized callback has
  passed shadow validation;
- the producer and consumer NATS users are distinct, the trusted subject is covered by
  the `QINTOPIA_QIWE_MESSAGES` JetStream, and the live ACL preflight passes;
- the intended Space policy, business definition, event mapping, and automation remain
  scoped to the target conversation ID;
- the current release contains both primary and `qiwe-production` sidecar artifacts;
- the database hash declaration was calculated from the exact persistent database URL;
- no production automation relies on an unverified or nickname-only identity match.

## Activate

Run `Activate Production Timers` against the exact current Release SHA with a target
list containing `space-automation-runtime`. The workflow default deliberately omits this
target, so an operator must add it explicitly after production-environment owner
approval.

The signed request causes the root runner to call only the release-local activation
script with this fixed approval:

```text
QINTOPIA_SPACE_AUTOMATION_RUNTIME_ACTIVATION=approved-production-space-automation-runtime
```

Before touching systemd, the script verifies the persistent enablement, disabled
agent-turn readiness, Space turn policy enforcement, durable authenticated capture
declarations, execution approval, exact Qiwe host allowlist, database URL hash,
immutable current Release, and exact companion feature manifest. It then runs the fixed
release-local NATS ACL preflight in an empty environment. The preflight requires a
JetStream PubAck from the producer, requires the consumer to read the fixed
stream/consumer metadata and prove its exact create, pull, and ack ACL, requires the
consumer to receive the same no-Space probe, and requires explicit publish denial for
both the consumer and an anonymous connection. A missing or malformed ack,
credential/schema error, publish-permission leak, subscription or JetStream API failure,
wrong stream/consumer filter configuration, or timeout stops activation before any
`systemctl` mutation.

Only after that proof does activation enable and restart the dispatcher timer and
execution worker, check both states and the timer's next trigger, and run the
enabled-state observation in an empty environment. Any failure after the first systemd
mutation attempts every shutdown action, verifies disabled/inactive state, and fails
without claiming rollback success if shutdown cannot be proven.

Activation does not create or enable any Space policy, business definition, event
mapping, or automation. Those remain governed by the conversation confirmation path.

## Observe

Run `Observe Production Runtime` against the exact current Release SHA with:

```text
observation_targets=space-automation-runtime
```

The observation infers `enabled` or `disabled` from the persistent execution flag and
then verifies:

- the immutable primary and Qiwe companion binaries and exact companion feature set;
- the explicit `QINTOPIA_SPACE_AGENT_TURN_RUNTIME_READY=0` production boundary;
- byte-for-byte reviewed contents of the dispatcher service/timer and execution worker
  units, including the one-minute interval, immutable binary paths, and final migration
  binding;
- enabled/active state, a non-sentinel scheduled timer value, and a live worker PID
  whose `/proc/<pid>/exe` resolves to the current immutable Qiwe companion when enabled;
- disabled/inactive timer and worker plus an inactive dispatcher service when disabled.

Deploy evidence includes only state, artifact profile, Release SHA, scheduled-value
presence, and worker Release-identity verification. It never includes environment
values, process arguments, journal output, credentials, database addresses, payloads,
Space IDs, or group names.

## Roll Back

First use the reviewed production configuration channel to set exactly:

```text
QINTOPIA_SPACE_AUTOMATION_EXECUTION_ENABLED=0
```

Then run `Run Production Runtime One-Shot` against the exact current Release SHA:

```text
runtime_one_shot_target=space-automation-runtime-rollback
backfill_date=
payload_sha256=
approval=approved-production-space-automation-runtime-rollback
```

The rollback script attempts to disable the dispatcher timer and execution worker before
reading the persistent flag, stops and resets both services even when an earlier command
fails, verifies all final states, requires the flag to be `0`, and runs the
disabled-state observation. If the flag was not updated, the request fails but the
runtime remains stopped; fix the persistent flag and repeat the same fixed rollback
request.

Rollback does not delete definitions or work items. Pause or roll back individual
versioned definitions through the Space configuration tools when only one automation
must be disabled.

## Local Validation

```bash
pnpm deploy:space-automation-runtime:test
pnpm deploy:runner:check
pnpm deploy:contracts:check
```

The focused Space runtime test includes a fake NATS protocol server. Test-only auth
paths and loopback ports are accepted by the preflight only when its explicit test mode
is enabled; production activation has no caller-controlled path, port, URL, subject, or
command override.
