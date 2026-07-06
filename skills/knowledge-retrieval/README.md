# Knowledge Retrieval Skill

This package owns the active WenYuanGe/Dify public-safe evidence retrieval capability.
It returns a filtered answer basis, source metadata, risk flags, and safe reply guidance
for frontline Agents.

Hermes still loads stable tool names through the `skills/qintopia-tools` registration
shell. This qintopia-tools registration shell delegates Dify and
`qintopia_wenyuange_lookup` behavior here. Change retrieval behavior in this package,
not in `skills/qintopia-tools`.

## Capability

- retrieve public-safe answer basis for frontline Agents;
- keep raw Dify tools scoped to Wenyuange;
- return filtered answer basis, source metadata, risk flags, and safe reply guidance;
- avoid exposing raw internal chunks to Erhua, Xiaoqin, or public channels.

## Tools

- `qintopia_wenyuange_lookup`: safe lookup surface for Erhua member replies and future
  Xiaoqin external-customer replies. It filters raw Dify chunks before the frontend
  Agent can answer.
- `qintopia_dify_dataset_list`
- `qintopia_dify_dataset_get`
- `qintopia_dify_knowledge_retrieve`
- `qintopia_dify_document_list`
- `qintopia_dify_document_get`
- `qintopia_dify_indexing_status_get`
- `qintopia_dify_segment_list`
- `qintopia_dify_segment_get`

Raw `qintopia_dify_*` tools remain Wenyuange-only and opt-in through the qintopia-tools
registration shell. Frontline Agents should use only `qintopia_wenyuange_lookup`.

## Not Owned Here

- `qintopia_message_store_search` stays in `skills/qintopia-tools` for now and should
  move later with the Postgres/context migration.
- Public JSONL snapshot search and GIS lookup remain in `skills/qintopia-tools` until
  they receive dedicated capability packages.
- Dify write APIs are not implemented. Any future write surface needs separate
  owner-approved architecture documentation, audit, and human authorization.

## Runtime Boundary

- Dify Knowledge API keys come from profile/server environment only.
- `QINTOPIA_DIFY_ALLOWED_DATASET_IDS` should be set in production profiles.
- `QINTOPIA_DIFY_LOOKUP_DATASET_ID` should point to the single approved dataset used by
  `qintopia_wenyuange_lookup`, or the allowlist must contain exactly one dataset.
- This package does not send external messages and does not write databases.

## Validation

```bash
pnpm skills:knowledge-retrieval:check
pnpm skills:qintopia-tools:check
pnpm check:light
```
