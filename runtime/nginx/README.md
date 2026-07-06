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

## Validation

```bash
pnpm runtime:nginx:check
```
