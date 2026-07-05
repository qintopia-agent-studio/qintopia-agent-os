# Qintopia Tools Source Snapshot

Snapshot date: 2026-07-05

Mode: read-only server inventory followed by monorepo source adoption. Runtime state,
secrets, pycache files, and `.env` files were excluded.

## Sources

| Variant   | Source path                                                      | SHA-256 `__init__.py`                                              | Notes                                  |
| --------- | ---------------------------------------------------------------- | ------------------------------------------------------------------ | -------------------------------------- |
| Erhua     | `/home/ubuntu/.hermes/profiles/erhua/plugins/qintopia-tools`     | `c662ffc385f101a73301670212c7134ee601a652e02712962a643bd97a448902` | no git metadata; 25 advertised tools   |
| Xiaoman   | `/home/ubuntu/.hermes/profiles/xiaoman/plugins/qintopia-tools`   | `1289ca29b2b6ec90de387794afd927d8df4ff7c0fd52f2bfdc0b67de07f46911` | no git metadata; 23 advertised tools   |
| Wenyuange | `/home/ubuntu/.hermes/profiles/wenyuange/plugins/qintopia-tools` | `a69c2b5f2a386319d8978ea25e755e052d32d961e32a87d2f932b65fe665a49a` | no git metadata; 23 advertised tools   |
| Huabaosi  | `/home/ubuntu/.hermes/profiles/huabaosi/plugins/qintopia-tools`  | n/a                                                                | directory missing during M10-C check   |
| Xiaoqin   | `/home/ubuntu/.hermes/profiles/xiaoqin/plugins/qintopia-tools`   | not adopted                                                        | deprecated; M12 cleanup candidate only |

## Variant Differences

- Wenyuange is the smallest active baseline and includes message-store search.
- Xiaoman adds `qintopia_xiaoman_activity_record_get` and related implementation
  helpers.
- Erhua includes weather and daily digest publisher wrappers, plus message-store search.
- Xiaoman had server-local backup files. They are preserved under
  `docs/server-backups/xiaoman/` for audit, not used as runtime package files.

## Validation Evidence

M10-C local adoption validation:

```bash
pnpm skills:qintopia-tools:check
```

Additional exploratory variant tests:

| Variant   | Command                                                                                   | Result                                                                                          |
| --------- | ----------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| Xiaoman   | `cd skills/qintopia-tools/variants/xiaoman && python3 -m unittest discover -s tests -v`   | passed, 23 tests                                                                                |
| Erhua     | `cd skills/qintopia-tools/variants/erhua && python3 -m unittest discover -s tests -v`     | 38 passed, 1 weather fixture failure tied to fixed 2026-06-28 timestamps versus current runtime |
| Wenyuange | `cd skills/qintopia-tools/variants/wenyuange && python3 -m unittest discover -s tests -v` | 29 passed, 4 tests attempted live Dify HTTP instead of staying fixture-only                     |

The repository gate intentionally uses syntax, structure, registry, and secret/runtime
state checks until these historical variant tests are made hermetic.

Production profiles were not repointed during this snapshot adoption.

## Release Packaging

M10-C release packaging adds this package to `qintopia-agent-os-deploy-bundle`:

- `skills/qintopia-tools/manifest.yaml`
- `skills/qintopia-tools/README.md`
- `skills/qintopia-tools/docs/source-snapshot.md`
- `skills/qintopia-tools/variants/*`

The deploy bundle is only a release input. A production profile plugin repoint still
requires a separate release assembly, backup, one-profile-at-a-time restart, plugin
import/tool-registration validation, and rollback note.
