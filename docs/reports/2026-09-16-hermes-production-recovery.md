# Hermes production recovery — 2026-09-16

## Status and boundary

Implementation in progress; **production recovery is not accepted**. This is separate
from PR #704. No production source edits, service restarts, external test messages,
credential changes, lock deletion, or business-job replay were performed by this repair.
Owner: Qintopia operations; implementation: Codex recovery branch.

Production was rechecked on September 16 (Asia/Shanghai): Qintopia release
`c3c605ab1ba8ca46155e467fbe22c5ef5ad03bb4`, clean official Hermes core
`2237be355906fbe6065ce1815711eee52b2d646e` (`v2026.9.7`). Seven Gateway services are
active. Active/connected status is not a successful message round trip.

## Evidence and disposition

| Area                  | Evidence                                                                                                                                                     | Disposition                                                                                                         |
| --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------- |
| Console               | Old process still runs; current `hermes_cli/web_dist/index.html` is absent; loopback homepage returns 500                                                    | Build matching official assets; pin immutable core, Python and assets; preserve Nginx authentication and ports      |
| Erhua QiWe            | Fresh discovery against deployed bundle fails: missing `qiwe.space_agent_completion`; broad ImportError fallback masks it as missing `image_callback_bridge` | Include missing module; preserve packaged import errors; check actual payload and official discovery                |
| WeCom                 | Silaoshi 846609 and fallback failure; Xiaoman expired-request failures                                                                                       | Cause not proven; no core hot patch or restart-loop workaround                                                      |
| Duplicate connections | Seven Gateway processes at inspection; four env-based bot bindings compared locally without duplicates                                                       | Does not exclude config-based bindings, transient senders or another host; investigation remains open               |
| Silaoshi sender       | Historical missing `gateway.platforms.wecom`; server-local sender was separately modified September 16; it creates its own adapter connection                | Do not treat historical import error as current; independent connection may conflict, but causality is unproven     |
| Tools                 | Terminal hardline rejection; execute_code exception was truncated                                                                                            | Do not relax security; full sanitized exception and legitimate-operation reproduction still needed                  |
| Cron acknowledgement  | Candidate writer creates visible `.ready` before JSON write; deterministic delayed-write probe exposes incomplete JSON                                       | Upstream blocker; two historical warning executions are completed with no pending handoff, not lost jobs; no replay |
| Snapshot              | Fixed weekly-plan wrapper is root:root 0700 and matches reviewed source                                                                                      | Narrow byte-verified root:ubuntu 0750 repair; no recursive permission changes                                       |

Official cron already has a durable delivery queue for external workers; missing
platform discovery must not be generalized into a claim that every worker directly sends
messages. Existing Silaoshi bridge tests establish local bridge behavior, not production
tool or WeCom acceptance.

## Legacy source inventory

The retained pre-clean checkout has 11 tracked dirty files: six implementation files and
five tests. Implementation paths are `gateway/platforms/base.py`,
`gateway/platforms/webhook.py`, `gateway/platforms/wecom.py`, `hermes_cli/kanban_db.py`,
`tools/kanban_tools.py` and `tools/send_message_tool.py`. The Kanban changes include
claimer identity, task creation, completion and worker session metadata. These are
additional **parity review items**, not confirmed causes of this incident. Do not copy
this mixed tree back or declare full behavioral migration from clean-core status.

The old WeCom entry moved to the official plugin path; old frontend assets exist while
the active source lacks them. Both old and current cores retain terminal and code
execution entrypoints, so their failures are not explained by missing tool source files.
Server-local bridge inspection confirms its action script/interpreter exist, its action
digest matches the pinned binding, and its socket is Ubuntu-owned 0600. This is not an
execution acceptance test. Recent tool-result classification found policy, JSON parsing,
file-path and secret-scope errors; no blanket security/permission relaxation is
justified.

## Profile acceptance baseline

`未验证` means no accepted user-triggered round trip in this recovery. Preserve both
intentionally disabled WeCom configurations; no traffic is not permission to remove
them.

| Profile   | Service | Channel baseline                             | Model / tools                                 | Cron / delivery                                           | User round trip                    |
| --------- | ------- | -------------------------------------------- | --------------------------------------------- | --------------------------------------------------------- | ---------------------------------- |
| default   | active  | WeCom enabled, stale connected status        | 未验证                                        | Two warning executions completed; atomic ack blocked      | 未验证                             |
| erhua     | active  | QiWe discovery failed; WeCom disabled        | 未验证                                        | QiWe unknown-platform delivery failures                   | 未验证                             |
| guanerye  | active  | WeCom enabled, stale connected status        | 未验证                                        | 未验证                                                    | 未验证                             |
| huabaosi  | active  | WeCom enabled, stale connected status        | 未验证                                        | 未验证                                                    | 未验证                             |
| silaoshi  | active  | WeCom enabled; 846609 observed               | Local bridge replay passed; production 未验证 | Historical sender import failures; current outcome 未验证 | 未验证                             |
| wenyuange | active  | WeCom disabled intentionally                 | 未验证                                        | 未验证                                                    | Not applicable to disabled channel |
| xiaoman   | active  | WeCom enabled; expired-request scenario open | 未验证                                        | Recent failures require classification                    | 未验证                             |

## Validation evidence

- Actual deploy payload dependency test passed and now covers relative QiWe imports.
- Fresh Gateway config, cron platform resolution and standalone send preparation
  discover the packaged QiWe plugin against the pinned official core. No network or live
  profile was used; this is registration acceptance, not delivery acceptance.
- QiWe unit suite: 303 tests, passed with one skip.
- Hermes runtime suite initially passed 35 tests under candidate Python 3.12. Initial
  default Python 3.14 run lacked PyYAML; changing the local interpreter resolved setup.
- Dashboard artifact tests: seven passed. Installer transaction tests: seven passed,
  including preflight failure, retry, operator drift, idempotence, rollback and
  concurrent activation refusal.
- Server-local recovery backup tests: four passed, including a committed live-WAL
  snapshot, source permission preservation, symlink inventory without traversal and
  refusal to overwrite an existing backup. Production backup execution remains pending.
- Snapshot permissions tests: five passed; timer installation fixture passed. Real Linux
  root/ubuntu execution remains pending.
- Silaoshi script bridge suite: 16 passed, including bounded retries, action
  deduplication, notification failure without action replay, and child-process-group
  timeout.
- Pinned official Vite bundle and isolated dashboard HTTP/API smoke passed. Upstream
  `tsc -b` failed on shared Vitest imports and optional Vite filter typing. Artifact
  records this failure; activation requires an explicit reviewed exception.
- Deterministic cron ack publication probe: **blocked**, incomplete JSON exposed.
- Official WeCom synthetic transport probe: stale-request clearing passed; an accepted
  passive reply with a lost acknowledgement triggers proactive fallback. **Blocked:**
  duplicate effects are possible. This is a reproduced core behavior, not proof that the
  observed production 846609 had the same cause.
- Console rebuilt artifact manifest:
  `b4e894e5a08fce18c84c8fd89034fc5f420face17279e012cd901d19b07ca918`. Two local builds
  have identical frontend inventories and pnpm lock checksums; the rebuilt artifact
  passed the real isolated HTTP/API smoke.
- Official WeCom regression selection: 80 passed, three skipped, one expected failure,
  one failed (`test_download_remote_bytes_blocks_connect_time_rebind`). This does not
  qualify the core as fully compatible; full subscription-lifecycle replay remains open.
- PR checks initially failed formatting; files were formatted. Markdown lint also
  traversed ignored local official-core evidence; its ignore list now matches the
  repository-owned `.local-workspace`/`.worktrees` lifecycle convention.
- `deploy:hermes-core:check` and `deploy:contracts:check` passed.
- Heavy check light tier, default Rust tests (850 passed), feature-boundary tests,
  Clippy and all-feature tests (865 passed, 61 ignored) passed. Local heavy exits
  blocked at PostgreSQL readiness because `pg_isready` is unavailable; it did not use
  the unrelated existing database on port 5432. Disposable PostgreSQL CI is separate.
- Reusing the official-core editable venv for repository QiWe unit tests caused enum
  registration failures. Those tests passed again in a clean project Python 3.12 venv;
  real-core discovery is verified independently in the candidate venv.
- First Linux artifact CI passed: run `35057214493`, artifact digest
  `sha256:8581277de596a0f9dc97d7a8c2678ef0cb9c01e89cf4239f56c9a0ff27a10bfa`. Later
  commits add Linux root/Ubuntu permission acceptance and backup tests; their final CI
  status remains to be recorded.
- Chrome plugin/browser acceptance, authenticated production browsing, user messages,
  15-minute rolling observations and 24-hour/natural-cron observations remain pending.

## Release and rollback

Use the [recovery runbook](../operations/hermes-production-recovery.md). A successful
local test does not authorize bypassing artifact review, immutable installation or
per-service acceptance. Server-local consistent SQLite/config backups and busy-task
inventory are mandatory before activation and have not yet been taken by this repair.

Known-faulty previous console state is a downgrade rollback, not restored service. Keep
this report open until the user-triggered flows and observation windows pass.
