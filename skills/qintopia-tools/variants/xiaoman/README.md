# Qintopia Tools Hermes Plugin

Hermes-native tools for Qintopia Agent OS.

The knowledge and GIS tools are read-only. The complaint tools are narrow write-capable
wrappers for 二花's controlled complaint/service-recovery workflow; they are not a
general Kanban intake surface. The 小秦 tools are controlled sales/customer wrappers for
Public-safe product answers, lead capture, demo planning, proposal drafts, disclosure
filtering, and conversation handoff. Dify Knowledge tools are read-only wrappers over
Dify's Knowledge Service API and can be enabled for any profile that should use the
shared `qintopia` toolset.

## Tools

- `qintopia_kb_search`: searches approved Qintopia knowledge snapshot indexes. Defaults
  to `Public` only.
- `qintopia_gis_location_lookup`: resolves public Qintopia GIS locations from
  `gis-locations.md` and returns structured coordinates for channel adapters.
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
- `qintopia_xiaoman_activity_status_update`, `qintopia_xiaoman_activity_gap_update`, and
  `qintopia_xiaoman_activity_phase_update`: create sidecar commands for AgentOS
  `event_signals` mutations with `event_signal_id` and `mutation_id`; they do not accept
  Feishu `record_id` / `table_role` as write identifiers.
- `qintopia_xiaoman_activity_announcement_prepare`: prepares the text-only community
  activity announcement MVP for operations review. It turns sanitized activity records
  into a draft for 刘珊, missing-field follow-ups, and an Erhua handoff draft that still
  requires human confirmation before any group delivery.
- `qintopia_xiaoman_activity_handoff_create`: currently exposes only the mapped
  `visual_asset_request -> huabaosi` handoff because the Rust sidecar routes that pair
  to `huabaosi.create_visual_asset`.
- `qintopia_xiaoman_activity_promotion_review_draft`: turns already-read sanitized
  activity records into a human-reviewable summary, promotion assessment, copy draft,
  poster brief, and controlled record-path payload. It does not read Feishu, write
  Postgres, call Huabaosi, publish, queue, or send.

## Xiaoman Activity Mutations

Xiaoman's activity read tools may use allowlisted Feishu `record_id` and `table_role`
inputs. The status and gap write tools use a different boundary: they mutate only
Xiaoman-owned AgentOS `event_signals` and require both an internal `event_signal_id`
UUID and a caller-supplied `mutation_id` UUID. An exact retry must retain the same
`mutation_id`.

`qintopia_xiaoman_activity_list_by_date` normally returns the bounded sidecar command
for local execution. Set `QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE=1` only in a
runtime that should let the tool directly receive sanitized read results from the
sidecar. Read-through is limited to read-only, non-dry-run operations and returns the
worker's `record_count`, `records`, and `summaries`; write operations still return
commands.

`qintopia_xiaoman_activity_status_update` accepts only `待处理`, `处理中`, `已完成`, or
`已关闭`. `qintopia_xiaoman_activity_gap_update` accepts one non-sensitive `gap_summary`
of at most 500 characters. `qintopia_xiaoman_activity_phase_update` accepts only
`pre_event`, `in_event`, or `post_event`; the sidecar enforces forward-only transitions
and derives the route from the stored AgentOS phase fact. These wrappers default to
dry-run and return a bounded sidecar command for the runtime executor. They do not
accept Feishu record ids, write Feishu, send QiWe messages, or call an external adapter.

`qintopia_xiaoman_activity_announcement_prepare` is the current text-first operations
MVP. It may use records already returned by `qintopia_xiaoman_activity_list_by_date`, or
perform read-through only when that read-only path is explicitly enabled. It skips
temporary meal records by default, keeps paid planned activities in the scheduling pool,
flags missing time/location/owner/material fields, and returns only drafts. It does not
create work items, call Huabaosi, call Erhua, call QiWe, publish, or send.

For the boss-visible promotion review path, Xiaoman should first read activity records,
then pass the selected sanitized record to
`qintopia_xiaoman_activity_promotion_review_draft`. That draft is a stateless review
artifact for a human owner: it can suggest the controlled dry-run
`qintopia_xiaoman_activity_handoff_create` payload to record the next step after human
confirmation, but it does not approve, generate images, queue group messages, publish,
or send. Postgres/AgentOS remains the fact source; Hermes only calls the tool.

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
- The plugin exposes fixed read paths only. It does not provide create, update, delete,
  arbitrary URL, or raw HTTP tools.
- Any future Dify write tool must be separate, named explicitly, audited, and gated by
  Human Owner authorization.
- External-facing agents may use Dify results only through their own disclosure rules;
  raw internal chunks should not be sent directly to customers or groups.

## Server Install

Install per profile that needs Qintopia tools because Hermes discovers user plugins from
the active `HERMES_HOME/plugins` directory:

```bash
rsync -az --delete config/hermes/plugins/qintopia-tools/ \
  ubuntu@122.51.77.220:/home/ubuntu/.hermes/profiles/erhua/plugins/qintopia-tools/
```

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
```

Configure Dify read access in the target profile/server environment:

```bash
export QINTOPIA_DIFY_KB_BASE_URL=https://www.qintopia.cn/remote/v1
export QINTOPIA_DIFY_KB_API_KEY='...Knowledge Service API key...'
export QINTOPIA_DIFY_ALLOWED_DATASET_IDS='dataset_id_1,dataset_id_2'
export QINTOPIA_DIFY_LOOKUP_DATASET_ID='dataset_id_1'
export QINTOPIA_PROFILE_ID=wenyuange
export QINTOPIA_DIFY_RAW_TOOLS_ENABLE=1
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
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover \
  -s skills/qintopia-tools/variants/xiaoman/tests -p 'test_*.py'
node tools/skills/check-qintopia-tools.mjs
```
