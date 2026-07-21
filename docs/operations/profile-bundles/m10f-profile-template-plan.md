# M10-F Profile Template And Symlink Plan

Updated: 2026-07-21

M10-F defines how Hermes profile files should move toward the release/current model. It
does not repoint `SOUL.md`, `config.yaml`, or whole profile directories yet. The
field-limited Erhua Livecool overlay is the first governed exception; it atomically
updates only approved model fields while keeping the profile runtime-local.

## Current State

Hermes profile roots under `/home/ubuntu/.hermes/profiles/*` are still live runtime
state. They contain a mix of:

- reviewed human prompt/config files
- local `.env` secrets
- auth/session/cache/log/state files
- backup files from direct server edits
- generated memory and runtime DBs
- release-managed plugin symlinks created in M10-C, M10-D, and M10-E

Current release-managed profile-adjacent paths:

| Profile   | Release-managed paths                                                     |
| --------- | ------------------------------------------------------------------------- |
| Erhua     | `plugins/qintopia-tools`, `plugins/qiwe-platform`, `qintopia-context` MCP |
| Xiaoman   | `plugins/qintopia-tools`, `qintopia-collab` MCP                           |
| Wenyuange | `plugins/qintopia-tools`, `qintopia-context` MCP                          |
| Huabaosi  | `plugins/qintopia-base-read`, `qintopia-collab` MCP                       |
| Silaoshi  | `qintopia-collab` MCP                                                     |

## Decision

Do not copy or replace whole profile directories from CI.

Do not repoint `SOUL.md` or `config.yaml` until each profile has a reviewed profile
bundle with:

- a non-secret source template
- explicit render inputs
- a diff against the live profile file
- a rollback path
- profile-specific smoke checks
- owner approval for the changed behavior

The current `agents/*/profile.template.yaml` files remain package contracts and planning
inputs. They are not live profile files.

Erhua's approved design exception is governed by
`docs/operations/profile-bundles/erhua-livecool-profile-overlay-runbook.md`. It
satisfies the review, diff, rollback, and smoke design gates above without adopting the
future whole-file symlink shape. Each production activation still requires review of the
redacted dry run and explicit owner approval.

Xiaoman now has the first observation-only implementation under
`agents/xiaoman/profile-bundle`. It packages a strict renderer, fake fixtures, and a
read-only parity smoke without creating a live symlink. This does not change the M10-F
rule: activation requires a later owner-reviewed cutover with production parity and
rollback evidence.

## Target Shape

Future profile bundles should be generated under the release directory:

```text
/home/ubuntu/qintopia-agent-os-releases/<sha>/
  agents/
    erhua/
      profile.template.yaml
      rendered/
        SOUL.md
        config.yaml
      checks/
        smoke.sh
```

Hermes keeps runtime state in place:

```text
/home/ubuntu/.hermes/profiles/erhua/
  .env
  sessions/
  logs/
  cache/
  memories/
  state.db
  SOUL.md -> /home/ubuntu/qintopia-agent-os-releases/current/agents/erhua/rendered/SOUL.md
  config.yaml -> /home/ubuntu/qintopia-agent-os-releases/current/agents/erhua/rendered/config.yaml
```

This symlink shape is a future target, not current M10-F execution.

## Allowed In Git

- `agents/*/agent.yaml`
- `agents/*/profile.template.yaml`
- `agents/*/capabilities.md`
- `agents/*/runtime-notes.md`
- source snapshot notes with paths, checksums, and non-secret observations
- future render scripts that take explicit non-secret inputs
- future fixture-based smoke checks

## Not Allowed In Git

- `.env` or copied environment files
- auth files, sessions, pairing, cache, logs, state DBs, locks, generated memory
- private chat logs or raw member profile memory
- server backup files copied as runtime source
- `SOUL.md` or `config.yaml` copied directly from a live profile without a reviewed
  render plan

## Per-Profile M10-F Status

| Profile   | Current M10-F disposition | Notes                                                                 |
| --------- | ------------------------- | --------------------------------------------------------------------- |
| Erhua     | governed field overlay    | Livecool model fields only; no whole config or prompt repoint         |
| Xiaoman   | observation bundle        | Release parity smoke pending; sensitive config and state remain local |
| Wenyuange | template only             | Evidence/message-store access must preserve disclosure boundaries     |
| Huabaosi  | review-pool template      | Visual adapter/Rust/shadow material remains unapproved direction      |
| Silaoshi  | template only             | Temporary scripts remain separate workflow/script candidates          |
| Guanerye  | template only             | No immediate plugin/MCP migration observed                            |

## M10-F Exit Criteria

M10-F is complete when:

1. The profile bundle direction is documented.
2. `agents/*/profile.template.yaml` remains the only active profile template contract.
3. Repository checks prevent runtime state from entering agent packages.
4. Deploy bundle checks prevent unreviewed live profile files from being packaged.
5. M11 can start archive-ready marking without confusing profile template planning with
   actual cleanup.

## Next Production Step

The first whole profile-file repoint should be a separate future migration after M11/M12
planning, not part of M10-F. It should start with one low-risk profile, generate a
rendered profile bundle, compare it against live `SOUL.md`/`config.yaml`, and run a
profile-specific smoke before switching symlinks. The Erhua field overlay does not
authorize that broader migration.
