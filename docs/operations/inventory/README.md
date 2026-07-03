# M1 Inventory

Inventory date: 2026-07-03

This directory contains the M1 inventory for the Agent OS monorepo migration. M1 is not
code migration. It records source locations, current source state, disposition, target
package candidates, risk, and next actions.

## Files

- [local-sources.yaml](local-sources.yaml): local sibling repositories and local-only
  source directories under `/Users/evans/qintopia`.
- [server-sources.yaml](server-sources.yaml): read-only server checkout and runtime
  source inventory.
- [runtime-assets.yaml](runtime-assets.yaml): server profile assets, services, and
  runtime-only inputs.

## Dispositions

| Disposition    | Meaning                                                   |
| -------------- | --------------------------------------------------------- |
| `adopt`        | Move into a package and make it part of the future system |
| `template`     | Convert to a template after removing live state           |
| `runtime-only` | Keep as operational evidence; do not copy into packages   |
| `review-pool`  | Keep for owner review before accepting as direction       |
| `deprecated`   | Keep only for audit or migration reference                |
| `remove`       | Remove after confirming it has no audit value             |

## Required Fields

Each record should include:

- `id`
- `source.path`
- `source.kind`
- `source.reference`
- `state`
- `disposition`
- `target`
- `owner`
- `risk_level`
- `production_boundary`
- `validation`
- `next_action`

## M1 Coverage

This first pass covers:

- local sibling repositories
- server git checkouts
- live `.hermes` profile-level plugins and scripts
- active server services related to Agent OS
- known WorkTool, OpenClaw, and Hermes Kanban legacy areas

Detailed file-level hashes should be added during package adoption PRs. This M1 pass
records enough structure to decide migration order without copying runtime directories
or secrets.
