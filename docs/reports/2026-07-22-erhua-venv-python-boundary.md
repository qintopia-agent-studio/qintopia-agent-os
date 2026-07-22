# Erhua Venv Python Boundary Dry-Run Failure

Date: 2026-07-22

## Observed Evidence

Production deploy request `deploy-20260722T023744Z-cc53df6e89ed` was the first
`hermes-profile-erhua` dry-run against release
`cc53df6e89ed80e5b2e232e2a843679a8f7bb489`. The signed request used the exclusive Erhua
profile scope, `hermes-erhua` restart target, rollback enabled, and `dry_run=true`.

The server rejected the request before profile rendering, credential migration, runtime
resolver execution, backup creation, service restart, or a live file write. The runner
reported:

```text
Erhua Hermes Python must resolve within the allowed venv or release boundary
```

The fixed Hermes entry point is `/home/ubuntu/.hermes/hermes-agent/venv/bin/python`. It
is a normal venv symlink whose resolved base interpreter is managed outside the venv
directory. Requiring the final symlink target to remain below the venv therefore rejects
a valid Python virtual environment.

## Security Boundary

The fix must not permit the root deploy runner to execute an arbitrary caller-selected
interpreter. It must:

- accept only the fixed Hermes venv entry point or an interpreter contained in the
  immutable release;
- require the fixed venv and its `pyvenv.cfg` to be regular, non-aliased paths;
- allow the fixed venv entry point to link to its external base interpreter;
- keep release-local interpreter symlink targets inside the immutable release; and
- execute the Hermes runtime resolver as the unprivileged `ubuntu` runtime owner when
  the deploy runner is root.

The failed request is not activation evidence and must not be referenced by a later
non-dry-run request. A replacement release and a new successful dry-run request are
required before activation.

## Validation

- `pnpm runtime:hermes:check`
- `pnpm agents:profile-bundles:check`
- `pnpm deploy:contracts:check`
- `pnpm deploy:runner:check`
- `.husky/pre-commit`
