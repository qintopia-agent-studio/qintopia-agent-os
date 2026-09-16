# Hermes runtime contract

These are retained operating constraints, not a new permission grant. Read the sections
relevant to the change before editing. Original conditions and exceptions remain
binding; summaries in AGENTS.md do not replace them.

Backtick paths in root-source rules are repository-relative; paths in Sidecar-source
rules retain their original `runtime/sidecar/` base. New navigation links are relative
to this document.

[Migration inventory](../../../docs/plans/active/agents-guidance/README.md) records the
baseline and source locations.

<a id="root-182"></a>

## Core Rules — root-182

<!-- preserved-rule: root-182 -->

- Hermes remains the Agent runtime. It should not become the business database.

<!-- /preserved-rule: root-182 -->

<a id="root-184"></a>

## Core Rules — root-184

<!-- preserved-rule: root-184 -->

- The production Hermes venv is uv-managed. Its `pyvenv.cfg` base home may use uv's
  stable `cpython-<major>.<minor>-<platform>` alias, which resolves to an exact patch
  version below `/home/ubuntu/.local/share/uv/python`. Interpreter validation may allow
  only that single in-root alias with matching version/platform identity; do not require
  the uv home path to be textually unaliased and do not broaden it to arbitrary
  symlinks.

<!-- /preserved-rule: root-184 -->
