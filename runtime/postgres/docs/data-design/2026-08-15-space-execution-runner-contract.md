# 2026-08-15.001 Space Execution Runner Contract

## Purpose

Close two execution gaps without adding a workflow engine or a second runtime database:

- deterministic businesses select a fixed executor through registered capability
  metadata instead of a capability-key branch; and
- `agent_turn` children can be claimed and completed by one authenticated, bounded
  runner identity while retaining the existing Space, definition, work-item, artifact,
  and audit tables.

## Deterministic Recipes

A deterministic capability must retain the existing Space invocation metadata and add
`metadata.space_execution_recipe`. The runtime recognizes only a compiled fixed recipe
catalog. Version 1 registers `qiwe_text_template_v1`; it performs the existing
exact-room roster verification, Space-derived targeting, bounded template rendering, and
non-retryable Qiwe send. A second capability may reuse that recipe without a Rust
capability-key branch. Unknown recipes fail before execution. Recipes cannot provide
code, SQL, HTTP endpoints, credentials, or destinations.

## Agent-Turn Broker

`run-space-agent-turn-broker` is a separate Unix-socket server. It is disabled unless
all of the following exact gates pass before socket bind or Postgres access:

- `QINTOPIA_SPACE_AGENT_TURN_RUNNER_ENABLED=1`;
- the owner approval phrase;
- the reviewed database URL SHA-256; and
- the SHA-256 of a high-entropy runner bearer token; and
- the dedicated non-root runner uid plus its private shared socket-group gid.

The socket is mode `0660` and must live in an owner-provisioned exact-mode `0750`
directory whose owner is not the runner and whose group is shared only with the isolated
runner OS identity. Startup verifies the directory and bound-socket uid/gid/mode. Each
connection must match the configured Unix peer uid/gid before the request is read; the
request must also name the fixed identity `erhua-space-agent-runner-v1` and prove the
bearer token. Claim responses never contain a Space UUID, provider room id, destination,
actor id, database credential, or network endpoint.

The claim transaction revalidates the active automation, business, default Space policy,
optional event mapping, current conversation, handoff capability, parent work item, and
exact immutable digests. It returns only the goal, bounded canonical trigger, exact
business output contract, and the intersection of capabilities that are:

- granted by the active Space policy;
- allowed by the exact business version;
- globally enabled and owned by Erhua;
- callable by `erhua` for `space_agent_turn`; and
- registered with `runner_access=bounded_catalog_v1` and invocation boundary
  `erhua.space_agent_turn`.

The runner exposes only that returned catalog to the completion bridge. The repository
does not invent a model provider: a default-disabled QiWe-local UDS reuses the
Hermes-owned `ctx.llm` handle and returns only a final object or one requested catalog
call. It never executes capabilities. The isolated standard-library runner sends every
requested call back through the broker.

Each broker invoke locks the live claim, reauthorizes the complete Space and definition
binding, validates the input against the current capability schema, and writes a durable
idempotent capability-call work item. It derives all Space, conversation, subject, and
scope values from trusted work-item state. Version 1 registers only the read-only
`trigger_subject_identity_lookup_v1` recipe, restricted to exact trigger subject IDs and
a recent current-member roster sync in the exact Space. Finish derives actual usage from
the persisted receipts and requires an exact match with the runner report; usage is no
longer accepted as unverified telemetry.

Production enablement still depends on the isolated runner having no alternate tool,
database, credential, or network path. The repository runner is intentionally manual
`--once`; installation or scheduling under a dedicated OS identity remains a separate
owner-reviewed activation step.

## Output Contract

Every `agent_turn` business owns `definition.output_contract`. Proposal validation
accepts a bounded JSON-Schema subset with a closed object root, typed properties,
required fields, arrays, string and precise-integer bounds, enum, and const. It rejects
floating-point `number`, open objects, routing-sensitive property names, regex,
references, combinators, formats, executable expressions, and unknown schema keywords.

The same exact contract is digest-bound into the child and checked again against the
active business version at claim and finish. Successful output is capped at 64 KiB,
validated recursively, and persisted as an inert `space_agent_turn_result` artifact. The
artifact is explicitly marked as untrusted agent output, ineligible for direct
execution, and never a routing authority. A future consumer must derive Space,
recipient, endpoint, credentials, and authorization again from trusted runtime context;
field-name filtering is only defense in depth. Invalid output terminalizes the one-shot
child; it is never silently coerced or retried.

The goal and bounded canonical trigger are also checked against the active business and
exact parent work item. Event type, provider reference, opaque subject ids, and RFC3339
timestamps are bounded before they can cross the broker.

## Failure And Rollback

Claims are one-shot. Expiry has an unknown capability outcome and becomes terminal
failed with no automatic retry. The database clock owns the lease deadline. The broker
reconciles expired claims independently of runner traffic in bounded transactional
batches; a late finish can terminalize the same row only after proving its exact stored
token, and its result is ignored. Child and parent expiry audit events are committed in
the same transaction. Pausing the automation or disabling any participating capability
prevents new claims and makes an in-flight completion fail closed on its live
authorization check.

Rollback disables the broker environment flag and the global `erhua.space_agent_turn`
capability. Existing immutable work items, artifacts, and events remain for audit. The
migration creates no table and does not enable a capability or automation.
