# Qintopia Weather Skill

This package owns the Agent-facing `qintopia_weather_lookup` capability.
`skills/qintopia-tools` keeps only the Hermes registration shell and delegates weather
calls here.

## Capability

- fixed Qintopia location weather lookup only
- QWeather current, hourly, minutely, alert, and air-quality data
- 07:00 Erhua morning weather broadcast contract with forecast-first wording
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

## Morning Broadcast Contract

The 07:00 Erhua group weather broadcast must be a day forecast and outing hint, not a
current-conditions report.

Returned payloads include:

- `daily_forecast`: primary day forecast structure for rain/umbrella windows,
  thunderstorm windows, warning state, and concise outing advice.
- `morning_reference`: current temperature, feels-like temperature, wind, humidity,
  precipitation, and AQI as secondary morning context only.
- `warning_status`: one of `present`, `none`, or `unknown`.
- `morning_broadcast`: short group-chat copy that starts with `秦托邦今日天气：` and
  keeps `今早参考` last.

Warning copy rules:

- `present`: include warning type, level, effective time, and a short action reminder.
- `none`: include `截至早上播报时，官方暂无秦托邦天气预警`.
- `unknown`: include `官方预警数据暂未确认`; do not write this as no warning.

The broadcast must not lead with copy like `现在：晴，约26°C...`. Current weather,
feels-like temperature, wind, and AQI belong only under `今早参考`.

## Validation

```bash
pnpm skills:qintopia-weather:check
pnpm skills:qintopia-tools:check
pnpm mcp:adapters:check
pnpm check:light
```
