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
  sidecar independently enforces the same boundary before Postgres writes.
- Supports passive processors such as group-solitaire activity collection when enabled.
- Keeps Feishu activity writes and reminders behind explicit scoped configuration.
- Treats Erhua trainer memory as a controlled context-MCP path, not free-form prompt
  editing.

## Validation

Package validation:

```bash
pnpm test:qiwe
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
