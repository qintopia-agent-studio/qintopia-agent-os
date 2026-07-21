# Codex Loopback Test Sandbox

Date: 2026-07-14

## Observed Evidence

The complete sidecar suite initially reported three failures while 277 tests passed:

```text
reserve loopback port: PermissionDenied: Operation not permitted
bind fake server: PermissionDenied: Operation not permitted
```

The affected tests were existing Huabaosi fake provider/media socket tests, not the new
QiWe callback redaction tests.

## Root Cause

The default Codex command sandbox did not permit `TcpListener::bind` on an ephemeral
loopback port. Those tests intentionally start local fake HTTP servers and therefore
need loopback socket permission even though they make no external network request.

## Resolution

Run the identical complete test command with the approved loopback-bind permission. Do
not skip, ignore, or weaken the fake server tests.

## Validation

The permitted rerun completed with:

```text
280 passed; 0 failed; 0 ignored
```

## Remaining Boundary

The rerun bound only local ephemeral sockets. It did not contact a provider, media
storage, QiWe, Feishu, Postgres, or production services.

## Follow-Up Owner Action

Future Codex runs must distinguish sandbox `PermissionDenied` from a product test
failure and repeat the full suite with loopback permission before reporting status.
