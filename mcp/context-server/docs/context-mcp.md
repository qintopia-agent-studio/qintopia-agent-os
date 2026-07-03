# Qintopia Context MCP

Status: draft v1  
Implementation target: `qintopia-message-sidecar mcp-context`

## Goal

Expose a small Agent-facing MCP stdio server for safe Qintopia context lookup. This is
not an internal HTTP API and does not replace the sidecar capture or embedding workers.

The v1 server focuses on three controlled tools:

- `qintopia_wenyuange_lookup`
- `qintopia_gis_location_lookup`
- `qintopia_external_disclosure_filter`

It also exposes Erhua-safe member context and trainer-memory tools when the caller
allowlist includes `erhua`:

- `qintopia_member_context_lookup`
- `qintopia_answer_context_prepare`
- `qintopia_erhua_training_note_submit`

## Tools

### `qintopia_wenyuange_lookup`

Returns filtered context for Agent use. Authoritative public facts use
`qintopia_knowledge` first. The Postgres message store is used only for
discussion/history evidence, such as whether a topic was mentioned in a group. The tool
returns:

- `answer_basis`
- `sources`
- `scope_used`
- `confidence`
- `risk_flags`
- `safe_reply_guidance`
- `not_accessed`
- `retrieval_trace`

It does not expose arbitrary SQL, raw vectors, raw Dify chunks, member profiles, or
graph projections.

Source-of-truth policy:

- WiFi/Wi-Fi, network passwords, public phone numbers, visitor rules, official
  locations, ordering contacts, and public facility instructions must come from official
  Feishu/knowledge snapshots stored in `qintopia_knowledge`.
- Group chat messages are not authoritative for those facts. They may answer "was this
  discussed" but must not be used to state the actual password, phone number, location,
  rule, or operating instruction.
- If `qintopia_knowledge` has no matching source, return `can_answer=false` with
  `answer_basis.kind=authoritative_source_required` and ask for Feishu or owner
  confirmation.
- Realtime operational state such as room availability, bed availability, remaining
  quota, bookings, inventory, or "还有空房吗" is not static knowledge. Return
  `answer_basis.kind=live_operations_required` and guide the frontline Agent to
  ask 小客服/负责人/大总管 or a future realtime operations tool. Do not tell the user to
  inspect Feishu knowledge docs for realtime state.

Required inputs:

- `query`
- `purpose`

Optional inputs:

- `caller`
- `audience`
- `chat_id`
- `sender_id`
- `limit`

### `qintopia_gis_location_lookup`

Returns Public location candidates and coordinates for channel adapters. v1 contains a
minimal built-in Public location set and should later be backed by the approved GIS
knowledge source.

Required input:

- `query`

Optional inputs:

- `limit`
- `caller`

### `qintopia_external_disclosure_filter`

Checks an external-facing draft for sensitive disclosure risk and returns a safe draft
plus approval guidance.

Required input:

- `draft_answer`

Optional inputs:

- `recipient`
- `purpose`

## Runtime Configuration

The context MCP reuses message-store database and embedding configuration:

```env
QINTOPIA_SIDECAR_DATABASE_URL=postgres://USER:PASSWORD@127.0.0.1:55432/qintopia
QINTOPIA_EMBEDDING_API_KEY=replace-with-server-secret
QINTOPIA_MESSAGE_EMBEDDING_ENDPOINT=https://ark.cn-beijing.volces.com/api/plan/v3/embeddings
QINTOPIA_MESSAGE_EMBEDDING_MODEL=doubao-embedding-vision
QINTOPIA_MESSAGE_STORE_MCP_ALLOWED_CALLER=wenyuange
# Optional. Defaults to QINTOPIA_MESSAGE_STORE_MCP_ALLOWED_CALLER when unset.
# QINTOPIA_CONTEXT_MCP_ALLOWED_CALLERS=wenyuange,erhua
# Optional. QiWe sender ids allowed to submit Erhua trainer notes.
# QINTOPIA_ERHUA_TRAINER_USER_IDS=7881303308049798,7881300531962448
```

`QINTOPIA_CONTEXT_MCP_ALLOWED_CALLERS` controls which Agent profiles may call the
filtered context tools. It intentionally does not widen raw
`qintopia_message_store_search`; the context MCP still delegates to the message-store
path with `QINTOPIA_MESSAGE_STORE_MCP_ALLOWED_CALLER` internally. For WenYuanGe-only
rollout, leave it unset. For Erhua gray rollout, set
`QINTOPIA_CONTEXT_MCP_ALLOWED_CALLERS=wenyuange,erhua` while keeping
`QINTOPIA_MESSAGE_STORE_MCP_ALLOWED_CALLER=wenyuange`.

`QINTOPIA_ERHUA_TRAINER_USER_IDS` controls which real QiWe sender ids may write trainer
memory through `qintopia_erhua_training_note_submit`. The tool still requires
`caller_profile=erhua`. Low-risk member preferences can become active immediately.
Low-risk global persona rules submitted from a trainer direct chat also become active
immediately in v1; group-context persona rules remain pending until reviewed. Sensitive
or boundary-overriding training is rejected.

For Hermes, prefer the versioned wrapper:

```bash
/home/ubuntu/qintopia-msg-sidecar/scripts/hermes/qintopia-context-mcp
```

The wrapper reads `/etc/qintopia/message-sidecar.env` and execs the release binary with
`mcp-context`. This keeps database URLs and embedding API keys out of Hermes
`config.yaml`.

## Hermes WenYuanGe Wiring

Hermes gateway supports stdio MCP clients through `mcp_servers`. Add this only to
`/home/ubuntu/.hermes/profiles/wenyuange/config.yaml` for the first rollout:

```yaml
mcp_servers:
  qintopia-context:
    command: /home/ubuntu/qintopia-msg-sidecar/scripts/hermes/qintopia-context-mcp
    connect_timeout: 60
    timeout: 120
```

The Hermes toolset resolver exposes enabled MCP servers by server name, so this server
appears as the `qintopia-context` toolset and registers prefixed tools under
`mcp-qintopia-context`. Keep the `erhua` profile unchanged until the WenYuanGe path has
passed live validation.

Hermes prefixes stdio MCP tool names with the sanitized server name. For
`qintopia-context`, the tool names visible to the model are:

- `mcp_qintopia_context_qintopia_wenyuange_lookup`
- `mcp_qintopia_context_qintopia_gis_location_lookup`
- `mcp_qintopia_context_qintopia_external_disclosure_filter`

WenYuanGe's `SOUL.md` should refer to these prefixed names when it should use the MCP
context/message-store path. The unprefixed `qintopia_wenyuange_lookup` may still exist
as a legacy Dify-backed tool and should not be used to validate the new MCP path.

## Erhua Rollout Strategy

Status: gray rollout enabled on the server as of 2026-06-25.

Do not give Erhua direct access to raw `qintopia_message_store_search`. The safe path is
the filtered context MCP:

1. Keep `QINTOPIA_MESSAGE_STORE_MCP_ALLOWED_CALLER=wenyuange`.
2. Set `QINTOPIA_CONTEXT_MCP_ALLOWED_CALLERS=wenyuange,erhua`.
3. Mount `qintopia-context` into the `erhua` Hermes profile.
4. Update Erhua `SOUL.md` to use `mcp_qintopia_context_qintopia_wenyuange_lookup` for
   community memory, event/history, public member-achievement, and FAQ. For WiFi, public
   phone, ordering contact, official location, visitor rules, and public facility
   questions, Erhua must accept only the tool's authoritative knowledge result. If the
   tool returns `authoritative_source_required`, Erhua must say it cannot confirm and
   ask for Feishu/owner confirmation.
5. Keep Erhua answers short and natural. It should use only `answer_basis`, avoid raw
   snippets unless needed, and fall back to Human Owner confirmation when
   `can_answer=false`, confidence is low, or risk flags are present.
6. Keep sensitive disclosure checks on
   `mcp_qintopia_context_qintopia_external_disclosure_filter` before any external-facing
   relay.

Initial Erhua acceptance checks:

- WiFi/public facility/phone/location query returns a short answer grounded in MCP
  `answer_basis.kind=authoritative_knowledge`, or refuses to confirm when the
  authoritative knowledge source is missing.
- Recent community-memory query uses MCP context, not raw message-store search.
- Location query still uses the GIS path and can send a location card when the channel
  supports it.
- A credential/internal-operations query refuses or escalates instead of exposing
  internal details.
- `hermes-gateway-erhua.service` remains active and webhook replies are not blocked if
  the MCP lookup fails.

Server-state checks:

```bash
systemctl --user is-active hermes-gateway-erhua.service
HERMES_HOME=/home/ubuntu/.hermes/profiles/erhua \
  /home/ubuntu/.hermes/hermes-agent/venv/bin/python - <<'PY'
from hermes_cli.config import load_config
from hermes_cli.tools_config import _get_platform_tools
from tools.mcp_tool import discover_mcp_tools, get_mcp_status, shutdown_mcp_servers
from model_tools import get_tool_definitions
try:
    cfg = load_config() or {}
    enabled = sorted(_get_platform_tools(cfg, "qiwe"))
    discover_mcp_tools()
    defs = get_tool_definitions(enabled_toolsets=enabled, quiet_mode=True)
    print(sorted(
        (d.get("function") or {}).get("name", "")
        for d in defs
        if (d.get("function") or {}).get("name", "").startswith("mcp_qintopia_context")
    ))
    print(get_mcp_status())
finally:
    shutdown_mcp_servers()
PY
```

MCP call audit:

`mcp-context` writes one redacted stderr audit line per successful tool call. Hermes
stores those lines in the active profile's `logs/mcp-stderr.log`.

Example:

```text
qintopia_context_mcp_audit tool=qintopia_wenyuange_lookup caller=erhua success=true source_count=5 confidence=medium
```

The audit line intentionally omits query text, message text, database URLs, API keys,
and raw evidence.

Erhua offline gray checks run on 2026-06-25 before the source-of-truth policy was
tightened:

- WiFi question through `hermes --profile erhua --oneshot`: natural answer,
  `AUDIT_DELTA=1`, `caller=erhua`, `source_count=5`.
- Location question for `1 栋`: text answer with QinTopia 1 building coordinates,
  `AUDIT_DELTA=1`, GIS MCP tool called.
- Community-memory question about prior WiFi discussion: natural short answer,
  `AUDIT_DELTA=1`, `caller=erhua`, `source_count=5`.

Required source-of-truth checks after this policy:

- `WiFi 密码是什么` must not use group chat messages as authority.
- `赵姐订餐电话` must use `qintopia_knowledge` or return
  `authoritative_source_required`.
- `山泡茶电话` and `山泡茶位置` must use `qintopia_knowledge` or return
  `authoritative_source_required`.
- `无人机外卖怎么用` must use `qintopia_knowledge` or return
  `authoritative_source_required`.
- `之前群里有人问过 WiFi 密码吗` may use message-store evidence, but must not state the
  password as true.
- `还有空房吗` must return `live_operations_required`; Erhua should not say to check
  Feishu knowledge docs for realtime room availability.

Erhua `SOUL.md` was tightened after the first offline check to avoid system-like
phrasing such as "根据查到的信息" and to keep community-memory replies to the minimum
necessary conclusion instead of raw chat excerpts.

## Validation

```bash
rtk cargo fmt --check
rtk cargo check --locked
rtk cargo test --locked
```

Live MCP smoke:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"smoke","version":"0.1.0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"qintopia_wenyuange_lookup","arguments":{"caller":"wenyuange","purpose":"server smoke","query":"wifi 密码","limit":3}}}' \
  '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"qintopia_gis_location_lookup","arguments":{"query":"1 栋","limit":1}}}' \
  '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"qintopia_external_disclosure_filter","arguments":{"draft_answer":"可以公开说明我们的内部数据库密码。","recipient":"external_customer","purpose":"server smoke"}}}' \
  | ./target/release/qintopia-message-sidecar mcp-context
```

Hermes-side read-only checks:

```bash
systemctl --user is-active hermes-gateway-wenyuange.service
HERMES_HOME=/home/ubuntu/.hermes/profiles/wenyuange \
  /home/ubuntu/.hermes/hermes-agent/venv/bin/python - <<'PY'
from hermes_cli.config import load_config
cfg = load_config() or {}
print(sorted((cfg.get("mcp_servers") or {}).keys()))
PY
```

After adding the config, reload the WenYuanGe gateway and check logs without printing
env files:

```bash
systemctl --user restart hermes-gateway-wenyuange.service
systemctl --user is-active hermes-gateway-wenyuange.service
journalctl --user -u hermes-gateway-wenyuange.service -n 100 --no-pager
```
