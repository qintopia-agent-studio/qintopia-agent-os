# Weather Fixtures

Fixtures for `skills/qintopia-weather`.

These files are safe replay inputs and expected outputs. They must not contain live
QWeather credentials, arbitrary city expansion, private user requests, or production
logs.

## Files

- `qweather-success.json`: QWeather success path for the fixed Qintopia location.
- `open-meteo-fallback.json`: fallback path when QWeather is unavailable.
- `missing-credentials.json`: safe degraded response when weather credentials are
  absent.

## Validation

```bash
pnpm skills:qintopia-weather:check
```
