# 2026-08-14 Qintopia Root TLS Renewal

## Scope

Restore the expired `qintopia.cn` origin certificate and move that origin to automatic
Certbot renewal. The `www.qintopia.cn` Tencent CDN certificate, DNS layout, application
routes, upstreams, and CDN configuration were intentionally left unchanged.

## Evidence

- The public `qintopia.cn` origin and `/etc/nginx/ssl/qintopia.cn_bundle.pem` presented
  the same TrustAsia certificate, which expired at `2026-07-29 23:59:59 UTC`.
- Nginx was active and its configuration test passed, so the failure was isolated to
  certificate validity rather than service availability or route syntax.
- `certbot.timer` was active, but Certbot managed only `pms.qintopia.cn`; no
  `qintopia.cn` renewal declaration existed.
- The Tencent CDN endpoint for `www.qintopia.cn` presented a separate valid certificate
  and continued returning the static site successfully.
- Authoritative DNS mapped `qintopia.cn` to the origin, and HTTP port 80 was publicly
  reachable for ACME HTTP-01 validation.

## Root Cause

The origin Nginx server referenced a manually installed certificate under
`/etc/nginx/ssl`. The certificate covered both the root and `www` names, but certificate
domain coverage did not deploy renewed Tencent certificates to the self-managed Nginx
host. The active Certbot timer exited successfully because its only managed production
name was unrelated to the root domain.

TLS validation failed before Nginx could return the root-page redirect to the healthy
Tencent CDN endpoint. HSTS therefore made the expired root certificate a complete
browser outage rather than a bypassable warning.

## Resolution

- Saved the pre-change Nginx site configuration at
  `/etc/nginx/backups/qintopia.cn.pre-certbot-20260814T030901Z` and verified that its
  SHA-256 matched the active pre-change file.
- Successfully ran a Let's Encrypt staging request through the Certbot Nginx plugin.
- Issued and installed a production ECDSA certificate for `qintopia.cn` only.
- Replaced the origin certificate paths with the Certbot-managed
  `/etc/letsencrypt/live/qintopia.cn/fullchain.pem` and `privkey.pem` paths.
- Retained Tencent CDN certificate ownership for `www.qintopia.cn`.
- Kept the existing twice-daily `certbot.timer`; the new renewal declaration uses the
  Nginx authenticator and installer.

## Validation

The following checks passed on 2026-08-14:

- Certbot staging HTTP-01 certificate request.
- Production certificate issuance and Nginx installation.
- `nginx -t` after installation and after renewal simulation.
- Public TLS verification for `qintopia.cn`, presenting the new Let's Encrypt
  certificate valid through `2026-11-12 02:11:19 UTC`.
- Public `https://qintopia.cn/` returned `301` to `https://www.qintopia.cn/` without
  disabling certificate verification.
- Public `https://www.qintopia.cn/` retained its Tencent CDN certificate and returned
  `200`.
- `certbot renew --cert-name qintopia.cn --dry-run --no-random-sleep-on-renew` completed
  successfully.
- `nginx.service` and `certbot.timer` remained active.

One initial renewal simulation used Certbot's normal randomized delay and remained
sleeping behind its process lock. That exact dry-run process was terminated, then the
simulation was repeated with `--no-random-sleep-on-renew` and passed. Production timer
behavior keeps the normal randomized delay.

## Rollback

The saved Nginx file can restore the pre-Certbot configuration if the generated config
causes an unrelated regression:

```bash
sudo cp \
  /etc/nginx/backups/qintopia.cn.pre-certbot-20260814T030901Z \
  /etc/nginx/sites-available/qintopia.cn
sudo nginx -t
sudo systemctl reload nginx
```

This configuration rollback points back to an expired certificate and is therefore not a
service-recovery certificate strategy. A rollback must pair the restored config with
another currently valid certificate before public traffic is considered healthy.

## Remaining Boundary

- Nginx still reports the pre-existing `protocol options redefined` warning for the
  shared IPv6 TLS listener. Syntax validation succeeds; this change did not introduce or
  remediate that listener ownership issue.
- A bare GET to `/webserver/wework/` reaches the TLS endpoint but returns an application
  layer `502`. The certificate diff did not modify that route or its upstream. Diagnose
  it separately if that bare path is expected to be healthy.
- The server now renews the root certificate automatically, but the repository still
  lacks an external expiry alert proving what clients actually receive. Add a bounded
  public certificate monitor with warning thresholds before the next renewal window.
