# Xiaoman Profile Bundle Migration

Updated: 2026-07-15

## Goal

Move Xiaoman's stable, non-secret Hermes profile behavior into the reviewed release flow
without copying runtime state or changing the live profile before parity and rollback
evidence exist.

## Source Inventory

The source is the live profile at `/home/ubuntu/.hermes/profiles/xiaoman`, observed
read-only on 2026-07-15. Hashes identify the reviewed source bytes without publishing
their values.

| Source file                  | SHA-256                                                            | Disposition  | Reason                                                       |
| ---------------------------- | ------------------------------------------------------------------ | ------------ | ------------------------------------------------------------ |
| `SOUL.md`                    | `4b54c777e09102385665554829df7b1665bde57d28b4c5bc5ce34fd1d052801e` | template     | Stable behavior; names and channel targets require inputs    |
| `profile.yaml`               | `b34f56b16eac72dc561faef1178d8242705000376561327054e9a15809c2de09` | template     | Non-secret profile description                               |
| `config.yaml`                | `31d6563d3abbf5b2b6956f6626a65add3e21a358a301e08346e3e68840fe63f4` | runtime-only | Contains API-key, provider, endpoint, and local overrides    |
| `webhook_subscriptions.json` | `99a4f8e8e63ea7fd42752c59ef82182d2259567ec2550b648c33a278ee06cd93` | runtime-only | Contains live webhook secrets and delivery declarations      |
| `channel_directory.json`     | `7faf233250b4590c26cc9daacf06765d1adbb7eb418efa1c154fb27d0bcfae72` | runtime-only | Contains real Feishu and WeCom conversation identifiers      |
| `cron/jobs.json`             | `f3767267c14a5c6e62e78b6538df2efe1e1a2185daa65c0658d6bca2b2db0d16` | runtime-only | Empty job state still contains a runtime-generated timestamp |

Owner: `qiaopengjun5162`

Risk level: high, because changing `SOUL.md` changes a production Agent's behavior.

## Reviewed Inputs

The repository may own:

- a `SOUL.md` template with fixed placeholders for the operations owner and technical
  owner names and WeCom targets;
- the non-secret `profile.yaml` description template;
- a strict renderer that accepts only the declared input names;
- fake fixture values, deterministic tests, bundle metadata, and a read-only parity
  smoke.

The real values remain in a root-owned server-local JSON file. The renderer must never
print those values or copy that file into a release artifact. Production parity
observation must run as root when reading the default file; local fixture observations
use a custom current-user-owned temporary file.

## Excluded State

This migration does not adopt `.env`, `config.yaml`, webhook secrets, channel
identifiers, cron timestamps, sessions, auth, messages, memories, logs, cache, locks,
state databases, server backups, or Hermes core patches.

## PR Sequence

1. Package the reviewed templates, strict renderer, fake fixtures, and read-only parity
   smoke. Do not render into the immutable release and do not change the live profile.
2. Provision the declared values through an owner-approved server configuration path,
   deploy the observation artifact, and record a byte-for-byte production parity run. Do
   not create profile symlinks.
3. Add the reviewed symlink cutover only after the previous release contains a
   parity-proven bundle. Preserve the original regular files as the first-cutover
   rollback source and make rollback restore them when the previous release has no
   compatible bundle.

Each step is a separate PR and must not include image-provider, QiWe-send, Hermes-core,
or unrelated workflow changes.

## Values Migration Command

The second PR adds one fixed, manual command that prepares the server-local values file
for production parity observation. It must:

- require root and the exact one-shot approval phrase before reading the live profile;
- read only the fixed live `SOUL.md` and `profile.yaml` paths and require their reviewed
  SHA-256 values;
- extract exactly the four declared identity values in memory without printing them;
- validate those values through the bundle's existing allowlist;
- render into a temporary directory and require byte-for-byte parity before writing;
- create `/etc/qintopia/xiaoman-profile-bundle-values.json` only when it does not
  already exist, with root ownership and mode `0600`;
- write the complete JSON atomically without replacing an existing file;
- emit only fixed status, input names, source hashes, and boolean boundary fields.

It must not accept arbitrary paths, source shell files, edit the live profile, create a
symlink, restart Hermes, use the network, write Postgres or Feishu, call an external
adapter, publish, or send. Failure before the final no-clobber write leaves no values
file. Because Hermes does not read this file, rollback before profile activation is to
leave it unused; owner-approved removal may happen later without urgency.

## Validation

- In-memory source-template parity reproduced the observed production `SOUL.md` and
  `profile.yaml` hashes without writing or printing the four live identity values.
- profile bundle manifest and placeholder allowlist checks;
- deterministic fixture render and negative tests;
- deploy-bundle content and secret scan;
- production observation smoke that renders only into a temporary directory and compares
  hashes without printing values;
- standard agent, deploy, restart-impact, policy, and secret checks.

## Production Boundary

The first PR packages observation inputs only. It must not edit the server, create a
symlink, restart Xiaoman, write Postgres or Feishu, call Huabaosi or QiWe, publish, or
send externally.

If the owner later publishes a Release containing this PR, the deploy workflow still
uses its minimum internal system-service restart because the deploy artifact changed.
The observation-only agent paths resolve to no Xiaoman gateway restart, and the runner
does not activate the bundle.

Publishing a later Release remains an owner-only action. A production profile cutover
requires a separate owner-reviewed PR, explicit deployment decision, smoke, and rollback
record.
