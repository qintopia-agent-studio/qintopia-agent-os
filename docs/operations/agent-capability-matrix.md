# Agent Capability Matrix

Updated: 2026-09-22

This matrix summarizes Agent packages and explicitly local additions. It is an
operations view, not a permission implementation.

| Agent                    | Status            | Main capabilities                                                            | External send risk | Writes data | Requires approval                                                   |
| ------------------------ | ----------------- | ---------------------------------------------------------------------------- | ------------------ | ----------- | ------------------------------------------------------------------- |
| `default`                | adopting          | routing, coordination, escalation                                            | possible           | possible    | production, publication, spending, policy, member decisions         |
| `erhua`                  | adopting          | QiWe group replies, Public-safe context, consultation handoff, trainer notes | yes                | yes         | live ops, refunds, compensation, complaints, internal disclosure    |
| `anan`                   | draft/local-only  | welcome orchestration through shared scopes, work items and artifacts        | no (local only)    | yes         | production activation, real attachments and real delivery           |
| `xiaoman`                | adopting          | activity signals, work-item creation, visual/evidence/send preparation       | yes                | yes         | publication, group sends, private material, unverified field claims |
| `wenyuange`              | adopting          | knowledge lookup, evidence, source quality, disclosure filtering             | no                 | yes         | member-scoped data, external/internal disclosure, writes            |
| `silaoshi`               | adopting          | operations SOP, checklists, follow-up drafts, scheduled ops jobs             | yes                | yes         | announcements, budget, rules, member handling, production changes   |
| `guanerye`               | adopting          | engineering analysis, dry-runs, validation, rollback, handoff                | yes                | yes         | production changes, secrets, destructive commands, migrations       |
| `huabaosi` (阿靓/画报司) | draft/review-pool | visual briefs, prompts, captions, internal creative artifacts                | no                 | yes         | external use, private material, production adapter changes          |

## Shared Rules

- Agent-to-Agent work must go through Agent OS capabilities, work items, artifacts, and
  events.
- Raw prompt handoff is not a system interface.
- Profile packages may contain reviewed templates and contracts, not live runtime state.
- `xiaoqin` is not an active Agent package in the current phase. Future Xiaoqin work
  requires a new non-WorkTool Agent contract and owner-approved registry change.
- Huabaosi shadow/Rust material remains review-pool until owner approval.
- Huabaosi's profile package and external image adapter remain draft/review-pool. The
  separate AgentOS visual-brief control-plane workflow is active and may only create
  internal `poster_brief` artifacts; it does not make the profile or image generation
  production-approved.
- The foundation batch wires Erhua identity, knowledge and self-memory tools and the
  Anan/Huabaosi/Erhua welcome executors to local durable services. Welcome PNG rendering
  is real; source facts, application attachments and sends are synthetic. Real-model
  understanding, real channel ingestion and production delivery are not established.
- `anan` is independently registered but excluded from production profile and deploy
  targets. A package registration does not authorize runtime activation.
- Every active Agent template must declare `dry_run_expectations`.

## Minimum Checks

Run before changing an Agent package:

```bash
pnpm agents:check
pnpm policy:check
```

Run package-specific checks when touching behavior:

| Agent       | Minimum behavior check                                                          |
| ----------- | ------------------------------------------------------------------------------- |
| `default`   | routing dry-run note plus relevant workflow smoke before enabling routes        |
| `erhua`     | `pnpm test:qiwe`                                                                |
| `anan`      | `pnpm test:business -- --feature person-foundation`; local-only registry checks |
| `xiaoman`   | `pnpm smoke:sidecar`                                                            |
| `wenyuange` | context/message-store smoke for lookup behavior                                 |
| `silaoshi`  | script-level dry-run after scheduled jobs are split into workflows              |
| `guanerye`  | local or sandbox validation only unless production approval exists              |
| `huabaosi`  | `pnpm smoke:sidecar`; production visual adapter remains out of scope            |

## Runtime State Exclusions

Do not add these under `agents/*`:

- `.env` or `.env.*`
- `auth.json`, `auth.lock`, tokens, credentials, private keys
- `memories/`, `sessions/`, `cache/`, `logs/`, `state/`, `tmp/`
- `state.db`, `*.db`, `*.sqlite`, WAL/SHM files
- raw private chat, member profiles, request dumps, generated runtime artifacts
