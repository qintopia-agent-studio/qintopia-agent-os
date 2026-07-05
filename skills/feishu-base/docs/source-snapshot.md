# Feishu Base Read Source Snapshot

Snapshot date: 2026-07-05

Mode: read-only server inventory followed by sanitized monorepo adoption. Runtime state,
secrets, pycache files, and `.env` files were excluded.

## Source

| Item                    | Value                                                                         |
| ----------------------- | ----------------------------------------------------------------------------- |
| Source path             | `/home/ubuntu/.hermes/profiles/huabaosi/plugins/qintopia-base-read`           |
| `plugin.yaml` SHA-256   | `c3716c85438b9b8bbcb7543198f4936b535a0013612ddadb0553a74c6ab66149`            |
| `__init__.py` SHA-256   | `117477a0dac5bb6defc6c21659b4dbdd7f698a6a2098471c661cc566fd2bcbad`            |
| Advertised tools        | `qintopia_xiaoman_activity_record_get`, `qintopia_huabaosi_design_record_get` |
| Server git metadata     | none observed                                                                 |
| Server runtime consumer | `hermes-gateway-huabaosi.service` user service                                |

## Adoption Changes

- Kept the Hermes plugin name `qintopia-base-read`.
- Removed hardcoded Feishu app credential fallback from the adopted source.
- Moved Base app tokens and table ids behind explicit runtime environment variable
  names.
- Added structured failure responses when required runtime configuration is missing.
- Removed response fields that would echo Base tokens or table ids.
- Excluded `__pycache__` and `.pyc` files.

## Required Runtime Variables

```text
FEISHU_APP_ID or LARK_APP_ID
FEISHU_APP_SECRET or LARK_APP_SECRET
QINTOPIA_BASE_READ_XIAOMAN_ACTIVITY_BASE_TOKEN
QINTOPIA_BASE_READ_XIAOMAN_ACTIVITY_PLAN_TABLE_ID
QINTOPIA_BASE_READ_HUABAOSI_DESIGN_BASE_TOKEN
QINTOPIA_BASE_READ_HUABAOSI_POSTER_TABLE_ID
```

These values are production secrets or sensitive source identifiers and must stay in
server-side runtime configuration.

## Validation Evidence

M10-E package adoption should pass:

```bash
pnpm skills:feishu-base:check
pnpm artifact:deploy-bundle
pnpm check:light
```

Production repoint validation should verify Huabaosi active state, plugin import/tool
registration, deploy bundle manifest, release/current symlink target, and rollback path.
