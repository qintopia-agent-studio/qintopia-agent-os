# Qintopia Weather Skill

This package defines the standalone weather capability that should be extracted from the
current `qintopia-tools` Hermes profile variants.

The active implementation still lives inside
`skills/qintopia-tools/variants/erhua/__init__.py` for compatibility. New weather
behavior should be designed here first, then migrated behind the existing
`qintopia_weather_lookup` interface.

## Capability

- fixed Qintopia location weather lookup only
- QWeather current, hourly, minutely, alert, and air-quality data
- Open-Meteo fallback as limited trend-only evidence
- member-safe output for Erhua and other approved profiles

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
pnpm check:light
```
