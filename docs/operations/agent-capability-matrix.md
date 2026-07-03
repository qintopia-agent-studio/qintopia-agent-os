# Agent Capability Matrix

Updated: 2026-07-03

This matrix summarizes the active Agent packages after M6.1. It is an operations view,
not a permission implementation.

| Agent       | Status            | Main capabilities                                                            | External send risk | Writes data | Requires approval                                                   |
| ----------- | ----------------- | ---------------------------------------------------------------------------- | ------------------ | ----------- | ------------------------------------------------------------------- |
| `default`   | adopting          | routing, coordination, escalation                                            | possible           | possible    | production, publication, spending, policy, member decisions         |
| `erhua`     | adopting          | QiWe group replies, Public-safe context, consultation handoff, trainer notes | yes                | yes         | live ops, refunds, compensation, complaints, internal disclosure    |
| `xiaoman`   | adopting          | activity signals, work-item creation, visual/evidence/send preparation       | yes                | yes         | publication, group sends, private material, unverified field claims |
| `wenyuange` | adopting          | knowledge lookup, evidence, source quality, disclosure filtering             | no                 | yes         | member-scoped data, external/internal disclosure, writes            |
| `silaoshi`  | adopting          | operations SOP, checklists, follow-up drafts, scheduled ops jobs             | yes                | yes         | announcements, budget, rules, member handling, production changes   |
| `guanerye`  | adopting          | engineering analysis, dry-runs, validation, rollback, handoff                | yes                | yes         | production changes, secrets, destructive commands, migrations       |
| `huabaosi`  | draft/review-pool | visual briefs, prompts, captions, internal creative artifacts                | no                 | yes         | external use, private material, production adapter changes          |

## Shared Rules

- Agent-to-Agent work must go through Agent OS capabilities, work items, artifacts, and
  events.
- Raw prompt handoff is not a system interface.
- Profile packages may contain reviewed templates and contracts, not live runtime state.
- `xiaoqin` is not an active Agent package.
- Huabaosi shadow/Rust material remains review-pool until owner approval.

## Runtime State Exclusions

Do not add these under `agents/*`:

- `.env` or `.env.*`
- `auth.json`, `auth.lock`, tokens, credentials, private keys
- `memories/`, `sessions/`, `cache/`, `logs/`, `state/`, `tmp/`
- `state.db`, `*.db`, `*.sqlite`, WAL/SHM files
- raw private chat, member profiles, request dumps, generated runtime artifacts
