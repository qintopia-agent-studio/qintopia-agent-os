# MCP: Qintopia Collab

`mcp/qintopia-collab` is the M10-B package boundary for the Hermes `qintopia-collab-mcp`
command that was imported from:

```text
/home/ubuntu/.hermes/scripts/qintopia-collab-mcp
```

The adopted source is now:

```text
mcp/qintopia-collab/bin/qintopia-collab-mcp
```

Imported SHA256:

```text
3240bebd6288e9a1b9abe2b42fd2408e41d0a88f36ba510ccccca41a6b5bcb83
```

## Current Consumers

- `hermes-gateway-huabaosi.service`
- `hermes-gateway-silaoshi.service`
- `hermes-gateway-xiaoman.service`

Erhua and Wenyuange already use the release-managed `qintopia-context` wrapper and are
not part of this collab MCP migration.

## Boundary

This package may contain reviewed MCP wrapper code and non-secret command logic. It must
not contain:

- profile `.env` files
- profile sessions, logs, cache, or auth state
- raw private chat logs
- generated runtime memory
- server-local credentials

## Migration Plan

1. Include the package in the release/deploy bundle.
2. Publish and verify a deploy bundle artifact for the approved commit.
3. Assemble a new immutable release directory from COS artifacts.
4. Repoint one Hermes profile at a time:
   - back up `config.yaml`
   - change only the MCP command path
   - restart the affected profile
   - verify profile active state and MCP child process path
5. Stop before M12 cleanup. Cleanup requires a separate readiness audit.

## Validation

Minimum validation after each profile repoint:

```bash
systemctl --user is-active <profile-service>
ps -eo pid,ppid,args --width 360 | grep qintopia-collab
```

The old `/home/ubuntu/.hermes/scripts/qintopia-collab-mcp` process count must reach `0`
only after all three consumers have been repointed.
