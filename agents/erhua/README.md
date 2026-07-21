# Agent: Erhua

`erhua` is the QiWe/WeCom front-office cat-assistant Agent for Qintopia community
groups. It represents the community-facing digital presence of Erhua the cat while
handling Public-safe group replies, member-aware greetings, light consultation intake,
controlled handoff, and trainer memory submission through audited backend paths.

## Scope

- Reply only when mentioned or clearly cued in allowed groups.
- Recognize the current speaker and mentioned members when safe Postgres context exists,
  including direct chat and group mention flows.
- Use controlled context lookup for Public-safe answers.
- Escalate availability, booking, refund, compensation, policy, complaint, and uncertain
  operational questions to a human owner or live-ops path.
- Submit trainer notes through the audited Erhua training-memory path when allowed.

## Boundaries

- Must not promise price, availability, refunds, compensation, contract changes, or
  policy exceptions.
- Must not expose internal SOPs, member records, raw message history, or private profile
  state.
- Must not guess a member identity when context is missing or ambiguous.
- Must not directly read unrestricted message stores or Feishu documents.
- Must not send direct messages unless the channel policy and contact guard allow it.

## Runtime Source

- Server profile observed read-only: `/home/ubuntu/.hermes/profiles/erhua`
- Current service observed read-only: `hermes-gateway-erhua.service`
- Related active package: `skills/qiwe`
- Release-owned weather output entrypoint:
  `skills/qintopia-weather/scripts/qintopia-erhua-weather-broadcast.py`
- Runtime `.env`, memories, identities, caches, locks, logs, and state databases are
  excluded from this package.
- `config.template.yaml` is a non-secret, field-limited model overlay. It is rendered
  against the runtime-local config by the governed deploy runner; it is not a complete
  Hermes config and must never receive an inline credential.

The weather entrypoint is packaged but not activated. Erhua's live 07:00 job and
`cron/jobs.json` remain runtime-local until their non-secret structure and script hashes
are inventoried and a separate reviewed profile cutover supplies rollback evidence.

## Validation

```bash
pnpm test:qiwe
pnpm runtime:hermes:check
pnpm agents:profile-bundles:check
pnpm registry:check
pnpm policy:check
```
