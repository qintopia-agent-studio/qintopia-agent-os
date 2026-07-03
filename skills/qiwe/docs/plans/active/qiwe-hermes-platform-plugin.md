# QiWe Hermes Platform Plugin Plan

## Objective

Create a Hermes Gateway Platform Plugin for QiWe third-party webhook messages. The
plugin should eventually replace the current OpenClaw QiWe plugin after test-group
validation.

## Scope

In scope:

- Hermes platform plugin scaffold.
- Deterministic QiWe payload parser.
- Mention-triggered Hermes Agent messages.
- Direct/private Hermes Agent messages with no permission escalation.
- QiWe `/msg/sendHyperText` outbound replies.
- QiWe `/msg/sendText` direct outbound replies.
- QiWe `/msg/sendLocation` channel mapping for structured location cards.
- Sender mention in replies.
- Dedupe, group slash-command gate, and direct allowlist/test gate.
- Fixture-based tests.
- Server deployment runbook.

Out of scope for v1:

- Production cutover from OpenClaw.
- Full all-message memory pipeline.
- Daily summary / Feishu knowledge writing.
- Long-term identity graph.

## Recommended Implementation Path

1. Create `plugin.yaml`.
2. Create parser module or parser functions in `adapter.py`.
3. Add fixtures from real QiWe payloads.
4. Add parser tests before Hermes integration.
5. Implement `QiWeAdapter(BasePlatformAdapter)`.
6. Implement `connect()` using `aiohttp` HTTP server.
7. Implement webhook POST route, likely `/qiwe/webhook`.
8. Implement `send()` using QiWe `/msg/sendHyperText`, `/msg/sendText`, and
   `/msg/sendLocation` where metadata contains a safe `location_card`.
9. Register platform with `ctx.register_platform()`.
10. Register `qiwe_send_location_card` as the controlled Hermes channel-action tool for
    native location cards.
11. Deploy to `/home/ubuntu/.hermes/plugins/qiwe/` in a test group.

## Plugin Registration

Target registration shape:

```python
def register(ctx):
    ctx.register_platform(
        name="qiwe",
        label="QiWe",
        adapter_factory=lambda cfg: QiWeAdapter(cfg),
        check_fn=check_requirements,
        validate_config=validate_config,
        env_enablement_fn=_env_enablement,
        cron_deliver_env_var="QIWE_HOME_GROUP",
        standalone_sender_fn=_standalone_send,
        allowed_users_env="QIWE_ALLOWED_USERS",
        allow_all_env="QIWE_ALLOW_ALL_USERS",
        max_message_length=3500,
        platform_hint=(
            "You are chatting in a QiWe group. Keep replies concise and "
            "mention the asker when appropriate."
        ),
        emoji="💬",
    )
```

## Expected Environment Variables

Names can change during implementation, but start with:

```text
QIWE_API_URL=http://manager.qiweapi.com/qiwe/api/qw/doApi
QIWE_TOKEN=<third-party-token>
QIWE_NODE_ID=<node id if needed by send API>
QIWE_BOT_USER_ID=1688857683805864
QIWE_BOT_NAMES=二花
QIWE_WEBHOOK_HOST=127.0.0.1
QIWE_WEBHOOK_PORT=<choose non-conflicting port>
QIWE_ALLOWED_USERS=
QIWE_ALLOW_ALL_USERS=false
QIWE_DIRECT_ENABLED=true
QIWE_DIRECT_ALLOWED_USERS=
QIWE_DIRECT_ALLOW_ALL=false
QIWE_DEDUPE_TTL_SECONDS=600
QIWE_HOME_GROUP=
```

Do not commit real secrets.

## Parser Rules

Input is the third-party wrapper JSON.

```python
payload = parse_json(request_body)
raw_event = json.loads(payload["data"]) if isinstance(payload.get("data"), str) else payload.get("data")
group_id = str(raw_event["fromRoomId"])
sender_id = str(raw_event["senderId"])
message_id = str(raw_event.get("msgUniqueIdentifier") or raw_event.get("msgServerId") or payload.get("guid"))
text = raw_event["msgData"]["content"] or payload.get("content")
```

If `eventCode=group_msg_event` and `fromRoomId` is missing or zero:

- do not call Hermes Agent;
- return accepted false or ignored;
- log `not_group_message`.

If `fromRoomId` is zero and the event is a direct/private message:

- use `senderId` as the Hermes DM chat id;
- apply `QIWE_DIRECT_ENABLED`, `QIWE_DIRECT_ALLOWED_USERS`, and `QIWE_DIRECT_ALLOW_ALL`;
- default to allowlist-only direct/private handling when `QIWE_DIRECT_ALLOW_ALL` is not
  explicitly true;
- do not grant group/member-scope permissions by channel alone.

If outer `fromGroup` differs from inner `fromRoomId`:

- log `group_id_mismatch`;
- continue with inner `fromRoomId`.

## Hermes MessageEvent

Construct a `MessageEvent` with:

- `text`: cleaned user text with bot mention stripped if applicable.
- `message_type`: `MessageType.TEXT`.
- `source`: from `self.build_source(...)`.
- `message_id`: QiWe message unique id.

Session should be isolated by group and sender:

```text
qiwe:<fromRoomId>:user:<senderId>
```

Direct/private messages use `chat_type=dm` and no synthetic group thread.

If the base `build_source()` does not expose a direct session-key override, inspect
current Hermes platform adapters for the idiomatic mapping. Preserve the group/sender
isolation.

## Reply Rules

Use:

```text
POST QiWe doApi /msg/sendHyperText
```

Use:

```text
toId = fromRoomId
```

Reply body should mention sender:

```text
@<sender> <Hermes reply>
```

Use hyperText segments:

- `subtype=1` for sender mention;
- `subtype=0` for text.

Do not try alternate group ids on `[-3020]`. Log it as invalid room or send business
error.

Direct replies use `/msg/sendText`. Markdown is preserved as plain channel text.
Location cards use `/msg/sendLocation` with `guid`, `toId`, `title`, `address`,
`latitude`, and `longitude`, followed by an optional short text reply. Native cards are
sent through the `qiwe_send_location_card` Hermes tool after GIS/business logic has
selected an approved structured result. If location-card delivery fails, send a text
fallback instead of retry-spamming.

## Validation Plan

Local:

```bash
python3 -m py_compile adapter.py tests/test_parser.py
python3 -m unittest discover -s tests -v
```

Server smoke:

```bash
systemctl --user status hermes-gateway.service --no-pager
journalctl --user-unit hermes-gateway.service -f --no-pager
```

Test group only:

- Send `@二花 hi`.
- Verify webhook accepted.
- Verify Hermes Agent receives message.
- Verify QiWe `/msg/sendHyperText` returns success.
- Verify reply appears in the same group and mentions sender.
- Send a direct/private test message from an allowlisted sender.
- Verify the reply uses `/msg/sendText`.
- Trigger a location-card response through a controlled Hermes skill/tool path and
  verify `/msg/sendLocation` or text fallback.

## Current Local Status

Implemented locally:

- `plugin.yaml` platform metadata and env var declarations.
- `adapter.py` QiWe parser, aiohttp webhook server, Hermes adapter, sendHyperText reply
  sender, sendText direct reply sender, sendLocation location-card sender, env
  enablement, platform registration, and controlled `qiwe_send_location_card` tool
  registration.
- Fixture-backed parser tests for mention group text, normal group text, outer
  `fromGroup` mismatch, missing inner `data.fromRoomId`, cue trigger, group slash
  blocking, direct allowlist behavior, and dedupe.
- Send-body tests for group hyperText, direct sendText, Markdown passthrough,
  location-card shape, and location-card tool idempotency.

Not yet done:

- Install to `/home/ubuntu/.hermes/plugins/qiwe/`.
- Configure test-group webhook route to the Hermes plugin port.
- Run live Hermes gateway/test-group smoke validation.

## Rollback

- Leave OpenClaw production path untouched during initial development.
- If Hermes plugin fails, disable or remove `/home/ubuntu/.hermes/plugins/qiwe` and
  restart `hermes-gateway.service`.
- Do not modify nginx or production `/qiwe/webhook` route until the Hermes test group is
  proven.
