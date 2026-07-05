# Qintopia Tools Skill

Status: adopting profile variants

`skills/qintopia-tools` is the monorepo home for the Hermes `qintopia-tools` plugin
currently installed under active profile plugin directories.

## Current Shape

The live server does not have one clean git-managed plugin copy. It has profile-local
variants:

| Variant   | Server source                                                    | Current role                                 |
| --------- | ---------------------------------------------------------------- | -------------------------------------------- |
| Erhua     | `/home/ubuntu/.hermes/profiles/erhua/plugins/qintopia-tools`     | Broad Qintopia tools plus weather/digest     |
| Xiaoman   | `/home/ubuntu/.hermes/profiles/xiaoman/plugins/qintopia-tools`   | Xiaoman activity wrappers and shared tools   |
| Wenyuange | `/home/ubuntu/.hermes/profiles/wenyuange/plugins/qintopia-tools` | Knowledge, Dify, and message-store read      |
| Huabaosi  | no observed `qintopia-tools` directory during M10-C inventory    | Not migrated in this step                    |
| Xiaoqin   | `/home/ubuntu/.hermes/profiles/xiaoqin/plugins/qintopia-tools`   | Deprecated profile; excluded from M10 active |

M10-C imports the three active variants as snapshots under `variants/`. It does not
repoint production profiles yet.

## Boundary

Allowed in this package:

- reviewed plugin source
- `plugin.yaml`
- package-local tests
- source snapshot notes
- historical backup hashes needed for audit

Not allowed:

- profile `.env` files
- profile sessions, logs, cache, auth files, or state databases
- generated runtime memory
- raw private chat logs
- Xiaoqin WorkTool runtime as an active package

## Validation

```bash
pnpm skills:qintopia-tools:check
```

The check compiles each active variant and blocks committed runtime cache files.

## Production Migration

Before any server repoint:

1. Decide whether the target shape is a single shared plugin with profile overlays or
   separate release-managed profile variants.
2. Add release packaging for the chosen shape.
3. Back up each profile-local plugin directory.
4. Repoint one profile at a time.
5. Verify Hermes service active state, plugin import, tool registration, and rollback.
6. Do not delete old profile-local directories until M11/M12 cleanup gates pass.
