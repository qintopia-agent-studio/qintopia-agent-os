# Xiaoman V3 Trusted Session Binding Gap

Date: 2026-08-02

## Summary

A real Xiaoman Feishu direct-message test returned the safe
`trusted_feishu_session_required` failure. No poster workflow, image generation, Feishu
result delivery, or public send was started.

The released V3 code was present, but production was still configured for the older
direct-return boundary. In addition, the Xiaoman plugin assumed the Feishu message id
would already exist in Hermes session context. The real Feishu adapter placed that id on
`MessageEvent` but not on the `SessionSource` used to build tool session context.

## Evidence

A read-only, redacted production probe confirmed:

- `release/current` resolved to `eb9a0c2850e959361e0e8beb80a8673a937f664e` (`v0.2.64`);
- the live Xiaoman plugin resolved to the immutable release plugin;
- the gateway, operations intake, review callback, notification starter, and direct
  delivery timer were active, while group delivery was inactive;
- neither fixed environment contained the V3 ingress enable flag or ingress HMAC key;
- the preceding 45-minute gateway window contained one `trusted_feishu_session_required`
  result and no ingress-unavailable marker;
- the operations-intake journal contained no `feishu_message_ingest`; and
- the production Hermes runtime supported `HERMES_SESSION_MESSAGE_ID`, but its Feishu
  adapter built `MessageEvent(message_id=...)` without copying that id into
  `SessionSource.message_id`.

The probe emitted only release identity, boolean configuration facts, service states,
and marker counts. It did not print chat ids, user ids, message text, credentials,
database addresses, or raw logs.

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

Production acceptance still requires a reviewed release, V3 ingress configuration in
both fixed environments, a successful no-network preflight, and a new real direct
request that produces one ingress receipt and one accepted workflow. Image generation,
same-image return, review persistence, and zero-publication evidence remain subsequent
gates.
