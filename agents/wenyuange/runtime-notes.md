# Wenyuange Runtime Notes

Observed read-only on 2026-07-03:

- Runtime path: `/home/ubuntu/.hermes/profiles/wenyuange`
- User service: `hermes-gateway-wenyuange.service`
- Observed plugin: `qintopia-tools`
- Observed non-secret config shape includes `SOUL.md`, `config.yaml`, `profile.yaml`,
  and `channel_directory.json`.

The current default caller gate for message-store MCP is Wenyuange. Other Agents should
reach message evidence through controlled context tools, not direct raw access.
