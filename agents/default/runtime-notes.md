# Default Runtime Notes

Observed read-only on 2026-07-03:

- Runtime path: `/home/ubuntu/.hermes`
- User service: `hermes-gateway.service`
- Observed top-level runtime areas include `profiles`, `skills`, `plugins`, `scripts`,
  `cron`, `memories`, `sessions`, `cache`, `logs`, and `state`.
- Observed non-secret config shape includes `SOUL.md`, `config.yaml`, `profile.yaml`,
  `channel_directory.json`, and `cron/jobs.json`.

Do not copy the runtime root wholesale. The monorepo should keep reviewed dispatcher
templates only.
