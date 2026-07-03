# QiWe Server Backup Review

Date: 2026-07-03

Mode: read-only server inspection

## Scope

Compared the current production QiWe adapter file with the untracked server backup:

```text
/home/ubuntu/.hermes/profiles/erhua/plugins/qiwe-platform/adapter.py
/home/ubuntu/.hermes/profiles/erhua/plugins/qiwe-platform/adapter.py.bak.home-group-send-20260607-1050
```

## Source State

```text
branch=main
head=6f69794
untracked=adapter.py.bak.home-group-send-20260607-1050
```

## Hashes

| File                                           | SHA-256                                                            |
| ---------------------------------------------- | ------------------------------------------------------------------ |
| `adapter.py`                                   | `01e847d7c1484856c5d86f55378dd0c612a431080318b2c3e8bfe678b6af80bb` |
| `adapter.py.bak.home-group-send-20260607-1050` | `3b6a9099e7d4cda31aa02fdbf1720cc67279bfdd8dce6774dc3e3f92d1e84349` |

## Diff Summary

```text
1 file changed, 40 insertions(+), 1922 deletions(-)
```

The backup lacks later tracked capabilities, including passive pipeline, NATS capture,
rich/revoke/voice/handoff tools, activity handling, and context preparation.

## Disposition

`runtime-only` audit evidence.

Do not use this backup as an adoption source. Keep it on the server until the owner
approves cleanup or archival.
