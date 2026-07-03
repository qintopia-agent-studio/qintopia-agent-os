# Guanerye Runtime Notes

Observed read-only on 2026-07-03:

- Runtime path: `/home/ubuntu/.hermes/profiles/guanerye`
- User service: `hermes-gateway-guanerye.service`
- Observed script name: `send_promo_reminder.sh`
- Observed non-secret config shape includes `SOUL.md`, `config.yaml`, `profile.yaml`,
  `channel_directory.json`, and `cron/jobs.json`.

Engineering automation should produce reviewed runbooks and dry-runs. Production writes
still require explicit human approval and git-reviewed changes.
