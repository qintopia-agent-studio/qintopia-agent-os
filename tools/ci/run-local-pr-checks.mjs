#!/usr/bin/env node

import { execFileSync, spawnSync } from "node:child_process";
import process from "node:process";

const repoRoot = process.cwd();
const mode = process.argv[2] ?? "auto";
const supportedModes = new Set(["quick", "heavy", "postgres", "auto"]);

if (!supportedModes.has(mode)) {
  process.stderr.write(
    `Unsupported mode "${mode}". Use one of: ${Array.from(supportedModes).join(", ")}.\n`
  );
  process.exit(1);
}

const heavyRiskPrefixes = [
  ".github/workflows/",
  "runtime/sidecar/",
  "runtime/postgres/",
  "deploy/runner/",
  "deploy/rollback/",
  "deploy/sidecar/",
  "tools/deploy/",
];

const postgresEnv = {
  ...process.env,
  QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE:
    process.env.QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE ?? "1",
  QINTOPIA_SIDECAR_DATABASE_URL:
    process.env.QINTOPIA_SIDECAR_DATABASE_URL ??
    "postgres://postgres:postgres@127.0.0.1:5432/qintopia_test",
};

function run(command, args, options = {}) {
  const display = [command, ...args].join(" ");
  process.stdout.write(`\n$ ${display}\n`);
  execFileSync(command, args, {
    cwd: repoRoot,
    stdio: "inherit",
    env: options.env ?? process.env,
  });
}

function readGitLines(args) {
  try {
    return execFileSync("git", args, {
      cwd: repoRoot,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    })
      .split("\n")
      .map((line) => line.trim())
      .filter(Boolean);
  } catch {
    return [];
  }
}

function resolveMergeBase() {
  for (const ref of [
    process.env.QINTOPIA_LOCAL_PR_BASE_REF,
    "origin/master",
    "master",
  ]) {
    if (!ref) {
      continue;
    }
    try {
      return execFileSync("git", ["merge-base", "HEAD", ref], {
        cwd: repoRoot,
        encoding: "utf8",
        stdio: ["ignore", "pipe", "ignore"],
      }).trim();
    } catch {
      continue;
    }
  }
  return null;
}

function collectChangedPaths() {
  const changed = new Set([
    ...readGitLines(["diff", "--name-only"]),
    ...readGitLines(["diff", "--name-only", "--cached"]),
  ]);
  const mergeBase = resolveMergeBase();
  if (mergeBase) {
    for (const path of readGitLines(["diff", "--name-only", `${mergeBase}...HEAD`])) {
      changed.add(path);
    }
  }
  return Array.from(changed).sort();
}

function isPostgresReady() {
  const result = spawnSync(
    "pg_isready",
    ["-h", "127.0.0.1", "-p", "5432", "-d", "qintopia_test", "-U", "postgres"],
    {
      cwd: repoRoot,
      env: postgresEnv,
      stdio: "ignore",
    }
  );
  return result.status === 0;
}

function runQuickChecks() {
  run("pnpm", ["check:light"]);
}

function runHeavyRustChecks() {
  run("pnpm", ["check:runtime"]);
  run("cargo", [
    "clippy",
    "--manifest-path",
    "runtime/sidecar/Cargo.toml",
    "--all-targets",
    "--no-default-features",
    "--",
    "-D",
    "warnings",
  ]);
  run("cargo", [
    "clippy",
    "--manifest-path",
    "runtime/sidecar/Cargo.toml",
    "--all-targets",
    "--all-features",
    "--",
    "-D",
    "warnings",
  ]);
  run(
    "cargo",
    ["test", "--manifest-path", "runtime/sidecar/Cargo.toml", "--all-features"],
    {
      env: {
        ...process.env,
        RUST_MIN_STACK: process.env.RUST_MIN_STACK ?? "33554432",
      },
    }
  );
}

function runPostgresChecks() {
  const postgresTests = [
    [
      "--features",
      "postgres-integration-tests",
      "group_message_send::tests::postgres_send_ready_is_idempotent_and_fails_closed",
      "--",
      "--ignored",
      "--exact",
    ],
    [
      "--features",
      "postgres-integration-tests",
      "image_generation::tests::postgres_stale_processing_claim_is_terminal_ambiguous",
      "--",
      "--ignored",
      "--exact",
    ],
    [
      "--features",
      "postgres-integration-tests",
      "huabaosi_feishu_artifact_mirror::tests::postgres_mirror_state_is_idempotent_and_redacted",
      "--",
      "--ignored",
      "--exact",
    ],
    [
      "--features",
      "postgres-integration-tests",
      "operations::tests::postgres_feishu_backed_approval_requires_matching_revalidation_evidence",
      "--",
      "--ignored",
      "--exact",
    ],
    [
      "--features",
      "postgres-integration-tests",
      "event::tests::postgres_callback_storage_redacts_credentials",
      "--",
      "--ignored",
      "--exact",
    ],
    [
      "--features",
      "postgres-integration-tests,huabaosi-staging-adapter,qiwe-staging-adapter",
      "qiwe_image_send_state::tests::postgres_qiwe_send_state_claims_feishu_bridge_without_persisting_uri",
      "--",
      "--ignored",
      "--exact",
    ],
    [
      "--features",
      "postgres-integration-tests",
      "qiwe_image_send_state::tests::postgres_qiwe_send_state_is_idempotent_and_redacted",
      "--",
      "--ignored",
      "--exact",
    ],
    [
      "--features",
      "postgres-integration-tests",
      "qiwe_image_send_state::tests::postgres_qiwe_send_state_rejects_stale_claim",
      "--",
      "--ignored",
      "--exact",
    ],
    [
      "--features",
      "postgres-integration-tests",
      "qiwe_image_send_state::tests::postgres_qiwe_send_state_recovers_expired_callback_and_terminalizes_ambiguous_send",
      "--",
      "--ignored",
      "--exact",
    ],
    [
      "--features",
      "postgres-integration-tests",
      "qiwe_image_send_state::tests::postgres_qiwe_send_state_expires_missing_callback_during_reclaim",
      "--",
      "--ignored",
      "--exact",
    ],
    [
      "--features",
      "postgres-integration-tests",
      "qiwe_image_send_state::tests::postgres_qiwe_send_state_terminalizes_stale_upload_and_send",
      "--",
      "--ignored",
      "--exact",
    ],
    [
      "--features",
      "postgres-integration-tests",
      "qiwe_image_send_state::tests::postgres_qiwe_send_state_terminalizes_legacy_unrecorded_claim",
      "--",
      "--ignored",
      "--exact",
    ],
  ];

  for (const args of postgresTests) {
    run("cargo", ["test", "--manifest-path", "runtime/sidecar/Cargo.toml", ...args], {
      env: postgresEnv,
    });
  }
  run("deploy/sidecar/scripts/operations-control-plane-apply-smoke.sh", [], {
    env: postgresEnv,
  });
}

if (mode === "quick") {
  runQuickChecks();
  process.stdout.write("\nLocal PR quick checks passed.\n");
  process.exit(0);
}

if (mode === "postgres") {
  if (!isPostgresReady()) {
    process.stderr.write(
      "Local PostgreSQL tier requires qintopia_test on 127.0.0.1:5432 for postgres/postgres.\n"
    );
    process.exit(1);
  }
  runPostgresChecks();
  process.stdout.write("\nLocal PR PostgreSQL checks passed.\n");
  process.exit(0);
}

if (mode === "heavy") {
  runQuickChecks();
  runHeavyRustChecks();
  if (!isPostgresReady()) {
    process.stderr.write(
      "Local PostgreSQL tier requires qintopia_test on 127.0.0.1:5432 for postgres/postgres.\n"
    );
    process.exit(1);
  }
  runPostgresChecks();
  process.stdout.write("\nLocal PR heavy checks passed.\n");
  process.exit(0);
}

const changedPaths = collectChangedPaths();
const heavyRiskPaths = changedPaths.filter((candidate) =>
  heavyRiskPrefixes.some((prefix) => candidate.startsWith(prefix))
);

process.stdout.write(
  `Detected ${changedPaths.length} changed path(s) for local PR auto checks.\n`
);
if (heavyRiskPaths.length > 0) {
  process.stdout.write(
    `High-risk paths detected:\n${heavyRiskPaths.map((path) => `- ${path}`).join("\n")}\n`
  );
} else {
  process.stdout.write("No high-risk paths detected; staying on the quick tier.\n");
}

runQuickChecks();

if (heavyRiskPaths.length === 0) {
  process.stdout.write("\nLocal PR auto checks passed on the quick tier.\n");
  process.exit(0);
}

runHeavyRustChecks();

if (isPostgresReady()) {
  runPostgresChecks();
  process.stdout.write(
    "\nLocal PR auto checks passed with quick, heavy Rust, and PostgreSQL tiers.\n"
  );
  process.exit(0);
}

process.stdout.write(
  "\nSkipped local PostgreSQL tier because qintopia_test is not ready on 127.0.0.1:5432.\n"
);
process.stdout.write(
  "Run `pnpm check:pr:postgres` after provisioning the disposable local database, or let CI cover that tier.\n"
);
process.stdout.write(
  "\nLocal PR auto checks passed with quick and heavy Rust tiers.\n"
);
