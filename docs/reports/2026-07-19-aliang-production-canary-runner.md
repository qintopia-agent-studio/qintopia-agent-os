# Aliang Production Canary Runner

Date: 2026-07-19 Asia/Shanghai

## Gap

The reviewed production binary can generate one Feishu-backed JPEG, but the first real
canary still requires an operator to compose five separate commands: approve a pending
`poster_brief`, create its image-generation request, run the provider worker, extract
the generated artifact id, and revalidate the Feishu attachment. Each command is
individually guarded, but there is no release-local boundary proving that they all refer
to the same new brief, request, artifact, and immutable JPEG.

That leaves a final-Release risk: production may contain all low-level commands while
the owner still cannot run one complete, auditable canary without ad hoc shell or SQL.

## Decision

Add one release-local production canary runner with these fixed inputs:

- exact owner approval phrase;
- exact published release SHA and packaged sidecar SHA-256;
- exact production database URL SHA-256;
- one pending `poster_brief` artifact UUID; and
- the existing allowlisted reviewer identity `trainer`.

The runner must use the immutable release binary and parse only the image/Feishu and
operations policy keys it needs from the fixed production environment file. It must not
`source` or evaluate that file. Before any Postgres or external I/O it must prove the
release path, binary digest, disabled provider timer, generation configuration, release
binding, and database binding.

It then performs exactly one bounded chain:

```text
pending poster_brief
  -> trainer approval
  -> one new image_generation_request
  -> one production provider/Feishu apply
  -> one pending generated_image
  -> authenticated Feishu attachment revalidation
```

The generated image remains `pending`. The runner must not approve it, enable a timer,
write the Feishu mirror, publish, call QiWe, or send. A terminal or ambiguous provider
outcome stays terminal and is not retried by the runner.

## Evidence Contract

Successful execution emits one sanitized record for each phase: `preflight`,
`brief_review`, `request_intake`, `generation`, and `revalidation`. Retained evidence
may contain only AgentOS UUIDs, release/binary/database hashes, fixed action states,
JPEG identity, dimensions, byte size, review state, and booleans describing external
calls and database writes.

It must not emit the database URL, provider or Feishu credentials, endpoint URLs, Base
or table identifiers, Feishu record or attachment tokens, profile paths, prompts,
provider responses, filenames, QiWe fields, or raw errors.

## Validation

- fake immutable release and sidecar success path;
- strict release, digest, timer, env-parser, reviewer, and UUID failures;
- mismatched generation/revalidation identity rejection;
- sensitive-output rejection;
- deploy-bundle and release contract checks; and
- shell syntax, formatting, Markdown, and secret scans.

## Production Boundary

This report and implementation do not run the production canary. They do not edit the
server, approve a live brief, call the provider or Feishu, create an artifact, enable a
timer, publish, call QiWe, or send. Actual use remains after owner merge, final Release
publication, deployment, release-local observation, and the explicit one-shot approval.
