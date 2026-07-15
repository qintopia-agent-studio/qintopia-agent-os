# Xiaoman Runtime Notes

Observed read-only on 2026-07-15:

- Runtime path: `/home/ubuntu/.hermes/profiles/xiaoman`
- User service: `hermes-gateway-xiaoman.service`
- Observed plugin: `qintopia-tools`
- Observed non-secret config shape includes `SOUL.md`, `config.yaml`, `profile.yaml`,
  `webhook_subscriptions.json`, `channel_directory.json`, and `cron/jobs.json`.

Xiaoman should create Agent OS work items through governed workflow APIs. Profile-local
webhook subscriptions are runtime configuration, not the long-term workflow source.

`profile-bundle/` is observation-only. Its strict renderer uses four server-local
identity inputs and writes only to a new directory. Do not link its output into the live
profile until a later PR records production parity and first-cutover rollback evidence.
The values migration command is manual, root-only, source-hash locked, no-clobber, and
may create only `/etc/qintopia/xiaoman-profile-bundle-values.json`.
