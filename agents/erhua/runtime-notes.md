# Erhua Runtime Notes

Observed read-only on 2026-07-03:

- Runtime path: `/home/ubuntu/.hermes/profiles/erhua`
- User service: `hermes-gateway-erhua.service`
- Observed plugins: `qiwe-platform`, `qintopia-tools`
- Observed script names: `check-dify-kb.sh`, weather context/broadcast scripts, and
  nightly reminder script.
- Observed non-secret config shape includes `SOUL.md`, `config.yaml`,
  `activity-feishu-mapping.json`, `activity-reminder-policy.json`,
  `channel_directory.json`, and `cron/jobs.json`.

Trainer memory belongs in audited sidecar/Postgres paths. Stable persona and guardrails
can be templated here only after owner review.

The broadcast script name now has a reviewed source at
`skills/qintopia-weather/scripts/qintopia-erhua-weather-broadcast.py`. It emits only the
canonical forecast-first `morning_broadcast` and performs no send. The live 07:00 job,
the current broadcast script hash, and `qintopia-erhua-weather-context.py` remain
runtime-only evidence pending a read-only inventory; do not infer or overwrite their
contents from this note.

## Local welcome task runtime

`welcome_runtime.py` registers this Agent's bounded welcome tools with
`workflows/resident-welcome/scripts/local_agent_runtime.py`. The host supplies the
locked WorkItem and business input through one stdin pipe; model arguments must be
empty. Tool output is consumed by the governed Rust workflow and tied to call/task,
input/output hashes and the exact plugin source hash. The child exposes no database,
network or direct-send tools. This is `local_scripted_agent_runtime` evidence, not real
Hermes or LLM execution and not production activation.
