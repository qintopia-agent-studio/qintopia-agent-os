# Deploy Tools

`tools/deploy/preflight.mjs` is a non-mutating deployment gate. It does not deploy code
and does not connect to the server.

Use the CI-safe gate as part of repository validation:

```bash
pnpm deploy:preflight:ci
```

Use the local preflight before an approved deployment window:

```bash
pnpm deploy:preflight
```

Local mode additionally requires:

- current branch is `master`
- worktree is clean
- deployment policy, CI/CD gate docs, and sidecar cutover plan exist
- repository checks include registry, manifest, policy, secret, tests, smoke, and deploy
  preflight gates
