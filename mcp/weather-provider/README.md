# MCP Adapter: Weather Provider

`mcp/weather-provider` defines the provider-level weather adapter contract. It owns how
weather data is fetched and normalized. Agent-facing policy remains in
`skills/qintopia-weather`.

## Responsibility

- Adapt QWeather current, hourly, minutely, alert, and air-quality data.
- Adapt Open-Meteo fallback data when QWeather is unavailable.
- Normalize provider errors, timeouts, and partial responses.
- Keep API credentials, private keys, auth files, request logs, and raw provider dumps
  out of git.

## Non-Responsibility

- It does not decide which Agent may use weather.
- It does not expose arbitrary city lookup to frontline Agents by default.
- It does not decide Qintopia fixed-location policy, answer wording, or member-facing
  guardrails.

Those rules live in `skills/qintopia-weather`.

## Production Boundary

- External network access is allowed only through reviewed provider calls.
- `secrets` are runtime-only and must not be committed.
- No database writes or external sends are owned by this adapter.
- New provider capabilities such as typhoon, ocean, marine, tide, solar radiation, POI,
  station detail, or historical weather require owner-approved architecture docs before
  being exposed to any Agent skill.

## Validation

```bash
pnpm mcp:adapters:check
pnpm skills:qintopia-weather:check
```
