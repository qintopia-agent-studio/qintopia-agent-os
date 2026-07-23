# Erhua uv Python Home Alias Dry-Run Failure

Date: 2026-07-23

## Observed Evidence

Published Release `v0.2.24` initially failed before promotion because the production
server still used the `v0.2.20` runner, whose sidecar fetch contract required the now
forbidden `qiwe-production-adapter` feature in the Huabaosi production artifact. A
reviewed mixed-SHA bootstrap first proved the transition with dry-run request
`deploy-20260723T024405Z-d6f1d014809b`, then promoted transition release
`87cf0d85cbfcb0e5a27c5dcc3dbf40a4ab93605c` with the `v0.2.20` runtime and `v0.2.24`
deploy bundle.

The new runner accepted the two-feature `v0.2.24` Huabaosi sidecar and promoted it, but
the required Erhua smoke detected the still-unactivated Livecool profile and rolled the
release back successfully. The governed `hermes-profile-erhua` dry run
`deploy-20260723T025157Z-d6f1d014809b` then failed before rendering or writes with:

```text
Hermes Python validation failed: Hermes venv base interpreter home must be an existing non-aliased directory
```

Read-only server inspection showed a normal uv-managed venv:

```text
pyvenv.cfg home = /home/ubuntu/.local/share/uv/python/cpython-3.11-linux-x86_64-gnu/bin
cpython-3.11-linux-x86_64-gnu
  -> /home/ubuntu/.local/share/uv/python/cpython-3.11.15-linux-x86_64-gnu
venv/bin/python
  -> /home/ubuntu/.local/share/uv/python/cpython-3.11.15-linux-x86_64-gnu/bin/python3.11
```

The validator correctly rejected arbitrary external interpreters but incorrectly assumed
that every valid `pyvenv.cfg` home is textually unaliased. uv deliberately records a
stable major/minor alias while its venv entry points to the resolved patch version.

No profile file was changed by the failed dry run. `release/current` remains the
transition release, and the failed `v0.2.24` promotion was rolled back to it.

## Revised Boundary

The fixed Hermes venv and regular `pyvenv.cfg` requirements remain unchanged. An aliased
base home is accepted only when all of these conditions hold:

- the alias is directly below the same runtime user's fixed `.local/share/uv/python`
  root;
- its name is `cpython-<major>.<minor>-<platform>`;
- it is one absolute symlink to an in-root `cpython-<major>.<minor>.<patch>-<platform>`
  directory;
- the major/minor version and platform suffix match exactly; and
- the resolved executable is a standard Python name directly below the resolved `bin`
  directory and matches the alias major/minor version.

Relative, chained, out-of-root, version-mismatched, platform-mismatched,
release-escaping, or caller-selected interpreter paths remain rejected.

## Recovery Sequence

1. Merge and release the bounded uv alias fix through the normal manual Release flow.
2. Bootstrap that release's deploy bundle while retaining the last accepted sidecar.
3. Run a new `hermes-profile-erhua` dry run and review its sanitized evidence.
4. After explicit owner approval, activate the exact dry-run result within 24 hours.
5. Deploy the final release with the normal sidecar, deploy-bundle, and Hermes restart
   checks.
