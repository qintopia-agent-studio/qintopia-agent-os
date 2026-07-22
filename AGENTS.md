# Project Instructions

## Map

- Human entrypoint: `README.md`
- Agent-facing rules: `AGENTS.md`
- Claude Code rules: `CLAUDE.md`
- Documentation hub: `docs/README.md`
- Architecture overview: `docs/architecture/agent-os-overview.md`
- Product scope: `docs/product/agent-os-prd.md`
- Agent OS design: `docs/agent-os/README.md`
- Runtime baseline: `docs/operations/runtime-baseline.md`
- Collaboration model: `docs/engineering/collaboration-model.md`
- Migration policy: `docs/engineering/migration-policy.md`
- Server change policy: `docs/engineering/server-change-policy.md`
- Programming agent guardrails: `docs/engineering/programming-agent-guardrails.md`
- Change routing index: `docs/engineering/change-routing-index.md`
- Current roadmap: `docs/plans/active/current-roadmap.md`
- Source document inventory: `docs/operations/source-document-inventory.md`
- Registry indexes: `registry/`
- Agent packages: `agents/`
- Skill packages: `skills/`
- Workflow packages: `workflows/`
- MCP adapters: `mcp/`
- Runtime templates: `runtime/`
- Deployment scripts and manifests: `deploy/`
- Engineering docs: `docs/engineering/`
- Operations docs: `docs/operations/`
- Fixtures and replay data: `fixtures/`
- Historical POC material: `deprecated/`

## Commands

- Install dependencies: `pnpm install`
- Format: `pnpm format`
- Pre-commit quick checks: `.husky/pre-commit`
- Repository check: `pnpm check`
- Markdown lint: `pnpm lint:md`
- PR readiness: `pnpm pr:doctor`
- PR body validation: `pnpm pr:check-body`
- PR creation: `pnpm pr:create -- --body-file <completed-pr-body.md>`
- Release Please PR manual CI validation:
  `gh workflow run ci.yml --ref <release-please-head-branch> -f release_please_pr_number=<pr-number>`
- Staging-only sidecar artifact for Huabaosi/QiWe evidence smokes:
  `pnpm artifact:sidecar:staging`
- If the local pnpm version shim cannot verify a registry signature, do not set
  `pmOnFail=ignore`. Confirm the exact `package.json` script first; when it is a fixed
  repository-local Node entrypoint, run that entrypoint directly and record the failed
  pnpm validation attempt.
- Xiaoman activity signal timer observation smoke:
  `QINTOPIA_XIAOMAN_ACTIVITY_SIGNAL_TIMER_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/xiaoman-activity-signal-timer-observation-smoke.sh`
- Xiaoman activity promotion starter timer observation smoke:
  `QINTOPIA_XIAOMAN_ACTIVITY_PROMOTION_STARTER_TIMER_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/xiaoman-activity-promotion-starter-timer-observation-smoke.sh`
- Xiaoman activity downstream observation smoke:
  `QINTOPIA_XIAOMAN_ACTIVITY_DOWNSTREAM_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/xiaoman-activity-downstream-observation-smoke.sh`
- Xiaoman activity send request starter observation smoke:
  `QINTOPIA_XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/xiaoman-activity-send-request-starter-observation-smoke.sh`
- Xiaoman activity image generation starter observation smoke:
  `QINTOPIA_XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/xiaoman-activity-image-generation-starter-observation-smoke.sh`
- Staging runtime values metadata observation smoke:
  `QINTOPIA_STAGING_RUNTIME_VALUES_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/staging-runtime-values-observation-smoke.sh`
- Huabaosi image generation staging readiness smoke:

  ```bash
  QINTOPIA_HUABAOSI_IMAGE_STAGING_READINESS_ENABLE=1 \
  QINTOPIA_HUABAOSI_IMAGE_STAGING_APPROVAL=approved-staging-image-generation \
  QINTOPIA_HUABAOSI_IMAGE_STAGING_RELEASE_SHA=<approved-staging-release-sha> \
  QINTOPIA_HUABAOSI_IMAGE_STAGING_SIDECAR_SHA256=<approved-staging-sidecar-sha256> \
  deploy/sidecar/scripts/huabaosi-image-generation-staging-readiness-smoke.sh
  ```

- Huabaosi image generation production state observation smoke:
  `QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/huabaosi-image-generation-production-observation-smoke.sh`
- Huabaosi image generation one-shot production canary:

  ```bash
  QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_ENABLE=1 \
  QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_APPROVAL=approved-production-image-generation-canary \
  QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_BRIEF_ARTIFACT_ID=<pending-poster-brief-uuid> \
  QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_DATABASE_URL_SHA256=<approved-database-url-sha256> \
  QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_RELEASE_SHA=<approved-release-sha> \
  QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_SIDECAR_SHA256=<approved-sidecar-sha256> \
    deploy/sidecar/scripts/huabaosi-image-generation-production-canary-smoke.sh
  ```

- Huabaosi image generation production canary evidence validation:
  `node tools/deploy/check-huabaosi-image-production-canary-evidence.mjs <production-canary-output.txt>`
- Huabaosi Feishu-backed generated-image read-only revalidation:
  `qintopia-message-sidecar huabaosi-feishu-primary-storage-revalidate --artifact-id <generated-image-uuid>`
- Huabaosi generated-image Feishu mirror production observation smoke:
  `QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/huabaosi-feishu-artifact-mirror-production-observation-smoke.sh`
- Huabaosi generated-image Feishu mirror activation is guarded, not automatic. It
  requires the persistent mirror flag to be present exactly once and set to `1`, then
  runs the release-local preflight service before enabling the dedicated timer:
  `QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_ACTIVATION=approved-production-huabaosi-feishu-artifact-mirror deploy/sidecar/scripts/activate-huabaosi-feishu-artifact-mirror-production.sh`
- Huabaosi generated-image Feishu mirror immediate timer rollback:
  `QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_ROLLBACK=approved-production-huabaosi-feishu-artifact-mirror-rollback deploy/sidecar/scripts/rollback-huabaosi-feishu-artifact-mirror-production.sh`
- Huabaosi image generation production activation after manual Release publish:
  `QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_ACTIVATION=approved-production-image-generation deploy/sidecar/scripts/activate-huabaosi-image-generation-production.sh`
- Huabaosi image generation immediate timer rollback:
  `QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_ROLLBACK=approved-production-image-generation-rollback deploy/sidecar/scripts/rollback-huabaosi-image-generation-production.sh`
- Huabaosi WeCom gateway read-only observation smoke:
  `QINTOPIA_HUABAOSI_WECOM_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/huabaosi-wecom-gateway-observation-smoke.sh`
- Huabaosi WeCom canary disabled-state observation smoke:
  `QINTOPIA_HUABAOSI_WECOM_CANARY_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/huabaosi-wecom-canary-observation-smoke.sh`
- QiWe image-send staging readiness smoke:

  ```bash
  QINTOPIA_QIWE_IMAGE_STAGING_READINESS_ENABLE=1 \
  QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL=approved-staging-qiwe-image-send \
  QINTOPIA_QIWE_IMAGE_STAGING_RELEASE_SHA=<approved-staging-release-sha> \
  QINTOPIA_QIWE_IMAGE_STAGING_SIDECAR_SHA256=<approved-staging-sidecar-sha256> \
    deploy/sidecar/scripts/qiwe-image-send-staging-readiness-smoke.sh
  ```

- QiWe image-send production observation smoke:
  `QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/qiwe-image-send-production-observation-smoke.sh`
- QiWe image callback bridge production observation smoke:
  `QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/qiwe-image-callback-bridge-production-observation-smoke.sh`
- QiWe image callback bridge production activation after manual Release publish and
  persistent Erhua env approval. The activation script validates the bridge is already
  bound to release/current, production mode, the approved sidecar SHA-256, and the
  approved production database URL hash before restarting Erhua; it must not enable
  timers, process callbacks, call QiWe, or source env files:
  `QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_ACTIVATION=approved-production-qiwe-image-callback-bridge deploy/sidecar/scripts/activate-qiwe-image-callback-bridge-production.sh`
- QiWe image callback bridge immediate rollback after persistent Erhua env disables the
  bridge:
  `QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_ROLLBACK=approved-production-qiwe-image-callback-bridge-rollback deploy/sidecar/scripts/rollback-qiwe-image-callback-bridge-production.sh`
- QiWe image-send production activation after manual Release publish and production env
  approval. The activation script must fail before preflight or timer changes unless the
  persistent `QINTOPIA_SIDECAR_DATABASE_URL` hashes to the approved
  `QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256`; never weaken this to a
  format-only hash check. Activation and rollback must read the fixed reviewed
  `/etc/qintopia/message-sidecar.env` and must not accept env-file or systemctl command
  overrides from the caller; use a fixed system PATH and absolute systemctl path:
  `QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_ACTIVATION=approved-production-qiwe-image-send deploy/sidecar/scripts/activate-qiwe-image-send-production.sh`
- QiWe image-send immediate timer rollback:
  `QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_ROLLBACK=approved-production-qiwe-image-send-rollback deploy/sidecar/scripts/rollback-qiwe-image-send-production.sh`
- Real Xiaoman activity production evidence export after owner-confirmed completion:

  ```bash
  QINTOPIA_XIAOMAN_REAL_ACTIVITY_PRODUCTION_SIDECAR_SHA256=<approved-production-sidecar-sha256> \
  QINTOPIA_XIAOMAN_REAL_ACTIVITY_PRODUCTION_DATABASE_URL_SHA256=<approved-production-database-url-sha256> \
  qintopia-message-sidecar xiaoman-real-activity-production-evidence \
    --workflow-root-id <completed-xiaoman-activity-root-uuid> > production-evidence-output.txt
  ```

- Real Xiaoman activity production evidence validation:
  `node tools/deploy/check-xiaoman-real-activity-production-evidence.mjs <production-evidence-output.txt>`
- Xiaoman QiWe group-arrival human confirmation evidence validation after a real
  activity send:
  `node tools/deploy/check-xiaoman-qiwe-group-arrival-confirmation-evidence.mjs <production-evidence-output.txt> <qiwe-group-arrival-confirmation-output.txt>`
- Build the non-secret Xiaoman production completion manifest after the Huabaosi canary,
  real-activity, and QiWe group-arrival evidence checks pass. Run it where `gh` can
  verify the Release Please PR, QiWe production enablement PR, and published release
  commit facts:

  ```bash
  node tools/deploy/build-xiaoman-production-completion-manifest.mjs \
    --release-please-pr-number <release-please-pr-number> \
    --release-please-head-sha <release-please-head-sha> \
    --release-tag <published-release-tag> \
    --released-commit-sha <published-release-commit-sha> \
    --qiwe-production-enablement-pr-number <qiwe-production-enablement-pr-number> \
    --qiwe-production-enablement-head-sha <qiwe-production-enablement-head-sha> \
    --huabaosi-production-canary <production-canary-output.txt> \
    --production-real-activity <production-evidence-output.txt> \
    --qiwe-group-arrival-confirmation <qiwe-group-arrival-confirmation-output.txt> \
    --output <completed-xiaoman-production-completion-evidence.json>
  ```

- Full Xiaoman production completion evidence validation after all completion gates have
  retained sanitized evidence:

  ```bash
  node tools/deploy/check-xiaoman-production-completion-evidence.mjs \
    --manifest <completed-xiaoman-production-completion-evidence.json> \
    --staging-runtime-readiness <staging-runtime-readiness-output.txt> \
    --huabaosi-staging <huabaosi-staging-output.txt> \
    --qiwe-staging <qiwe-staging-output.txt> \
    --huabaosi-production-canary <huabaosi-production-canary-output.txt> \
    --production-real-activity <production-evidence-output.txt> \
    --qiwe-group-arrival-confirmation <qiwe-group-arrival-confirmation-output.txt>
  ```

- Combined Huabaosi/QiWe staging runtime readiness evidence:

  ```bash
  QINTOPIA_STAGING_RUNTIME_READINESS_EVIDENCE_ENABLE=1 \
  QINTOPIA_STAGING_RUNTIME_RELEASE_SHA=<approved-staging-release-sha> \
  QINTOPIA_STAGING_RUNTIME_SIDECAR_SHA256=<approved-staging-sidecar-sha256> \
  QINTOPIA_STAGING_RUNTIME_DATABASE_URL_SHA256=<approved-staging-database-url-sha256> \
    deploy/sidecar/scripts/staging-runtime-readiness-evidence-smoke.sh
  ```

- Huabaosi WeCom shadow capture fixture replay:
  `cargo test --manifest-path runtime/sidecar/Cargo.toml huabaosi_wecom_shadow`
- Huabaosi WeCom policy preview fixture replay:
  `cargo test --manifest-path runtime/sidecar/Cargo.toml huabaosi_wecom_policy`
- Huabaosi WeCom canary gateway fixture replay:
  `cargo test --manifest-path runtime/sidecar/Cargo.toml huabaosi_wecom_canary`
- Xiaoman activity production preflight smoke:
  `QINTOPIA_XIAOMAN_ACTIVITY_PRODUCTION_PREFLIGHT_ENABLE=1 deploy/sidecar/scripts/xiaoman-activity-production-preflight-smoke.sh`
- Xiaoman legacy Hermes cron observation smoke:
  `QINTOPIA_XIAOMAN_LEGACY_CRON_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/xiaoman-legacy-cron-observation-smoke.sh`
- AgentOS downstream evidence/visual timers observation smoke:
  `QINTOPIA_OPERATIONS_DOWNSTREAM_TIMERS_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/operations-downstream-timers-observation-smoke.sh`
- Sidecar dependency vulnerability audit:
  `cd runtime/sidecar && cargo deny check advisories bans sources`. A full
  `cargo deny check` currently fails license checks because the repository has no
  `deny.toml` license policy; do not treat that as unresolved RustSec advisories.

Use `rg` and `rg --files` for search.

## Core Rules

- Organize by Agent OS capability, not by programming language.
- Rust, Python, TypeScript, shell, and SQL are implementation details inside a package.
- Do not create top-level `python/`, `rust/`, `typescript/`, or similar language
  buckets.
- On macOS, run the complete sidecar unit suite with
  `RUST_MIN_STACK=33554432 cargo test --manifest-path runtime/sidecar/Cargo.toml`. The
  default test-thread stack can overflow in an existing Xiaoman async test; see
  `docs/reports/2026-07-13-rust-test-stack-limit.md`.
- The ignored `group_message_send` PostgreSQL integration test may run only with
  `QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE=1` against a database named exactly
  `qintopia_test` on loopback and with the explicit Cargo feature
  `postgres-integration-tests`. It validates internal send-ready state and must never
  call QiWe or an external adapter.
- A `group_message_send` claim must clear `claimed_by`, `locked_at`, and
  `claim_expires_at` together when it records send-ready or policy-denied state. The
  transition must update exactly the locked work item before appending its audit event.
- Do not develop directly on `master`; create a feature branch first.
- Document first for new features, behavior changes, migrations, runtime changes, or
  production-adjacent work.
- Use Conventional Commits for commit messages. Allowed types are `build`, `chore`,
  `ci`, `docs`, `feat`, `fix`, `perf`, `refactor`, `revert`, `style`, and `test`.
- Do not manually edit root `CHANGELOG.md` in ordinary feature or fix PRs. Release
  Please owns routine release changelog updates from merged Conventional Commits.
- Merging a Release Please PR prepares a version and draft GitHub Release. Production
  deployment still requires the owner to manually publish that draft Release.
- Release Please PRs and draft GitHub Releases must be merged or published only through
  an explicit manual owner decision. Do not enable or use auto-merge, automatic release
  publishing, or bot/agent-driven release merging.
- Do not describe a Xiaoman-adjacent Release as production-complete unless
  `docs/plans/active/xiaoman-production-completion-gate.md` is satisfied. Infrastructure
  or activation-ready Releases may ship staging/provisioning/deploy tooling, but they
  must not be treated as the usable activity-to-QiWe group-send workflow.
- Before publishing a draft GitHub Release, confirm its tag points to current
  `origin/master`. If `master` advanced after the draft was prepared, do not publish or
  retry the stale tag; validate and publish the next Release Please PR instead.
- Do not merge a Release Please PR unless the draft GitHub Release will be published or
  intentionally deleted in the same release decision. The repository release manifest
  must track the latest published Release tag; deleted draft-only releases must not
  remain as the Release Please baseline.
- A Release Please PR created or updated with `GITHUB_TOKEN` may have no automatic PR
  checks because GitHub suppresses recursive workflow triggers. Before merging such a
  PR, run the manual CI validation command on its exact head branch and require the
  workflow `changes` and `check` jobs plus the PR-attached `Release Please validation`
  commit status to pass. The dispatch must fail if the PR is not open, does not target
  `master`, is not bot-authored, or the checked-out SHA differs from the PR head.
- Do not hand humans a prefilled GitHub compare URL as the normal PR flow. Use
  `pnpm pr:doctor`, then `pnpm pr:create` with a completed PR body. If GitHub CLI is
  missing, run `pnpm pr:bootstrap`.
- In the Codex desktop environment, do not run extra GitHub authentication checks before
  creating a PR. Use `pnpm pr:create` directly after PR readiness checks; only handle
  authentication when the actual push or PR creation command fails.
- In Codex sandboxed command execution, a repo-owned Node PR script can fail when its
  child `gh` process reaches `api.github.com` even though a top-level `gh pr ...`
  command works. Treat that as sandbox network permission, not an auth failure; rerun
  the repo-owned PR entrypoint with network approval instead of re-authenticating `gh`.
- PR-Agent must not automatically edit PR descriptions. The completed repository PR
  template is author-owned because CI validates its required sections.
- Before merging any PR, read the complete PR Reviewer Guide, submitted reviews,
  conversation comments, and inline review threads for the latest head SHA. A green
  PR-Agent check is not sufficient. Resolve every security concern and recommended
  review item in code or record an explicit disposition, then wait for replacement CI
  and review results before merge.
- Do not introduce Java, Gradle, Maven, Kotlin, Go, Swift, C#, PHP, Ruby, Elixir, or a
  new language/toolchain stack without an explicit owner-approved architecture decision.
- Do not hot-edit production servers.
- Any script expected to exist under `/home/ubuntu/qintopia-agent-os-releases/current`
  after deployment must be included in `tools/deploy/build-deploy-bundle.mjs` and
  guarded by `tools/deploy/check-deploy-contracts.mjs`; adding a repo file alone does
  not put it on `paxon-server`.
- Production COS fetch must leave `artifact-manifest.json`, `SHA256SUMS`, and packaged
  archives mode `0444`, while the sidecar binary remains `0755`. These files are
  immutable non-secret release evidence needed by unprivileged release-local
  observation; mode `0640` can make a valid release unverifiable after root-owned
  promotion.
- Production COS archive extraction runs under the root deploy runner and must use
  `tar --no-same-owner` for both sidecar and deploy-bundle payloads. Never preserve
  GitHub runner numeric owners from an artifact archive or propagate them into the
  immutable production release with `cp -a`; the promoted release tree must remain owned
  by the deploy runner.
- A production same-SHA follow-up may repair owner and mode metadata only after the
  existing manifest identity matches the request, the complete release tree matches
  freshly fetched verified artifacts, and both packaged checksum files pass. It must
  fail before metadata mutation on any content or path drift; do not hot-fix release
  ownership with server-side `chown` or `chmod` outside this reviewed runner path.
- Staging sidecar provisioning runs as the `ubuntu` operator, not root. It must create
  the fixed staging release root, release directory, and sidecar directory with explicit
  mode `0755` independent of ambient `umask`, then freeze the immutable release and
  sidecar directories to `0555`. Failed attempts may remove only paths they created;
  they must not reuse or delete an existing release directory.
- The staging-only sidecar artifact must package the exact Huabaosi and QiWe staging
  smoke runners with manifest and checksum identities. Provision them under the same
  immutable staging release at `deploy/sidecar/scripts/`; a real staging smoke must not
  fall back to a mutable checkout or use test mode because the runner is absent.
- Do not copy secrets, live `.env` files, tokens, table ids, private chat logs, raw
  member profiles, or server-only runtime state into git.
- WorkTool is not a Qintopia Agent OS channel for new work. Treat WorkTool and the
  WorkTool Hermes plugin as deprecated or audit-only material.
- Hermes Kanban is not the future task/orchestration backbone. Do not build new
  workflows on Hermes Kanban.
- Postgres/AgentOS is the system fact source. Feishu is a human workbench and mirror,
  not the source of truth.
- For the Huabaosi production image canary, the owner-selected first storage boundary is
  the fixed Feishu Base `huabaosi-generated-image-v1` table. The image worker may upload
  the exact final JPEG attachment and idempotently upsert one row by
  `generated_image_artifact_id`; it must read the uploaded bytes back through the
  authenticated Feishu media API and verify the complete JPEG identity before creating a
  pending AgentOS artifact. Do not require a separate media upload/public URL service
  for this Feishu-backed canary. The production image storage backend must be
  `QINTOPIA_HUABAOSI_IMAGE_STORAGE_BACKEND=feishu-base`; the generated-image table id
  comes from the owner-provided Feishu URL's `table` query parameter and must not be
  committed to git. `QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED=1` is required by the
  current production binary for Feishu-backed storage validation, but it does not enable
  the mirror worker timer by itself. Feishu automation may notify reviewers or mirror
  reviewed status after the row exists, but it must not generate images, approve
  artifacts, become the fact source, call QiWe, or publish.
- Huabaosi generated-image Feishu mirroring must use the fixed
  `huabaosi-generated-image-v1` artifact-version schema and key idempotency by
  `generated_image_artifact_id`. It may mirror only a fully revalidated immutable final
  JPEG and sanitized review metadata. It must not update the legacy poster task summary
  without a stable AgentOS workflow id, treat Feishu state as approval, call QiWe, or
  publish. Production artifacts compile exactly `huabaosi-production-adapter` plus the
  guarded `huabaosi-feishu-mirror-adapter`; staging, QiWe, and all-features production
  artifacts remain forbidden. The ordinary release installer may install the dedicated
  mirror preflight, worker, and timer units, but must not enable the external write
  timer automatically.
- Huabaosi Feishu mirror apply must validate the exact owner phrase, production release
  SHA binding, database URL hash, Base and table exact allowlists, fixed schema version,
  Huabaosi profile path, and media host allowlist before Postgres or external I/O. The
  production observation may run only the non-secret mirror observation preflight; it
  must not run full configuration preflight, preview the queue, upload media, write
  Feishu/Postgres, approve, publish, call QiWe, or send.
- Huabaosi Feishu primary-storage apply must reuse the bounded Rust Feishu client and
  the same exact Base/table allowlists, schema version, profile path, production release
  SHA, and database URL hash gates as the reviewed mirror. Feishu attachment tokens and
  credentials are memory-only and must not appear in Postgres metadata, reports, logs,
  CLI arguments, or environment-derived output. A failed or ambiguous Feishu write must
  not create a pending artifact or be retried automatically as if no external write
  occurred.
- A Feishu-backed image canary may cross from `pending` to `approved` only through an
  explicit human apply that first completes authenticated Feishu attachment revalidation
  and then matches the memory-only evidence against the transaction-locked Postgres
  artifact identity. Rejection and changes-requested decisions must not require Feishu
  I/O. Feishu fields, automation, and workbench events alone must never approve. QiWe
  intake must continue to fail closed for `feishu-base://` until a separate reviewed
  delivery path exists. The `huabaosi-feishu-primary-storage-revalidate` sidecar command
  is read-only evidence: it may authenticate to Feishu, reload the fixed record by
  generated-image artifact id, download the `最终JPEG` attachment, and emit a sanitized
  report, but it must not approve, write Postgres or Feishu, call QiWe, publish, send,
  or expose attachment tokens/record ids/Base/table ids/credentials. The current QiWe
  async upload contract requires a stable allowlisted HTTPS `fileUrl`; do not bridge
  Feishu private attachments by exposing Feishu attachment tokens, storing a private
  media URL, introducing an unreviewed public proxy/upload service, or falling back to
  QiWe synchronous upload APIs marked deprecated in the reviewed protocol plan.
- Huabaosi Feishu production observation must discover the immutable
  `release/current/sidecar/qintopia-message-sidecar` binary, or accept an explicit
  `QINTOPIA_SIDECAR_BIN` only when it resolves to that same release-local binary with
  the approved production features; it must fail closed instead of falling back to
  `cargo run` or a mutable source tree. Its shell may parse only the mirror enable flag;
  the child launcher may pass only that parsed flag and the non-secret release SHA to
  the immutable binary, without `source`, `eval`, command substitution, shell secret
  import, or a secret-bearing temporary file. It must not pass database URL, Base token,
  table id, Feishu token, profile env path, or allowlist values to the child process.
  Ignore non-allowlisted env values before applying mirror-flag value validation.
  Activation must fail before preflight or timer changes unless the persistent mirror
  enable flag is present exactly once and exactly `1`. Rollback must stop the timer
  first and may report completion only after that flag is present exactly once and
  exactly `0` in the reviewed sidecar environment file.
- Hermes remains the Agent runtime. It should not become the business database.
- `agents/xiaoman/profile-bundle` is observation-only. It may package the reviewed
  `SOUL.md`/`profile.yaml` templates, strict renderer, fake fixtures, and read-only
  parity smoke, but the deploy runner must not render it, read its server-local values,
  create live profile symlinks, or restart Xiaoman for it until a separate cutover PR
  records production parity and first-cutover rollback. Keep Xiaoman `config.yaml`,
  webhook secrets, channel identifiers, cron state, `.env`, sessions, auth, messages,
  memories, logs, cache, locks, and databases out of the bundle.
- `agents/xiaoman/profile-bundle/migrate_values.py --apply` is a one-time manual
  observation prerequisite. It must require root and the exact owner approval before
  reading the fixed live files, lock both reviewed source hashes, validate exactly four
  values, prove complete rendered parity, and no-clobber create only the root-owned mode
  `0600` `/etc/qintopia/xiaoman-profile-bundle-values.json`. It must not accept path
  overrides, print values, edit the live profile, create symlinks, restart Hermes, use
  the network, write Postgres/Feishu, call external adapters, publish, or send. The
  deploy runner must never invoke it automatically.
- Xiaoman activity signal intake uses `xiaoman-activity signal-ingest` to create
  `xiaoman.create_activity_request` through the operations control plane with
  `requester_agent=default` and `target_agent=xiaoman`; do not bypass capability policy
  by making Xiaoman call its own provider capability directly.
- `qintopia_xiaoman_activity_list_by_date` may execute read-through only when
  `QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE=1`. In that mode it may run the
  configured sidecar for read-only, non-dry-run queries and return sanitized
  `record_count`, `records`, and `summaries`; write wrappers must continue to return
  bounded worker commands.
- `qintopia_xiaoman_activity_promotion_review_draft` may only transform already-read
  sanitized Xiaoman activity records into a human-reviewable activity summary, promotion
  assessment, copy draft, poster brief, and dry-run controlled record-path payload. It
  must not read Feishu, write Postgres, call Huabaosi, queue or send QiWe messages,
  publish, or skip human confirmation. Hermes remains the runtime caller, not the
  business fact source.
- `qintopia_xiaoman_activity_text_group_message_request_prepare` may only prepare an
  `operations-create` command for an `erhua.send_group_message` /
  `group_message_request` from an approved text announcement artifact. It must require
  `approved_artifact_id`, bind `message_text` to the approved artifact `content_hash`,
  keep the request before human final confirmation, and must not queue, run send-ready,
  call Erhua, call QiWe, publish, send, or accept raw group ids, URLs, Feishu/Base ids,
  secrets, or unapproved text. Text announcement MVP evidence must not be used as
  Xiaoman production-complete evidence or as proof of QiWe group delivery; production
  completion still requires the image/send-ready/QiWe arrival evidence checkers.
- Xiaoman activity lifecycle phase is a Postgres `event_signals` fact. Allowed values
  are `pre_event`, `in_event`, and `post_event`; transitions are forward-only and each
  phase maps to its fixed root/child route. Event-signal root creation must lock and
  match the current phase. Do not accept caller-selected routes, rewrite historical
  phase roots, add a timer, or extend `in_event` routing into visual/image generation,
  Feishu writeback, QiWe send, or publishing. `post_event` may use the reviewed internal
  starter path from approved recap brief to image-generation request and then approved
  generated image to awaiting-publish group-message request; those starters must not
  call providers, write Feishu, confirm, queue, publish, call QiWe, or send.
- Xiaoman signal apply smokes should use sanitized non-UUID event signal ids unless a
  matching `qintopia_agent_os.event_signals` row is created first; UUID
  `event_signal_id` values are stored as `source_event_signal_id` and must satisfy the
  Postgres foreign key.
- Xiaoman `status-update`, `gap-update`, and `phase-update` may only mutate
  Xiaoman-owned Postgres `event_signals` by internal `event_signal_id` with an explicit
  UUID `mutation_id`. Each apply must update one allowlisted field and append one
  `event_signal_mutations` audit row transactionally. Do not accept Feishu record ids,
  write Feishu, send QiWe, or reuse these commands for arbitrary metadata updates.
- `run-xiaoman-activity-signal-worker` only scans eligible Xiaoman `event_signals` and
  submits the existing `xiaoman-activity signal-ingest` work item contract. It must not
  write Feishu, send QiWe messages, create visual assets, or be added to production
  scheduling without owner-reviewed runtime changes.
- `qintopia-agentos-xiaoman-activity-signal-worker.timer` may only run
  `run-xiaoman-activity-signal-worker --once --apply` for AgentOS work item intake. Do
  not repurpose it for Feishu writeback, QiWe sends, visual asset creation, or external
  adapters.
- `run-xiaoman-activity-promotion-starter-worker` may only create missing AgentOS
  evidence/visual child `work_items` under existing Xiaoman activity request parents. It
  must not execute evidence retrieval, visual generation, Feishu writeback, QiWe sends,
  group-send readiness, or external adapters.
- `qintopia-agentos-xiaoman-activity-promotion-starter-worker.timer` may only run
  `run-xiaoman-activity-promotion-starter-worker --once --apply` for AgentOS child work
  item intake. Do not repurpose it for evidence execution, visual generation, Feishu
  writeback, QiWe sends, group-send readiness, or external adapters.
- `xiaoman-activity-downstream-observation-smoke.sh` is a read-only production
  observation check for existing evidence and visual workers. It may only run
  `run-evidence-worker --once --dry-run` and
  `run-collaboration-worker --work-item-type visual_asset_request --once --dry-run`; do
  not turn it into an apply smoke, Feishu write, QiWe send, poster generation, or
  external adapter trigger.
- Evidence and visual worker reports must derive `dry_run` from `apply_requested` so a
  `--dry-run` observation cannot report `dry_run=false`; preflight must fail closed on
  any mismatch rather than weakening that assertion.
- `qintopia-agentos-operations-evidence-worker.timer` may only run
  `run-evidence-worker --once --apply` for internal `evidence_summary` artifact writes.
  Xiaoman activity evidence with `source_type=event_signal` must resolve
  `source_event_signal_id` to explicitly linked Postgres messages, with a same-chat
  bounded-window local keyword fallback. It must fail closed when no source evidence
  exists and must not export platform message ids, raw chat ids, sender ids, or
  unbounded raw chat. Do not repurpose it for Feishu writeback, QiWe sends, external
  Wenyuange or embedding search, raw message export, or external adapters.
- `qintopia-agentos-operations-visual-worker.timer` may only run
  `run-collaboration-worker --work-item-type visual_asset_request --once --apply` for
  internal pending `poster_brief` artifact writes. For `activity_promotion`, it must
  wait for the sibling completed `evidence_summary`; do not repurpose it for Huabaosi
  production generation, Feishu writeback, QiWe sends, group-send readiness, or external
  adapters.
- `run-xiaoman-activity-send-request-starter-worker` may only create an
  `awaiting_publish` AgentOS `erhua.send_group_message` / `group_message_request` child
  from an approved Xiaoman `generated_image` whose image-generation request is
  completed. It must not record final confirmation, queue the group message, run
  send-ready, publish, call QiWe, write Feishu, or call external adapters.
- `xiaoman-activity-send-request-starter-observation-smoke.sh` is read-only unless a
  reviewed timer exists and may run the starter in `--check-only` mode only. Do not turn
  it into an apply smoke, final confirmation, send-ready worker, Feishu write, QiWe
  send, or external adapter trigger.
- `qintopia-agentos-xiaoman-activity-send-request-starter-worker.timer` may only run
  `run-xiaoman-activity-send-request-starter-worker --once --apply` for AgentOS
  awaiting-publish group message request intake. Do not repurpose it for final
  confirmation, queueing, send-ready, Feishu writeback, QiWe sends, or external
  adapters.
- `run-xiaoman-activity-image-generation-starter-worker` may only create an
  `image_generation_request` from an approved Xiaoman `poster_brief`; it must not call
  an image provider, upload media, write Feishu, send QiWe, or publish.
- `qintopia-agentos-xiaoman-activity-image-generation-starter-worker.timer` may only run
  `run-xiaoman-activity-image-generation-starter-worker --once --apply` for AgentOS
  image-generation request intake. Do not repurpose it for provider calls, media upload,
  generated-image creation, Feishu writeback, QiWe sends, or publishing.
- QiWe asynchronous `cmd=20000` callback events must be sanitized before NATS capture
  publication and independently before the sidecar writes Postgres. Persist only hashed
  correlation and fixed field-presence metadata; never publish or persist callback file
  credentials, media URLs, filenames, identities, message content, unknown values, or an
  unredacted callback event id. Invalid/dead-letter payloads must store only a digest
  and byte count, never the raw payload. A callback id is already sanitized only when it
  is exactly `qiwe-callback:` plus a 64-character hexadecimal SHA-256 digest; a prefix
  alone is untrusted and the complete value must be hashed again.
- QiWe callback credential-shape reports may emit only a fixed reviewed schema id and an
  additional-field count. They must reject simultaneous canonical and alias spellings
  and must never emit request ids, credential values, filenames, MD5 values, unknown
  field names, or unknown values.
- QiWe outbound text filtering may suppress only complete, narrowly recognized Hermes
  internal-process templates. Every added template needs positive and negative tests;
  never block ordinary answers through broad standalone terms such as `plain text` or
  `纯文本`.
- Hermes/WeCom and QiWe outbound paths must never send raw provider/runtime retry
  diagnostics such as `Retrying in ...`, `API call failed after ...`, HTTP status codes,
  stack traces, paths, record ids, or command text to user chats. Classify them as
  internal process state, keep details in logs/audit, and use a short user-safe Chinese
  fallback only where the reviewed path intentionally sends one.
- `qintopia_agent_os.qiwe_image_send_attempts` may store only canonical hashes, AgentOS
  UUIDs, claim state, allowlisted failure codes, and sanitized audit metadata. Never
  persist QiWe callback file credentials or raw request/callback/message ids. Commit
  `sending` before calling `/msg/sendImage`; an uncertain result becomes `ambiguous` and
  must record `external_send_executed=null` with outcome `unknown` and must not be
  retried automatically. Non-2xx and non-success business responses after the request
  may have been sent are also ambiguous unless a reviewed failure-code allowlist proves
  no send occurred. Treat QiWe target group ids as opaque, case-sensitive values;
  allowlists must use exact matching. A callback arriving after the upload claim TTL
  must terminalize that attempt as `expired` and release the work item for a new
  correlation. Claim scans must also expire an `awaiting_callback` attempt whose
  callback never arrived; an active attempt must not remain solely because no callback
  invoked the callback handler. Once `sending` is committed, the same attempt and claim
  token may record `sent`, `failed`, or `ambiguous` after the short TTL; HTTP failures
  or provider non-success after the send gate are ambiguous unless the bounded client
  proves the request was not sent. Wall-clock expiry must not leave an external outcome
  stuck in `sending`.
- The QiWe upload claim transaction must persist an `uploading` attempt before external
  I/O. A stale `uploading` attempt or legacy unrecorded claim has an unknown external
  outcome and must become terminal `ambiguous` with `automatic_retry_allowed=false`;
  never requeue it automatically. Dry-run and disabled previews must enforce the same
  exact target-group and media-host allowlists as apply.
- Default sidecar builds must fail QiWe upload/callback apply before configuration,
  Postgres claim/mutation, or network access even if runtime enable flags are
  misconfigured; callback apply must also fail before reading stdin. Production artifact
  manifests must record exactly
  `cargo_features: [huabaosi-production-adapter, huabaosi-feishu-mirror-adapter, qiwe-production-adapter]`;
  artifact and server-source build checks must reject `qiwe-staging-adapter`,
  `huabaosi-staging-adapter`, and all-features builds. The Huabaosi production feature
  alone must not make QiWe live helpers available.
- CI must execute non-ignored sidecar tests with all Cargo features so staging-only
  adapter tests actually run. This is test coverage only: ignored PostgreSQL tests
  remain in the disposable integration job. Production artifacts must still use only
  reviewed production features; an all-features CI build must never be promoted or
  treated as a production artifact.
- Heavy PR checks are risk-tiered. Keep `check` meaningful for ordinary PRs, but run
  `rust-quality-baseline` and `xiaoman-postgres-integration` only for sidecar, Postgres,
  deploy sidecar script, or CI workflow changes, plus explicit non-Release manual
  dispatches. Do not weaken Release Please manual validation, production deploy, or
  published Release gates; those remain the full safety boundary.
- QiWe image-send production activation is guarded, not automatic. Activation requires
  the persistent sidecar env file to contain exactly one
  `QINTOPIA_QIWE_IMAGE_SEND_ENABLED=1`, exactly one
  `QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_APPROVAL=approved-production-qiwe-image-send`,
  and exactly one canonical `QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256`
  before starting `qintopia-agentos-qiwe-image-send-preflight.service`. That service
  must run release-local `qiwe-image-send-production-preflight`, and activation must
  stop before `enable --now` unless the production apply gate validates the actual
  database URL hash and Feishu delivery config. Rollback must stop the timer first and
  may report completion only after the persistent enablement flag is present exactly
  once and set to `0`. Do not install or enable QiWe production apply units by
  hot-editing systemd outside the reviewed release runner and activation scripts. If a
  test or accidental build includes both `qiwe-production-adapter` and
  `qiwe-staging-adapter`, apply commands must still select the production gate and must
  never fall back to staging approval or staging database hashing. Production
  observation may inspect the immutable release/current binary and fixed production env
  file, but it must not pass database/QiWe secrets to observation children, bypass the
  async callback/send state machine, write Feishu as part of sending, or treat staging
  evidence as production completion.
- The Hermes QiWe image callback bridge is a memory-only callback ingress, not a
  scheduler or release activation path. Production mode must require
  `QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_MODE=production`, exact production owner
  approval, canonical production database URL hash, image-send/webhook readiness, and
  the exact
  `/home/ubuntu/qintopia-agent-os-releases/current/sidecar/qintopia-message-sidecar`
  binary with root `/home/ubuntu/qintopia-agent-os-releases/current`. It must reject
  direct release-directory paths, mutable checkout binaries, staging roots, missing
  `current` symlinks, unsafe ownership or group/world-writable paths, and sidecar
  SHA-256 drift. Staging mode must continue to use only the fixed staging release root
  and staging owner/database gates. The child process may receive only the reviewed
  database, QiWe, target allowlist, and Huabaosi Feishu primary-storage delivery
  environment for its selected mode; do not inherit Hermes, NATS, proxy, unrelated
  runtime state, callback credentials, or raw provider values. Callback bytes may flow
  only through bounded stdin, and bridge enablement must never approve artifacts, enable
  timers, publish a Release, write Feishu by itself, or bypass the Rust production apply
  gate.
- A real Xiaoman activity may be described as production-complete only after the
  retained sanitized evidence passes
  `tools/deploy/check-xiaoman-real-activity-production-evidence.mjs` and the full
  completion manifest plus staging/production evidence files pass
  `tools/deploy/check-xiaoman-production-completion-evidence.mjs`. The report may keep
  only the fixed schema ids, AgentOS UUIDs, release/database hashes, the owner-approved
  sidecar binary hash, release-binary verification booleans, `artifact_content_hash`,
  reviewed PR numbers/head SHAs, production Release commit binding, and boolean
  execution facts; it must not retain raw QiWe callback bodies, request ids, file
  credentials, group ids, message ids, media URLs, database URLs, provider responses,
  raw chat, or raw logs.
- `xiaoman-real-activity-production-evidence` is a read-only retention exporter. It may
  run only from the immutable
  `/home/ubuntu/qintopia-agent-os-releases/current/sidecar/qintopia-message-sidecar`
  binary whose resolved release directory matches `QINTOPIA_DEPLOYED_COMMIT_SHA` and
  whose SHA-256 matches `QINTOPIA_XIAOMAN_REAL_ACTIVITY_PRODUCTION_SIDECAR_SHA256`. It
  must hash the configured database URL and match
  `QINTOPIA_XIAOMAN_REAL_ACTIVITY_PRODUCTION_DATABASE_URL_SHA256` before opening a
  database connection. It may read Postgres, hash the verified release-local sidecar
  binary, and emit the fixed `xiaoman_real_activity_production_evidence=` records for
  one already completed Xiaoman activity chain. It must not run from a mutable checkout,
  connect to a database whose URL hash is not owner-approved, write Postgres or Feishu,
  approve artifacts, call QiWe, publish, send, expose raw group ids, request ids,
  callback bodies, file credentials, message ids, media URLs, database URLs, provider
  responses, raw chat, or logs. Its send-ready query must bind the completed
  `group_message_request`, queued-state final confirmation, send-ready event payload,
  approved generated-image artifact id, and sanitized QiWe `sent` attempt before
  emitting production-complete evidence.
- In a separately owner-approved staging-feature build, `run-qiwe-image-send-worker` may
  only claim one reviewed send-ready work item, call the reviewed asynchronous
  URL-upload method, and persist hashed upload correlation. Its dry-run preview must
  reuse the apply path's exact target-group, media-host, and approved JPEG identity
  validation; preview must not report policy-ineligible work. Staging-feature apply must
  additionally require the exact reviewed one-shot owner approval phrase before adapter
  configuration, callback stdin, Postgres, or network access; feature compilation,
  enable flags, or credentials alone are insufficient.
  `process-qiwe-image-send-callback` must read one bounded callback from stdin, keep
  file credentials memory-only, require callback filename/MD5/byte size to match the
  approved final JPEG before committing `sending`, commit that state before one send
  call, and terminalize every outcome. Scheduling or production enablement must remain
  bound to approved staging evidence, isolated group allowlists, release/database hash
  gates, and rollback.
- The Feishu-backed QiWe staging bridge may claim `feishu-base://` generated images only
  when the immutable staging artifact contains both `huabaosi-staging-adapter` and
  `qiwe-staging-adapter`. It must commit the existing `uploading` attempt before Feishu
  or QiWe I/O, authenticated-readback the approved JPEG, upload those bytes only to the
  non-deprecated QiWe SDK temporary-storage endpoint, keep the returned `cloudUrl`
  memory-only, exact-allowlist and read back that URL, and prove SHA-256, MD5, and byte
  size before invoking the existing asynchronous URL upload. Default, production,
  Huabaosi-only, and QiWe-only builds must fail closed. Temporary URLs, Feishu tokens,
  multipart bodies, and raw bytes must not enter Postgres, reports, logs, CLI arguments,
  or environment-derived output.
- A staging-feature QiWe callback apply must validate explicit enablement, exact
  API/media/group allowlists, and webhook readiness before reading stdin. Upload apply
  must validate the same adapter configuration before connecting to Postgres.
- `qiwe-image-send-staging-smoke.sh` is the only reviewed one-shot staging entrypoint
  for the async upload and callback send exercise. It requires an exact work item UUID
  for upload/callback, owner phrase, staging env path, exact owner-reviewed staging
  database URL hash, exact owner-reviewed staging release SHA, exact owner-reviewed
  packaged sidecar binary SHA-256, and explicit `preflight`, `upload`, or `callback`
  phase. It must fail closed unless it is running from
  `/home/ubuntu/qintopia-agent-os-staging-releases/<approved 40-hex sha>` and executing
  that release-local `sidecar/qintopia-message-sidecar`; do not allow
  `QINTOPIA_SIDECAR_BIN`, `cargo run`, symlinked binaries, untrusted owners,
  owner/group/world-writable binary paths, mutable source-tree fallbacks, or a binary
  whose SHA-256 changes between the initial check and any child sidecar spawn. Callback
  credentials may flow only from bounded stdin to the callback processor and memory-only
  send request; never store them in a file, environment variable, CLI argument, NATS
  event, report, or log. The smoke must attach `/dev/null` to preflight and upload
  subprocesses, parse only the fixed staging env key allowlist without evaluating the
  env file as shell, revalidate the sidecar path and digest immediately before each
  child sidecar command, and run child sidecar commands with a minimal explicit
  environment rather than inheriting ambient operator secrets. Subprocess output must be
  captured, scanned, and schema-validated through memory and anonymous pipes, and raw
  child JSON must not be passed to a sanitizer through environment variables; no
  subprocess output may be written to a file. The local fake smoke test creates a
  temporary `sidecar/qintopia-message-sidecar`; if interrupted, remove any leftover
  source-tree `sidecar/` before treating the checkout as clean or rerunning staging
  checks. Upload and callback evidence may retain only the canonical final JPEG
  `artifact_content_hash` for Huabaosi/QiWe hash matching; it must not retain media URI,
  filename, MD5 value, file size, or callback credentials. It must not install a
  listener, service, timer, production feature build, Feishu write, or broad group send.
- A QiWe production-enablement PR must retain Huabaosi staging generated-image evidence
  and QiWe staging send evidence that pass
  `tools/deploy/check-xiaoman-image-send-staging-evidence.mjs`, proving the Huabaosi
  final JPEG `content_hash` equals the QiWe `artifact_content_hash` without recording
  media URI, filename, MD5 value, callback credentials, group id, database URL, or raw
  provider output. Record that cross-flow result only in
  `docs/reports/templates/xiaoman-image-send-staging-evidence.md`; staging evidence is a
  prerequisite, not proof that production sending is complete.
- The staging-only sidecar artifact `qintopia-message-sidecar-staging-linux-x86_64-gnu`
  may be built only by manual artifact workflow dispatch or
  `pnpm artifact:sidecar:staging`. It must compile exactly `huabaosi-staging-adapter`
  and `qiwe-staging-adapter`, record `staging_only=true` and
  `production_eligible=false`, and be installed only under
  `/home/ubuntu/qintopia-agent-os-staging-releases/<approved 40-hex sha>` for
  owner-approved evidence smokes. Because staging image storage is fixed to
  `feishu-base`, the staging Huabaosi adapter must compile the guarded Feishu Base
  primary-storage upload/readback path while preserving the staging owner approval,
  database hash, Base/table allowlists, schema, and Huabaosi profile gates before
  external I/O. Production deploy, COS upload, Release builds, and production artifact
  fetchers must never fetch or promote it.
- `qiwe-image-send-staging-readiness-smoke.sh` is the read-only gate before the real
  QiWe staging preflight. It may only check metadata for the fixed staging env file,
  fixed immutable staging release root, owner-approved release SHA, and packaged sidecar
  binary digest. Its sidecar release directory and binary permission checks must match
  the staging smoke's owner/group/world-writable rejection, while the secret-bearing env
  file may remain owner-writable. Path checks must lstat every parent component and
  reject symlinks, non-directories, group/world-writable parents, unexpected parent
  owners, and a sidecar binary the running user cannot execute; tests for these checks
  must use repository-local temporary roots, not `/tmp`. It must not read env file
  contents, execute the sidecar, connect to Postgres, contact QiWe/Feishu/provider/media
  endpoints, create release directories, install or enable services/timers, or report
  secret-bearing values.
- `staging-runtime-prerequisite-observation-smoke.sh` is a read-only observation gate
  for fixed staging env and immutable release prerequisites. It must never read env
  contents, execute the sidecar, connect to Postgres, call external services, install
  units, enable timers, or report secret-bearing values. Its path checks must lstat
  every parent component and reject symlinks, non-directories, group/world-writable
  parents, unexpected parent owners, and a sidecar binary the running user cannot
  execute; tests for these checks must use repository-local temporary roots, not `/tmp`.
- A QiWe webhook bridge for `cmd=20000` may invoke only one explicitly configured
  staging sidecar with fixed `process-qiwe-image-send-callback --apply` arguments. It
  must default disabled, require the exact staging owner phrase and canonical approved
  staging database URL hash before callback stdin, stream the bounded raw callback only
  through child stdin, discard child stderr, bound and validate the sanitized Rust
  report, and never persist or log callback bytes, credentials, request ids, filenames,
  MD5 values, unknown fields, or subprocess output. Ordinary callback capture must still
  be sanitized independently before NATS publication. If the bridge is explicitly
  enabled but any gate is invalid, callback handling must return a non-2xx response; it
  must not silently downgrade to disabled and acknowledge an unprocessed callback.
  Callback detection must require the reviewed top-level QiWe success envelope, bounded
  `data` list, request id, `msgData` object, and complete credential-field presence. It
  must fail closed on excessive JSON depth and must not classify an arbitrary nested
  `cmd=20000` field as an image callback. The child process may inherit only the fixed
  staging callback allowlist: sidecar database URL and pool size, approved database
  hash, send/webhook/owner gates, QiWe API URL/token/guid, API/media host allowlists,
  and target-group allowlist. It must not inherit Hermes webhook secrets, NATS, Feishu,
  proxy, or unrelated runtime variables. Any explicit callback-processor enable value
  other than `0` or `1` is configuration invalid and must also return a non-2xx callback
  response. The processor path must be exactly
  `/home/ubuntu/qintopia-agent-os-staging-releases/<40-hex-sha>/sidecar/qintopia-message-sidecar`
  with the matching configured release root and approved binary SHA-256. The fixed
  staging root, release directory, sidecar directory, and binary must not be symlinks,
  owned by an unexpected uid, or group/world-writable. Revalidate the path and digest
  immediately before spawn; never accept a writable staging-like path such as `/tmp`.
- Huabaosi and QiWe external HTTP calls must use the shared bounded Rust client. It must
  reject invalid methods/headers before connect, require HTTPS outside tests, enforce
  header/body/chunk limits while reading, set socket timeouts, zeroize sensitive request
  and response buffers, and classify whether an error occurred after a request may have
  been sent.
- Huabaosi image generation may override the shared 60-second socket timeout only
  through `QINTOPIA_HUABAOSI_IMAGE_HTTP_TIMEOUT_SECONDS`, defaulting to 180 seconds and
  bounded from 60 through 240 seconds. The upper bound must leave room inside the fixed
  10-minute image claim lease for the five bounded Feishu auth/search/upload/readback/
  upsert calls, transform, and the final transaction. A provider transport or protocol
  error after request bytes may have been sent is ambiguous and must not be retried
  automatically. Image-generation failure audits may record
  `external_generation_executed=false` only before provider request execution, `true`
  only after a valid provider payload is accepted, and `null` when the provider outcome
  cannot be proved. `external_media_write_executed` must likewise remain `null` for an
  unprovable upload or Feishu write and become `true` only after confirmed storage. Do
  not change the shared timeout for QiWe, WeCom, Feishu, or other adapters to remediate
  image-provider latency.
- `run-huabaosi-image-generation-worker` defaults to
  `QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED=0`. Production generation may run only
  from a release artifact compiled with the reviewed `huabaosi-production-adapter`
  feature, explicit production enablement bound to the deployed release SHA and database
  URL hash, valid provider/media configuration, and the fixed production timer. It may
  create only pending `generated_image` artifacts; it must not approve, publish, write
  Feishu, or send QiWe.
- As of 2026-07-15, 阿靓/Huabaosi real image production has not completed final
  activation. Do not treat it as live until the same reviewed release has follow-up
  deploy evidence, the Huabaosi timer is activated, and the first real pending
  `generated_image` has review evidence. The `v0.2.10` follow-up deploy and systemd
  installation evidence now exist, but the no-network preflight remains fail-closed
  because provider/media configuration is not provisioned; the timer must remain
  disabled until that gate passes.
- 阿亮画报师生产 WeCom Bot 的 `Interrupting current task` / `Response formatting failed`
  用户可见中断提示来自 live Hermes gateway busy-ack and platform send fallback
  (`hermes-gateway-huabaosi.service`, `gateway/run.py`, `gateway/platforms/base.py`),
  not Rust sidecar image generation or QiWe image-send state. Diagnose this path through
  Huabaosi Hermes/WeCom runtime first, and do not hot-edit the server.
- Server Hermes patches under `docs/operations/review-pool/hermes/` are non-deployable
  migration evidence. Do not add them to release bundles or apply them to production;
  migrate each accepted behavior into an owned package with focused tests and a separate
  cutover PR.
- Huabaosi live provider/media helpers may compile only with one reviewed Huabaosi live
  feature: `huabaosi-staging-adapter` for guarded staging or
  `huabaosi-production-adapter` for production. A build containing neither or both must
  reject apply before Postgres or network access. Staging keeps its one-shot owner
  phrase and reviewed staging database hash gate. Production must bind explicit
  enablement to the deployed release SHA and production database URL hash before
  connecting to Postgres. Production artifacts must not contain QiWe live adapter
  features until a separate owner-approved production send boundary exists.
- The Huabaosi production image-generation service and timer may be installed from the
  immutable release but must not be enabled by the ordinary release installer. After the
  owner manually publishes the Release, the reviewed activation command must run the
  no-network preflight from that release and then enable the fixed timer for canary
  generation. Rollback disables the timer first and turns the generation enable flag off
  through reviewed runtime configuration. Do not repurpose this timer for artifact
  approval, Feishu, QiWe, or publishing.
- The disposable operations apply smoke may exercise the Huabaosi retry state only when
  both `huabaosi-staging-adapter` and `postgres-integration-tests` are compiled,
  `QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE=1`, the database is exactly `qintopia_test` on
  a literal loopback IP with its approved URL hash, and every provider/media endpoint
  and allowlist host is a literal loopback IP. This exception must never accept an
  external host or production database.
- `operations-artifact-review-decision` may approve a `generated_image` only after its
  Huabaosi worker provenance, stable JPEG HTTPS URI, final JPEG sha256/metadata, source
  PNG sha256, fixed `png_to_jpeg_white_background_q92_v1` transform metadata, source
  brief/prompt refs, and `generated_image_created` audit match its image-generation
  request. Human approval applies to the exact final JPEG bytes; the transient provider
  PNG is never an approvable artifact. Integrity denial must leave the artifact pending
  and must not complete the work item or unlock downstream send intake.
- Generated-image media URIs used by Huabaosi artifact creation, operations approval,
  and QiWe send intake must reject raw backslashes and percent-encoded path separators
  before URL parsing; parsers or downstream services may normalize them into path
  separators, which can hide unstable or secret-shaped input from later filename checks.
- A content-hash conflict may reuse an existing pending `generated_image` only when its
  stable URI, source refs, and complete immutable worker metadata exactly match the new
  final JPEG result. Reviewed, stale, or modified artifacts must fail closed and must
  never be overwritten by retry processing.
- When the Huabaosi adapter is explicitly enabled in an approved staging boundary, it
  may retry only provider transport failures and HTTP 408, 429, or 5xx responses. It
  must stop after three total attempts, use delayed requeueing, and record only
  sanitized attempt/stage/outcome metadata. Authentication, payload, PNG decode,
  PNG-to-JPEG conversion, JPEG validation, media upload, readback, persistence, and
  claim failures are terminal and must not be retried. When explicitly enabled in a
  reviewed staging configuration, every provider, upload, and readback response must be
  size-capped before parsing, and an already reviewed `generated_image` must never be
  overwritten or returned to `pending` by a retry. Every outbound HTTP header name/value
  must reject control characters before socket connection. Each work-item claim must use
  a unique token; artifact or failure writes must lock and match that unexpired token,
  with exactly one affected work-item row.
- An expired or structurally incomplete Huabaosi image-generation `processing` claim is
  an unknown provider/media outcome. Reconciliation must atomically mark it failed,
  release the complete claim tuple, append one sanitized ambiguous-outcome event, and
  disable automatic retry; it must never reclaim the row for another external attempt.
- `huabaosi-image-generation-production-observation-smoke.sh` may verify either the
  disabled pre-activation state or the enabled production timer state, run configuration
  preflight, and run `run-huabaosi-image-generation-worker --once --dry-run` for a
  read-only queue preview. It must not use `--apply`, contact provider/media endpoints,
  write Postgres or Feishu, call QiWe, create a generated image, or publish.
- `huabaosi-image-generation-production-canary-smoke.sh` is the release-local one-shot
  entrypoint for the first post-deploy image. It must run from the exact immutable
  release with the provider timer disabled and inactive, parse only allowlisted keys
  from the fixed production env without `source` or `eval`, reuse reviewer `trainer`,
  and bind one pending brief to one new request, one pending Feishu-backed JPEG, and
  authenticated same-byte revalidation. Its approval request must transactionally
  require the target artifact to be a pending `poster_brief` before mutation or Feishu
  revalidation. Each retained canary evidence phase must preserve
  `release_binary_verified=true`, `approved_sidecar_sha256_matched=true`, and
  `approved_database_url_sha256_matched=true` so the standalone and final completion
  checkers can prove the immutable release/database boundary. It must not approve the
  generated image, enable timers, run the mirror worker, publish, call QiWe, send, or
  retry terminal/ambiguous outcomes.
- `huabaosi-wecom-gateway-observation-smoke.sh` may only inspect the live Huabaosi
  Hermes WeCom user-service active state through `systemctl --user`, fixed service
  command, public `busy_input_mode`, release/current presence, and sanitized
  user-journal marker counts. It must not source `.env`, print raw journal lines, print
  user messages, read tokens, restart services, send WeCom messages, run image
  generation, write Postgres or Feishu, call QiWe/provider/media endpoints, or modify
  live Hermes profile state.
- `huabaosi-wecom-canary-observation-smoke.sh` may only verify that the canary gateway
  remains unscheduled and disabled, then run `huabaosi-wecom-canary-preflight` for a
  sanitized local configuration summary. From release/current it must discover the
  immutable `sidecar/qintopia-message-sidecar` binary rather than fall back to source
  Cargo execution. It must not use `--apply`, read stdin, source `.env`, print
  endpoint/token/id values, write Postgres or Feishu, call WeCom, QiWe, provider, or
  media endpoints, run image generation, publish messages, install units, or modify the
  live Hermes profile.
- `huabaosi-wecom-shadow-capture` may only preview one supplied WeCom event from bounded
  stdin and emit sanitized metadata, hashes, byte counts, field presence, and fixed
  guardrails. It must not add `--apply`, open Postgres or network connections, write
  artifacts, send WeCom/QiWe messages, call image providers, upload media, write Feishu,
  or emit raw ids, user text, media URLs, filenames, tokens, or callback file
  credentials.
- `huabaosi-wecom-policy-preview` may only preview one supplied WeCom event from bounded
  stdin and emit sanitized policy decisions for message classification, busy-session
  handling, internal-process filtering, formatting fallback, user-safe fallback copy,
  and idempotency. It must not add `--apply`, open Postgres or network connections,
  write artifacts, send WeCom/QiWe messages, call image providers, upload media, write
  Feishu, or emit raw ids, user text, media URLs, filenames, tokens, or callback file
  credentials. Suppression rules must match narrow complete internal templates; do not
  block ordinary user requests through broad words such as `plain text` or `纯文本`.
- `huabaosi-wecom-canary-preflight` must not read stdin, open network or database
  connections, source env files, reveal configuration values, write Feishu/Postgres, or
  send WeCom/QiWe messages. `huabaosi-wecom-canary-gateway --apply` is allowed only in
  an owner-reviewed staging command built with the non-default
  `huabaosi-wecom-canary-gateway` Cargo feature, explicit enable flag, approval phrase,
  HTTPS endpoint, token, and exact Bot/chat/user allowlists. Default production builds
  must fail closed before stdin, network, database, or send access. The command must not
  change the production Bot route, install timers, broaden sends beyond the allowlist,
  run image generation, upload media, or write Feishu/Postgres.
- `huabaosi-image-generation-preflight` may only validate and emit a sanitized summary
  of local image-adapter configuration. It must not open network or database
  connections, reveal configuration values, enable generation, write Feishu, send QiWe,
  or publish. Its `missing_configuration` field may contain only fixed public env names
  already documented in `.env.example`; it must never contain values, URLs, hosts, ids,
  or enable flags.
- `huabaosi-image-generation-staging-readiness-smoke.sh` may only inspect staging env
  file metadata, immutable staging release root metadata, the exact owner-reviewed
  release SHA, and packaged sidecar binary SHA-256. It must not read env file contents,
  execute the sidecar, run Cargo, connect to Postgres, call Huabaosi/provider/media,
  write Feishu, send QiWe, inspect services, install timers, or reveal paths containing
  secrets. Path checks must lstat every parent component and reject symlinks,
  non-directories, group/world-writable parents, unexpected parent owners, and a sidecar
  binary the running user cannot execute; tests for these checks must use
  repository-local temporary roots, not `/tmp`. It is a read-only prerequisite before
  the owner-approved Huabaosi staging generation smoke.
- `huabaosi-image-generation-staging-smoke.sh` may only run one owner-approved staging
  image request after the fail-closed preflight, explicit smoke flag and approval
  phrase, staging-only env file, a repository-reviewed database URL hash allowlist, and
  an explicit UUID work item id. It must parse the staging env file through a fixed key
  allowlist without evaluating it as shell. Known unrelated QiWe staging keys may exist
  in the same env file, but the Huabaosi smoke must ignore them and never pass them to
  child sidecar/cargo commands. Child commands must run with a minimal explicit
  environment rather than inheriting ambient operator secrets, keep subprocess output in
  memory for sensitive-output scanning, emit only sanitized
  `huabaosi_image_generation_staging_evidence` records for preflight and the pending
  final JPEG, and pass raw child JSON to evidence sanitizers only through
  stdin/anonymous pipes. It must leave the image pending review and must not run in
  production, add a timer, write Feishu, send QiWe, or publish. Evidence may include the
  staging database URL hash, work item UUID, final JPEG SHA-256, dimensions, byte count,
  MIME type, and pending review state; it must not include provider/media URLs,
  filenames, tokens, database URLs, provider responses, Feishu ids, or QiWe credentials.
  Record the retained result in
  `docs/reports/templates/huabaosi-image-generation-staging-evidence.md` before it is
  used as input to QiWe staging evidence.
- `render-staging-runtime-env.py` must render the same reviewed Huabaosi Feishu Base
  staging key contract consumed by `huabaosi-image-generation-staging-smoke.sh`. It must
  not require or emit the retired HTTP media upload/public URL keys for the Huabaosi
  staging path, and the generation evidence must prove the worker returned a
  `feishu-base://` artifact boundary rather than trusting the env-selected storage
  backend.
- `operations-group-send-ready-timer-observation-smoke.sh` may only inspect the group
  send-ready systemd timer, unit commands, and sanitized journal output. It must not run
  the worker, record final confirmation, write Postgres, call QiWe, or send externally.
- `qiwe-image-send-preflight` may only validate the disabled async URL-upload/send-image
  contract from local configuration. It must report whether a live adapter was compiled
  and fail disabled/default release checks when any QiWe live adapter is present. It
  must not open network or database connections, emit tokens, device/group ids, media
  URLs, file credentials, or message identifiers, write Feishu, or send externally. The
  production artifact may compile only the reviewed `qiwe-production-adapter`; it must
  still fail closed unless the dedicated `qiwe-image-send-production-preflight`
  validates the exact owner approval phrase, actual database URL hash, Feishu delivery
  config, live feature bridge, enablement flag, and allowlisted configuration before
  timer activation. Final request construction must recheck the target group allowlist,
  response parsing must fail closed unless both `code=0` and `isSendSuccess=1`, and this
  disabled-state preflight must fail when the send-enable flag is `1`. All future
  outbound header values must reject every control character before socket connection.
  Its `missing_configuration` field follows the same public-name-only rule as the image
  preflight and must never include enable flags or configuration values.
- `xiaoman-activity-production-preflight-smoke.sh` is a read-only composition of Xiaoman
  timer observation smokes, shared evidence/visual timer observation, Xiaoman downstream
  evidence/visual preview, and the group send-ready timer observation. It must not set
  apply-smoke flags, deploy units, publish releases, write Feishu, call QiWe, run the
  send-ready worker, or run external adapters. It must invoke child observations through
  `env -i` with only a fixed PATH, the child enable flag, and the release-local sidecar
  path when present; do not pass caller-provided test overrides, systemctl/journalctl
  overrides, env-file overrides, or ambient deployment secrets into child observations.
- As of 2026-07-15, real end-to-end acceptance is not complete. Do not claim the Xiaoman
  activity flow is accepted until one real activity is observed from Xiaoman signal
  intake through image generation, human approval, and QiWe group-send arrival.
- `operations-work-item-status` must resolve nested work items to the top-level workflow
  root and report every descendant while preserving each direct `parent_work_item_id`.
  `operations-workflow-sync` may persist that recursive AgentOS summary, but neither
  command may execute workers, schedule a general DAG, call external adapters, or send.
- The `feishu_task_dry_run` workbench mirror must preserve immediate `child_status_refs`
  and expose nested work only through sanitized `descendant_status_refs` with direct
  parent and depth. It must not copy raw payloads, call Feishu, or make the workbench a
  fact source. Keep the description bounded and report truncation explicitly.
- `install-release-systemd-units.sh` may only render units from the promoted immutable
  release, install its fixed allowlist, and enable AgentOS internal workflow timers. Do
  not extend it to execute arbitrary commands, enable Feishu/QiWe/external adapters, or
  source a writable server checkout.
- The first release containing a deploy-runner behavior change is processed by the
  previous runner. Use a reviewed follow-up `workflow_dispatch` request for the same
  published SHA to activate the new runner behavior; do not bootstrap it with server
  edits.
- As of 2026-07-15, the corrected `v0.2.10` same-SHA follow-up deploy installed the new
  systemd units. A same-SHA request for an existing release must reuse the immutable
  manifest's exact runtime, bundle, commit, scope, and restart-target fields; narrowing
  `restart_targets` is rejected before promotion and does not trigger rollback. Content,
  path, type, or symlink drift must fail before mutation. After the bounded metadata
  repair allowed above, the existing tree must satisfy the same deploy-runner owner,
  non-writable, directory accessibility, regular/symlink type, sidecar `0755`, and
  metadata `0444` checks as a new staging tree. Same-SHA reuse must preserve a distinct
  `previous` target. Production release and staging roots must be created explicitly as
  `0755` so the validation contract does not depend on ambient `umask`.
- PR #140 and PR #141 completed the Xiaoman profile bundle and values migration, but the
  live profile symlink cutover remains a separate PR. Do not repoint the live Xiaoman
  profile symlink without that reviewed cutover, smoke evidence, and rollback note.
- `xiaoman-postgres-integration` in GitHub Actions may enable the guarded apply smoke
  only against its disposable `qintopia_test` PostgreSQL service. It must not use a
  production database URL, secrets, Feishu, QiWe, or external adapters.

## Package Placement

- Agent profile, prompt, allowed skills, memory policy, and forbidden actions:
  `agents/<agent>/`.
- Reusable channel or business capability: `skills/<capability>/`.
- Cross-Agent business process: `workflows/<workflow>/`.
- MCP server or adapter: `mcp/<adapter>/`.
- Runtime template or render/check logic: `runtime/<runtime-area>/`.
- Release, smoke, rollback, or server install logic: `deploy/<area>/`.
- Historical POC or removed direction: `deprecated/<topic>/`.

## Package Contract

Every adopted package should eventually include:

- `README.md`
- `manifest.yaml`, `agent.yaml`, or `workflow.yaml`
- `tests/` or `fixtures/`
- owner and risk level
- validation command
- production boundary
- rollback or decommission notes when relevant

Do not migrate a package as production-ready until these are present or there is a
documented exception.

## Migration Rules

Migration is inventory-first:

1. Identify the current source path.
2. Record whether it is `adopt`, `template`, `runtime-only`, `deprecated`, or `remove`.
3. Preserve source hash or commit reference.
4. Add package metadata.
5. Add focused tests or fixtures.
6. Only then wire it into registry and deployment.

Server runtime directories under `.hermes/profiles/*` must be treated as live runtime
state. They can produce inventory records, templates, or diffs; they must not be copied
wholesale into this repository.

## Server Change Policy

The server is a deployment target, not an editing workspace.

Allowed server activity:

- read-only inventory
- service status checks
- log inspection
- smoke checks
- deploying an approved commit SHA through a runbook
- emergency rollback with a follow-up patch and owner record

On an approved operator workstation, use its configured SSH host alias for these
activities. Connecting to the inventory address directly may bypass the approved
identity selection. An authentication failure is not authorization to inspect, copy, or
change private keys.

Disallowed server activity:

- editing docs directly
- editing code directly
- editing `.hermes` runtime files directly
- scp overwrites of single source files
- committing unreviewed experiments on the server and treating them as product direction

## Validation Expectations

Before a PR:

- Run package-level tests.
- Run fixture replay when available.
- Run registry/manifest checks when available.
- Validate the completed PR body with `pnpm pr:check-body` or `pnpm pr:doctor`.
- For runtime/deploy changes, include dry-run output and rollback notes.
- For user-facing HTML reports, run HTML parse and browser overflow checks.
- For production-adjacent changes, state whether the change touches external sends,
  database writes, profile runtime, secrets, Feishu, QiWe, or systemd.

## Documentation Rules

- Keep decisions in git, not only in chat.
- For every production, deploy, preflight, or CI integration failure, add or update a
  dated, indexed record under `docs/reports/` in the same PR. Include the observed
  evidence, root cause, resolution, validation, remaining boundary, and follow-up owner
  action. Update affected runbooks, package READMEs, manifests, or checks in that same
  PR; do not leave the repair documented only in a report or chat.
- Prefer short, focused docs over one large manual.
- Mark server-side exploration as unapproved until owner review confirms it.
- Avoid formalistic phrasing when writing internal engineering docs.
- Keep technical reports concrete: current state, evidence, risk, next action.

## First Read For New Agents

1. `README.md`
2. `AGENTS.md`
3. `docs/README.md`
4. `docs/architecture/agent-os-overview.md`
5. `docs/plans/active/current-roadmap.md`
6. `docs/engineering/programming-agent-guardrails.md`
7. `docs/engineering/change-routing-index.md`
8. `docs/product/agent-os-prd.md` for product scope changes
9. `docs/agent-os/README.md` for Agent OS design changes
10. `docs/plans/completed/monorepo-migration.md` for historical migration evidence
11. Target package README or manifest
12. Relevant docs under `docs/engineering/` or `docs/operations/`

Report what you read, what you plan to touch, validation commands, and production
boundaries before making broad changes.
