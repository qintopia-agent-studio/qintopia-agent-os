# Deploy Tools

`tools/deploy/preflight.mjs` is a non-mutating deployment gate. It does not deploy code
and does not connect to the server.

Use the CI-safe gate as part of repository validation:

```bash
pnpm deploy:preflight:ci
```

Use the local preflight before an approved deployment window:

```bash
pnpm deploy:preflight
```

Local mode additionally requires:

- current branch is `master`
- worktree is clean
- deployment policy, CI/CD gate docs, and sidecar cutover plan exist
- repository checks include registry, manifest, policy, secret, tests, smoke, and deploy
  preflight gates

## Systemd Cutover Preview

Validate that the sidecar systemd renderer can produce reviewable units:

```bash
pnpm deploy:systemd:check
```

The renderer is non-mutating. It writes review files to `dist/` by default and refuses
to write directly into `/etc/systemd/system`.

## Release/Current Model

Validate the stable release/current service and Hermes MCP wrapper model:

```bash
pnpm deploy:release-model:check
```

Validate the production deploy request runner contracts:

```bash
pnpm deploy:runner:check
```

The deploy runner is the server-side pull model for manual production deployments:
GitHub writes a schema-validated request to COS, and the server runner promotes reviewed
artifacts into `release/current`.

The fixed `hermes-profile-erhua` scope renders and activates only the reviewed Erhua
Livecool provider overlay and accepts only `hermes-erhua` as its restart target. It
never accepts a profile path from request data. Use a dry-run request and review its
redacted evidence before the separately approved activation request.

The check is non-mutating. It verifies the worker units render through
`qintopia-agent-os-releases/current`, avoid `/home/ubuntu/qintopia-msg-sidecar`, and
that the Hermes `mcp-context` wrapper can run from a verified artifact,
`release/current`, or explicit `QINTOPIA_SIDECAR_BIN`.

## Deploy Contract Checks

Validate deploy package metadata and production-adjacent smoke boundaries:

```bash
pnpm deploy:contracts:check
```

The check is non-mutating. It also protects the aggregate Xiaoman production preflight
smoke so it remains a composition of read-only observation scripts and does not grow
apply smoke, deploy, release, Feishu write, QiWe, or external-send behavior. The same
gate protects the Xiaoman production preflight record template so production evidence
keeps timer status, fixed commands, secret scans, queue counts, and pass/hold decisions.
It also runs `tools/deploy/check-xiaoman-preflight-readiness.mjs`, a repository-only
audit that verifies the Xiaoman workflow metadata, systemd command contracts, aggregate
preflight smoke, evidence record, and guarded apply smoke still describe one coherent
AgentOS-only production preflight path.

Rerun the full repository-local Xiaoman production evidence chain verification bundle:

```bash
node tools/deploy/check-xiaoman-production-evidence-chain-local.mjs
```

Run the Erhua member recognition local release-current readiness bundle before treating
the reviewed release as ready for production evidence capture. It runs the sidecar
compile check, focused Rust recognition/profile/context tests, deploy evidence fixtures,
deploy contract check, deploy bundle build, and release-current manifest presence check:

```bash
node tools/deploy/check-erhua-member-recognition-local.mjs
```

Apply the reviewed persistent Erhua member-recognition config before production repair
or canary runs. This only updates `/etc/qintopia/message-sidecar.env`; it does not call
QiWe, run SQL, or repair identities. Keep real group and sender ids server-local:

```bash
QINTOPIA_ERHUA_MEMBER_RECOGNITION_PRODUCTION_CONFIG=approved-production-erhua-member-recognition-config \
QINTOPIA_ERHUA_MEMBER_RECOGNITION_CONFIG_CHAT_ID=<reviewed-erhua-qiwe-group-id> \
QINTOPIA_ERHUA_MEMBER_RECOGNITION_CONFIG_CANARY_SENDER_ID=<reviewed-erhua-canary-sender-id> \
  deploy/sidecar/scripts/apply-erhua-member-recognition-production-config.sh --apply
```

The config script binds `QINTOPIA_PROFILE_TARGET_CHAT_IDS` and
`QINTOPIA_ERHUA_MEMBER_RECOGNITION_CANARY_CHAT_ID` to the same reviewed group. If the
persistent env already has exactly one reviewed `QINTOPIA_PROFILE_TARGET_CHAT_IDS`, the
chat-id override may be omitted; the canary sender id is still required unless it is
already present from a prior reviewed apply.

Run the read-only production config observation before roster sync. It emits only
sanitized booleans, counts, and `scope_fingerprint`; it does not print the real group
id, sender id, or database URL:

```bash
QINTOPIA_ERHUA_MEMBER_RECOGNITION_CONFIG_OBSERVATION_ENABLE=1 \
  deploy/sidecar/scripts/erhua-member-recognition-production-config-observation-smoke.sh
```

Continue only when it reports `action_status=ready_for_member_recognition_runbook`.

Validate Erhua room member roster sync evidence before treating recognition coverage as
current-group coverage. The checker accepts sanitized
`identity-backfill --sync-room-members` JSON output and fails on zero discovered
members, partial apply counts, missing stale-roster count, missing `scope_fingerprint`,
or sensitive fragments:

```bash
node tools/deploy/check-erhua-room-member-sync.mjs \
  <identity-backfill-room-member-sync-output.json>
```

Validate Erhua member recognition coverage from a same-chat
`identity-bootstrap-persons --dry-run --chat-id <reviewed-erhua-qiwe-group-id>` output
before treating QiWe group members as answerable by profile. The checker fails on
unresolved non-ambiguous QiWe identities, missing aliases, missing `sender_person_id`,
missing materializable platform identities, linked people without a safe answer-context
canary name, missing or mismatched mentioned/speaker/referenced canary spec arrays,
missing stable running hints, and forbidden sensitive fragments such as phone-like digit
runs:

```bash
node tools/deploy/finalize-erhua-member-recognition-coverage.mjs \
  --coverage <identity-bootstrap-dry-run-output.json> \
  --summary-output <sanitized-coverage-summary.json>
```

Use strict coverage after the `member-profile --limit 5000 --apply --quiet` repair when
claiming every linked current-room person has an active safe profile:

```bash
node tools/deploy/finalize-erhua-member-recognition-coverage.mjs \
  --coverage <identity-bootstrap-dry-run-output.json> \
  --require-active-profiles \
  --expect-pass \
  --summary-output <sanitized-coverage-summary.json>
```

The coverage summary is count-only retained evidence. It is written even when the
coverage check fails, so production review can see whether the remaining gap is identity
bootstrap, safe aliases, QiWe platform identities, active profiles, or canary coverage
without retaining member names, raw ids, profile text, or database identifiers. The
coverage finalizer runs the coverage checker and then revalidates that retained summary
with `node tools/deploy/check-erhua-member-recognition-coverage-summary.mjs`.

When the coverage checker reports unlinked current-room potential member identities,
create a reviewed safe identity payload through the release-local sidecar. The payload
must contain only redacted `identity_key` prefixes and owner-reviewed safe names:

```bash
node tools/deploy/build-erhua-member-safe-identity-payload-template.mjs \
  --coverage <identity-bootstrap-dry-run-output.json> \
  --output <safe-identity-template.json>

node tools/deploy/check-erhua-member-safe-identity-payload.mjs \
  <safe-identity.json>

qintopia-message-sidecar erhua-member-safe-identity \
  --payload-file <safe-identity.json>

QINTOPIA_ERHUA_MEMBER_SAFE_IDENTITY_APPROVAL=approved-production-erhua-member-safe-identity \
  qintopia-message-sidecar erhua-member-safe-identity \
    --payload-file <safe-identity.json> \
    --apply
```

When the coverage checker reports a linked person with no safe answer-context canary
name, add a reviewed alias through the release-local sidecar instead of hand-editing
Postgres:

```bash
node tools/deploy/build-erhua-member-safe-alias-payload-template.mjs \
  --coverage <identity-bootstrap-dry-run-output.json> \
  --output <safe-alias-template.json>

node tools/deploy/check-erhua-member-safe-alias-payload.mjs \
  <safe-alias.json>

qintopia-message-sidecar erhua-member-safe-alias \
  --payload-file <safe-alias.json>

QINTOPIA_ERHUA_MEMBER_SAFE_ALIAS_APPROVAL=approved-production-erhua-member-safe-alias \
  qintopia-message-sidecar erhua-member-safe-alias \
    --payload-file <safe-alias.json> \
    --apply
```

Build sanitized Erhua answer-context canary evidence from captured MCP responses:

```bash
qintopia-message-sidecar erhua-member-speaker-canary-sender-map \
  --chat-id <reviewed-erhua-qiwe-group-id> \
  > <private-speaker-sender-map.json>

node tools/deploy/build-erhua-member-recognition-canary-mcp-input.mjs \
  --spec <identity-bootstrap-dry-run-output.json> \
  --chat-id-env <canary-chat-id-env> \
  --sender-id-env <canary-sender-id-env> \
  --speaker-sender-map <private-speaker-sender-map.json> \
  --output <context-mcp-input.jsonl>

node tools/deploy/build-erhua-member-recognition-canary-evidence.mjs \
  --spec <identity-bootstrap-dry-run-output.json> \
  --mcp-output <context-mcp-output.jsonl> \
  --output <answer-context-canary-output.jsonl>
```

Validate post-repair Erhua answer-context canaries:

```bash
node tools/deploy/check-erhua-member-recognition-canary.mjs \
  <answer-context-canary-output.jsonl>

node tools/deploy/finalize-erhua-member-recognition-completion.mjs \
  --room-sync <identity-backfill-room-member-sync-apply-output.json> \
  --profile <member-profile-quiet-apply-output.json> \
  --coverage <identity-bootstrap-dry-run-output.json> \
  --canary <answer-context-canary-output.jsonl> \
  --summary-output <sanitized-completion-summary.json> \
  --require-active-profiles
```

The completion checker requires applied room-sync evidence, quiet single-scope
member-profile evidence, and matching scope fingerprints between room-sync, profile, and
bootstrap coverage evidence, so wrong-room, multi-room, or message-sender-only coverage
cannot be treated as current-group recognition. The finalizer runs
`check-erhua-member-recognition-completion.mjs` and then
`check-erhua-member-recognition-completion-summary.mjs`; when `--summary-output` is
used, the written summary contains only non-sensitive scope/count fields and the
`explicit no-secret` boundary flags. Use `--require-active-profiles` for the reviewed
production completion claim; it fails when any linked current-room person is still
missing an active `reply_context` profile, when the quiet member-profile
`current_room_linked_people` count does not match coverage `linked_people_total`, or
when any canary resolves only as `identity_only`. Resolved non-identity-only canaries
must have non-empty safe profile hints; members without useful profile signals are
represented by active no-stable-profile snapshots with an explicit do-not-infer hint
instead of empty profiles. The retained summary records only route-level hint coverage
counts such as `linked_profile_hint_people`, not profile text, and the mentioned-member,
speaker self, and referenced-member hint coverage counts must agree. The checker also
requires `qiwe_room_channel_identities_raw_total` to equal the applied room-sync
`room_members_discovered`, so the current-room roster denominator cannot be understated
or overstated by stale rows. The applied sync marks current roster identities with
`current_qiwe_room_member=true` and same-room historical identities that are no longer
in the roster with stale metadata; bootstrap same-chat coverage counts only the current
marker. The room-scoped safe/excluded/linked identity counts must not be inflated by
`chat_id=''` platform identities. Coverage also emits
`qiwe_room_potential_member_identities_*`; that denominator excludes only
bot/system/test identities, so current-room people with unsafe display names such as
phone-like digit runs cannot disappear into `excluded`. Completion requires
`qiwe_room_potential_member_identities_unlinked = 0`, and the retained completion
summary must also show `unsafe_display_unlinked = 0`; resolve any redacted
`identity_key` samples through an owner-reviewed identity path before claiming full
recognition. It also requires room-scoped `linked_people_total` and
`answer_context_canary_specs`, plus `linked_people_without_qiwe_platform_identity = 0`,
plus `answer_context_speaker_canary_specs` resolving every linked current-room person
through sanitized `speaker` output, plus `answer_context_referenced_canary_specs`
resolving every linked current-room person through sanitized `referenced_member` output.
The retained completion summary also records `qiwe_speaker_identities` and requires
`platform_identities_missing = 0` plus `ambiguous_users = 0`, so self-identification is
proven at the QiWe user lookup boundary rather than only at the person aggregate.
Retained canary records must exactly match the coverage report's `expected_mention` /
`expected_speaker_label` / `expected_referenced_label`, `canonical_key`, and
`required_profile_terms`, so a stale or manually shortened canary list cannot satisfy
completion. Retained canary evidence must include mentioned-member, speaker self-canary,
and referenced-member records, and all three paths must resolve the same people. The
evidence builder converts raw answer-context `person_id` values into irreversible
`person_ref` SHA-256 markers before retention; retained canary JSONL must not contain
`person_id`. The private speaker sender map and MCP input contain raw sender ids; keep
them in server-local `/tmp` only, require its `scope_fingerprint` and canonical-key set
to match the coverage report's speaker/referenced canary people, and retain only
sanitized canary JSONL/checker output as evidence. For one-shot member-recognition
repair, capture the profile evidence from
`member-profile --chat-id <reviewed-erhua-qiwe-group-id> --limit 5000 --apply --quiet`
so older self-introductions, interests, and recurring activity signals are considered;
the completion checker requires the quiet report's `requested_message_limit` to be at
least `5000` and its `current_room_linked_people` to match the current-room linked
person denominator. The quiet report also records `baseline_profile_targets` and
`baseline_profiles_inserted` for linked members that needed a safe no-stable-profile
snapshot. Canary specs may include `required_profile_terms` for safe concrete profile
signals such as `跑步`, `摄影`, `AI`, and `写作`; those terms must appear in the
sanitized answer-context summary or hints. The canary checker also requires returned
`mentioned_members[].mention_text` to exactly match each `expected_mention`, so a
similar display name cannot satisfy the evidence. Resolved canaries must have
`match_count = 1`; non-unique member-name matches are not full recognition. Pronoun-only
questions such as "他是谁" require the channel adapter to pass `referenced_sender_id`
from the replied-to message and are not satisfied by vector search or recent-message
proximity guesses.

Use it before any owner-operated Huabaosi production canary, QiWe companion
verification, real-activity evidence export, or final completion-manifest capture.

After the owner has retained the reviewed staging and production evidence files, build
and validate the final Xiaoman production completion manifest in one step:

```bash
pnpm deploy:xiaoman-production-evidence:finalize -- \
  --release-please-pr-number <release-please-pr-number> \
  --release-please-head-sha <release-please-head-sha> \
  --release-tag <published-release-tag> \
  --released-commit-sha <published-release-commit-sha> \
  --qiwe-production-enablement-pr-number <qiwe-production-enablement-pr-number> \
  --qiwe-production-enablement-head-sha <qiwe-production-enablement-head-sha> \
  --staging-runtime-readiness <staging-runtime-readiness-output.txt> \
  --huabaosi-staging <huabaosi-staging-output.txt> \
  --qiwe-staging <qiwe-staging-output.txt> \
  --huabaosi-production-canary <huabaosi-production-canary-output.txt> \
  --production-real-activity <production-evidence-output.txt> \
  --qiwe-group-arrival-confirmation <qiwe-group-arrival-confirmation-output.txt> \
  --output <completed-xiaoman-production-completion-evidence.json>
```

## GitHub App Git Access

Validate the GitHub App git wrapper without credentials:

```bash
pnpm deploy:github-app-git:check
```

Run git commands against the private repository with a short-lived installation token:

```bash
GITHUB_APP_ID=4214034 \
GITHUB_APP_INSTALLATION_ID=144332887 \
GITHUB_APP_PRIVATE_KEY_PATH=/etc/qintopia/github-app/qintopia-agent-os-deployer.pem \
deploy/sidecar/scripts/github-app-git.sh -- \
  ls-remote https://github.com/qintopia-agent-studio/qintopia-agent-os.git refs/heads/master
```

The wrapper uses a temporary `GIT_ASKPASS` helper and never writes the token into the
remote URL, git config, or command arguments.

## Artifact Build

Build the sidecar CI artifact layout locally:

```bash
pnpm artifact:sidecar
```

The command writes `dist/sidecar-artifacts/qintopia-message-sidecar-linux-x86_64-gnu`
with the release binary, compressed bundle, `artifact-manifest.json`, and `SHA256SUMS`
covering all three payload files. `dist/` is ignored by git.

Build the independent QiWe production sidecar artifact layout locally:

```bash
pnpm artifact:sidecar:qiwe-production
```

This writes
`dist/sidecar-artifacts/qintopia-message-sidecar-qiwe-production-linux-x86_64-gnu` with
manifest profile `qiwe-production` and exactly `qiwe-production-adapter` plus
`huabaosi-feishu-mirror-adapter`. Production promotion installs it only at
`sidecar-profiles/qiwe-production`; deploy requests keep
`runtime_artifact_profile=huabaosi-production` for the primary runtime.

The CI artifact job uses Rust 1.96.0 to match `runtime/sidecar/Cargo.toml`
`rust-version`. Server deployment downloads the uploaded artifact and does not require
Node.js, pnpm, Rust, or Docker on the production host.

Build the staging-only sidecar artifact layout locally:

```bash
pnpm artifact:sidecar:staging
```

This writes `dist/sidecar-artifacts/qintopia-message-sidecar-staging-linux-x86_64-gnu`
with a manifest compiled only with `huabaosi-staging-adapter` and
`qiwe-staging-adapter`. It is for owner-approved staging evidence under
`/home/ubuntu/qintopia-agent-os-staging-releases/<sha>` only; production deployment
scripts must keep using `pnpm artifact:sidecar`.

## Artifact Retention

Prune old GitHub Actions sidecar artifacts:

```bash
GITHUB_TOKEN="replace-with-actions-write-token" \
GITHUB_REPOSITORY="qintopia-agent-studio/qintopia-agent-os" \
pnpm artifact:prune:sidecar
```

The command keeps the latest ten artifacts named
`qintopia-message-sidecar-linux-x86_64-gnu` by default. Override the count with
`QINTOPIA_ARTIFACT_KEEP_COUNT` or `--keep <count>`. Older same-name artifacts are
deleted through the GitHub Actions Artifacts API.

Prune old independent QiWe production sidecar artifacts:

```bash
GITHUB_TOKEN="replace-with-actions-write-token" \
GITHUB_REPOSITORY="qintopia-agent-studio/qintopia-agent-os" \
pnpm artifact:prune:sidecar:qiwe-production
```

That command keeps the latest ten artifacts named
`qintopia-message-sidecar-qiwe-production-linux-x86_64-gnu` by default.
