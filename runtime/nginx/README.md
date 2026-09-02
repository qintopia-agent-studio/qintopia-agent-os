# Runtime: nginx

`runtime/nginx` owns reviewed webhook and API ingress templates. Templates are inert in
the release until an explicit production operation renders them with server-local
values.

## Responsibility

- Version nginx route templates only when Agent OS owns the route.
- Keep secrets, TLS certificates, private keys, live snippets, and server-only includes
  outside git.
- Require config validation, endpoint smokes, and rollback for production ingress.

## Production Boundary

- The Qiwe ingress scaffold ships in the deploy bundle but remains disabled by default.
- The repository contains no callback path token or internal authentication token.
- Only the signed, owner-approved `production-runtime-one-shot` apply target may render
  the template through `release/current`; rollback is a separate fixed target.
- Real provider callback registration and event-mapping activation are separate owner
  steps.

## Qiwe Webhook Contract

`templates/qiwe-webhook.location.conf.template` and
`templates/qiwe-webhook.disabled.conf` define the ingress slot:

- The public URL contains a server-local high-entropy path token and uses an exact nginx
  location. The token is never committed or written to access/error logs.
- nginx discards client upstream headers and injects `X-Qintopia-Qiwe-Ingress-Auth` with
  a different server-local secret before proxying to the adapter on `127.0.0.1:18661`.
- The adapter must run with `QIWE_WEBHOOK_AUTH_REQUIRED=true` and the matching
  `QIWE_WEBHOOK_AUTH_TOKEN`; a missing token prevents startup.
- Request size and upstream timeouts are bounded so the adapter can acknowledge the
  webhook within Qiwe's three-second deadline and schedule work asynchronously.

The high-entropy route authenticates possession of the callback URL, and the internal
header proves that a request traversed this nginx boundary. Neither is a vendor
signature. Production activation still requires owner review, a server-local secret
handoff, the fixed disabled include bootstrap, and one sanitized real callback captured
in shadow mode. See `docs/operations/qiwe-webhook-ingress-production-runbook.md`.

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

The check is local-only. It verifies the exact route, loopback upstream, fixed header
overwrite, logging boundary, limits, fixed deployment wiring, atomic restore, and
positive/negative smoke behavior with fake processes. It performs no nginx, Qiwe, or
production action.
