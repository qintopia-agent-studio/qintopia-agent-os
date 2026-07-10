# Weather Fixtures

Fixtures for `skills/qintopia-weather`.

These files are safe replay inputs and expected outputs. They must not contain live
QWeather credentials, arbitrary city expansion, private user requests, or production
logs.

## Files

- `qweather-success.json`: QWeather success path for the fixed Qintopia location.
- `qweather-full-day.json`: sanitized fixed-location QWeather bundle covering the three
  local-day periods, minutely and later hourly rain, multiple warnings, and next-day
  rows that must be excluded from the current-day summary.
- `open-meteo-fallback.json`: fallback path when QWeather is unavailable.
- `missing-credentials.json`: safe degraded response when weather credentials are
  absent.

## Validation

```bash
pnpm skills:qintopia-weather:check
```
