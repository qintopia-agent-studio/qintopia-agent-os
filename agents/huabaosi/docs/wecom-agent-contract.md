# Huabaosi WeCom contract

These are retained operating constraints, not a new permission grant. Read the sections
relevant to the change before editing. Original conditions and exceptions remain
binding; summaries in AGENTS.md do not replace them.

Backtick paths in root-source rules are repository-relative; paths in Sidecar-source
rules retain their original `runtime/sidecar/` base. New navigation links are relative
to this document.

[Migration inventory](../../../docs/plans/active/agents-guidance/README.md) records the
baseline and source locations.

<a id="root-091"></a>

## Commands — root-091

<!-- preserved-rule: root-091 -->

- Huabaosi WeCom gateway read-only observation smoke:
  `QINTOPIA_HUABAOSI_WECOM_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/huabaosi-wecom-gateway-observation-smoke.sh`
  Run it as the `ubuntu` Hermes systemd user, not through `sudo`, because it inspects
  `systemctl --user`. Its journal scan is fixed to the latest 30 minutes and 160 lines;
  production commands, paths, and the journal window must not accept caller-controlled
  overrides. Keep test doubles in the Node fixture only.

<!-- /preserved-rule: root-091 -->

<a id="root-092"></a>

## Commands — root-092

<!-- preserved-rule: root-092 -->

- Huabaosi WeCom canary disabled-state observation smoke:
  `QINTOPIA_HUABAOSI_WECOM_CANARY_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/huabaosi-wecom-canary-observation-smoke.sh`

<!-- /preserved-rule: root-092 -->

<a id="root-122"></a>

## Commands — root-122

<!-- preserved-rule: root-122 -->

- Huabaosi WeCom shadow capture fixture replay:
  `cargo test --manifest-path runtime/sidecar/Cargo.toml huabaosi_wecom_shadow`

<!-- /preserved-rule: root-122 -->

<a id="root-123"></a>

## Commands — root-123

<!-- preserved-rule: root-123 -->

- Huabaosi WeCom policy preview fixture replay:
  `cargo test --manifest-path runtime/sidecar/Cargo.toml huabaosi_wecom_policy`

<!-- /preserved-rule: root-123 -->

<a id="root-124"></a>

## Commands — root-124

<!-- preserved-rule: root-124 -->

- Huabaosi WeCom canary gateway fixture replay:
  `cargo test --manifest-path runtime/sidecar/Cargo.toml huabaosi_wecom_canary`

<!-- /preserved-rule: root-124 -->

<a id="root-240"></a>

## Core Rules — root-240

<!-- preserved-rule: root-240 -->

- Huabaosi image generation may override the shared 60-second socket timeout only
  through `QINTOPIA_HUABAOSI_IMAGE_HTTP_TIMEOUT_SECONDS`, defaulting to 180 seconds and
  bounded from 60 through 240 seconds. The upper bound must leave room inside the fixed
  10-minute image claim lease for the five bounded Feishu auth/search/upload/readback/
  upsert calls, transform, and the final transaction. A provider transport or protocol
  error after request bytes may have been sent is ambiguous and must not be retried
  automatically. Image-generation failure audits may record
  `external_generation_executed=false` only before provider request execution, `true`
  only after a valid provider payload is accepted, and `null` when the provider outcome
  cannot be proved. `external_media_write_executed` must likewise remain `null` for an
  unprovable upload or Feishu write and become `true` only after confirmed storage. Do
  not change the shared timeout for QiWe, WeCom, Feishu, or other adapters to remediate
  image-provider latency.

<!-- /preserved-rule: root-240 -->

<a id="root-243"></a>

## Core Rules — root-243

<!-- preserved-rule: root-243 -->

- 阿亮画报师生产 WeCom Bot 的 `Interrupting current task` / `Response formatting failed`
  用户可见中断提示来自 live Hermes gateway busy-ack and platform send fallback
  (`hermes-gateway-huabaosi.service`, `gateway/run.py`, `gateway/platforms/base.py`),
  not Rust sidecar image generation or QiWe image-send state. Diagnose this path through
  Huabaosi Hermes/WeCom runtime first, and do not hot-edit the server.

<!-- /preserved-rule: root-243 -->

<a id="root-255"></a>

## Core Rules — root-255

<!-- preserved-rule: root-255 -->

- `huabaosi-wecom-gateway-observation-smoke.sh` may only inspect the live Huabaosi
  Hermes WeCom user-service active state through `systemctl --user`, fixed service
  command, fixed
  `/home/ubuntu/.config/systemd/user/hermes-gateway-huabaosi.service.d/env.conf`
  drop-in, fixed required `/home/ubuntu/.hermes/profiles/huabaosi/.env`, public
  `busy_input_mode`, release/current presence, and sanitized user-journal marker counts.
  It must reject missing or additional drop-ins and optional or alternate environment
  files. It must not source `.env`, print raw journal lines, print user messages, read
  tokens, restart services, send WeCom messages, run image generation, write Postgres or
  Feishu, call QiWe/provider/media endpoints, or modify live Hermes profile state.

<!-- /preserved-rule: root-255 -->

<a id="root-256"></a>

## Core Rules — root-256

<!-- preserved-rule: root-256 -->

- `huabaosi-wecom-canary-observation-smoke.sh` may only verify that the canary gateway
  remains unscheduled and disabled, then run `huabaosi-wecom-canary-preflight` for a
  sanitized local configuration summary. From release/current it must discover the
  immutable `sidecar/qintopia-message-sidecar` binary rather than fall back to source
  Cargo execution. It must not use `--apply`, read stdin, source `.env`, print
  endpoint/token/id values, write Postgres or Feishu, call WeCom, QiWe, provider, or
  media endpoints, run image generation, publish messages, install units, or modify the
  live Hermes profile.

<!-- /preserved-rule: root-256 -->

<a id="root-257"></a>

## Core Rules — root-257

<!-- preserved-rule: root-257 -->

- `huabaosi-wecom-shadow-capture` may only preview one supplied WeCom event from bounded
  stdin and emit sanitized metadata, hashes, byte counts, field presence, and fixed
  guardrails. It must not add `--apply`, open Postgres or network connections, write
  artifacts, send WeCom/QiWe messages, call image providers, upload media, write Feishu,
  or emit raw ids, user text, media URLs, filenames, tokens, or callback file
  credentials.

<!-- /preserved-rule: root-257 -->

<a id="root-258"></a>

## Core Rules — root-258

<!-- preserved-rule: root-258 -->

- `huabaosi-wecom-policy-preview` may only preview one supplied WeCom event from bounded
  stdin and emit sanitized policy decisions for message classification, busy-session
  handling, internal-process filtering, formatting fallback, user-safe fallback copy,
  and idempotency. It must not add `--apply`, open Postgres or network connections,
  write artifacts, send WeCom/QiWe messages, call image providers, upload media, write
  Feishu, or emit raw ids, user text, media URLs, filenames, tokens, or callback file
  credentials. Suppression rules must match narrow complete internal templates; do not
  block ordinary user requests through broad words such as `plain text` or `纯文本`.

<!-- /preserved-rule: root-258 -->

<a id="root-259"></a>

## Core Rules — root-259

<!-- preserved-rule: root-259 -->

- `huabaosi-wecom-canary-preflight` must not read stdin, open network or database
  connections, source env files, reveal configuration values, write Feishu/Postgres, or
  send WeCom/QiWe messages. `huabaosi-wecom-canary-gateway --apply` is allowed only in
  an owner-reviewed staging command built with the non-default
  `huabaosi-wecom-canary-gateway` Cargo feature, explicit enable flag, approval phrase,
  HTTPS endpoint, token, and exact Bot/chat/user allowlists. Default production builds
  must fail closed before stdin, network, database, or send access. The command must not
  change the production Bot route, install timers, broaden sends beyond the allowlist,
  run image generation, upload media, or write Feishu/Postgres.

<!-- /preserved-rule: root-259 -->

<a id="sidecar-010"></a>

## Commands — sidecar-010

<!-- preserved-rule: sidecar-010 -->

- Huabaosi WeCom shadow capture fixture tests: `cargo test huabaosi_wecom_shadow`

<!-- /preserved-rule: sidecar-010 -->

<a id="sidecar-011"></a>

## Commands — sidecar-011

<!-- preserved-rule: sidecar-011 -->

- Huabaosi WeCom policy preview fixture tests: `cargo test huabaosi_wecom_policy`

<!-- /preserved-rule: sidecar-011 -->

<a id="sidecar-012"></a>

## Commands — sidecar-012

<!-- preserved-rule: sidecar-012 -->

- Huabaosi WeCom canary gateway fixture tests: `cargo test huabaosi_wecom_canary`

From the monorepo root, prefer:

<!-- /preserved-rule: sidecar-012 -->

<a id="sidecar-056"></a>

## Rules — sidecar-056

<!-- preserved-rule: sidecar-056 -->

- `huabaosi-wecom-shadow-capture` is a preview-only migration command. It may read one
  event from bounded stdin and emit only sanitized hashes, byte counts, field presence,
  classification, and fixed guardrails. It must not gain an apply mode, connect to
  Postgres or external services, send WeCom/QiWe messages, generate or upload media,
  write Feishu, create artifacts, or print raw ids, user text, media URLs, filenames,
  tokens, or callback credentials.

<!-- /preserved-rule: sidecar-056 -->

<a id="sidecar-057"></a>

## Rules — sidecar-057

<!-- preserved-rule: sidecar-057 -->

- `huabaosi-wecom-policy-preview` is a preview-only migration command. It may read one
  event from bounded stdin and emit only sanitized policy classifications, fixed
  fallback copy, and hash-based idempotency metadata. It must not gain an apply mode,
  connect to Postgres or external services, send WeCom/QiWe messages, generate or upload
  media, write Feishu, create artifacts, or print raw ids, user text, media URLs,
  filenames, tokens, or callback credentials. Internal-process suppression must use
  narrow full-template matches with negative fixture coverage for ordinary user text
  containing terms such as `plain text`.

<!-- /preserved-rule: sidecar-057 -->

<a id="sidecar-058"></a>

## Rules — sidecar-058

<!-- preserved-rule: sidecar-058 -->

- `huabaosi-wecom-canary-preflight` is a local configuration preflight only. It must not
  read stdin, open network or database connections, source env files, or emit
  endpoint/token/id values. `huabaosi-wecom-canary-gateway --apply` is staging-only,
  requires the non-default `huabaosi-wecom-canary-gateway` Cargo feature plus explicit
  enablement, approval phrase, HTTPS endpoint, token, and exact Bot/chat/user
  allowlists, and must remain unscheduled. Default builds must fail closed before stdin,
  network, database, or send access. It must not change production routing, run image
  generation, upload media, write Feishu/Postgres, or send outside the allowlist.

<!-- /preserved-rule: sidecar-058 -->
