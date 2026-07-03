# Xiaoman Runtime Notes

Observed read-only on 2026-07-03:

- Runtime path: `/home/ubuntu/.hermes/profiles/xiaoman`
- User service: `hermes-gateway-xiaoman.service`
- Observed plugin: `qintopia-tools`
- Observed non-secret config shape includes `SOUL.md`, `config.yaml`, `profile.yaml`,
  `webhook_subscriptions.json`, `channel_directory.json`, and `cron/jobs.json`.

Xiaoman should create Agent OS work items through governed workflow APIs. Profile-local
webhook subscriptions are runtime configuration, not the long-term workflow source.
