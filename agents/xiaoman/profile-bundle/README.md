# Xiaoman Profile Bundle

This package converts reviewed Xiaoman profile behavior into deterministic files for a
future Hermes release mount.

## Current Status

Status: observation only.

The package is not installed into the live profile. It contains templates, a strict
renderer, fake fixtures, and a read-only parity boundary. A later PR must provide
production parity and first-cutover rollback evidence before any symlink is created.

## Inputs

The renderer accepts one JSON object containing exactly these server-local values:

- `QINTOPIA_XIAOMAN_OPERATIONS_OWNER_NAME`
- `QINTOPIA_XIAOMAN_OPERATIONS_OWNER_WECOM_TARGET`
- `QINTOPIA_XIAOMAN_TECHNICAL_OWNER_NAME`
- `QINTOPIA_XIAOMAN_TECHNICAL_HOME_CHANNEL`

Real values belong in a root-owned server file outside the release tree. The renderer
does not print values, and its output manifest records only input names and file hashes.
The production observation smoke must run as root when it reads the default
`/etc/qintopia/xiaoman-profile-bundle-values.json` path.

## One-Time Values Migration

After an owner-approved Release deploys this package, prepare the values file once:

```bash
sudo env \
  QINTOPIA_XIAOMAN_PROFILE_VALUES_MIGRATION_APPROVAL=approved-xiaoman-profile-values-migration \
  /home/ubuntu/qintopia-agent-os-releases/current/agents/xiaoman/profile-bundle/migrate_values.py \
  --apply
```

The command has no path overrides. It requires root, locks both live source hashes,
extracts exactly the four declared values in memory, renders and compares both files,
then creates the root-owned mode-`0600` values JSON without replacing an existing file.
It prints only sanitized status and hashes. It does not edit the live profile or make
the bundle active.

## Excluded State

`config.yaml`, webhook subscriptions, channel directories, cron state, `.env`, sessions,
auth, messages, memories, logs, cache, locks, databases, and Hermes core patches are not
part of this bundle.

## Validation

```bash
python3 agents/xiaoman/profile-bundle/render.py --check-only
python3 agents/xiaoman/profile-bundle/migrate_values.py --check-only
python3 -m unittest discover -s agents/xiaoman/profile-bundle/tests -p 'test_*.py'
pnpm agents:profile-bundles:check
```

Fixture render:

```bash
python3 agents/xiaoman/profile-bundle/render.py \
  --values-file agents/xiaoman/profile-bundle/tests/fixtures/values.json \
  --output-dir /tmp/xiaoman-profile-bundle
```

## Production Boundary

Rendering writes only to a new output directory. The manual migration command may create
only the fixed server-local values JSON after complete parity. Neither command reads
`.env`, connects to the network, restarts Hermes, edits the live profile, writes a
database, or sends externally.

Rollback for this observation-only package is to leave the values file unused; an owner
may remove it later. The live profile remains unchanged.
