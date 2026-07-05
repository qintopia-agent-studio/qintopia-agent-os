# Feishu Base Read Skill

Status: adopting Huabaosi runtime plugin

`skills/feishu-base` is the monorepo home for the Hermes `qintopia-base-read` plugin
currently loaded by Huabaosi.

## Runtime Plugin

| Item        | Value                                                                         |
| ----------- | ----------------------------------------------------------------------------- |
| Plugin name | `qintopia-base-read`                                                          |
| Server path | `/home/ubuntu/.hermes/profiles/huabaosi/plugins/qintopia-base-read`           |
| Consumer    | `hermes-gateway-huabaosi.service` user service                                |
| Tools       | `qintopia_xiaoman_activity_record_get`, `qintopia_huabaosi_design_record_get` |

## Boundary

Allowed:

- read one allowlisted Xiaoman activity-plan record
- read one allowlisted Huabaosi design ledger record
- return normalized fields, facts, and human-readable missing information

Not allowed:

- arbitrary Feishu Base browsing
- Base writes or table mutation
- committed Feishu app credentials, Base app tokens, table ids, `.env`, logs, sessions,
  cache, or generated runtime state
- using the tool result as authorization for external publishing

## Required Runtime Configuration

The plugin reads credentials from Hermes session environment or process environment:

```text
FEISHU_APP_ID or LARK_APP_ID
FEISHU_APP_SECRET or LARK_APP_SECRET
QINTOPIA_BASE_READ_XIAOMAN_ACTIVITY_BASE_TOKEN
QINTOPIA_BASE_READ_XIAOMAN_ACTIVITY_PLAN_TABLE_ID
QINTOPIA_BASE_READ_HUABAOSI_DESIGN_BASE_TOKEN
QINTOPIA_BASE_READ_HUABAOSI_POSTER_TABLE_ID
```

These values must stay in server-side secret/runtime configuration. They must not be
committed to this repository.

## Validation

```bash
pnpm skills:feishu-base:check
```

The check compiles the plugin, runs focused unit tests, and blocks pycache, `.env`,
hardcoded Feishu app credentials, and hardcoded Base app/table identifiers.

## Production Migration

Before repointing Huabaosi:

1. Verify the deploy bundle contains `skills/feishu-base`.
2. Ensure the required runtime variables are available to the Huabaosi user service.
3. Back up the existing profile-local plugin directory.
4. Repoint only `/home/ubuntu/.hermes/profiles/huabaosi/plugins/qintopia-base-read` to
   the release/current package.
5. Restart Huabaosi and validate active service state plus import/tool registration.
6. Keep the old plugin copy for M11 archive-ready evidence; do not clean it in M10.
