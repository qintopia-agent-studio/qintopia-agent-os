# Deprecated: WorkTool Hermes Plugin

The WorkTool Hermes plugin is deprecated. It should not be used for new Qintopia Agent
OS channel work.

## Source

- Local source: `../worktool-hermes-plugin`
- Local branch observed on 2026-07-03: `master`
- Local reference observed on 2026-07-03: `04e95e1556cb820f5630a0f4781073cddf23c4f4`
- Server plugin path observed on 2026-07-03:
  `/home/ubuntu/.hermes/profiles/xiaoqin/plugins/worktool-platform`
- Server unit observed on 2026-07-03: `hermes-gateway-xiaoqin-worktool.service` is
  loaded, disabled, and inactive.

## Decision

Keep this package as audit material only. New channel integrations should use active
skills such as `skills/qiwe`, not WorkTool.
