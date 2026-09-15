# Huabaosi Qintopia Tools Variant

This Hermes plugin preserves the Qintopia-specific Huabaosi completion policy without
patching Hermes core. It blocks `kanban_complete` unless completion evidence cites a
`rec...` design-output record. When `metadata.artifacts` is present, the evidence must
also state the `成品图` attachment status.

The hook is inert for other profiles and tools. The profile plugin directory is
repointed to this release-managed variant during the Hermes core cutover.

Validate with:

```bash
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover \
  -s skills/qintopia-tools/variants/huabaosi/tests -v
```
