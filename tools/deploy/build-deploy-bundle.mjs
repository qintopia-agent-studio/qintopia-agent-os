#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { execFileSync } from "node:child_process";

const repoRoot = process.cwd();
const bundleName = "qintopia-agent-os-deploy-bundle";
const outputRoot = path.join(repoRoot, "dist", "deploy-bundles");
const bundleDir = path.join(outputRoot, bundleName);
const payloadDir = path.join(bundleDir, "payload");
const archiveName = `${bundleName}.tar.gz`;
const archivePath = path.join(bundleDir, archiveName);
const manifestPath = path.join(bundleDir, "artifact-manifest.json");
const checksumPath = path.join(bundleDir, "SHA256SUMS");

const sourceFiles = [
  "deploy/sidecar/scripts/hermes/qintopia-context-mcp",
  "deploy/sidecar/scripts/fetch-cos-artifact.sh",
  "deploy/sidecar/scripts/fetch-staging-sidecar-artifact.sh",
  "deploy/sidecar/scripts/render-staging-runtime-env.py",
  "deploy/sidecar/scripts/staging-runtime-prerequisite-observation-smoke.sh",
  "deploy/sidecar/scripts/staging-runtime-readiness-evidence-smoke.sh",
  "deploy/sidecar/scripts/staging-runtime-values-observation-smoke.sh",
  "deploy/sidecar/scripts/huabaosi-image-generation-staging-readiness-smoke.sh",
  "deploy/sidecar/scripts/huabaosi-image-generation-staging-smoke.sh",
  "deploy/sidecar/scripts/huabaosi-image-generation-production-observation-smoke.sh",
  "deploy/sidecar/scripts/huabaosi-image-generation-production-canary-smoke.sh",
  "deploy/sidecar/scripts/activate-huabaosi-image-generation-production.sh",
  "deploy/sidecar/scripts/rollback-huabaosi-image-generation-production.sh",
  "deploy/sidecar/scripts/huabaosi-feishu-artifact-mirror-production-observation-smoke.sh",
  "deploy/sidecar/scripts/activate-huabaosi-feishu-artifact-mirror-production.sh",
  "deploy/sidecar/scripts/rollback-huabaosi-feishu-artifact-mirror-production.sh",
  "deploy/sidecar/scripts/apply-xiaoman-feishu-poster-production-config.py",
  "deploy/sidecar/scripts/apply-xiaoman-conversation-policies-production.py",
  "deploy/sidecar/scripts/xiaoman-shared-db-password-rollover-production.py",
  "deploy/sidecar/scripts/activate-xiaoman-feishu-poster-production.sh",
  "deploy/sidecar/scripts/rollback-xiaoman-feishu-poster-production.sh",
  "deploy/sidecar/scripts/xiaoman-feishu-internal-group-production-observation-smoke.sh",
  "deploy/sidecar/scripts/activate-xiaoman-feishu-internal-group-production.sh",
  "deploy/sidecar/scripts/rollback-xiaoman-feishu-internal-group-production.sh",
  "deploy/sidecar/scripts/huabaosi-wecom-canary-observation-smoke.sh",
  "deploy/sidecar/scripts/huabaosi-wecom-gateway-observation-smoke.sh",
  "deploy/sidecar/scripts/install-coscli.sh",
  "deploy/sidecar/scripts/qiwe-image-send-staging-readiness-smoke.sh",
  "deploy/sidecar/scripts/qiwe-image-send-staging-smoke.sh",
  "deploy/sidecar/scripts/qiwe-image-send-production-observation-smoke.sh",
  "deploy/sidecar/scripts/apply-qiwe-image-send-production-config.py",
  "deploy/sidecar/scripts/qiwe-image-callback-bridge-production-observation-smoke.sh",
  "deploy/sidecar/scripts/activate-qiwe-image-callback-bridge-production.sh",
  "deploy/sidecar/scripts/rollback-qiwe-image-callback-bridge-production.sh",
  "deploy/sidecar/scripts/activate-qiwe-image-send-production.sh",
  "deploy/sidecar/scripts/rollback-qiwe-image-send-production.sh",
  "deploy/sidecar/scripts/erhua-legacy-cron-observation-smoke.sh",
  "deploy/sidecar/scripts/retire-erhua-legacy-cron-production.sh",
  "deploy/sidecar/scripts/operations-downstream-timers-observation-smoke.sh",
  "deploy/sidecar/scripts/operations-group-send-ready-timer-observation-smoke.sh",
  "deploy/sidecar/scripts/erhua-morning-brief-worker.sh",
  "deploy/sidecar/scripts/erhua-morning-brief-timer-observation-smoke.sh",
  "deploy/sidecar/scripts/apply-erhua-morning-brief-production-config.sh",
  "deploy/sidecar/scripts/apply-erhua-member-recognition-production-config.sh",
  "deploy/sidecar/scripts/erhua-member-recognition-production-config-observation-smoke.sh",
  "deploy/sidecar/scripts/erhua-morning-brief-one-shot-production.sh",
  "deploy/sidecar/scripts/activate-erhua-morning-brief-production.sh",
  "deploy/sidecar/scripts/rollback-erhua-morning-brief-production.sh",
  "deploy/sidecar/scripts/apply-erhua-morning-brief-hermes-cron.sh",
  "deploy/sidecar/scripts/apply-xiaoman-activity-read-through-production-config.py",
  "deploy/sidecar/scripts/apply-xiaoman-daily-case-report-production-config.py",
  "deploy/sidecar/scripts/apply-xiaoman-creative-profile-candidates-production.sh",
  "deploy/sidecar/scripts/xiaoman-daily-case-report-auto-publish-worker.sh",
  "deploy/sidecar/scripts/xiaoman-daily-case-report-auto-publish-backfill.sh",
  "deploy/sidecar/scripts/repair-xiaoman-daily-case-report-production-approval.sh",
  "deploy/sidecar/scripts/repair-xiaoman-daily-case-report-read-through-production.sh",
  "deploy/sidecar/scripts/repair-xiaoman-daily-case-report-chat-id-production.sh",
  "deploy/sidecar/scripts/xiaoman-daily-case-report-auto-publish-production-observation-smoke.sh",
  "deploy/sidecar/scripts/activate-xiaoman-daily-case-report-auto-publish-production.sh",
  "deploy/sidecar/scripts/rollback-xiaoman-daily-case-report-auto-publish-production.sh",
  "deploy/sidecar/scripts/apply-xiaoman-daily-case-report-hermes-cron.sh",
  "deploy/sidecar/scripts/production-worker-run-evidence-smoke.sh",
  "deploy/sidecar/scripts/hermes-cron-snapshot-observation-smoke.sh",
  "deploy/sidecar/scripts/hermes-cron-live-parity-observation-smoke.sh",
  "deploy/sidecar/scripts/apply-xiaoman-weekly-recruitment-production-config.sh",
  "deploy/sidecar/scripts/xiaoman-weekly-recruitment-worker.sh",
  "deploy/sidecar/scripts/xiaoman-weekly-recruitment-production-observation-smoke.sh",
  "deploy/sidecar/scripts/activate-xiaoman-weekly-recruitment-production.sh",
  "deploy/sidecar/scripts/rollback-xiaoman-weekly-recruitment-production.sh",
  "deploy/sidecar/scripts/apply-xiaoman-weekly-recruitment-hermes-cron.sh",
  "deploy/sidecar/scripts/apply-xiaoman-weekly-plan-confirmation-production-config.sh",
  "deploy/sidecar/scripts/xiaoman-weekly-plan-confirmation-worker.sh",
  "deploy/sidecar/scripts/xiaoman-weekly-plan-confirmation-production-observation-smoke.sh",
  "deploy/sidecar/scripts/activate-xiaoman-weekly-plan-confirmation-production.sh",
  "deploy/sidecar/scripts/rollback-xiaoman-weekly-plan-confirmation-production.sh",
  "deploy/sidecar/scripts/apply-xiaoman-weekly-plan-confirmation-hermes-cron.sh",
  "deploy/sidecar/scripts/apply-xiaoman-weekly-preview-production-config.sh",
  "deploy/sidecar/scripts/xiaoman-weekly-preview-worker.sh",
  "deploy/sidecar/scripts/xiaoman-weekly-preview-production-observation-smoke.sh",
  "deploy/sidecar/scripts/activate-xiaoman-weekly-preview-production.sh",
  "deploy/sidecar/scripts/rollback-xiaoman-weekly-preview-production.sh",
  "deploy/sidecar/scripts/apply-xiaoman-weekly-preview-hermes-cron.sh",
  "deploy/sidecar/scripts/render-systemd-units.sh",
  "deploy/sidecar/scripts/xiaoman-activity-downstream-observation-smoke.sh",
  "deploy/sidecar/scripts/xiaoman-activity-image-generation-starter-observation-smoke.sh",
  "deploy/sidecar/scripts/xiaoman-activity-production-preflight-smoke.sh",
  "deploy/sidecar/scripts/xiaoman-legacy-cron-observation-smoke.sh",
  "deploy/sidecar/scripts/retire-xiaoman-legacy-cron-production.sh",
  "deploy/sidecar/scripts/sync-hermes-cron-snapshot.sh",
  "deploy/sidecar/scripts/install-hermes-cron-snapshot-timer.sh",
  "deploy/sidecar/scripts/xiaoman-activity-promotion-starter-timer-observation-smoke.sh",
  "deploy/sidecar/scripts/xiaoman-activity-send-request-starter-observation-smoke.sh",
  "deploy/sidecar/scripts/xiaoman-activity-signal-timer-observation-smoke.sh",
  "deploy/sidecar/scripts/xiaoman-profile-bundle-observation-smoke.sh",
  "mcp/qintopia-collab/bin/qintopia-collab-mcp",
  "deploy/sidecar/docs/m9f-legacy-reference-removal.md",
  "deploy/sidecar/docs/systemd-cutover-plan.md",
  "deploy/runner/README.md",
  "agents/erhua/config.template.yaml",
  "runtime/hermes/render_profile_overlay.py",
  "runtime/hermes/migrate_erhua_livecool_env.py",
  "runtime/hermes/profile_transaction.py",
  "runtime/hermes/verify_runtime_provider.py",
  "runtime/hermes/validate_hermes_python.py",
  "runtime/hermes/cron/reviewed-cron-jobs.json",
  "runtime/hermes/cron/xiaoman/weekly-plan-confirmation.job.json",
  "runtime/hermes/cron/erhua/morning-brief.job.json",
  "runtime/hermes/scripts/qintopia-hermes-cron-wrapper.template.sh",
  "runtime/hermes/scripts/qintopia_xiaoman_weekly_plan_confirmation.sh",
  "runtime/hermes/scripts/qintopia_erhua_morning_brief.sh",
  "docs/operations/profile-bundles/erhua-livecool-profile-overlay-runbook.md",
  "deploy/runner/deploy-request.schema.json",
  "deploy/runner/deploy-result.schema.json",
  "deploy/runner/install-release-systemd-units.sh",
  "deploy/runner/manifest.yaml",
  "deploy/runner/qintopia-agent-os-deploy-runner",
  "deploy/runner/activate-erhua-profile.sh",
  "deploy/runner/poll-deploy-requests.sh",
  "deploy/runner/promote-release.sh",
  "deploy/runner/rollback-release.sh",
  "deploy/runner/rollback-erhua-profile.sh",
  "deploy/runner/smoke-release.sh",
  "deploy/runner/upload-deploy-request.sh",
  "deploy/runner/wait-deploy-result.sh",
  "deploy/restart-target-rules.yaml",
  "deploy/runner/qintopia-agent-os-deploy-runner.service",
  "deploy/runner/qintopia-agent-os-deploy-runner.timer",
  "tools/deploy/collect-release-deploy-results.mjs",
  "tools/deploy/build-erhua-member-recognition-canary-evidence.mjs",
  "tools/deploy/build-erhua-member-recognition-canary-mcp-input.mjs",
  "tools/deploy/build-erhua-member-recognition-roster-audit.mjs",
  "tools/deploy/build-erhua-member-safe-alias-payload-template.mjs",
  "tools/deploy/build-erhua-member-safe-identity-payload-template.mjs",
  "tools/deploy/check-erhua-member-recognition-canary.mjs",
  "tools/deploy/check-erhua-member-recognition-completion.mjs",
  "tools/deploy/check-erhua-member-recognition-completion-summary.mjs",
  "tools/deploy/check-erhua-member-recognition-coverage.mjs",
  "tools/deploy/check-erhua-member-recognition-coverage-summary.mjs",
  "tools/deploy/finalize-erhua-member-recognition-coverage.mjs",
  "tools/deploy/finalize-erhua-member-recognition-completion.mjs",
  "tools/deploy/check-erhua-member-safe-alias-payload.mjs",
  "tools/deploy/check-erhua-member-safe-identity-payload.mjs",
  "tools/deploy/check-erhua-room-member-sync.mjs",
  "tools/deploy/resolve-release-deploy-base.mjs",
  "tools/deploy/resolve-release-restart-targets.mjs",
  "tools/deploy/resolve-restart-targets.mjs",
  "docs/operations/m9-server-cutover-runbook.md",
  "docs/operations/erhua-member-recognition-production-runbook.md",
  "docs/operations/message-sidecar-staging-values.template.json",
  "docs/operations/release-acceptance-checklist.md",
  "docs/operations/release-current-model.md",
  "docs/operations/staging-runtime-provisioning-runbook.md",
  "docs/operations/xiaoman-feishu-poster-production-closeout-runbook.md",
  "skills/qintopia-tools/manifest.yaml",
  "skills/qintopia-tools/README.md",
  "skills/qintopia-tools/docs/source-snapshot.md",
  "skills/qintopia-weather/manifest.yaml",
  "skills/qintopia-weather/README.md",
  "skills/qintopia-weather/__init__.py",
  "skills/qintopia-weather/plugin.yaml",
  "skills/qintopia-weather/scripts/qintopia-erhua-weather-broadcast.py",
  "skills/erhua-csv/manifest.yaml",
  "skills/erhua-csv/README.md",
  "skills/erhua-csv/__init__.py",
  "skills/knowledge-retrieval/manifest.yaml",
  "skills/knowledge-retrieval/README.md",
  "skills/knowledge-retrieval/__init__.py",
  "skills/knowledge-retrieval/plugin.yaml",
  "mcp/weather-provider/manifest.yaml",
  "mcp/weather-provider/README.md",
  "skills/qiwe/manifest.yaml",
  "skills/qiwe/README.md",
  "skills/qiwe/__init__.py",
  "skills/qiwe/adapter.py",
  "skills/qiwe/image_callback_bridge.py",
  "skills/qiwe/nats_capture.py",
  "skills/qiwe/passive_pipeline.py",
  "skills/qiwe/plugin.yaml",
  "skills/qiwe/qiwe_events.py",
  "skills/feishu-base/manifest.yaml",
  "skills/feishu-base/README.md",
  "skills/feishu-base/__init__.py",
  "skills/feishu-base/plugin.yaml",
];
const sourceDirs = [
  "agents/xiaoman/profile-bundle",
  "workflows/xiaoman-daily-case-report",
  "workflows/xiaoman-weekly-loop",
  "workflows/xiaoman-weekly-preview",
  "runtime/postgres/migrations",
  "skills/qintopia-tools/variants",
  "skills/qintopia-weather/tests",
  "skills/erhua-csv/tests",
  "skills/knowledge-retrieval/tests",
  "skills/qiwe/docs",
  "skills/qiwe/scripts",
  "skills/qiwe/solitaire",
  "skills/qiwe/tests",
  "skills/feishu-base/docs",
  "skills/feishu-base/tests",
  "workflows/erhua-morning-brief",
  "runtime/hermes/cron",
  "runtime/hermes/scripts",
];
const sourceDirExcludes = [
  /^agents\/xiaoman\/profile-bundle\/tests(\/|$)/,
  /(^|\/)__pycache__(\/|$)/,
  /\.pyc$/,
  /(^|\/)\.DS_Store$/,
  /\.bak/,
];

const run = (command, args, options = {}) =>
  (
    execFileSync(command, args, {
      cwd: repoRoot,
      encoding: "utf8",
      stdio: options.stdio ?? ["ignore", "pipe", "pipe"],
    }) ?? ""
  ).trim();

const sha256File = (filePath) => {
  const hash = crypto.createHash("sha256");
  hash.update(fs.readFileSync(filePath));
  return hash.digest("hex");
};

const gitOutput = (args, fallback = "") => {
  try {
    return run("git", args);
  } catch {
    return fallback;
  }
};

const toolOutput = (command, args, fallback = "") => {
  try {
    return run(command, args);
  } catch {
    return fallback;
  }
};

const copyFile = (relativePath) => {
  const sourcePath = path.join(repoRoot, relativePath);
  if (!fs.existsSync(sourcePath)) {
    throw new Error(`deploy bundle source file not found: ${relativePath}`);
  }

  const targetPath = path.join(payloadDir, relativePath);
  fs.mkdirSync(path.dirname(targetPath), { recursive: true });
  fs.copyFileSync(sourcePath, targetPath);

  const mode = fs.statSync(sourcePath).mode & 0o777;
  fs.chmodSync(targetPath, mode);

  return {
    path: `payload/${relativePath}`,
    source_path: relativePath,
    sha256: sha256File(targetPath),
    size_bytes: fs.statSync(targetPath).size,
    mode: mode.toString(8).padStart(4, "0"),
  };
};

const collectDirectoryFiles = (relativeDir) => {
  const absoluteDir = path.join(repoRoot, relativeDir);
  if (!fs.existsSync(absoluteDir)) {
    throw new Error(`deploy bundle source directory not found: ${relativeDir}`);
  }

  const discovered = [];
  const walk = (dir) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const absolutePath = path.join(dir, entry.name);
      const relativePath = path.relative(repoRoot, absolutePath);
      if (sourceDirExcludes.some((pattern) => pattern.test(relativePath))) {
        continue;
      }
      if (entry.isDirectory()) {
        walk(absolutePath);
      } else if (entry.isFile()) {
        discovered.push(relativePath);
      }
    }
  };
  walk(absoluteDir);
  return discovered.sort();
};

const buildStartedAt = new Date().toISOString();
const commitSha = process.env.GITHUB_SHA || gitOutput(["rev-parse", "HEAD"], "unknown");
const branch =
  process.env.GITHUB_REF_NAME || gitOutput(["branch", "--show-current"], "unknown");

fs.rmSync(bundleDir, { recursive: true, force: true });
fs.mkdirSync(payloadDir, { recursive: true });

const files = [...sourceFiles, ...sourceDirs.flatMap(collectDirectoryFiles)].map(
  copyFile
);

run("tar", ["-C", bundleDir, "-czf", archivePath, "payload"]);
const archiveSha256 = sha256File(archivePath);
files.push({
  path: archiveName,
  sha256: archiveSha256,
  size_bytes: fs.statSync(archivePath).size,
  content: ["payload/"],
  compression: "gzip",
  mode: "0644",
});

const manifest = {
  schema_version: 1,
  artifact_name: bundleName,
  package_name: "qintopia-agent-os-deploy",
  target: "server-operator-files",
  repository: process.env.GITHUB_REPOSITORY || "local",
  commit_sha: commitSha,
  branch,
  run_id: process.env.GITHUB_RUN_ID || null,
  run_attempt: process.env.GITHUB_RUN_ATTEMPT || null,
  build_started_at: buildStartedAt,
  build_finished_at: new Date().toISOString(),
  runner: {
    os: process.env.RUNNER_OS || os.platform(),
    arch: process.env.RUNNER_ARCH || os.arch(),
  },
  toolchain: {
    node: process.version,
    git: toolOutput("git", ["--version"]),
  },
  files,
  validation: {
    required_workflow_jobs: ["check", "deploy-bundle-artifact"],
    paired_runtime_artifact:
      "M9-F must also name an approved sidecar runtime artifact SHA; deploy bundle does not contain the runtime binary.",
    server_verification: [
      "download only from Tencent COS or GitHub Actions artifact for the approved deploy bundle commit SHA",
      "sha256sum -c SHA256SUMS",
      "verify payload wrapper does not reference /home/ubuntu/qintopia-msg-sidecar",
      "render systemd units from payload/render-systemd-units.sh for the approved runtime artifact SHA",
      "use payload/runtime/postgres/migrations as QINTOPIA_SIDECAR_MIGRATIONS_DIR",
      "verify skills/qintopia-tools variants are present before any profile plugin repoint",
      "verify skills/qintopia-weather is present before any qintopia-tools repoint that delegates weather lookup",
      "verify skills/qintopia-weather/scripts/qintopia-erhua-weather-broadcast.py is present before any Erhua 07:00 cron cutover",
      "verify skills/erhua-csv is present before exposing qintopia_erhua_csv_* through the Erhua qintopia-tools plugin",
      "verify skills/knowledge-retrieval is present before any qintopia-tools repoint that delegates Dify or WenYuanGe lookup",
      "verify mcp/weather-provider is present before enabling provider-level weather adapters",
      "verify skills/qiwe is present before any Erhua qiwe-platform plugin repoint",
      "verify skills/feishu-base is present before any Huabaosi qintopia-base-read plugin repoint",
    ],
  },
};

fs.writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
fs.writeFileSync(checksumPath, `${archiveSha256}  ${archiveName}\n`);

console.log(`Built ${bundleName}`);
console.log(`Manifest: ${path.relative(repoRoot, manifestPath)}`);
console.log(`Checksum: ${path.relative(repoRoot, checksumPath)}`);
