#!/usr/bin/env node

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const repoRoot = process.cwd();
const checker = path.join(repoRoot, "tools/deploy/check-erhua-room-member-sync.mjs");
const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "erhua-room-member-sync-"));
const ROOM_SCOPE =
  "sha256:c5c4e70d823efa23b83de70ce5008d746e76bdce54e37605b967b4bfd4036356";

try {
  let evidence = writeCase("applied.json", validEvidence());
  let result = runChecker(evidence);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /room member sync check passed/);
  assert.match(result.stdout, /scope=sha256:/);

  evidence = writeCase(
    "prefixed.txt",
    `erhua_room_member_sync=${JSON.stringify(validEvidence())}\n`
  );
  result = runChecker(evidence);
  assert.equal(result.status, 0, result.stderr);

  evidence = writeCase(
    "noisy.txt",
    `2026-08-10T08:00:00Z INFO identity backfill\n${JSON.stringify(
      validEvidence(),
      null,
      2
    )}\n`
  );
  result = runChecker(evidence);
  assert.equal(result.status, 0, result.stderr);

  evidence = writeCase(
    "dry-run.json",
    validEvidence({
      dry_run: true,
      room_member_identities_upserted: 0,
      stale_room_member_identities_marked: 0,
    })
  );
  result = runChecker(evidence);
  assert.equal(result.status, 0, result.stderr);

  evidence = writeCase(
    "zero-members.json",
    validEvidence({ room_members_discovered: 0, room_member_identities_upserted: 0 })
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /at least one room member/);

  evidence = writeCase(
    "partial-apply.json",
    validEvidence({ room_members_discovered: 8, room_member_identities_upserted: 7 })
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /upsert every discovered/);

  evidence = writeCase(
    "dry-run-upserted.json",
    validEvidence({
      dry_run: true,
      room_member_identities_upserted: 1,
      stale_room_member_identities_marked: 0,
    })
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /dry-run/);

  evidence = writeCase(
    "dry-run-stale-marked.json",
    validEvidence({
      dry_run: true,
      room_member_identities_upserted: 0,
      stale_room_member_identities_marked: 1,
    })
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /stale identities marked/);

  evidence = writeCase(
    "wrong-source.json",
    validEvidence({ source: "message_sender_backfill" })
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /current_qiwe_room_member_roster/);

  evidence = writeCase("missing-scope.json", {
    ...validEvidence(),
    scope_fingerprint: undefined,
  });
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /scope_fingerprint/);

  evidence = writeCase(
    "secret-leak.json",
    `${JSON.stringify(validEvidence())}\nDATABASE_URL=postgresql://example\n`
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /forbidden sensitive fragment/);

  evidence = writeCase(
    "chat-id-leak.json",
    JSON.stringify({ ...validEvidence(), chat_id: "secret-room" })
  );
  result = runChecker(evidence);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /forbidden sensitive fragment/);
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Erhua room member sync test passed.");

function validEvidence(overrides = {}) {
  return {
    total_identity_keys: 0,
    resolved: 0,
    unresolved: 0,
    room_members_discovered: 12,
    room_member_identities_upserted: 12,
    stale_room_member_identities_marked: 3,
    messages_updated: 0,
    platform_identities_materialized: 0,
    source: "current_qiwe_room_member_roster",
    scope_fingerprint: ROOM_SCOPE,
    dry_run: false,
    unresolved_keys: [],
    ...overrides,
  };
}

function writeCase(name, value) {
  const file = path.join(tmpRoot, name);
  fs.writeFileSync(
    file,
    typeof value === "string" ? value : JSON.stringify(value, null, 2)
  );
  return file;
}

function runChecker(file) {
  return spawnSync(process.execPath, [checker, file], {
    cwd: repoRoot,
    encoding: "utf8",
  });
}
