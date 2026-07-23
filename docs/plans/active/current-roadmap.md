# Current Roadmap

Updated: 2026-07-19

The monorepo migration and server cleanup phases are complete. The historical execution
log is archived at
[../completed/monorepo-migration.md](../completed/monorepo-migration.md).

Use this document for current and future work. Do not reopen the completed migration log
unless correcting historical evidence.

## Current State

- The monorepo is the collaboration source of truth.
- Production Agent OS sidecar, workers, release-managed MCP commands, and reviewed
  Hermes plugins run from `/home/ubuntu/qintopia-agent-os-releases/current`.
- Xiaoman's AgentOS-only activity workflow passed the `v0.2.9` aggregate production
  preflight on 2026-07-15. Its plugin and workers are release-managed; its live profile
  files still require a reviewed bundle migration. Internal intake, evidence, visual
  brief, image-generation request, and awaiting-publish request paths are production
  schedulable; Huabaosi image generation plus Feishu artifact mirroring move to explicit
  production activation in the next release, while QiWe final group send remains
  disabled.
- Hermes remains the Agent runtime under `/home/ubuntu/.hermes`.
- Hermes profile live state, including `.env`, sessions, logs, cache, memory, auth, and
  local config overrides, stays outside git.
- `v0.2.10` is the first Release containing the new deploy-runner behavior. Its
  corrected same-SHA follow-up deploy succeeded and installed the three Huabaosi
  production image units. The worker timer remains disabled until the owner supplies
  reviewed provider and Feishu-backed generated-image table configuration, runs
  production preflight, and activates the timer through the guarded script. The
  generated-image table id comes from the owner-provided Feishu URL's `table` query
  parameter and must not be committed to git.
- Xiaoman production completion is gated separately from infrastructure releases. A
  Release may ship staging/provisioning or activation tooling without being a usable
  Xiaoman completion. Use the
  [Xiaoman production completion gate](xiaoman-production-completion-gate.md) before
  describing any Release as fully usable for the real Xiaoman activity-to-group-send
  workflow.
- WorkTool, OpenClaw, and the current WorkTool-bound Xiaoqin runtime are archived and
  deprecated. Future Xiaoqin work requires a new non-WorkTool Agent design.

## Active Directions

1. Profile distribution and bundle design
   - Align with Hermes profile distribution behavior.
   - Treat `SOUL.md`, skills, cron, and MCP declarations as reviewed distribution-owned
     files.
   - Preserve runtime data and local secrets on the server.
   - Start with one low-risk profile before touching group-facing behavior.
   - Migrate Xiaoman's reviewed non-secret profile behavior through the phased
     [Xiaoman profile bundle migration](xiaoman-profile-bundle-migration.md). The first
     PR packages a strict renderer and read-only parity smoke only; live profile
     symlinks require later parity and rollback evidence.
   - Migrate the live Huabaosi WeCom conversation boundary into the reviewed release
     flow through [Huabaosi WeCom migration](huabaosi-wecom-migration.md). The
     production Bot remains on Hermes until observation, Rust shadow capture, policy
     preview, canary, and rollback evidence are reviewed in separate PRs.
   - PR #140 and PR #141 completed Xiaoman profile bundle and values migration work, but
     the live profile symlink cutover still requires a separate reviewed PR.

2. External adapter allowlists
   - Keep real external send paths disabled until allowlists, runtime config, smoke, and
     rollback are reviewed. Feishu-backed Huabaosi image storage and artifact mirroring
     are production canary workbench paths, not approval or send paths.
   - Do not broaden Feishu, QiWe, or workbench permissions in the same PR as unrelated
     feature work.
   - Mirror Huabaosi generated images into the design Base through the dedicated
     [artifact mirror plan](huabaosi-feishu-artifact-mirror.md). The first PR defines a
     fixed image-version schema and a feature-gated writer. The production enablement PR
     adds the feature to immutable release artifacts, installs a dedicated timer without
     auto-enabling it, and provides guarded activation/rollback. Remaining work is
     merge, manual Release publication, production configuration, explicit activation,
     and first-record evidence.
   - The owner selected the fixed Feishu image-version table as the first production
     storage boundary for the Huabaosi canary. The
     [Feishu primary-storage plan](huabaosi-feishu-primary-storage.md) reuses the
     activity-ledger Base API upsert pattern, requires authenticated attachment readback
     before a pending AgentOS artifact is created, and leaves Feishu automation limited
     to later notification/review coordination.
   - The proposed
     [Aliang image-generation adapter](aliang-production-image-generation.md) uses the
     historically observed OpenAI-compatible `gpt-image-2` path as its implementation
     target. The `v0.2.7` production release installed the internal Xiaoman
     image-generation request starter timer, but real image generation, user-media
     storage, human review, and publication remain separate gates; Huabaosi provider
     execution is moving through an owner-approved production enablement PR. The
     production worker remains inactive until the owner manually publishes the Release,
     the release binary passes production preflight, and the dedicated timer is
     explicitly activated for canary generation. Image review and publication remain
     separate gates. As of 2026-07-15, the same-SHA follow-up deploy and systemd
     installation evidence are complete. Final activation still requires reviewed
     provider/media configuration, a successful production preflight, Huabaosi timer
     activation, and the first real pending `generated_image` review evidence. The
     production artifact can also compile the guarded Feishu mirror adapter so the
     reviewed generated-image workbench row can be activated in the same release
     boundary; approval and publishing still stay separate. A first real canary on
     2026-07-19 proved brief approval, starter intake, production claims, bounded
     retries, and rollback, but all three provider calls reached the fixed 60-second
     socket timeout before an image response. The timer is disabled and no artifact was
     created. Production completion now requires the reviewed image-specific timeout
     remediation, a new Release deployment, and a newly approved canary that reaches one
     pending Feishu-backed JPEG. Release `v0.2.16` deployed the timeout remediation, but
     production acceptance found that the first assembly retained the previous runner's
     owner and metadata modes. A same-SHA follow-up reported success without replacing
     the existing immutable tree and also collapsed `previous` onto `current`. Keep the
     generation timer disabled until a distinct Release includes existing-tree
     validation, is assembled by the corrected runner, and passes release-local
     observation. The final Release also includes a release-local one-shot runner so the
     next production attempt can approve one pending brief as `trainer`, create one new
     request, generate one pending Feishu-backed JPEG, and authenticate the same bytes
     without first enabling the long-running timer. Generated-image approval, mirror
     scheduling, publishing, QiWe, and sending remain separate gates.
   - The final Xiaoman image-send boundary is tracked in
     [Xiaoman QiWe image send](xiaoman-qiwe-image-send.md). The reviewed contract uses
     QiWe async URL upload plus a correlated Webhook before `/msg/sendImage`. The
     deterministic provider-PNG-to-final-JPEG path resolves the code-level format gap;
     the additive Postgres state stores only hashed correlation/idempotency and
     sanitized claim/audit facts. A guarded upload worker, bounded callback command,
     disabled-by-default staging webhook bridge, production worker activation, and
     production callback bridge activation boundary now exist, but sending remains
     disabled until owner-approved evidence and persistent runtime enablement prove the
     full route. The staging webhook bridge accepts only a digest-pinned sidecar under
     the fixed immutable staging release root; the provider/storage/readback path plus
     callback credential shape still require owner-approved staging evidence.
     Feishu-backed generated images use a separately gated staging bridge: authenticated
     Feishu bytes are uploaded to the non-deprecated QiWe SDK temporary-storage
     endpoint, its memory-only URL is allowlisted and read back for complete JPEG
     identity, and only then enters the existing asynchronous URL upload path. Track
     this reviewed boundary in
     [Xiaoman Feishu-To-QiWe Delivery Boundary](xiaoman-feishu-qiwe-delivery-boundary.md).
     Only the matched Huabaosi/QiWe staging live feature pair may claim this storage
     type: staging requires `huabaosi-staging-adapter` plus `qiwe-staging-adapter`.
     Huabaosi production artifacts must contain only `huabaosi-production-adapter` plus
     the guarded `huabaosi-feishu-mirror-adapter` and must not bundle
     `qiwe-production-adapter`. Single-feature builds still fail closed. This is
     code-level implementation evidence, not real staging or send evidence. A read-only
     2026-07-16 Asia/Shanghai server observation confirmed that `paxon-server` still
     lacks the fixed staging env file and immutable staging release root, so real
     staging must first provision those owner-reviewed inputs instead of treating local
     fake smokes as runtime evidence.
   - The guarded [QiWe image-send adapter worker](qiwe-image-send-adapter-worker.md)
     merged in `#119` with shared bounded Rust HTTP, one upload worker, one bounded
     callback command, fake-server coverage, and disposable PostgreSQL integration
     tests. The next boundary is owner-approved isolated staging evidence for the final
     JPEG and callback credential shape before production worker or callback bridge
     activation is used for a real send.
   - The Huabaosi live provider/media entrypoint is being moved behind the non-default
     `huabaosi-staging-adapter` Cargo feature. Its Rust command gate binds an exact
     owner phrase and repository-reviewed database URL hash allowlist before Postgres.
     This is production misuse prevention, not provider/storage approval or staging
     evidence.
   - Real end-to-end acceptance is still incomplete. The acceptance bar is one real
     activity observed from Xiaoman signal intake through image generation, human
     approval, and QiWe group-send arrival. A Release remains infrastructure-only or
     activation-ready until the [completion gate](xiaoman-production-completion-gate.md)
     records the required staging, production enablement, activation, and real activity
     evidence.

3. Product feature packages
   - New Agent behavior belongs in `agents/`, `skills/`, `workflows/`, `mcp/`,
     `runtime/`, or `deploy/` according to ownership.
   - Every new package needs documentation, manifest metadata, validation, and a
     production-boundary note before implementation is considered complete.
   - Priority package boundaries now exist for weather, knowledge retrieval, Postgres
     context, Erhua consultation, Xiaoman activity signal, visual asset request, Si
     Laoshi daily operations, Feishu MCP, Postgres MCP, Hermes bundles, systemd, nginx,
     release manifests, rollback, smoke, fixtures, inventory, and CI helpers.
   - `skills/qintopia-tools` remains a compatibility package. Do not add unrelated new
     capabilities there; create or extend a capability package instead.
   - Xiaoman activity lifecycle routing uses the Postgres-owned `pre_event`, `in_event`,
     and `post_event` phases. Keep phase transitions forward-only and route only to the
     fixed internal capability sets documented in
     [Xiaoman activity lifecycle phases](xiaoman-activity-lifecycle-phases.md).
   - Package scaffolding is not production enablement. Runtime behavior still needs
     package-level tests, replay fixtures, reviewed manifests, and owner approval before
     any server repoint or external adapter change.

4. Archive retention cleanup
   - Permanent deletion of M12 archives is not approved.
   - Retention cleanup requires a separate owner-approved window and a new plan.

5. Documentation system hygiene
   - Keep current-state docs aligned with the release/current runtime model.
   - Keep M9/M10/M12 execution records as historical evidence rather than active plans.
   - Do not delete required deploy evidence docs while `pnpm deploy:preflight` and
     `pnpm deploy:release-model:check` still use them as validation inputs.

## Rules For New Work

- Create a branch from `master` before development.
- Start with a short design or task note in `docs/plans/active/`, the target package
  README, or the package manifest.
- Keep code organized by Agent OS capability, not by implementation language.
- Use only the existing implementation languages and tooling families: TypeScript or
  JavaScript, Python, Rust, shell, SQL, YAML, JSON, and Markdown.
- Do not introduce Java, Gradle, Maven, Kotlin, Go, Swift, C#, PHP, Ruby, Elixir, or
  other new language/toolchain stacks without an explicit architecture decision from the
  owner.
- Do not add a top-level language bucket such as `python/`, `rust/`, `typescript/`, or
  `java/`.
- Do not copy live Hermes profile state or production secrets into git.
- Do not edit production servers directly. Server changes must use reviewed artifacts,
  release directories, runbooks, smoke checks, and rollback notes.

## Required PR Evidence

Every PR should include:

- branch name and affected domain
- document or manifest updated before implementation
- validation commands and results
- production boundary touched or not touched
- rollback or decommission note when runtime behavior changes
- owner decision link or note for architecture, language, external adapter, or profile
  distribution changes
