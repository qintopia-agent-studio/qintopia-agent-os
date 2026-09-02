# Qiwe Authenticated Webhook Ingress Runbook

This runbook prepares a deployable but default-disabled callback ingress. Repository
tests use fake nginx, systemd, curl, and provider commands; they perform no production
action.

## Fixed Boundary

The signed deploy request accepts only `qiwe-webhook-ingress-apply` or
`qiwe-webhook-ingress-rollback` plus its exact approval phrase. It cannot carry the
callback path, authentication token, callback URL, nginx path, provider command, or
service name.

The release-local script fixes these production paths:

```text
/home/ubuntu/qintopia-agent-os-releases/current
/etc/qintopia/qiwe-webhook-ingress.env
/etc/nginx/sites-available/qintopia.cn
/etc/nginx/snippets/qintopia-qiwe-webhook.conf
/var/lib/qintopia-agent-os-deploy/qiwe-webhook-ingress/rollback.conf
http://127.0.0.1:18661/qiwe/webhook
```

The nginx location is an exact match under `/qiwe/webhook/<secret>`, disables location
access/error logging, accepts only POST, bounds body/timeouts, discards client-provided
upstream headers, and injects the different server-local `X-Qintopia-Qiwe-Ingress-Auth`
value. The adapter remains loopback-only and must require the same internal token.

## One-Time Owner Bootstrap

Do this only in a separately reviewed production change after the Release containing the
scaffold is current:

1. Install `runtime/nginx/templates/qiwe-webhook.disabled.conf` at the fixed snippet
   path, owned by root and not group/world writable.
2. Add exactly this line inside the intended `qintopia.cn` TLS server block:

   ```nginx
   include /etc/nginx/snippets/qintopia-qiwe-webhook.conf;
   ```

3. Remove or disable any broader legacy `/qiwe/webhook` public route. The negative smoke
   requires the fixed wrong path to return 404.
4. Run `nginx -t`, reload nginx, and confirm the disabled include changes no public
   behavior.
5. Generate both values on the production host with its CSPRNG and create the config as
   root mode `0600`; do not choose, paste, or reuse a human-created value:

   ```bash
   umask 077
   public_token="$(openssl rand -base64 48 | tr '+/' '-_' | tr -d '=\n')"
   internal_token="$(openssl rand -base64 48 | tr '+/' '-_' | tr -d '=\n')"
   {
     printf 'QINTOPIA_QIWE_WEBHOOK_PUBLIC_PATH_TOKEN=%s\n' "$public_token"
     printf 'QIWE_WEBHOOK_AUTH_TOKEN=%s\n' "$internal_token"
   } > /etc/qintopia/qiwe-webhook-ingress.env
   chmod 0600 /etc/qintopia/qiwe-webhook-ingress.env
   unset public_token internal_token
   ```

   Apply also rejects obvious repeated and placeholder values. This is a typo guard, not
   a substitute for CSPRNG generation.

6. Configure the Erhua Qiwe adapter with `QIWE_WEBHOOK_AUTH_REQUIRED=true` and the same
   `QIWE_WEBHOOK_AUTH_TOKEN`, keep it on `127.0.0.1:18661`, restart it through its
   reviewed runtime path, and verify direct requests without the header return 401.

Do not paste either value into GitHub inputs, task text, PRs, shell history, journal
queries, or evidence files.

## Apply

Run `Run Production Runtime One-Shot` against the exact current Release SHA:

```text
runtime_one_shot_target=qiwe-webhook-ingress-apply
backfill_date=
payload_sha256=
approval=approved-production-qiwe-webhook-ingress-apply
```

The runner validates the signed request and current manifest, then the script:

1. validates fixed file ownership, modes, bounds, include line, and token shapes;
2. proves adapter requests without auth return 401 and the server-local token returns
   2xx;
3. accepts the old snippet only when it is byte-for-byte the fixed disabled template or
   a canonical rendering of the reviewed active template, then retains it for rollback;
4. renders into a root-only temporary file and atomically replaces only the fixed
   snippet;
5. runs `nginx -t` and reloads `nginx.service`;
6. requires the exact HTTPS callback path to return 2xx and a fixed wrong path to return
   404;
7. automatically restores, validates, reloads, and smokes the old snippet on failure.

Deployment results retain only `qiwe_webhook_ingress=enabled` or a bounded safe failure
class. They do not retain script output, URLs, tokens, request bodies, or nginx output.

## Provider Callback Configuration

No repository file evidences an official Qiwe endpoint and parameter contract for
changing the provider callback URL. The event callback documents describe payloads, not
this configuration mutation. Therefore the repository deliberately does not invent an
API route or parameters and ingress apply does not contact Qiwe.

After confirming the exact current official semantics, an owner may place a reviewed,
root-owned, read-only command at:

```text
/etc/qintopia/qiwe-webhook-provider-callback-command
```

The command receives no arguments. It reads exactly one newline-terminated derived
callback URL from stdin and receives only a fixed provider env-file hint plus a minimal
PATH. The URL does not enter argv or the shared `ubuntu` process environment. The
command must use server-local credentials, verify Qiwe's semantic success response, and
exit nonzero on ambiguity. Bind review to its exact SHA-256, then validate without
execution:

```bash
QINTOPIA_QIWE_WEBHOOK_PROVIDER_COMMAND_SHA256=<reviewed-sha256> \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/qiwe-webhook-provider-callback-reviewed-command.sh --check
```

The separate owner execution is:

```bash
QINTOPIA_QIWE_WEBHOOK_PROVIDER_COMMAND_SHA256=<same-reviewed-sha256> \
QINTOPIA_QIWE_WEBHOOK_PROVIDER_COMMAND_APPROVAL=approved-production-qiwe-webhook-provider-callback-command \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/qiwe-webhook-provider-callback-reviewed-command.sh --execute
```

This command boundary is intentionally not callable from a deploy request. It runs as
the fixed `ubuntu` user for at most 30 seconds with a clean environment and suppresses
raw stdout/stderr. The ingress config must remain root-owned mode `0600`. A different
file hash, missing approval, mutable file, unsafe ownership, timeout, or ambiguous
provider result fails closed.

Provider configuration is not acceptance evidence by itself. Keep mappings in shadow,
capture one real sanitized authenticated callback, and validate its Space and event
fields before enabling any automation.

## Rollback

Run the same workflow with:

```text
runtime_one_shot_target=qiwe-webhook-ingress-rollback
backfill_date=
payload_sha256=
approval=approved-production-qiwe-webhook-ingress-rollback
```

Rollback atomically restores the root-only retained snippet, runs `nginx -t`, reloads,
and runs the appropriate active or disabled positive/negative smoke. If rollback smoke
fails, the script restores the current snippet and reports failure. Provider-side
callback removal or replacement remains a separate reviewed owner operation because its
official mutation semantics are not evidenced locally.

## Local Validation

```bash
pnpm runtime:nginx:check
pnpm deploy:runner:check
```
