# v0.2.63 Erhua Profile Overlay Model Drift

Date: 2026-08-02

## Summary

The production deployment for `v0.2.63`, commit
`304a75f21b5e575803934877c8204954de174357`, failed closed during the Erhua release smoke
and rolled back successfully to `65ab1ab57df650667ac394d61463c34e1a58efcd` (`v0.2.62`).

The release artifacts built and passed their repository contract checks. The server
promoted the candidate, restarted the computed targets, and then rejected the candidate
because the live Erhua `model.default` no longer matched the repository's historical
Livecool overlay. Profile activation was not attempted. No poster service, Feishu call,
QiWe call, image generation, or group delivery was activated by this deployment.

## Evidence

GitHub Actions run `30735451892` recorded:

- deploy request `deploy-20260802T062019Z-304a75f21b5e`;
- failure stage `smoke-release` with exit status `1`;
- candidate promotion before smoke;
- `profile_activation_attempted=false`;
- rollback attempted and succeeded; and
- restored `current` target
  `/home/ubuntu/qintopia-agent-os-releases/65ab1ab57df650667ac394d61463c34e1a58efcd`.

The bounded post-failure profile comparison reported only:

```json
{ "changed_paths": ["model.default"] }
```

The live Erhua profile used `model.default: gpt-5.6-luna`. Its Livecool route and
provider registration still matched the approved endpoint, provider model, environment
binding name, and API mode. The release overlay instead required
`model.default: gpt-5.5`.

## Root Cause

PR #243 introduced the first governed Hermes profile overlay to repair Erhua's missing
Livecool provider registration. For a deterministic first activation, it froze both the
provider contract and the then-current default model in one exact overlay.

That combined two ownership concerns:

- Livecool provider registration and credential boundaries, which this overlay must
  continue to govern; and
- the active conversational default model, which can change independently of provider
  registration.

No equivalent repository-owned default-model overlay exists for the other Hermes Agents.
The Erhua-specific activation contract therefore became an unintended stale
model-version gate for ordinary releases that restart Erhua.

## Resolution

Narrow the existing Erhua overlay without introducing a new model-policy subsystem:

- stop managing or validating `model.default`;
- preserve the runtime-local default model during render and verification;
- continue to require `model.provider: custom:livecool.net`, the empty model base URL,
  and the exact non-secret Livecool provider contract;
- retain the fixed environment binding and forbidden credential-field checks; and
- add regression coverage proving a `gpt-5.6-luna` default survives render and passes
  verification while provider drift still fails closed.

The historical `gpt-5.5` provider field remains unchanged. This repair does not select a
new default model and does not create a shared Agent model policy.

## Validation

The remediation PR must pass:

```bash
pnpm runtime:hermes:check
pnpm agents:profile-bundles:check
pnpm deploy:runner:check
pnpm check:pr:auto
```

Production acceptance requires a new Release tag. Its deployment must preserve the live
Erhua default model, pass the Livecool resolver and Hermes doctor checks, leave Erhua
and all required services active, and avoid rollback.

## Remaining Safety Boundary

This repair touches the release-owned Erhua profile verification path but does not
authorize direct profile edits, a Hermes core change, an Agent-wide model migration, an
inference call, image generation, Feishu or QiWe access, or any message delivery.

Do not reuse `v0.2.63`. Release the reviewed correction under a new version and deploy
through the existing release/current runner. Before production promotion, revalidate the
current release pointer and non-secret Erhua model/provider fields through an approved
server access path.

## Next Owner Action

Review and merge the scoped remediation PR. After Release Please prepares the next
version, the owner must make the repository-required manual merge and Release
publication decisions before production deployment resumes.
