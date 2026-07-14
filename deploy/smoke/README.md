# Smoke Checks

This package is the future common entrypoint for cross-profile, MCP, worker, and
deployment smoke checks.

Existing sidecar-specific smoke scripts remain in `deploy/sidecar/scripts/` until they
are wrapped here without changing behavior.

## Scope

- release manifest validation
- profile bundle dry-run validation
- MCP command resolution checks
- worker binary availability checks
- read-only external adapter preflight

Smoke checks must be safe by default. Real external sends require separate owner review,
allowlists, and explicit runtime configuration.

## Xiaoman Production Preflight

After an owner-approved deploy, run the aggregate Xiaoman production preflight from the
sidecar scripts and record the sanitized result in
[`docs/xiaoman-production-preflight-record.md`](docs/xiaoman-production-preflight-record.md).
The record captures timer health, fixed service commands, read-only preview counts,
secret-scan results, and the pass/hold decision. This includes read-only observations of
the image-generation request starter, the disabled Huabaosi provider worker, and the
group send-ready timer. The preflight runs the provider worker only with
`--once --dry-run` and does not run the send-ready worker. It is not a release approval
and does not authorize Feishu writes, QiWe sends, poster publishing, or external
adapters.

An explicit `no_claimable_*` worker result is a valid empty-queue observation only when
it reports `dry_run=true`, `apply_requested=false`, and empty artifact ids/previews.
When a worker returns an actual preview, its limitations or guardrails must still state
the external-adapter boundary. A release that changes the deploy runner's systemd unit
allowlist needs one owner-approved follow-up deployment for the same SHA because its
first promotion is processed by the previous release runner.

## Validation

```bash
pnpm deploy:smoke:check
pnpm check:light
```
