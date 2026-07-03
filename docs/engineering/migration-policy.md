# Migration Policy

Migration into this monorepo is inventory-first. The goal is to preserve current
production behavior while moving ownership, validation, deployment, and documentation
into git.

## Dispositions

Every source artifact must be classified before adoption:

| Disposition    | Meaning                                                   |
| -------------- | --------------------------------------------------------- |
| `adopt`        | Move into a package and make it part of the future system |
| `template`     | Convert to a template after removing live state           |
| `runtime-only` | Keep as operational evidence; do not copy into packages   |
| `review-pool`  | Keep for owner review before accepting as direction       |
| `deprecated`   | Keep only for audit or migration reference                |
| `remove`       | Remove after confirming it has no audit value             |

## Required Migration Record

Each migration PR should record:

- source path
- source branch, commit, hash, or checksum when available
- disposition
- target package path
- owner
- risk level
- validation command
- production boundary
- rollback or decommission plan

## Current Source Classes

| Source                             | Default disposition                          |
| ---------------------------------- | -------------------------------------------- |
| `../qintopia-agent-os/docs`        | adopt stable docs; deprecate historical POCs |
| `../qintopia-message-sidecar`      | adopt data/MCP/deploy docs after review      |
| `../qiwei-hermes-plugin`           | first skill adoption candidate               |
| server `.hermes/profiles/*`        | runtime-only inventory and template input    |
| server `qintopia-agent-os` docs    | evidence and review-pool                     |
| server `qintopia-msg-sidecar` docs | runtime evidence and review-pool             |
| `../worktool`                      | deprecated or remove                         |
| `../worktool-hermes-plugin`        | deprecated or remove                         |

## WorkTool Cleanup

WorkTool is not a future Agent OS channel. During migration:

1. Inventory WorkTool references in local repositories and server runtime directories.
2. Classify any reference with audit value as `deprecated`.
3. Remove dead dependencies, scripts, and docs after confirming no runtime use.
4. Keep a decommission note when removal affects operator runbooks or deployment
   scripts.

## Server Runtime Handling

Server runtime files can be read for evidence and inventory. They must not be copied
wholesale into git. Convert only stable behavior into templates or package docs after
removing:

- secrets and live `.env` files
- member profile raw text
- private chat logs
- generated caches
- machine-local paths
- unreviewed hotfixes
