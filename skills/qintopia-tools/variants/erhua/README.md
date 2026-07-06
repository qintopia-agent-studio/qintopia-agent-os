# Qintopia Tools Hermes Plugin

Hermes-native tools for Qintopia Agent OS.

The knowledge and GIS tools are read-only. The complaint tools are narrow write-capable
wrappers for 二花's controlled complaint/service-recovery workflow; they are not a
general Kanban intake surface. The 小秦 tools are controlled sales/customer wrappers for
Public-safe product answers, lead capture, demo planning, proposal drafts, disclosure
filtering, and conversation handoff. Dify Knowledge tool registration stays in this
Hermes plugin for stable tool names, but the active Dify and `qintopia_wenyuange_lookup`
implementation lives in `skills/knowledge-retrieval`. Change Dify allowlists, filtered
answer basis, source ranking, and risk flags there. Complaint intake, sales/customer
handoff, proposal/demo draft, disclosure filtering, and conversation summary behavior
lives in `skills/operations-intake`. Change those behaviors there; this plugin keeps
stable Hermes tool registration.

## Tools

- `qintopia_kb_search`: searches approved Qintopia knowledge snapshot indexes. Defaults
  to `Public` only.
- `qintopia_gis_location_lookup`: resolves public Qintopia GIS locations from
  `gis-locations.md` and returns structured coordinates for channel adapters.
- `qintopia_weather_lookup`: fixed-location Qintopia weather lookup through a narrow
  QWeather MCP wrapper, with Open-Meteo as a limited fallback. It covers current
  weather, 24-hour rain/umbrella windows, thunderstorm windows, warnings, and current
  air quality.
- `qintopia_wenyuange_lookup`: synchronously retrieves Dify-backed knowledge through
  WenYuanGe guardrails for 二花 and 小秦. It returns answer basis, source metadata, risk
  flags, and safe reply guidance, not raw Dify chunks.
- `qintopia_dify_dataset_list`: lists Dify Knowledge datasets. If
  `QINTOPIA_DIFY_ALLOWED_DATASET_IDS` is set, the returned list is filtered to that
  allowlist.
- `qintopia_dify_dataset_get`: reads metadata for one allowed Dify dataset.
- `qintopia_dify_knowledge_retrieve`: retrieves matching chunks from one allowed Dify
  dataset. Dify exposes this read operation as `POST /retrieve`; the tool sends
  `search_method=semantic_search` and `reranking_enable=false` by default for
  compatibility with the current Dify API.
- `qintopia_dify_document_list`: lists documents in one allowed Dify dataset.
- `qintopia_dify_document_get`: reads details for one Dify document.
- `qintopia_dify_indexing_status_get`: reads indexing status for a Dify batch.
- `qintopia_dify_segment_list`: lists chunks/segments for one Dify document.
- `qintopia_dify_segment_get`: reads one chunk/segment.
- `qintopia_message_store_search`: WenYuanGe-only read tool that searches the Postgres
  `qintopia_messages.messages` and `message_embeddings` tables for recent QiWe group
  memory with time, chat, sender, kind, keyword, and pgvector semantic filters. It
  defaults to hybrid retrieval and falls back to keyword/recent search when the query
  embedding endpoint is not configured.
- `qintopia_complaint_intake_create`: creates only a `complaint_intake` card
  for 大总管 / `default` dispatch, then asks the channel adapter to private message the
  complainant for minimum details.
- `qintopia_complaint_intake_update`: appends complainant-provided details to the same
  complaint card. It does not change owner or assignee.
- `qintopia_complaint_followup_send`: prepares an approved private follow-up message
  after the complaint card is completed/reviewed.
- `qintopia_external_product_kb_search`: searches Public-only Agent OS product knowledge
  for 小秦 and returns approved baseline statements when the Public KB is thin.
- `qintopia_public_case_search`: searches approved Public case/demo references; if none
  exist, it returns a safe Human Owner escalation message instead of inventing a case.
- `qintopia_customer_context_lookup`: returns only current-channel and customer-provided
  context. It is not a CRM or other-customer lookup.
- `qintopia_lead_capture`: creates only controlled `sales_lead`, `demo_request`,
  `proposal`, or `external_disclosure_review` Kanban task requests.
- `qintopia_proposal_outline_generate`: creates a proposal outline draft, with no
  binding price, contract, delivery, schedule, scope, or SLA commitment.
- `qintopia_demo_script_generate`: creates a low-risk demo script using public samples,
  redacted materials, or customer-authorized materials only.
- `qintopia_external_disclosure_filter`: filters external-facing drafts and marks
  whether Human Owner approval is required.
- `qintopia_conversation_summary`: turns a customer conversation into the
  standard 小秦 handoff format.
- `qintopia_daily_digest_publish`: returns a narrow Agent OS publisher command for one
  Xiaoman-owned daily community event radar digest. It accepts only `digest_id`, never
  arbitrary Markdown or generic Feishu document targets.

Complaint guardrails:

- 二花 must not expose raw `kanban_create` / `kanban_create_task`.
- Complaint cards use `task_type=complaint_intake`, `tenant=qintopia`, `board=default`,
  and owner/assignee 大总管 / `default`.
- Complaint tools call the shared Hermes Kanban runtime directly when it is available;
  do not enable the raw `kanban` toolset for 二花.
- 二花 never assigns the executor, decides compensation, publishes private details in a
  group, or invents a final handling result.
- Private-message delivery remains a channel adapter responsibility.
- For QiWe complaint intake, `requester_channel_user_id` is the webhook `senderId`. If
  the Agent does not pass it explicitly, the tool falls back to
  `HERMES_SESSION_USER_ID`, which the QiWe adapter sets from `senderId`. The resulting
  `qiwe_send_direct_message.recipient_user_id` is used as `/msg/sendText` `toId`.
- Returned `qiwe_send_direct_message` actions must include the approved `purpose` and
  stable `idempotency_key` required by the controlled QiWe direct message executor.

小秦 guardrails:

- 小秦 defaults to `Public-safe` information and current customer context only.
- Public product search may use approved baseline Agent OS statements when the current
  Public KB has no matching product document.
- No tool may invent public cases, customer names, quotes, prices, delivery timelines,
  contract terms, SLA, or internal architecture details.
- `qintopia_lead_capture` is the only sales Kanban-create wrapper and it can create only
  the controlled sales task types.
- Proposal and demo outputs are drafts and should pass through
  `qintopia_external_disclosure_filter` before external sending.

Dify guardrails:

- These tools use the Dify Knowledge Service API key, not a Dify app key.
- The API key must come from profile/server environment, never from SOUL.md, skill text,
  Kanban cards, or repository files.
- `QINTOPIA_DIFY_ALLOWED_DATASET_IDS` should be set for production profiles so agents
  cannot browse unrelated datasets.
- 二花 and 小秦 should use `qintopia_wenyuange_lookup`, not raw `qintopia_dify_*` tools.
- Raw `qintopia_dify_*` read tools are registered only when
  `QINTOPIA_PROFILE_ID=wenyuange` and `QINTOPIA_DIFY_RAW_TOOLS_ENABLE=1`.
- `qintopia_message_store_search` is registered only when
  `QINTOPIA_PROFILE_ID=wenyuange` and `QINTOPIA_MESSAGE_STORE_ENABLE=1`. Frontline
  agents should not receive this raw message-store tool; they should ask WenYuanGe for a
  filtered, sourced answer.
- The plugin exposes fixed read paths only. It does not provide create, update, delete,
  arbitrary URL, or raw HTTP tools.
- Any future Dify write tool must be separate, named explicitly, audited, and gated by
  Human Owner authorization.
- External-facing agents may use Dify results only through their own disclosure rules;
  raw internal chunks should not be sent directly to customers or groups.

Weather guardrails:

- `qintopia_weather_lookup` is fixed to Qintopia coordinates `108.5876,33.9996` by
  default. It is not a general city-weather, POI, or location search tool.
- City-based QWeather warning and air-quality APIs use `QINTOPIA_WEATHER_QWEATHER_CITY`,
  defaulting to `鄠邑区`; the member-facing location name remains `秦托邦`.
- The wrapper may call only `hefeng-qweather-mcp` tools: `get_weather_now`,
  `get_hourly_weather`, `get_minutely_5m`, `get_warning`, and `get_air_quality`.
- Do not expose or wrap tropical cyclone/typhoon, ocean/marine/tide/current,
  solar-radiation, POI, historical weather, station-detail, astronomy, grid weather, or
  arbitrary-city tools for 二花.
- Open-Meteo fallback is trend-only. It must not claim official warnings, minute-level
  precipitation, air quality, typhoon, ocean, or solar-radiation data.
- The weather wrapper never runs shell snippets from member messages and never asks the
  group to approve networking or command execution.

小满 daily event radar guardrails:

- `qintopia_daily_digest_publish` is disabled unless
  `QINTOPIA_DAILY_DIGEST_PUBLISH_ENABLE=1`.
- The tool is a command wrapper over the Agent OS publisher, not a generic Feishu API
  client.
- The publisher must enforce `owner_agent=xiaoman`, configured target chats, allowlisted
  Feishu parent nodes, publish-status checks, and audit rows.
- 小满 must not receive generic Feishu document create/update permissions.
- The daily event radar is internal operations material and must not be posted directly
  to QiWe groups.

## Server Install

Install per profile that needs Qintopia tools because Hermes discovers user plugins from
the active `HERMES_HOME/plugins` directory. Production should use the release/current
layout, not an ad hoc `rsync` of only this plugin:

```text
/home/ubuntu/.hermes/profiles/erhua/plugins/qintopia-tools
  -> /home/ubuntu/qintopia-agent-os-releases/current/skills/qintopia-tools/variants/erhua
```

The same release must include delegated skill packages under
`/home/ubuntu/qintopia-agent-os-releases/current/skills`. Erhua currently delegates
weather to `skills/qintopia-weather` and Dify/WenYuanGe lookup to
`skills/knowledge-retrieval`. If Hermes loads this plugin from a copied profile-local
directory, set `QINTOPIA_AGENT_OS_SKILLS_DIR` to the release `skills` directory.

Enable in the profile `config.yaml`:

```yaml
plugins:
  enabled:
    - qintopia-tools
toolsets:
  - qintopia
  - qiwe
kanban:
  dispatch_in_gateway: false
agent:
  environment_probe: false
  disabled_toolsets:
    - terminal
    - code_execution
    - delegation
    - skills
    - browser
    - browser-cdp
    - file
    - cronjob
    - messaging
    - computer_use
    - session_search
    - memory
    - todo
    - tts
    - clarify
```

Configure Dify read access in the target profile/server environment:

```bash
export QINTOPIA_DIFY_KB_BASE_URL=https://qintopia.cn/remote/v1
export QINTOPIA_DIFY_KB_API_KEY='...Knowledge Service API key...'
export QINTOPIA_DIFY_ALLOWED_DATASET_IDS='dataset_id_1,dataset_id_2'
export QINTOPIA_DIFY_LOOKUP_DATASET_ID='dataset_id_1'
export QINTOPIA_PROFILE_ID=wenyuange
export QINTOPIA_DIFY_RAW_TOOLS_ENABLE=1
export QINTOPIA_MESSAGE_STORE_ENABLE=1
export QINTOPIA_MESSAGE_STORE_DATABASE_URL='postgres://USER:PASSWORD@127.0.0.1:55432/DB?sslmode=disable'
export QINTOPIA_MESSAGE_STORE_EMBEDDING_URL='http://127.0.0.1:PORT/v1/embeddings'
export QINTOPIA_MESSAGE_STORE_EMBEDDING_MODEL='text-embedding-3-small'
export QINTOPIA_MESSAGE_STORE_EMBEDDING_DB_MODEL='text-embedding-3-small'
# Optional, only if the embedding endpoint requires it:
export QINTOPIA_MESSAGE_STORE_EMBEDDING_API_KEY='...'
export QINTOPIA_DAILY_DIGEST_PUBLISH_ENABLE=1
export QINTOPIA_DAILY_DIGEST_PUBLISHER_BIN=/home/ubuntu/qintopia-msg-sidecar/target/release/qintopia-message-sidecar
```

Configure QWeather lookup for 二花:

```bash
/home/ubuntu/.hermes/hermes-agent/venv/bin/python -m pip install hefeng-qweather-mcp==0.5.0
mkdir -p /home/ubuntu/.hermes/profiles/erhua/secrets
openssl genpkey -algorithm ED25519 \
  -out /home/ubuntu/.hermes/profiles/erhua/secrets/qweather-ed25519-private.pem
chmod 600 /home/ubuntu/.hermes/profiles/erhua/secrets/qweather-ed25519-private.pem
openssl pkey -pubout \
  -in /home/ubuntu/.hermes/profiles/erhua/secrets/qweather-ed25519-private.pem \
  -out /home/ubuntu/.hermes/profiles/erhua/secrets/qweather-ed25519-public.pem

export HEFENG_API_HOST='...QWeather API host...'
export HEFENG_PROJECT_ID='...QWeather project id...'
export HEFENG_KEY_ID='...QWeather JWT credential id...'
export HEFENG_PRIVATE_KEY_PATH=/home/ubuntu/.hermes/profiles/erhua/secrets/qweather-ed25519-private.pem
export QINTOPIA_WEATHER_LOCATION=108.5876,33.9996
export QINTOPIA_WEATHER_LOCATION_NAME=秦托邦
export QINTOPIA_WEATHER_QWEATHER_CITY=鄠邑区
```

Any profile can use the Dify read tools when this plugin is installed for that profile
and its `config.yaml` includes `toolsets: [qintopia]`. For non-WenYuanGe profiles, leave
`QINTOPIA_DIFY_RAW_TOOLS_ENABLE` unset and use `qintopia_wenyuange_lookup` as the
synchronous safe lookup path. Profile SOUL files should still state when Dify-backed
knowledge is allowed and how results must be filtered.

For 二花 over QiWe, keep the profile constrained to channel, Qintopia knowledge, GIS,
and controlled complaint workflow tools. Do not enable `hermes-cli`: terminal and skill
management tools can leak internal approval/self-improvement messages into member chats.
The dedicated QiWe gateway should not own general Kanban dispatch; the main/default
gateway remains the dispatcher.

## Validation

```bash
python3 config/hermes/plugins/qintopia-tools/tests/test_qintopia_tools.py
python3 -m py_compile config/hermes/plugins/qintopia-tools/__init__.py
```
