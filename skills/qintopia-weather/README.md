# Qintopia Weather Skill

This package owns the Agent-facing `qintopia_weather_lookup` capability.
`skills/qintopia-tools` keeps only the Hermes registration shell and delegates weather
calls here.

## Capability

- fixed Qintopia location weather lookup only
- QWeather current, hourly, minutely, alert, and air-quality data
- Open-Meteo fallback as limited trend-only evidence
- member-safe output for Erhua and other approved profiles

## Layering

- `skills/qintopia-weather` owns Qintopia policy: fixed Qintopia location, member-safe
  payload, forbidden capabilities, and fallback wording.
- `mcp/weather-provider` owns the provider adapter contract: QWeather/Open-Meteo fetch,
  timeout, normalization, error, and secret boundaries.
- `skills/qintopia-tools/variants/erhua` owns only the current Hermes tool registration
  shell while production still loads that plugin.

## Guardrails

- Do not expose arbitrary city weather lookup.
- Do not expose typhoon, ocean, marine, tide, solar-radiation, POI, station-detail, or
  historical weather tools.
- Do not put QWeather credentials or private keys in git.
- Do not claim official warnings when fallback data is used.

## Validation

```bash
pnpm skills:qintopia-weather:check
pnpm skills:qintopia-tools:check
pnpm mcp:adapters:check
pnpm check:light
```
