# Silaoshi Runtime Notes

Observed read-only on 2026-07-03:

- Runtime path: `/home/ubuntu/.hermes/profiles/silaoshi`
- User service: `hermes-gateway-silaoshi.service`
- Observed scheduled script names include resident onboarding, daily check, daily brief,
  weekly report, and holiday announcement jobs.
- Observed non-secret config shape includes `SOUL.md`, `config.yaml`, `profile.yaml`,
  `webhook_subscriptions.json`, `channel_directory.json`, and `cron/jobs.json`.

Scripts should be classified into workflow packages before deployment from this
monorepo. Runtime reports and generated files stay out of git.
