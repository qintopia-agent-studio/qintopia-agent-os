# Runtime: nginx

`runtime/nginx` is the future package boundary for webhook and API ingress route
templates.

## Responsibility

- Version nginx route templates only when Agent OS owns the route.
- Keep secrets, TLS certificates, private keys, live snippets, and server-only includes
  outside git.
- Add route smoke checks before any production ingress change.

## Production Boundary

- This package is intentionally draft-only.
- Do not add or enable routes in production from this package without a separate
  owner-approved ingress plan.

## Observed Production TLS Ownership

As of 2026-08-14, production intentionally has two independent TLS termination points:

- `qintopia.cn` terminates on the origin Nginx host and uses a Certbot-managed Let's
  Encrypt certificate for that name only. The system `certbot.timer` owns renewal, and
  the renewal declaration uses the Nginx authenticator and installer.
- `www.qintopia.cn` terminates at Tencent CDN and keeps its CDN-managed certificate.

A certificate listing both DNS names does not deploy that certificate to both
termination points. Keep origin renewal and CDN certificate lifecycle checks explicit,
and never copy certificate private keys into this repository.

The sanitized incident and remediation evidence is recorded in
`docs/reports/2026-08-14-qintopia-root-tls-renewal.md`.

## Validation

```bash
pnpm runtime:nginx:check
```
