# Qintopia Tools Skill

Status: adopting profile variants

`skills/qintopia-tools` is the monorepo home for the Hermes `qintopia-tools` plugin
currently installed under active profile plugin directories. New capability
implementation should not be added here when a dedicated capability package exists.

## Current Shape

The live server does not have one clean git-managed plugin copy. It has profile-local
variants:

| Variant   | Server source                                                    | Current role                                 |
| --------- | ---------------------------------------------------------------- | -------------------------------------------- |
| Erhua     | `/home/ubuntu/.hermes/profiles/erhua/plugins/qintopia-tools`     | Broad Qintopia tools plus weather/digest     |
| Xiaoman   | `/home/ubuntu/.hermes/profiles/xiaoman/plugins/qintopia-tools`   | Xiaoman activity wrappers and shared tools   |
| Wenyuange | `/home/ubuntu/.hermes/profiles/wenyuange/plugins/qintopia-tools` | Knowledge, Dify, and message-store read      |
| Huabaosi  | no observed `qintopia-tools` directory during M10-C inventory    | Not migrated in this step                    |
| Xiaoqin   | `/home/ubuntu/.hermes/profiles/xiaoqin/plugins/qintopia-tools`   | Deprecated profile; excluded from M10 active |

M10-C imports the three active variants as snapshots under `variants/`. It does not
repoint production profiles yet.

Weather is already split out: Erhua's `qintopia_weather_lookup` registration delegates
to `skills/qintopia-weather`. Change weather behavior there, not in this package.

WenYuanGe/Dify knowledge retrieval is also split out: the Dify raw read tools and
`qintopia_wenyuange_lookup` registration delegates to `skills/knowledge-retrieval`.
Change filtered answer basis, Dify allowlist behavior, source ranking, and risk flags
there. This package keeps the current Hermes registration shell and the still-unmigrated
message-store, GIS, and Xiaoman wrappers.

Operations intake is also split out: complaint intake, controlled sales/customer
handoff, proposal/demo drafts, disclosure filtering, and conversation summaries delegate
to `skills/operations-intake`. Change those behaviors there. This package keeps stable
Hermes tool registration and runtime callback wiring only.

## Boundary

`skills/qintopia-tools` is a Hermes registration shell for delegated capabilities. Some
registered tools may return `qiwe_send_direct_message` action requests through
`skills/operations-intake`; this package does not send messages directly. The actual
send must stay behind the QiWe/channel adapter approval, recipient allowlist,
idempotency, and audit gates.

Allowed in this package:

- reviewed plugin source
- `plugin.yaml`
- package-local tests
- source snapshot notes
- historical backup hashes needed for audit

Not allowed:

- profile `.env` files
- profile sessions, logs, cache, auth files, or state databases
- generated runtime memory
- raw private chat logs
- Xiaoqin WorkTool runtime as an active package
- new weather provider logic; use `skills/qintopia-weather` and `mcp/weather-provider`
- new WenYuanGe/Dify retrieval behavior; use `skills/knowledge-retrieval`
- new complaint, sales handoff, proposal/demo, or disclosure behavior; use
  `skills/operations-intake`

## Validation

```bash
pnpm skills:qintopia-tools:check
pnpm skills:qintopia-weather:check
pnpm skills:knowledge-retrieval:check
pnpm skills:operations-intake:check
```

The check compiles each active variant and blocks committed runtime cache files.

## Production Migration

Before any server repoint:

1. Use the release/current layout as the source of truth. Do not copy one
   `qintopia-tools` directory into `.hermes/profiles/*/plugins` by itself.
2. Back up the current profile-local plugin directory.
3. Repoint one profile at a time to the reviewed variant, for example:

   ```text
   /home/ubuntu/.hermes/profiles/erhua/plugins/qintopia-tools
     -> /home/ubuntu/qintopia-agent-os-releases/current/skills/qintopia-tools/variants/erhua
   ```

4. Ensure the same release contains every delegated skill package:
   `skills/qintopia-weather` for Erhua weather, `skills/knowledge-retrieval` for
   Dify/WenYuanGe lookup, and `skills/operations-intake` for complaint/sales/disclosure
   handoff. If Hermes loads from a profile-local copy, set
   `QINTOPIA_AGENT_OS_SKILLS_DIR=/home/ubuntu/qintopia-agent-os-releases/current/skills`
   in the profile or service environment.
5. Verify Hermes service active state, plugin import, tool registration, and rollback.
6. Do not delete old profile-local directories until M11/M12 cleanup gates pass.
