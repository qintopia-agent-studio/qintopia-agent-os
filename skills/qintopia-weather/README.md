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

## Fixed Location Policy

Default lookups use QWeather coordinate/grid weather for the fixed `秦托邦·栗峪口` point
`108.5666545,34.0261288`. This point is aligned to the public OpenStreetMap/Nominatim
`栗峪口村` village node checked on 2026-07-06.

This is not raw measured data from a 栗峪口 weather station. Until a stable public
station id and API are confirmed, the primary source remains QWeather by fixed
coordinates. QWeather city-based warning and air-quality calls still use `鄠邑区`
through `QINTOPIA_WEATHER_QWEATHER_CITY`.

## Morning Broadcast Contract

The 07:00 Erhua group weather broadcast must be a day forecast and outing hint, not a
current-conditions report.

Returned payloads include:

- `daily_forecast`: primary local-day forecast structure with an explicit availability
  status and fixed midday, afternoon, and evening periods.
- `morning_reference`: current temperature, feels-like temperature, wind, humidity, and
  precipitation as secondary morning context only.
- `warning_status`: one of `present`, `none`, or `unknown`.
- `air_quality`: the independent `鄠邑区` AQI result, or `null` when it is unknown.
- `morning_broadcast`: deterministic group-chat copy of at most eight non-empty lines.

`daily_forecast.forecast_date` is the `Asia/Shanghai` calendar date of the forecast, not
a rolling 24-hour label. `daily_forecast.status` is one of:

- `complete`: every fixed period has all of its expected hourly rows.
- `partial`: at least one local-day hourly row is usable, but one or more periods are
  incomplete or unknown.
- `unknown`: there are no usable local-day hourly rows.

`daily_forecast.periods` always contains these entries in this order:

| `id`        | `label` | Local interval  | Expected hours |
| ----------- | ------- | --------------- | -------------- |
| `midday`    | 中午    | `11:00`–`13:59` | 3              |
| `afternoon` | 下午    | `14:00`–`17:59` | 4              |
| `evening`   | 晚上    | `18:00`–`22:59` | 5              |

Every period includes `id`, `label`, `start_local`, `end_local`, `status`, `condition`,
`temp_min_c`, `temp_max_c`, `max_precip_probability_pct`, `max_precip_mm`,
`wind_summary`, and `coverage_hours`. A period is `complete` only when all expected
local hours are available, `partial` when some are available, and `unknown` when none
are available. Unknown measurements use `null`; they must not be invented from the
current observation.

Hourly forecast data is the whole-day baseline. Minutely precipitation may add a
separate near-term hint for roughly the next two hours, but it must never replace or
hide later hourly rain or thunderstorm windows. Only hourly rows whose local date equals
`forecast_date` participate in the day periods, day temperature range, or day risk
summary. In particular, next-day `00:00` and later rows must not be included in the
current day's evening period or temperature range.

The member-facing copy follows this information order, while omitting only genuinely
unavailable optional detail:

1. Local date and `秦托邦·栗峪口` title.
2. Available whole-day trend.
3. One `分时：` line containing 中午、下午、晚上 in the fixed order; an unavailable
   period says `暂未确认`.
4. Official warning state.
5. Morning observation under `今早参考`.
6. AQI under `空气（鄠邑区）`; missing AQI says `AQI 暂未确认`.
7. A risk-matched reminder ending with `二花…播报完毕～`.

The first line must not begin with `现在` or `此时`, and the complete broadcast must
contain no more than eight non-empty lines. When hourly data is partial or unknown, the
copy must say what is unconfirmed and must not use optimistic claims such as
`降水信号不明显`, `天气稳`, or `轻松安排`.

Warning copy rules:

- `present`: sort alerts by red, orange, yellow, blue, then unknown severity; render at
  most two high-priority alerts, translate provider colors such as `Red`, `Orange`,
  `Yellow`, and `Blue` to Chinese, and append `另有 N 条` when more remain.
- `none`: include `截至早上播报时，官方暂无秦托邦天气预警`.
- `unknown`: include `官方预警数据暂未确认`; do not write this as no warning.

The broadcast must not lead with copy like `现在：晴，约26°C...`. Current weather,
feels-like temperature, humidity, wind, and precipitation belong only under `今早参考`.
AQI is a separate line labeled `空气（鄠邑区）` because its city-based source is not the
fixed-point forecast grid.

Open-Meteo remains a limited fallback. It may populate only measurements actually
returned by its response. It must keep official warning status `unknown`, AQI unknown,
and minute-level precipitation unknown; it must never turn unavailable evidence into an
official `none` or an optimistic weather conclusion.

## Validation

```bash
pnpm skills:qintopia-weather:check
pnpm skills:qintopia-tools:check
pnpm mcp:adapters:check
pnpm check:light
```
