# Agent Checks

`tools/agents` owns helper scripts and checks for programming-agent collaboration.

`check-agents.mjs` validates the active Agent package contract.

Run:

```bash
pnpm agents:check
```

The check verifies:

- required active Agents are registered
- `xiaoqin` is not registered as an active Agent
- every registered Agent has `README.md`, `agent.yaml`, `profile.template.yaml`,
  `capabilities.md`, `runtime-notes.md`, and `docs/source-snapshot.md`
- profile templates include purpose, prompt sections, capabilities, forbidden actions,
  runtime mounts, excluded runtime state, and dry-run expectations
- package-like allowed capabilities point to active registry entries
- active Agents do not depend on deprecated packages
- `huabaosi` remains draft/review-pool until owner approval

## Pull Request Creation

Programming agents must not hand humans a prefilled GitHub compare URL as the normal PR
flow. Use the repository-owned `gh` workflow instead:

```bash
pnpm pr:doctor
pnpm pr:tools:check
pnpm pr:create -- --body-file /path/to/completed-pr-body.md
```

If GitHub CLI is missing, run:

```bash
pnpm pr:bootstrap
```

`pnpm pr:bootstrap -- --install` may install GitHub CLI on supported macOS, Windows, or
Debian/Ubuntu environments. Do not run a separate authentication precheck before PR
creation; only handle authentication if the actual push or `gh pr create` command
reports an authentication failure.

PR bodies must start from `.github/PULL_REQUEST_TEMPLATE.md` and must fill Summary,
Planning, Domain, Validation, Production Boundary, Architecture / Tooling Boundary, and
Changelog. CI runs `pnpm pr:check-body` on pull requests and rejects empty template
sections.

## Space Programming Extension Runner

The default-disabled PR-only consumer is:

```bash
QINTOPIA_SPACE_PROGRAMMING_EXTENSION_DISPATCH_ENABLED=1 \
QINTOPIA_PROGRAMMING_AGENT_CODEX_HOME=/dedicated/codex-home \
QINTOPIA_PROGRAMMING_AGENT_HOME=/dedicated/tool-home \
QINTOPIA_PROGRAMMING_AGENT_GITHUB_TOKEN_HELPER=/usr/local/libexec/qintopia-programming-agent-github-token \
node tools/agents/run-space-programming-extension.mjs --once
```

The two home directories must be absolute and private, and the tool home must not
contain persisted GitHub CLI or git credentials. They separate tool configuration, but
they are not a credential-isolation boundary: a process with the same Unix UID can still
read other files available to that UID and may be able to inspect sibling process state.
Keep dispatch disabled until Codex runs under a dedicated OS identity or an equivalent
container that cannot read production env, Hermes, COS, database, server, or GitHub
credentials. The orchestration boundary that pushes and opens the PR must retain the
repository-scoped GitHub token outside that sandbox.

The runner rejects `QINTOPIA_PROGRAMMING_AGENT_GITHUB_TOKEN`, `GH_TOKEN`,
`GITHUB_TOKEN`, `GH_ENTERPRISE_TOKEN`, and `GITHUB_ENTERPRISE_TOKEN` in its startup
environment. The configured helper must be an absolute, non-symlinked, root-owned
executable whose root-owned parent path is not writable by group or other; the runner
revalidates those properties immediately before invocation. It is invoked without the
tool home or caller environment, only a fixed locale and PATH, and only after Codex
exits and the allowed paths, complete committed diff, fixed validation, low-risk
classification, and clean repository state have all passed. The helper receives only
`--repository qintopia-agent-studio/qintopia-agent-os` and a minimal environment, and
must print exactly one JSON object:

```json
{ "token": "<short-lived repository token>", "expires_at": "<RFC3339 timestamp>" }
```

The token must have more than five minutes and no more than one hour remaining. The
helper should acquire a GitHub App installation token through a privileged local broker;
it must not read a long-lived token from the runner environment. The existing
`deploy/sidecar/scripts/github-app-git.sh` demonstrates the reviewed GitHub App minting
and temporary credential pattern, but is git-command-only and therefore cannot directly
broker the combined `pnpm pr:create` push plus GitHub CLI flow.

The unauthenticated phase starts from the locally cached exact `origin/master`. Once the
helper returns a valid token, the runner performs its first authenticated fetch and
fails closed unless fetched `origin/master` is still the audited base SHA. A stale local
base is therefore discarded without push or PR creation.

Within that prerequisite, the runner connects only to
`/run/qintopia-agentos/operations-intake.sock`, uses a temporary worktree, runs fixed
validation, creates a PR through `pnpm pr:create`, and applies the fixed
`qintopia-low-risk-auto` label only after the broker records an `awaiting_publish` PR
handoff. It does not merge, publish, deploy, send or connect to Postgres.

The trusted current-Space status path reports only the PR number and short identity
fingerprints. It reaches `released/ready_to_replan` only when the active sidecar embeds
the exact generated mapping digest and has a valid deploy-injected commit SHA. The
trusted status wrapper then retrieves the retained intent through a same-Space internal
operation, reruns the bounded planner, and idempotently creates the normal shadow
proposal; administrator confirmation is still required.

Manual review is the default after the runner stops. A separate, default-disabled
`Low-Risk Auto Release` workflow may consume that label only after verifying its fixed
actor and token, exact PR head and required checks, append-only mapping/recipe
classification, exact CI-run-backed Release Please validation, strict candidate plus
metadata commit topology, draft identity digest, and the complete unpublished range at
every mutation. The workflow cannot activate production ingress, configuration,
capabilities, automations, credentials, services, timers, deployments, or sends.

If the existing mapping DSL cannot express a documented encoding, the runner may add one
append-only `*.primitive.json` recipe with the same mapping/fixture/expectation bundle.
Recipes compose only the fixed parser kernel and are never arbitrary Rust or other
source code. Adding a kernel operation remains an owner-reviewed infrastructure change;
see `../../docs/engineering/qiwe-restricted-parser-primitives.md`. The bundle may also
include one fixed-format append-only `*.mapping.md` summary that references only those
same added files and does not broaden runtime behavior. The complete bundle is capped at
five files.

Run the offline contract suite with:

```bash
pnpm agents:space-programming-extension:test
```

## Space Agent-Turn Runner

The model runner is a separate, default-disabled, standard-library process. It claims
one bounded `space_agent_turn`, asks the Hermes completion socket for either a final
output or one catalog capability call, invokes that call through the Sidecar broker, and
finishes the exact claim. It supports only `--once`; this repository does not install or
enable a timer or service for it.

The runner requires explicit, scoped Unix-socket paths and a dedicated raw token:

```bash
QINTOPIA_SPACE_AGENT_TURN_RUNNER_ENABLED=1 \
QINTOPIA_SPACE_AGENT_TURN_RUNNER_SOCKET=/run/qintopia-agentos-agent-turn/space-agent-turn.sock \
QINTOPIA_SPACE_AGENT_TURN_COMPLETION_SOCKET=/run/qintopia-agentos-agent-turn/hermes-completion.sock \
QINTOPIA_SPACE_AGENT_TURN_RUNNER_TOKEN='<dedicated-32-character-ascii-token>' \
QINTOPIA_SPACE_AGENT_TURN_SOCKET_TIMEOUT_SECONDS=45 \
python3 tools/agents/run-space-agent-turn.py --once
```

Provision a 32-512 character ASCII raw token only in the dedicated runner environment.
Its SHA-256 must match the separately owner-approved Sidecar broker and Hermes
completion configurations; do not place the raw token in either server configuration or
repository files. Both socket paths must be absolute, normalized, distinct, and below
scoped parent directories. Before every connection, the runner rejects symlinks and
requires the parent directory to be foreign-owned mode `0750` and the socket to be
foreign-owned mode `0660`; both must use the runner's primary group.

The runner accepts only strict, bounded, newline-framed JSON. It allows at most 16
completion rounds, rejects duplicate call ids and capabilities outside the broker claim,
and builds `capability_usage` only from successfully validated broker receipts. Any
failure after a valid claim triggers one best-effort failed finish. The process has no
HTTP client, subprocess, database access, provider/Qiwe credentials, or persistent file
writes; it communicates only through the two configured `AF_UNIX` sockets.

Run the offline fake-socket and static-boundary suite with:

```bash
pnpm agents:space-agent-turn:test
```
