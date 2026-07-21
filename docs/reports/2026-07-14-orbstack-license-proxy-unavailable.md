# OrbStack License Proxy Unavailable

Date: 2026-07-14

## Observed Evidence

The local QiWe callback capture-storage PostgreSQL integration test could not start
because the OrbStack Docker socket did not exist:

```text
failed to connect to the docker API at
unix:///Users/qiaopengjun/.orbstack/run/docker.sock
```

Starting OrbStack displayed this blocking alert:

```text
Can't verify license.
proxyconnect tcp: dial tcp 127.0.0.1:7897: connect: connection refused
```

OrbStack exited after the alert and did not create the Docker socket.

## Root Cause

OrbStack's license request followed the configured local proxy at `127.0.0.1:7897`, but
no proxy process was listening on that port. This is a local container-runtime
availability problem, not a Rust, PostgreSQL schema, production service, or
callback-redaction failure.

## Resolution

Do not change the developer machine's network or proxy settings from this PR. Keep the
guarded integration test in GitHub Actions, where it runs against the disposable
loopback-only `qintopia_test` pgvector service. Local unit, formatting, Clippy, and
contract checks remain runnable without OrbStack.

## Validation

The local Rust callback sanitization tests must pass. The PR is not mergeable until the
GitHub Actions `QiWe callback capture-storage redaction integration` step proves the
final raw and normalized stored events contain no callback request id, file credentials,
media URL, filename, identity, message content, or unknown field value.

## Remaining Boundary

No production database, QiWe endpoint, image provider, media storage, Feishu adapter, or
external send is touched. Failure to run the local container test must not be reported
as successful PostgreSQL validation.

## Follow-Up Owner Action

Restore a working proxy on the configured port or update OrbStack's reviewed network
configuration before relying on local container integration again. CI remains the
required database gate until then.
