# Runtime: Postgres

`runtime/postgres` is the schema, migration, and data runbook contract for Agent OS. It
is the system fact source for business and workflow state.

## Current Source

- Local source: `../qintopia-message-sidecar/migrations`
- Design notes: `../qintopia-message-sidecar/docs/data-design`
- Adopted reference: `eda2652f21999e4f32699463413372accbd3b76e`

## Responsibility

This package will hold versioned migrations, schema design notes, acceptance fixtures,
and rollback guidance for the Agent OS database. Migrations must be idempotent and safe
to run on sidecar startup.

## Boundaries

- Postgres is the Agent OS fact source.
- Feishu is a human workbench and mirror, not the fact source.
- Hermes remains runtime execution, not the business database.
- Live database credentials and table IDs must stay outside git.

## Migration Rule

Every schema migration needs a corresponding data-design note. When the
`qintopia_agent_os.schema_change_log` table exists, migrations must record their
application there.

## Validation

Before importing or changing migrations, run the sidecar tests and the relevant guarded
smoke from the source repository.
