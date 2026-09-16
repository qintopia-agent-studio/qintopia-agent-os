# Hermes production recovery runbook

Owner: Qintopia operations. Scope: the September 2026 official-core migration incident.
This runbook does not replace the approved release pipeline or the
[server change policy](../engineering/server-change-policy.md).

## Preconditions

1. Use the separately reviewed recovery PR and published Qintopia release SHA. Recheck
   core SHA, release pointer, seven profiles, five enabled/two disabled WeCom channels,
   service entrypoints and active jobs immediately before switching.
2. Create a root-owned 0700 server-local recovery directory. Record unit definitions,
   drop-ins, release targets, config/environment file hashes and permissions, plugin
   links, execution/delivery states and active workers there. Copy configuration and
   credentials only into this protected directory; never download them or emit values.
3. Inventory profile SQLite paths. For each database use Python
   `sqlite3.Connection.backup` from a read-only source connection into a fresh 0600
   backup, then `PRAGMA quick_check` on the backup. Do not copy a live database while
   omitting its WAL. These are consistent per-database snapshots, not a global
   transaction across independent databases.
4. Record backup completeness and available disk space. Keep all credentials, databases,
   prompts and message bodies on the server. Without a verified backup inventory,
   activation is blocked. Code rollback never restores live business data automatically.
5. Audit old-vs-official source behavior with synthetic fixtures. Old source is
   evidence; it is not a validated all-component rollback image.

Run the release-owned backup tool on the server before activation:

```bash
python3 <release>/runtime/hermes/recovery_backup.py
python3 <release>/runtime/hermes/recovery_backup.py --apply
```

It writes only a timestamped root-owned directory under
`/var/lib/qintopia-hermes-recovery` and prints counts. Inspect the server-local
manifest: `complete` covers the selected configuration, profile state,
cron/session/memory/scripts, plugin link inventory, bridge binding/secret and unit
files, **not every possible external memory provider or symlink target**. Inventory
links without following them; separately verify any required referenced state before
acceptance. Missing mandatory profile config, changed files, unsupported file types,
insufficient space or failed SQLite integrity checks block the backup. Partial
directories without a complete manifest are not usable activation evidence. The backup
contains no restore/replay operation.

## Console artifact and activation

Use the official commit's `git archive`, never archive the live working directory or
profile home. Build outside production, using pnpm and the official dependency lock:

```bash
python3 tools/deploy/build-hermes-dashboard.py \
  --source-archive <official-git-archive.tar.gz> \
  --core-commit 2237be355906fbe6065ce1815711eee52b2d646e \
  --output <new-artifact-directory>
```

The artifact contains matching `web_dist`, a pnpm lock, two runtime helpers and the
checksum-pinned manifest. Record the complete artifact digest and build/CI identity. The
manifest includes upstream typecheck status. Do not label a Vite-only build as a passing
upstream typecheck. A failed typecheck blocks ordinary activation; any exception must
explicitly accept that specific failure after reviewing the HTTP smoke and artifact.

Run the offline official-core HTTP check before promotion:

```bash
<candidate-python> runtime/hermes/check_dashboard_http.py \
  --core-dir <official-core> --dist <artifact>/web_dist
```

The check creates a synthetic profile and blocks external network access. Production
browser acceptance is separate.

Stage the **complete reviewed artifact** at
`/var/lib/qintopia-hermes-dashboard/incoming/<manifest-sha256>` using the reviewed
artifact delivery mechanism. All parents must be root-owned and not group/world
writable. Never scp individual source files, copy old `web_dist`, or install
dependencies at service startup. The matching immutable core release and release-local
Python must already exist under `/var/lib/qintopia-hermes-core/releases/<core-sha>`.

From the published immutable Qintopia release, run as root:

```bash
python3 <release>/deploy/runner/install-hermes-dashboard.py \
  --manifest-sha256 <manifest-sha256>
python3 <release>/deploy/runner/install-hermes-dashboard.py \
  --manifest-sha256 <manifest-sha256> --apply
```

The installer validates files and Ubuntu readability before changing the console
service. It pins the core, interpreter and `HERMES_WEB_DIST`; touches only its console
systemd drop-in; keeps loopback port 9120 and existing authenticated Nginx port 9119;
then checks homepage, assets, session-gated APIs and unauthenticated proxy rejection. It
does not restart Gateways. Any explicit typecheck exception uses
`--accept-upstream-typecheck-failure` in addition to the above arguments.

Verify authenticated browsing through the existing entry with the Codex Chrome plugin:
profiles, sessions and cron tasks must load without key API errors. Never use Playwright
for this incident. If the plugin is unavailable, leave browser acceptance pending for
the owner; HTTP success alone does not close it.

Rollback command (same artifact identity):

```bash
python3 <release>/deploy/runner/install-hermes-dashboard.py \
  --manifest-sha256 <manifest-sha256> --rollback
```

This restores the previous drop-in and restarts only the console; it refuses unexpected
operator drift. Rollback to the original missing-assets console remains degraded. Retain
both matching backend/frontend artifacts and previous drop-in records.

## Snapshot permissions

Only the reviewed global weekly-plan wrapper is eligible. The helper verifies exact
source bytes, fixed paths, parents and regular-file identity before changing metadata.
It saves previous metadata server-side. Credentials and other scripts are excluded.

```bash
python3 <release>/runtime/hermes/repair_snapshot_permissions.py --release <release>
python3 <release>/runtime/hermes/repair_snapshot_permissions.py --release <release> --apply
```

The snapshot timer installer invokes the same repair before root baseline initialization
and runs another snapshot as Ubuntu to exercise the actual timer permissions. Validate
both paths on Linux and repeat the installation before marking it accepted.

```bash
python3 <release>/runtime/hermes/repair_snapshot_permissions.py --release <release> --rollback
```

The last command restores saved metadata only when script bytes still match. Returning
to root:root 0700 reproduces the original snapshot failure and must be labeled degraded.

## QiWe and per-service rollout

Check the **built payload**, not just source:

```bash
<candidate-python> -B runtime/hermes/check_core_plugin_compatibility.py \
  --core-dir <official-core> --plugin-dir <payload>/skills/qiwe
```

The check starts a fresh process for Gateway configuration, cron platform resolution and
standalone send preparation. It uses real official discovery with a synthetic profile
and no external network. It does not claim message delivery coverage.

Publish the full release through CI. Do not repoint `current` with a blanket Gateway
restart. Use reviewed per-service version entries and keep the old release until no
process uses it. Process Silaoshi, then Erhua, then remaining affected profiles; a
service blocked on an unqualified WeCom core fix is explicitly deferred, not forcibly
restarted. For each service, inspect active jobs/workers, wait at most ten minutes, then
defer if still busy. No lock deletion, forced kill, cron-state clearing or automatic
replay.

After switching one idle service, confirm plugin discovery/config parity and request an
owner-triggered message in the original conversation. Confirm receipt, model invocation,
tools where applicable and the original-conversation reply. Wait 15 minutes without new
faults before proceeding. Never initiate messages to real contacts or business groups.

Stop further rollout for startup failure, configuration drift, sustained delivery
failure, duplicate effects or incompatible data. Roll back only that component through
its recorded previous version entry. Do not roll back live databases as a code rollback.

## Remaining core/tool gates

```bash
<candidate-python> runtime/hermes/check_cron_ack_publication.py --core-dir <official-core>
```

This deterministic probe runs only the extracted acknowledgement writer with synthetic
state. Current 2237be3 exposes incomplete JSON and is blocked for atomic-publication
acceptance. A changed upstream writer requires reviewing/adapting the probe, not
treating an unrecognized implementation as a pass. Historical completed executions are
not replayed.

The synthetic transport probe reproduces accepted-message/lost-ack fallback without
sending anything externally:

```bash
<candidate-python> -I runtime/hermes/check_wecom_uncertain_send.py --core-dir <official-core>
```

Current 2237be3 clears stale request caches but may resend after an uncertain result;
keep this as an upstream blocker until a qualified official fix passes the probe.

WeCom acceptance must exercise subscription confirmation, reconnect, kicked sessions,
expired request IDs, timeout/uncertain send outcomes and deduplication against a fake
service before production owner-triggered messages. Investigate short-lived Silaoshi
notification connections separately from long-lived Gateways. Do not use an import-only
fix that creates another competing bot connection, nor weaken terminal hardline checks.

## Closure

Record every profile using passed/failed/unverified, keeping disabled channels explicit.
Service restoration requires authenticated console and applicable owner-triggered
message/tool/isolated-cron loops. Incident closure requires 24 hours and at least one
natural daily-cron cycle. Low-frequency equivalent isolated tests must retain the first
natural-run observation item. No automatic replay of unresolved business jobs.

Update the [incident report](../reports/2026-09-16-hermes-production-recovery.md) with
artifact SHAs, checks, rollback status, pending owner acceptance and upstream blockers.
