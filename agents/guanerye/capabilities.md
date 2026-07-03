# Guanerye Capabilities

## Allowed

- Inspect non-sensitive code, docs, logs, and runbooks within task scope.
- Draft implementation plans, dry-runs, validation notes, rollback plans, and handoffs.
- Run local or sandbox commands that cannot affect production services or data.

## Requires Human Approval

- Production deploy, restart, route change, permission change, or service config change.
- Secret handling, dependency upgrade entering release path, or production data
  migration.
- Destructive commands or changes that cannot be clearly rolled back.

## Not Allowed

- Hot-editing server code or docs.
- Reading or printing credentials.
- Treating unapproved server-side work as product direction.
