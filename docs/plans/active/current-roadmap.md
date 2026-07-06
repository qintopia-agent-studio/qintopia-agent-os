# Current Roadmap

Updated: 2026-07-06

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
