# Current Roadmap

Updated: 2026-07-13

The monorepo migration and server cleanup phases are complete. The historical execution
log is archived at
[../completed/monorepo-migration.md](../completed/monorepo-migration.md).

Use this document for current and future work. Do not reopen the completed migration log
unless correcting historical evidence.

## Current State

- The monorepo is the collaboration source of truth.
- Production Agent OS sidecar, workers, release-managed MCP commands, and reviewed
  Hermes plugins run from `/home/ubuntu/qintopia-agent-os-releases/current`.
- Hermes remains the Agent runtime under `/home/ubuntu/.hermes`.
- Hermes profile live state, including `.env`, sessions, logs, cache, memory, auth, and
  local config overrides, stays outside git.
- WorkTool, OpenClaw, and the current WorkTool-bound Xiaoqin runtime are archived and
  deprecated. Future Xiaoqin work requires a new non-WorkTool Agent design.

## Active Directions

1. Profile distribution and bundle design
   - Align with Hermes profile distribution behavior.
   - Treat `SOUL.md`, skills, cron, and MCP declarations as reviewed distribution-owned
     files.
   - Preserve runtime data and local secrets on the server.
   - Start with one low-risk profile before touching group-facing behavior.

2. External adapter allowlists
   - Keep real external send paths disabled until allowlists, runtime config, smoke, and
     rollback are reviewed.
   - Do not broaden Feishu, QiWe, or workbench permissions in the same PR as unrelated
     feature work.
   - The proposed
     [Aliang image-generation adapter](aliang-production-image-generation.md) uses the
     historically observed OpenAI-compatible `gpt-image-2` path as its implementation
     target. Real image generation, user-media storage, human review, and publication
     remain separate gates; the merged request intake is not in production `v0.2.6`.
   - The final Xiaoman image-send boundary is tracked in
     [Xiaoman QiWe image send](xiaoman-qiwe-image-send.md). The reviewed contract uses
     QiWe async URL upload plus a correlated Webhook before `/msg/sendImage`. The
     deterministic provider-PNG-to-final-JPEG path resolves the code-level format gap;
     sending remains disabled until the provider/storage/readback path and callback
     credential shape are verified in owner-approved staging.

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
