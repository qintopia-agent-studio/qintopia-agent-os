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
