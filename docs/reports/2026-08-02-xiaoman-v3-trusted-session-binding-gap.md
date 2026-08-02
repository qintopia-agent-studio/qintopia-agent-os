# Xiaoman V3 Trusted Session Binding Gap

Date: 2026-08-02 Status: code closeout merged in #358

## Summary

A real Xiaoman Feishu direct-message test returned the safe
`trusted_feishu_session_required` failure. No poster workflow, image generation, Feishu
result delivery, or public send was started.

The released V3 code was present, but production was still configured for the older
direct-return boundary. In addition, the Xiaoman plugin assumed the Feishu message id
would already exist in Hermes session context. The real Feishu adapter placed that id on
`MessageEvent` but not on the `SessionSource` used to build tool session context.

## Evidence Boundary

The retained production probe output is external evidence and must not be copied into
git. The repository record keeps only the sanitized conclusion:

- the immutable release-local Xiaoman plugin was present;
- production was configured for the older direct-return boundary rather than the
  authenticated V3 ingress boundary;
- the failed real request stopped at `trusted_feishu_session_required`;
- no poster workflow, image generation, Feishu result delivery, public send, or group
  delivery was started; and
- the real Feishu adapter surfaced the trusted message id before the plugin session
  context consumed it.

The external probe may retain release identity, boolean configuration facts, service
states, and marker counts under the owner evidence boundary. This git report must not
store chat ids, user ids, message text, credentials, database addresses, raw logs,
specific live release SHAs, systemd state snapshots, environment-file permissions, or
server-only backup paths.

## Root Cause

Two independent preconditions were missing:

1. The existing direct activation script did not require authenticated V3 ingress, so
   the release could run with the V3 hook and HMAC configuration absent.
2. Plugin tests injected a complete `gateway.session_context` directly. They did not
   exercise the real SDK boundary where the trusted id first exists on `MessageEvent`
   and Hermes later reads it from `SessionSource`.

Enabling the V3 environment alone would therefore still have left the poster tool
without a trusted source message id.

## Resolution

- After the plugin validates the authentic Feishu SDK event and AgentOS confirms the
  signed ingress was persisted, copy that same message id into the existing mutable
  `SessionSource.message_id` field.
- Do not bind the id when parsing, signing, socket delivery, or response validation
  fails. The model continues normally, but poster intake fails closed.
- Require both fixed environments to enable V3 ingress, share one 32-to-512-byte ingress
  HMAC key, share the callback key, keep those keys distinct, and keep group delivery
  disabled before direct activation can cause service changes.
- Require both ingress enable flags to be `0` before a full poster rollback reloads
  Xiaoman.
- Add regression coverage for the real event-to-session transition and for mismatched
  ingress keys failing before systemd or gateway side effects.

The released binding fix exposed one operations gap: production had no reviewed
entrypoint that owned coordinated updates to the fixed sidecar and Xiaoman environment
files. Direct editing would have repeated the configuration drift that the
release/current model is intended to remove.

The closeout in #358 adds one release-local, root-only configuration transaction instead
of another feature slice. It accepts bounded JSON on stdin, validates the exact Release
and approved database URL hash, can consume a newly rotated database URL without
printing it, creates or rotates the dedicated ingress HMAC in memory, updates both fixed
files through one lock with rollback-on-error, and emits only sanitized counts and
booleans. An abrupt process stop can leave a partial replacement, which the activation
preflight rejects until the same reviewed payload is retried. It also removes the
irrelevant Bot open-id requirement from direct-only sidecar ingress; group ingress still
fails closed without that exact identity.

This is a Xiaoman plugin and deployment-contract correction. It does not modify or fork
Hermes core, accept identity from process environment, add a service or table, or allow
the model to choose a target, reviewer, provider, or authorization state.

## Validation

The focused remediation checks are:

```bash
pnpm skills:qintopia-tools:check
node tools/deploy/test-xiaoman-feishu-poster-production-activation.mjs
bash -n deploy/sidecar/scripts/activate-xiaoman-feishu-poster-production.sh
bash -n deploy/sidecar/scripts/rollback-xiaoman-feishu-poster-production.sh
```

Production acceptance still requires a reviewed release containing #358, V3 ingress
configuration in both fixed environments through the protected transaction, a successful
no-network preflight, and a new real direct request that produces one ingress receipt
and one accepted workflow. Image generation, same-image return, review persistence, and
zero-publication evidence remain subsequent gates.

The remaining closeout is operational: publish/deploy the reviewed Release, apply the
rotated database credential and hashes through the release-local transaction, run direct
acceptance, and run one group canary on the same SHA. Configuration, preflight,
activation, policy application, and canary are separate approval gates but should not
require another code PR or Release unless production evidence exposes a new defect.
