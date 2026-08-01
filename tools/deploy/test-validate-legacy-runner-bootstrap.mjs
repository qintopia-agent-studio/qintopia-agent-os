#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "qintopia-bootstrap-test-"));
const baseSha = "0123456789abcdef0123456789abcdef01234567";
const olderSha = "1111111111111111111111111111111111111111";
const bundleSha = "abcdef0123456789abcdef0123456789abcdef01";
const releaseSha = "2222222222222222222222222222222222222222";
const resultsPath = path.join(tmpRoot, "deploy-results.json");

const successfulResult = (sha, runId, timestamp) => ({
  status: "succeeded",
  environment: "production",
  release_sha: sha,
  commit_sha: sha,
  runtime_sha: sha,
  runtime_artifact_profile: "huabaosi-production",
  deploy_bundle_sha: sha,
  release_scope: ["sidecar-runtime", "deploy-bundle", "hermes-plugins"],
  restart_targets: ["qintopia-system-services"],
  workflow_run: { id: String(runId), run_started_at: timestamp },
});

const commonArgs = [
  "tools/deploy/validate-legacy-runner-bootstrap.mjs",
  "--deploy-results-file",
  resultsPath,
  "--commit-sha",
  baseSha,
  "--runtime-sha",
  baseSha,
  "--deploy-bundle-sha",
  bundleSha,
  "--release-sha",
  releaseSha,
  "--runtime-artifact-profile",
  "huabaosi-production",
  "--release-scope",
  "deploy-bundle",
  "--restart-targets",
  "qintopia-system-services",
  "--rollback-on-smoke-failure",
  "true",
];

const run = (replacements = {}) => {
  const args = [...commonArgs];
  for (const [name, value] of Object.entries(replacements)) {
    const index = args.indexOf(name);
    if (index < 0) {
      throw new Error(`unknown test argument ${name}`);
    }
    args[index + 1] = value;
  }
  return spawnSync("node", args, { cwd: repoRoot, encoding: "utf8" });
};

const assertRejected = (name, replacements, message) => {
  const result = run(replacements);
  if (result.status === 0 || !result.stderr.includes(message)) {
    throw new Error(
      `${name}: expected rejection containing ${message}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`
    );
  }
};

try {
  fs.writeFileSync(
    resultsPath,
    `${JSON.stringify(
      [
        successfulResult(olderSha, 10, "2026-07-30T00:00:00Z"),
        successfulResult(baseSha, 20, "2026-07-31T00:00:00Z"),
        {
          ...successfulResult(bundleSha, 30, "2026-07-31T01:00:00Z"),
          status: "failed",
        },
      ],
      null,
      2
    )}\n`,
    "utf8"
  );

  const accepted = run();
  if (accepted.status !== 0) {
    throw new Error(`valid bootstrap was rejected\n${accepted.stderr}`);
  }
  const evidence = JSON.parse(accepted.stdout);
  if (
    evidence.status !== "legacy_runner_bootstrap_ready" ||
    evidence.base_runtime_sha !== baseSha ||
    evidence.requested_deploy_bundle_sha !== bundleSha
  ) {
    throw new Error(`unexpected bootstrap evidence ${accepted.stdout}`);
  }

  assertRejected(
    "stale-runtime",
    { "--commit-sha": olderSha, "--runtime-sha": olderSha },
    "runtime must match the latest successful production deploy"
  );
  assertRejected(
    "commit-runtime-mismatch",
    { "--commit-sha": olderSha },
    "commit_sha must equal runtime_sha"
  );
  assertRejected(
    "broad-scope",
    { "--release-scope": "sidecar-runtime,deploy-bundle" },
    "release_scope=deploy-bundle"
  );
  assertRejected(
    "broad-restart",
    { "--restart-targets": "qintopia-system-services,hermes-xiaoman" },
    "restart_targets=qintopia-system-services"
  );
  assertRejected(
    "same-bundle",
    { "--deploy-bundle-sha": baseSha },
    "requires a newer deploy_bundle_sha"
  );
  assertRejected(
    "colliding-release",
    { "--release-sha": bundleSha },
    "release_sha must be distinct"
  );
  assertRejected(
    "wrong-profile",
    { "--runtime-artifact-profile": "qiwe-production" },
    "requires huabaosi-production"
  );
  assertRejected(
    "rollback-disabled",
    { "--rollback-on-smoke-failure": "false" },
    "rollback_on_smoke_failure=true"
  );

  fs.writeFileSync(
    resultsPath,
    `${JSON.stringify(
      [
        successfulResult(olderSha, 10, "2026-07-30T00:00:00Z"),
        {
          ...successfulResult(baseSha, 20, "2026-07-31T00:00:00Z"),
          environment: "staging",
        },
      ],
      null,
      2
    )}\n`,
    "utf8"
  );
  assertRejected(
    "non-production-result",
    {},
    "runtime must match the latest successful production deploy"
  );

  console.log("Legacy deploy-runner bootstrap validation tests passed.");
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}
