#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import YAML from "yaml";

const repoRoot = process.cwd();
const errors = [];

const packages = {
  "deploy/manifests": ["release-manifest.template.yaml", "commit SHA", "artifact SHA"],
  "deploy/rollback": ["rollback", "current", "previous"],
  "deploy/runner": ["deploy request", "release/current", "production environment"],
  "deploy/smoke": ["smoke", "profile", "MCP"],
};

const exists = (relativePath) => fs.existsSync(path.join(repoRoot, relativePath));
const readText = (relativePath) =>
  fs.readFileSync(path.join(repoRoot, relativePath), "utf8");
const isExecutable = (relativePath) =>
  (fs.statSync(path.join(repoRoot, relativePath)).mode & 0o111) !== 0;
const addError = (message) => errors.push(message);
const requireFragment = (relativePath, text, fragment) => {
  if (!text.includes(fragment)) {
    addError(`${relativePath}: must include ${fragment}`);
  }
};
const forbidFragment = (relativePath, text, fragment) => {
  if (text.includes(fragment)) {
    addError(`${relativePath}: must not include ${fragment}`);
  }
};
const requireExecutable = (relativePath) => {
  if (!exists(relativePath)) {
    addError(`${relativePath}: missing executable file`);
  } else if (!isExecutable(relativePath)) {
    addError(`${relativePath}: must be executable`);
  }
};

const sidecarAgentsPath = "runtime/sidecar/AGENTS.md";
if (!exists(sidecarAgentsPath)) {
  addError(`${sidecarAgentsPath}: missing sidecar agent rules`);
} else {
  const sidecarAgents = readText(sidecarAgentsPath);
  for (const fragment of [
    "Production sidecar artifacts compile exactly `huabaosi-production-adapter`, the",
    "guarded `huabaosi-feishu-mirror-adapter`",
    "`xiaoman-feishu-poster-adapter`",
    "QiWe live features, staging adapters,",
    "staging/production builds, and all-features production artifacts remain forbidden",
    "QiWe",
    "apply code must still fail before Postgres",
    "or network access unless",
    "Feishu delivery config",
    "QiWe live features must not be",
    "the explicit owner activation scripts may enable external timers",
    "The QiWe upload worker and callback processor may compile live helpers only through",
    "`qiwe-staging-adapter` or `qiwe-production-adapter`",
  ]) {
    requireFragment(sidecarAgentsPath, sidecarAgents, fragment);
  }
  for (const fragment of [
    "QiWe live adapters, staging adapters, and",
    "callback processor may compile live helpers for staging",
    "Default and production builds must fail apply before",
  ]) {
    forbidFragment(sidecarAgentsPath, sidecarAgents, fragment);
  }
}

const rootAgentsPath = "AGENTS.md";
if (!exists(rootAgentsPath)) {
  addError(`${rootAgentsPath}: missing repository agent rules`);
} else {
  const rootAgents = readText(rootAgentsPath);
  for (const fragment of [
    "`node tools/deploy/check-xiaoman-production-evidence-chain-local.mjs`",
    "Production evidence runbook: `docs/operations/xiaoman-production-evidence-runbook.md`",
    "Production release requests use `runtime_artifact_profile=huabaosi-production`",
    "install `qiwe-production` as a companion",
    "derive that path and SHA-256 from the companion manifest and `SHA256SUMS`",
    "A same-SHA request for an existing release must reuse the immutable",
    "manifest's exact runtime, runtime artifact profile, bundle, commit, scope, and",
    "restart-target fields",
    "apply-xiaoman-activity-read-through-production-config.py",
    "sourcing the Xiaoman Hermes profile",
  ]) {
    requireFragment(rootAgentsPath, rootAgents, fragment);
  }
}

const qiweImageSendPlanPath = "docs/plans/active/xiaoman-qiwe-image-send.md";
if (!exists(qiweImageSendPlanPath)) {
  addError(`${qiweImageSendPlanPath}: missing Xiaoman QiWe image send plan`);
} else {
  const plan = readText(qiweImageSendPlanPath);
  for (const fragment of [
    "Huabaosi production release artifacts record exactly",
    "`huabaosi-production-adapter`, `huabaosi-feishu-mirror-adapter`, and the",
    "default-disabled `xiaoman-feishu-poster-adapter`",
    "must reject",
    "QiWe live features, staging approval, staging databases",
    "the production gate and never falls back to staging approval",
  ]) {
    requireFragment(qiweImageSendPlanPath, plan, fragment);
  }
  for (const fragment of [
    "default and production binaries fail apply before Postgres or network access",
  ]) {
    forbidFragment(qiweImageSendPlanPath, plan, fragment);
  }
}

const qiweImageSendAdapterWorkerPlanPath =
  "docs/plans/active/qiwe-image-send-adapter-worker.md";
if (!exists(qiweImageSendAdapterWorkerPlanPath)) {
  addError(
    `${qiweImageSendAdapterWorkerPlanPath}: missing QiWe image send adapter worker plan`
  );
} else {
  const plan = readText(qiweImageSendAdapterWorkerPlanPath);
  for (const fragment of [
    "`cargo_features: [huabaosi-production-adapter, huabaosi-feishu-mirror-adapter, xiaoman-feishu-poster-adapter]`",
    "exclude QiWe live",
    "Production apply must use the production owner phrase",
    "back to staging gates",
  ]) {
    requireFragment(qiweImageSendAdapterWorkerPlanPath, plan, fragment);
  }
  for (const fragment of [
    "compile a QiWe live adapter into the production artifact",
    "production units absent",
  ]) {
    forbidFragment(qiweImageSendAdapterWorkerPlanPath, plan, fragment);
  }
}

const xiaomanFeishuQiweBoundaryPath =
  "docs/plans/active/xiaoman-feishu-qiwe-delivery-boundary.md";
if (!exists(xiaomanFeishuQiweBoundaryPath)) {
  addError(
    `${xiaomanFeishuQiweBoundaryPath}: missing Xiaoman Feishu-to-QiWe delivery boundary`
  );
} else {
  const plan = readText(xiaomanFeishuQiweBoundaryPath);
  for (const fragment of [
    "Staging requires",
    "`huabaosi-staging-adapter` plus `qiwe-staging-adapter`",
    "artifacts must contain only",
    "must not bundle `qiwe-production-adapter`",
    "QiWe-only builds continue to reject this route",
  ]) {
    requireFragment(xiaomanFeishuQiweBoundaryPath, plan, fragment);
  }
  for (const fragment of [
    "production builds continue to reject this route",
    "Default, production, Huabaosi-only, and QiWe-only builds must continue to",
    "reviewed QiWe staging live adapter may claim",
  ]) {
    forbidFragment(xiaomanFeishuQiweBoundaryPath, plan, fragment);
  }
}

const currentRoadmapPath = "docs/plans/active/current-roadmap.md";
if (!exists(currentRoadmapPath)) {
  addError(`${currentRoadmapPath}: missing current roadmap`);
} else {
  const roadmap = readText(currentRoadmapPath);
  for (const fragment of [
    "Only the matched Huabaosi/QiWe staging live feature pair may claim this storage",
    "Huabaosi production artifacts must contain only `huabaosi-production-adapter`, the",
    "`xiaoman-feishu-poster-adapter`",
    "must not bundle",
    "`qiwe-production-adapter`",
    "Single-feature builds still",
    "fail closed",
    "tools/deploy/finalize-xiaoman-production-completion-evidence.mjs",
    "last retained-evidence step after all sanitized files exist",
  ]) {
    requireFragment(currentRoadmapPath, roadmap, fragment);
  }
  for (const fragment of [
    "Only the combined Huabaosi/QiWe staging feature artifact may claim this storage",
    "production and single-feature builds still fail closed",
  ]) {
    forbidFragment(currentRoadmapPath, roadmap, fragment);
  }
}

const sidecarDeployPath = "deploy/sidecar/scripts/server-deploy.sh";
if (!exists(sidecarDeployPath)) {
  addError(`${sidecarDeployPath}: missing sidecar deploy script`);
} else {
  const sidecarDeploy = readText(sidecarDeployPath);
  for (const fragment of [
    'sudo chown root:ubuntu "$ENV_FILE"',
    'sudo chmod 0640 "$ENV_FILE"',
  ]) {
    requireFragment(sidecarDeployPath, sidecarDeploy, fragment);
  }
  forbidFragment(
    sidecarDeployPath,
    sidecarDeploy,
    'sudo chown ubuntu:ubuntu "$ENV_FILE"'
  );
}

for (const artifactFetchPath of [
  "deploy/sidecar/scripts/fetch-ci-artifact.sh",
  "deploy/sidecar/scripts/fetch-cos-artifact.sh",
]) {
  if (!exists(artifactFetchPath)) {
    addError(`${artifactFetchPath}: missing production sidecar artifact fetcher`);
  } else {
    const fetcher = readText(artifactFetchPath);
    requireFragment(artifactFetchPath, fetcher, "huabaosi-production-adapter");
    requireFragment(artifactFetchPath, fetcher, "huabaosi-feishu-mirror-adapter");
    requireFragment(artifactFetchPath, fetcher, "xiaoman-feishu-poster-adapter");
    if (
      fetcher.includes(
        '["huabaosi-production-adapter","huabaosi-feishu-mirror-adapter","xiaoman-feishu-poster-adapter","qiwe-production-adapter"]'
      )
    ) {
      addError(
        `${artifactFetchPath}: Huabaosi production artifact validation must not accept qiwe-production-adapter`
      );
    }
  }
}

const stagingArtifactProvisionPath =
  "deploy/sidecar/scripts/fetch-staging-sidecar-artifact.sh";
if (!exists(stagingArtifactProvisionPath)) {
  addError(`${stagingArtifactProvisionPath}: missing staging artifact provisioner`);
} else {
  const provisioner = readText(stagingArtifactProvisionPath);
  for (const fragment of [
    "QINTOPIA_STAGING_SIDECAR_PROVISION_APPROVAL",
    "QINTOPIA_STAGING_SIDECAR_PROVISION_SOURCE",
    "--source <cos|github>",
    "QINTOPIA_SIDECAR_ARTIFACT_PROFILE=combined-staging",
    "fetch-cos-artifact.sh",
    "approved-staging-sidecar-provision",
    'repo="qintopia-agent-studio/qintopia-agent-os"',
    'workflow="artifacts.yml"',
    "GITHUB_REPOSITORY override is not allowed",
    "GITHUB_WORKFLOW override is not allowed",
    "validate_timeout_seconds",
    "GITHUB_API_MAX_TIME",
    "GITHUB_DOWNLOAD_MAX_TIME",
    "signed_download_url",
    '--write-out "%{redirect_url}"',
    "GitHub artifact download did not return a signed redirect URL",
    "validate_artifact_zip",
    "artifact zip entry must stay under artifact root",
    "artifact zip entry is not allowlisted",
    "artifact zip entries must exactly match the staging allowlist",
    "qintopia-message-sidecar-staging-linux-x86_64-gnu",
    "huabaosi-image-generation-staging-smoke.sh",
    "qiwe-image-send-staging-smoke.sh",
    "huabaosi-staging-adapter",
    "qiwe-staging-adapter",
    "staging_only",
    "production_eligible",
    "/home/ubuntu/qintopia-agent-os-staging-releases",
    "--artifact-zip is test-only",
    "sha256sum -c SHA256SUMS",
    "qintopia-message-sidecar.tar.gz",
    "os.lstat(path)",
    "stat.S_ISLNK",
    "artifact entry must not be a symlink",
    "artifact entry must not be hardlinked",
    "SHA256SUMS entries must exactly match the staging allowlist",
    "path component is a symlink",
    "path component is group/world writable",
    "path component has unexpected owner",
    'mkdir -m 0755 "$release_root"',
    'mkdir -m 0755 "$release_dir"',
    'mkdir -m 0755 "$sidecar_dir"',
    'rm -rf "$release_dir"',
    'rmdir "$release_root"',
    "sidecar_dir_created=1",
    "release_dir_created=1",
    "release_root_created=1",
    "provision_complete=1",
    "chmod 0555",
  ]) {
    requireFragment(stagingArtifactProvisionPath, provisioner, fragment);
  }
  for (const fragment of [
    'repo="${GITHUB_REPOSITORY',
    'workflow="${GITHUB_WORKFLOW',
    "huabaosi-production-adapter",
    "huabaosi-feishu-mirror-adapter",
    "qiwe-production-adapter",
    "systemctl enable",
    "systemctl start",
    "gh release",
  ]) {
    forbidFragment(stagingArtifactProvisionPath, provisioner, fragment);
  }
}

const deployBundleBuilderPath = "tools/deploy/build-deploy-bundle.mjs";
if (!exists(deployBundleBuilderPath)) {
  addError(`${deployBundleBuilderPath}: missing deploy bundle builder`);
} else {
  const builder = readText(deployBundleBuilderPath);
  for (const fragment of [
    "deploy/sidecar/scripts/fetch-staging-sidecar-artifact.sh",
    "deploy/sidecar/scripts/render-staging-runtime-env.py",
    "deploy/sidecar/scripts/staging-runtime-prerequisite-observation-smoke.sh",
    "deploy/sidecar/scripts/staging-runtime-readiness-evidence-smoke.sh",
    "deploy/sidecar/scripts/staging-runtime-values-observation-smoke.sh",
    "deploy/sidecar/scripts/huabaosi-image-generation-production-canary-smoke.sh",
    "deploy/sidecar/scripts/qiwe-image-send-staging-smoke.sh",
    "deploy/sidecar/scripts/qiwe-image-send-production-observation-smoke.sh",
    "deploy/sidecar/scripts/apply-qiwe-image-send-production-config.py",
    "deploy/sidecar/scripts/qiwe-image-callback-bridge-production-observation-smoke.sh",
    "deploy/sidecar/scripts/activate-qiwe-image-callback-bridge-production.sh",
    "deploy/sidecar/scripts/rollback-qiwe-image-callback-bridge-production.sh",
    "deploy/sidecar/scripts/erhua-legacy-cron-observation-smoke.sh",
    "deploy/sidecar/scripts/retire-erhua-legacy-cron-production.sh",
    "deploy/sidecar/scripts/retire-xiaoman-legacy-cron-production.sh",
    "deploy/sidecar/scripts/erhua-morning-brief-timer-observation-smoke.sh",
    "deploy/sidecar/scripts/erhua-morning-brief-worker.sh",
    "deploy/sidecar/scripts/production-worker-run-evidence-smoke.sh",
    "deploy/sidecar/scripts/erhua-morning-brief-one-shot-production.sh",
    "deploy/sidecar/scripts/activate-erhua-morning-brief-production.sh",
    "deploy/sidecar/scripts/rollback-erhua-morning-brief-production.sh",
    "deploy/sidecar/scripts/apply-erhua-member-recognition-production-config.sh",
    "deploy/sidecar/scripts/erhua-member-recognition-production-config-observation-smoke.sh",
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
    "docs/operations/erhua-member-recognition-production-runbook.md",
    "deploy/sidecar/scripts/apply-xiaoman-activity-read-through-production-config.py",
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
    "deploy/sidecar/scripts/apply-xiaoman-weekly-preview-hermes-cron.sh",
    "docs/operations/message-sidecar-staging-values.template.json",
    "docs/operations/release-acceptance-checklist.md",
    "docs/operations/staging-runtime-provisioning-runbook.md",
    "skills/qintopia-weather/scripts/qintopia-erhua-weather-broadcast.py",
    "workflows/erhua-morning-brief",
    "workflows/xiaoman-weekly-loop",
    "runtime/hermes/validate_hermes_python.py",
    "runtime/hermes/cron",
    "runtime/hermes/scripts",
  ]) {
    requireFragment(deployBundleBuilderPath, builder, fragment);
  }
}

const deployRunnerCheckPath = "tools/deploy/check-deploy-runner.mjs";
if (!exists(deployRunnerCheckPath)) {
  addError(`${deployRunnerCheckPath}: missing deploy runner check`);
} else {
  requireFragment(
    deployRunnerCheckPath,
    readText(deployRunnerCheckPath),
    "tools/deploy/test-huabaosi-image-production-canary.mjs"
  );
  requireFragment(
    deployRunnerCheckPath,
    readText(deployRunnerCheckPath),
    "tools/deploy/test-huabaosi-image-production-canary-evidence.mjs"
  );
}

const stagingValuesTemplatePath =
  "docs/operations/message-sidecar-staging-values.template.json";
if (!exists(stagingValuesTemplatePath)) {
  addError(`${stagingValuesTemplatePath}: missing staging values template`);
} else {
  const template = readText(stagingValuesTemplatePath);
  for (const fragment of [
    "<staging-postgres-url-with-database-name-containing-staging>",
    "<staging-image-provider-api-key>",
    "<huabaosi-generated-image-base-token>",
    "<huabaosi-generated-image-v1-table-id>",
    "QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS",
    "<owner-reviewed-generated-image-and-qiwe-temporary-storage-hosts>",
    "<one-isolated-staging-group-id>",
  ]) {
    requireFragment(stagingValuesTemplatePath, template, fragment);
  }
  for (const fragment of [
    "postgres://",
    "postgresql://",
    "tenant_access_token",
    "xoxb-",
    "Bearer ",
  ]) {
    forbidFragment(stagingValuesTemplatePath, template, fragment);
  }
}

const stagingRuntimeRunbookPath =
  "docs/operations/staging-runtime-provisioning-runbook.md";
if (!exists(stagingRuntimeRunbookPath)) {
  addError(
    `${stagingRuntimeRunbookPath}: missing staging runtime provisioning runbook`
  );
} else {
  const runbook = readText(stagingRuntimeRunbookPath);
  for (const fragment of [
    "message-sidecar-staging-values.template.json",
    "/etc/qintopia/message-sidecar-staging-values.json",
    "/etc/qintopia/message-sidecar-staging.env",
    "server-local values file out of git",
    "approved-staging-runtime-env-provision",
    "approved-staging-sidecar-provision",
    "ready_for_huabaosi_qiwe_staging_smokes",
    "applied as-is",
    "ports outside `1..65535`",
  ]) {
    requireFragment(stagingRuntimeRunbookPath, runbook, fragment);
  }
  for (const fragment of [
    "systemctl enable --now",
    "gh release create",
    "QINTOPIA_SIDECAR_DATABASE_URL=",
    "QIWE_TOKEN=",
    "tenant_access_token",
  ]) {
    forbidFragment(stagingRuntimeRunbookPath, runbook, fragment);
  }
}

const releaseAcceptanceChecklistPath =
  "docs/operations/release-acceptance-checklist.md";
if (!exists(releaseAcceptanceChecklistPath)) {
  addError(`${releaseAcceptanceChecklistPath}: missing release acceptance checklist`);
} else {
  const checklist = readText(releaseAcceptanceChecklistPath);
  for (const fragment of [
    "Release Please validation",
    "exact current PR head",
    "force-updates the branch",
    "draft Release tag points to current `origin/master`",
    "/home/ubuntu/qintopia-agent-os-releases/current",
    "tools/deploy/build-deploy-bundle.mjs",
    "tools/deploy/check-deploy-contracts.mjs",
    "deploy/sidecar/scripts/fetch-staging-sidecar-artifact.sh",
    "deploy/sidecar/scripts/render-staging-runtime-env.py",
    "deploy/sidecar/scripts/staging-runtime-prerequisite-observation-smoke.sh",
    "deploy/sidecar/scripts/staging-runtime-readiness-evidence-smoke.sh",
    "deploy/sidecar/scripts/staging-runtime-values-observation-smoke.sh",
    "docs/operations/message-sidecar-staging-values.template.json",
    "docs/operations/staging-runtime-provisioning-runbook.md",
    "staging artifact fetch helper",
    "Do not create placeholder env files",
    "/etc/qintopia/message-sidecar-staging-values.json",
    "/etc/qintopia/message-sidecar-staging.env",
    "ready_for_huabaosi_qiwe_staging_smokes",
    "Xiaoman Completion Boundary",
    "infrastructure",
    "activation-ready",
    "production-complete",
    "Huabaosi canary evidence uses the ordinary Huabaosi production artifact",
    "QiWe companion in the same release",
    "Huabaosi canary sidecar SHA-256 separately from the QiWe real-activity sidecar SHA-256",
    "node tools/deploy/check-xiaoman-production-evidence-chain-local.mjs",
    "prefer the reviewed one-shot completion helper",
    "pnpm deploy:xiaoman-production-evidence:finalize --",
    "--staging-runtime-readiness <staging-runtime-readiness-output.txt>",
    "--qiwe-group-arrival-confirmation <qiwe-group-arrival-confirmation-output.txt>",
  ]) {
    requireFragment(releaseAcceptanceChecklistPath, checklist, fragment);
  }
  for (const fragment of [
    "QINTOPIA_SIDECAR_DATABASE_URL=",
    "QIWE_TOKEN=",
    "tenant_access_token",
    "systemctl enable --now",
    "gh release create",
  ]) {
    forbidFragment(releaseAcceptanceChecklistPath, checklist, fragment);
  }
}

const docsReadmePath = "docs/README.md";
if (!exists(docsReadmePath)) {
  addError(`${docsReadmePath}: missing documentation hub`);
} else {
  const readme = readText(docsReadmePath);
  for (const fragment of [
    "Xiaoman production evidence runbook:",
    "[operations/xiaoman-production-evidence-runbook.md](operations/xiaoman-production-evidence-runbook.md)",
  ]) {
    requireFragment(docsReadmePath, readme, fragment);
  }
}

const docsReadmeZhCnPath = "docs/README.zh-CN.md";
if (!exists(docsReadmeZhCnPath)) {
  addError(`${docsReadmeZhCnPath}: missing Chinese documentation hub`);
} else {
  const readme = readText(docsReadmeZhCnPath);
  for (const fragment of [
    "小满生产证据 runbook：",
    "[operations/xiaoman-production-evidence-runbook.md](operations/xiaoman-production-evidence-runbook.md)",
  ]) {
    requireFragment(docsReadmeZhCnPath, readme, fragment);
  }
}

const packageJsonPath = "package.json";
if (!exists(packageJsonPath)) {
  addError(`${packageJsonPath}: missing package manifest`);
} else {
  const packageJson = JSON.parse(readText(packageJsonPath));
  const scripts = packageJson?.scripts ?? {};
  if (
    scripts["deploy:xiaoman-production-evidence:local-check"] !==
    "node tools/deploy/check-xiaoman-production-evidence-chain-local.mjs"
  ) {
    addError(
      "package.json: deploy:xiaoman-production-evidence:local-check must run node tools/deploy/check-xiaoman-production-evidence-chain-local.mjs"
    );
  }
  if (
    scripts["deploy:erhua-member-recognition:local-check"] !==
    "node tools/deploy/check-erhua-member-recognition-local.mjs"
  ) {
    addError(
      "package.json: deploy:erhua-member-recognition:local-check must run node tools/deploy/check-erhua-member-recognition-local.mjs"
    );
  }
  if (
    scripts["deploy:erhua-member-recognition:coverage-finalize"] !==
    "node tools/deploy/finalize-erhua-member-recognition-coverage.mjs"
  ) {
    addError(
      "package.json: deploy:erhua-member-recognition:coverage-finalize must run node tools/deploy/finalize-erhua-member-recognition-coverage.mjs"
    );
  }
  if (
    scripts["deploy:erhua-member-recognition:finalize"] !==
    "node tools/deploy/finalize-erhua-member-recognition-completion.mjs"
  ) {
    addError(
      "package.json: deploy:erhua-member-recognition:finalize must run node tools/deploy/finalize-erhua-member-recognition-completion.mjs"
    );
  }
  if (
    scripts["deploy:xiaoman-production-evidence:finalize"] !==
    "node tools/deploy/finalize-xiaoman-production-completion-evidence.mjs"
  ) {
    addError(
      "package.json: deploy:xiaoman-production-evidence:finalize must run node tools/deploy/finalize-xiaoman-production-completion-evidence.mjs"
    );
  }
  if (
    !scripts["deploy:contracts:check"]?.includes(
      "node tools/deploy/test-erhua-member-recognition-roster-audit.mjs"
    )
  ) {
    addError(
      "package.json: deploy:contracts:check must run node tools/deploy/test-erhua-member-recognition-roster-audit.mjs"
    );
  }
}

const erhuaMemberRecognitionLocalCheckPath =
  "tools/deploy/check-erhua-member-recognition-local.mjs";
if (!exists(erhuaMemberRecognitionLocalCheckPath)) {
  addError(
    `${erhuaMemberRecognitionLocalCheckPath}: missing Erhua member recognition local check`
  );
} else {
  const script = readText(erhuaMemberRecognitionLocalCheckPath);
  for (const fragment of [
    '["cargo", ["check", "--manifest-path", "runtime/sidecar/Cargo.toml"]]',
    "identity_alias",
    "identity_bootstrap",
    "context_tools",
    "member_profile",
    "RUST_MIN_STACK",
    '["node", ["tools/deploy/test-erhua-room-member-sync.mjs"]]',
    '["node", ["tools/deploy/test-erhua-member-recognition-production-config.mjs"]]',
    "tools/deploy/test-erhua-member-recognition-production-config-observation.mjs",
    '["node", ["tools/deploy/test-erhua-member-recognition-coverage.mjs"]]',
    '["node", ["tools/deploy/test-erhua-member-recognition-coverage-summary.mjs"]]',
    '["node", ["tools/deploy/test-finalize-erhua-member-recognition-coverage.mjs"]]',
    '["node", ["tools/deploy/test-erhua-member-safe-alias-payload.mjs"]]',
    '["node", ["tools/deploy/test-erhua-member-safe-alias-payload-template.mjs"]]',
    '["node", ["tools/deploy/test-erhua-member-safe-identity-payload.mjs"]]',
    '["node", ["tools/deploy/test-erhua-member-safe-identity-payload-template.mjs"]]',
    '["node", ["tools/deploy/test-erhua-member-recognition-canary.mjs"]]',
    '["node", ["tools/deploy/test-erhua-member-recognition-canary-builder.mjs"]]',
    '["node", ["tools/deploy/test-erhua-member-recognition-canary-mcp-input.mjs"]]',
    '["node", ["tools/deploy/test-erhua-member-recognition-roster-audit.mjs"]]',
    '["node", ["tools/deploy/test-erhua-member-recognition-completion.mjs"]]',
    '["node", ["tools/deploy/test-erhua-member-recognition-completion-summary.mjs"]]',
    '["node", ["tools/deploy/test-finalize-erhua-member-recognition-completion.mjs"]]',
    '["node", ["tools/deploy/check-deploy-contracts.mjs"]]',
    '["node", ["tools/deploy/build-deploy-bundle.mjs"]]',
    "payload/tools/deploy/check-erhua-member-recognition-completion.mjs",
    "payload/tools/deploy/check-erhua-member-recognition-completion-summary.mjs",
    "payload/tools/deploy/check-erhua-member-recognition-coverage-summary.mjs",
    "payload/tools/deploy/build-erhua-member-recognition-roster-audit.mjs",
    "payload/tools/deploy/finalize-erhua-member-recognition-coverage.mjs",
    "payload/tools/deploy/finalize-erhua-member-recognition-completion.mjs",
    "payload/tools/deploy/build-erhua-member-safe-identity-payload-template.mjs",
    "payload/tools/deploy/check-erhua-member-safe-identity-payload.mjs",
    "payload/tools/deploy/check-erhua-room-member-sync.mjs",
    "payload/deploy/sidecar/scripts/apply-erhua-member-recognition-production-config.sh",
    "payload/deploy/sidecar/scripts/erhua-member-recognition-production-config-observation-smoke.sh",
    "payload/docs/operations/erhua-member-recognition-production-runbook.md",
    "Erhua member recognition local check passed",
  ]) {
    requireFragment(erhuaMemberRecognitionLocalCheckPath, script, fragment);
  }
}

const erhuaIdentityBackfillPath = "runtime/sidecar/src/identity_backfill.rs";
if (!exists(erhuaIdentityBackfillPath)) {
  addError(
    `${erhuaIdentityBackfillPath}: missing Erhua identity backfill implementation`
  );
} else {
  const source = readText(erhuaIdentityBackfillPath);
  for (const fragment of [
    "stale_room_member_identities_marked",
    '"current_qiwe_room_member": true',
    "'current_qiwe_room_member', false",
    "room member sync returned no members; refusing to mark roster",
    "mark stale room member identities",
  ]) {
    requireFragment(erhuaIdentityBackfillPath, source, fragment);
  }
}

const erhuaMemberRecognitionConfigPath =
  "deploy/sidecar/scripts/apply-erhua-member-recognition-production-config.sh";
if (!exists(erhuaMemberRecognitionConfigPath)) {
  addError(
    `${erhuaMemberRecognitionConfigPath}: missing Erhua member recognition production config script`
  );
} else {
  const script = readText(erhuaMemberRecognitionConfigPath);
  for (const fragment of [
    "approved-production-erhua-member-recognition-config",
    'ENV_FILE="/etc/qintopia/message-sidecar.env"',
    "QINTOPIA_ERHUA_MEMBER_RECOGNITION_CONFIG_CHAT_ID",
    "QINTOPIA_ERHUA_MEMBER_RECOGNITION_CONFIG_CANARY_SENDER_ID",
    "QINTOPIA_PROFILE_TARGET_CHAT_IDS",
    "QINTOPIA_ERHUA_MEMBER_RECOGNITION_CANARY_CHAT_ID",
    "QINTOPIA_ERHUA_MEMBER_RECOGNITION_CANARY_SENDER_ID",
    "requires exactly one QINTOPIA_SIDECAR_DATABASE_URL",
    "regular non-symlink file",
    "must not have hard links",
    "must not be group/world writable",
    "must differ from the reviewed group id",
    "chat_id=reviewed, canary_sender_id=reviewed",
  ]) {
    requireFragment(erhuaMemberRecognitionConfigPath, script, fragment);
  }
  for (const forbidden of ["QINTOPIA_SIDECAR_ENV_FILE", "SYSTEMCTL"]) {
    if (script.includes(forbidden)) {
      addError(
        `${erhuaMemberRecognitionConfigPath}: must not accept caller override ${forbidden}`
      );
    }
  }
}

const erhuaMemberRecognitionConfigTestPath =
  "tools/deploy/test-erhua-member-recognition-production-config.mjs";
if (!exists(erhuaMemberRecognitionConfigTestPath)) {
  addError(
    `${erhuaMemberRecognitionConfigTestPath}: missing Erhua member recognition production config test`
  );
} else {
  const test = readText(erhuaMemberRecognitionConfigTestPath);
  for (const fragment of [
    "explicit owner approval",
    "CONFIG_CANARY_SENDER_ID is required",
    "must differ from the reviewed group id",
    "exactly one reviewed group",
    "regular non-symlink file",
    "config apply output leaked reviewed ids",
    "explicit chat id was not applied",
  ]) {
    requireFragment(erhuaMemberRecognitionConfigTestPath, test, fragment);
  }
}

const erhuaMemberRecognitionConfigObservationPath =
  "deploy/sidecar/scripts/erhua-member-recognition-production-config-observation-smoke.sh";
if (!exists(erhuaMemberRecognitionConfigObservationPath)) {
  addError(
    `${erhuaMemberRecognitionConfigObservationPath}: missing Erhua member recognition production config observation`
  );
} else {
  const script = readText(erhuaMemberRecognitionConfigObservationPath);
  for (const fragment of [
    "QINTOPIA_ERHUA_MEMBER_RECOGNITION_CONFIG_OBSERVATION_ENABLE",
    'DEFAULT_ENV_FILE="/etc/qintopia/message-sidecar.env"',
    "ready_for_member_recognition_runbook",
    "profile_target_matches_canary_chat",
    "canary_sender_differs_from_chat",
    "qintopia-erhua-member-recognition-scope-v1",
    "does not print group id or sender id",
    "does not call QiWe, Postgres, MCP, systemctl, or network",
  ]) {
    requireFragment(erhuaMemberRecognitionConfigObservationPath, script, fragment);
  }
}

const erhuaMemberRecognitionConfigObservationTestPath =
  "tools/deploy/test-erhua-member-recognition-production-config-observation.mjs";
if (!exists(erhuaMemberRecognitionConfigObservationTestPath)) {
  addError(
    `${erhuaMemberRecognitionConfigObservationTestPath}: missing Erhua member recognition production config observation test`
  );
} else {
  const test = readText(erhuaMemberRecognitionConfigObservationTestPath);
  for (const fragment of [
    "ready_for_member_recognition_runbook",
    "profile_target_canary_chat_mismatch",
    "canary_sender_equals_chat",
    "env_file_not_regular",
    "observation skipped",
    "assertNoSecretOutput",
  ]) {
    requireFragment(erhuaMemberRecognitionConfigObservationTestPath, test, fragment);
  }
}

const erhuaIdentityAliasPath = "runtime/sidecar/src/identity_alias.rs";
if (!exists(erhuaIdentityAliasPath)) {
  addError(`${erhuaIdentityAliasPath}: missing Erhua safe alias implementation`);
} else {
  const source = readText(erhuaIdentityAliasPath);
  for (const fragment of [
    "approved-production-erhua-member-safe-identity",
    "current_qiwe_room_member",
    "normalize_alias_key",
    "duplicate safe_display_name",
    "a.alias_type = $3",
    "reviewed_safe_name",
    "erhua_member_recognition_safe_identity",
    "materialize_safe_identity_platform_identity",
    "backfill_safe_identity_messages",
  ]) {
    requireFragment(erhuaIdentityAliasPath, source, fragment);
  }
}

const erhuaIdentityBootstrapPath = "runtime/sidecar/src/identity_bootstrap.rs";
if (!exists(erhuaIdentityBootstrapPath)) {
  addError(
    `${erhuaIdentityBootstrapPath}: missing Erhua identity bootstrap implementation`
  );
} else {
  const source = readText(erhuaIdentityBootstrapPath);
  for (const fragment of [
    "ci.metadata->>'current_qiwe_room_member' = 'true'",
    "metadata->>'current_qiwe_room_member' = 'true'",
    "qiwe_room_potential_member_identities_unlinked",
    "load_unlinked_potential_member_identity_samples",
    "potential_member_identity_unlinked",
  ]) {
    requireFragment(erhuaIdentityBootstrapPath, source, fragment);
  }
}

const erhuaContextToolsPath = "runtime/sidecar/src/context_tools.rs";
if (!exists(erhuaContextToolsPath)) {
  addError(`${erhuaContextToolsPath}: missing Erhua context tools implementation`);
} else {
  const source = readText(erhuaContextToolsPath);
  for (const fragment of [
    "ci.metadata->>'current_qiwe_room_member' = 'true'",
    "metadata->>'current_qiwe_room_member' = 'true'",
    "channel_identity_candidate_is_current",
    "AND ci.id IS NOT NULL",
    "chat_id.is_empty()",
    "member_name_resolution_does_not_platform_fallback_for_current_chat_scope",
    "answer_context_identity_ignores_stale_qiwe_room_member_exact_chat",
    "member_safe_context_exact_scope_ignores_stale_qiwe_room_member",
  ]) {
    requireFragment(erhuaContextToolsPath, source, fragment);
  }
}

const erhuaCoverageCheckPath =
  "tools/deploy/check-erhua-member-recognition-coverage.mjs";
if (!exists(erhuaCoverageCheckPath)) {
  addError(
    `${erhuaCoverageCheckPath}: missing Erhua member recognition coverage checker`
  );
} else {
  const script = readText(erhuaCoverageCheckPath);
  for (const fragment of [
    "answer_context_canary_specs",
    "answer_context_speaker_canary_specs",
    "uniqueCanonicalKeyCount",
    "qiwe_room_potential_member_identities_unlinked",
    "unsafe-display potential member identities",
    "required_profile_terms must be an array when present",
    "--require-active-profiles",
    "full-profile coverage requires active profiles",
    "--summary-output",
    "erhua_member_recognition_coverage_v1",
    "retained_evidence_boundary",
  ]) {
    requireFragment(erhuaCoverageCheckPath, script, fragment);
  }
}

const erhuaCoverageTestPath = "tools/deploy/test-erhua-member-recognition-coverage.mjs";
if (!exists(erhuaCoverageTestPath)) {
  addError(
    `${erhuaCoverageTestPath}: missing Erhua member recognition coverage checker tests`
  );
} else {
  const test = readText(erhuaCoverageTestPath);
  for (const fragment of [
    "current_qiwe_room_member",
    "unsafe-potential-member-unlinked",
    "missing-canary-spec-array",
    "canary-spec-length-mismatch",
    "speaker-canary-people-mismatch",
    "requireActiveProfiles",
    "full-profile coverage requires active profiles",
    "valid-summary.json",
    "strict-summary.json",
    "retained_evidence_boundary.includes_person_id",
  ]) {
    requireFragment(erhuaCoverageTestPath, test, fragment);
  }
}

const erhuaCoverageSummaryCheckPath =
  "tools/deploy/check-erhua-member-recognition-coverage-summary.mjs";
if (!exists(erhuaCoverageSummaryCheckPath)) {
  addError(
    `${erhuaCoverageSummaryCheckPath}: missing Erhua member recognition coverage summary checker`
  );
} else {
  const script = readText(erhuaCoverageSummaryCheckPath);
  for (const fragment of [
    "erhua_member_recognition_coverage_v1",
    "--expect-pass",
    "--require-active-profiles",
    "coverage summary contains forbidden sensitive fragment",
    "retained_evidence_boundary",
    "all_linked_people_have_active_profiles",
  ]) {
    requireFragment(erhuaCoverageSummaryCheckPath, script, fragment);
  }
}

const erhuaCoverageSummaryTestPath =
  "tools/deploy/test-erhua-member-recognition-coverage-summary.mjs";
if (!exists(erhuaCoverageSummaryTestPath)) {
  addError(
    `${erhuaCoverageSummaryTestPath}: missing Erhua member recognition coverage summary checker tests`
  );
} else {
  const test = readText(erhuaCoverageSummaryTestPath);
  for (const fragment of [
    "valid-failed",
    "strict-failed",
    "valid-passed",
    "person-id-leak",
    "readiness-mismatch",
    "boundary-mismatch",
  ]) {
    requireFragment(erhuaCoverageSummaryTestPath, test, fragment);
  }
}

const erhuaCoverageFinalizerPath =
  "tools/deploy/finalize-erhua-member-recognition-coverage.mjs";
if (!exists(erhuaCoverageFinalizerPath)) {
  addError(
    `${erhuaCoverageFinalizerPath}: missing Erhua member recognition coverage finalizer`
  );
} else {
  const script = readText(erhuaCoverageFinalizerPath);
  for (const fragment of [
    "check-erhua-member-recognition-coverage.mjs",
    "check-erhua-member-recognition-coverage-summary.mjs",
    "--summary-output",
    "--expect-pass",
    "--require-active-profiles",
    "coverage checker did not write summary output",
    "sanitized summary written and verified",
  ]) {
    requireFragment(erhuaCoverageFinalizerPath, script, fragment);
  }
}

const erhuaCoverageFinalizerTestPath =
  "tools/deploy/test-finalize-erhua-member-recognition-coverage.mjs";
if (!exists(erhuaCoverageFinalizerTestPath)) {
  addError(
    `${erhuaCoverageFinalizerTestPath}: missing Erhua member recognition coverage finalizer tests`
  );
} else {
  const test = readText(erhuaCoverageFinalizerTestPath);
  for (const fragment of [
    "identity-gap",
    "strict-passed",
    "coverage summary check passed",
    "coverage finalized",
    "summary output directory does not exist",
    "/dev/null",
  ]) {
    requireFragment(erhuaCoverageFinalizerTestPath, test, fragment);
  }
}

const erhuaCanaryEvidenceBuilderPath =
  "tools/deploy/build-erhua-member-recognition-canary-evidence.mjs";
if (!exists(erhuaCanaryEvidenceBuilderPath)) {
  addError(
    `${erhuaCanaryEvidenceBuilderPath}: missing Erhua member recognition canary evidence builder`
  );
} else {
  const script = readText(erhuaCanaryEvidenceBuilderPath);
  for (const fragment of [
    "createHash",
    "person_ref: personRef",
    "erhua-member-recognition-person-ref-v1",
  ]) {
    requireFragment(erhuaCanaryEvidenceBuilderPath, script, fragment);
  }
}

const erhuaCanaryCheckPath = "tools/deploy/check-erhua-member-recognition-canary.mjs";
if (!exists(erhuaCanaryCheckPath)) {
  addError(`${erhuaCanaryCheckPath}: missing Erhua member recognition canary checker`);
} else {
  const script = readText(erhuaCanaryCheckPath);
  for (const fragment of [
    '/"person_id"\\s*:/',
    "readPersonRef",
    "missing a valid person_ref",
    "isPersonRef",
  ]) {
    requireFragment(erhuaCanaryCheckPath, script, fragment);
  }
}

const erhuaCanaryTestPath = "tools/deploy/test-erhua-member-recognition-canary.mjs";
if (!exists(erhuaCanaryTestPath)) {
  addError(`${erhuaCanaryTestPath}: missing Erhua member recognition canary tests`);
} else {
  const test = readText(erhuaCanaryTestPath);
  for (const fragment of [
    "person-id-leak.json",
    "person_ref: personRef",
    "erhua-member-recognition-person-ref-v1",
  ]) {
    requireFragment(erhuaCanaryTestPath, test, fragment);
  }
}

const erhuaCanaryBuilderTestPath =
  "tools/deploy/test-erhua-member-recognition-canary-builder.mjs";
if (!exists(erhuaCanaryBuilderTestPath)) {
  addError(
    `${erhuaCanaryBuilderTestPath}: missing Erhua member recognition canary builder tests`
  );
} else {
  const test = readText(erhuaCanaryBuilderTestPath);
  for (const fragment of [
    'assert.doesNotMatch(built, /"person_id"\\s*:/)',
    "personRef(xiaoqiaoPersonId)",
    "erhua-member-recognition-person-ref-v1",
  ]) {
    requireFragment(erhuaCanaryBuilderTestPath, test, fragment);
  }
}

const erhuaCompletionCheckPath =
  "tools/deploy/check-erhua-member-recognition-completion.mjs";
if (!exists(erhuaCompletionCheckPath)) {
  addError(
    `${erhuaCompletionCheckPath}: missing Erhua member recognition completion checker`
  );
} else {
  const script = readText(erhuaCompletionCheckPath);
  for (const fragment of [
    "MIN_PROFILE_REPAIR_MESSAGE_LIMIT = 5000",
    "requested_message_limit",
    "qiwe_room_potential_member_identities_unlinked",
    "unlinked current-room potential member identities",
    "member profile evidence must be generated with --limit",
    "running_people_profile_missing_running_hint",
    "answer-context speaker self-canary people must cover every linked person",
    "--summary-output",
    "erhua_member_recognition_completion_v1",
    "retained_evidence_boundary",
    "includes_person_id: false",
    '/"person_id"\\s*:/',
    "readPersonRef",
    "missing a valid person_ref",
    "--require-active-profiles",
    "non-empty safe profile hints",
    "linked_profile_hint_people",
    "profile hint evidence must cover the same people",
  ]) {
    requireFragment(erhuaCompletionCheckPath, script, fragment);
  }
}

const erhuaCompletionTestPath =
  "tools/deploy/test-erhua-member-recognition-completion.mjs";
if (!exists(erhuaCompletionTestPath)) {
  addError(
    `${erhuaCompletionTestPath}: missing Erhua member recognition completion checker tests`
  );
} else {
  const test = readText(erhuaCompletionTestPath);
  for (const fragment of [
    "low-profile-scan-limit",
    "requested_message_limit: 500",
    "requested_message_limit: 5000",
    "potential-member-unlinked",
    "--limit 5000",
    "valid-completion-summary.json",
    "valid-active-profile-strict",
    "requireActiveProfiles",
    "fullProfileCanaries",
    "profile-hint-route-mismatch",
    "non-empty safe profile hints",
    "identity-only canary people must be zero",
    "profile hint evidence must cover the same people",
    "retained_evidence_boundary.includes_person_id",
    "person_ref: personRef",
    'assert.doesNotMatch(JSON.stringify(summary), new RegExp(PERSON_PAXON, "i"))',
  ]) {
    requireFragment(erhuaCompletionTestPath, test, fragment);
  }
}

const erhuaCompletionSummaryCheckPath =
  "tools/deploy/check-erhua-member-recognition-completion-summary.mjs";
if (!exists(erhuaCompletionSummaryCheckPath)) {
  addError(
    `${erhuaCompletionSummaryCheckPath}: missing Erhua member recognition completion summary checker`
  );
} else {
  const script = readText(erhuaCompletionSummaryCheckPath);
  for (const fragment of [
    "erhua_member_recognition_completion_v1",
    "retained_evidence_boundary",
    "contains unsupported field",
    "current-room raw identity count must match synced room roster",
    "linked people must all have QiWe platform identities",
    "completion summary contains forbidden sensitive fragment",
    "--require-active-profiles",
    "active reply_context profiles",
    "identity-only canary people must be zero",
    "linked_profile_hint_people",
  ]) {
    requireFragment(erhuaCompletionSummaryCheckPath, script, fragment);
  }
}

const erhuaCompletionSummaryTestPath =
  "tools/deploy/test-erhua-member-recognition-completion-summary.mjs";
if (!exists(erhuaCompletionSummaryTestPath)) {
  addError(
    `${erhuaCompletionSummaryTestPath}: missing Erhua member recognition completion summary checker tests`
  );
} else {
  const test = readText(erhuaCompletionSummaryTestPath);
  for (const fragment of [
    "person-id-leak",
    "identity-count-mismatch",
    "missing-speaker-route",
    "unsafe-boundary",
    "unsupported-display-name",
    "unsupported-profile-note",
    "identity-only-default-allowed",
    "requireActiveProfiles",
    "profile-hint-route-mismatch",
    "mentioned_profile_hint_people",
  ]) {
    requireFragment(erhuaCompletionSummaryTestPath, test, fragment);
  }
}

const erhuaCompletionFinalizerPath =
  "tools/deploy/finalize-erhua-member-recognition-completion.mjs";
if (!exists(erhuaCompletionFinalizerPath)) {
  addError(
    `${erhuaCompletionFinalizerPath}: missing Erhua member recognition completion finalizer`
  );
} else {
  const script = readText(erhuaCompletionFinalizerPath);
  for (const fragment of [
    "check-erhua-member-recognition-completion.mjs",
    "check-erhua-member-recognition-completion-summary.mjs",
    "--summary-output",
    "--require-active-profiles",
    "summary output directory does not exist",
    "sanitized summary written and verified",
  ]) {
    requireFragment(erhuaCompletionFinalizerPath, script, fragment);
  }
}

const erhuaCompletionFinalizerTestPath =
  "tools/deploy/test-finalize-erhua-member-recognition-completion.mjs";
if (!exists(erhuaCompletionFinalizerTestPath)) {
  addError(
    `${erhuaCompletionFinalizerTestPath}: missing Erhua member recognition completion finalizer tests`
  );
} else {
  const test = readText(erhuaCompletionFinalizerTestPath);
  for (const fragment of [
    "completion finalized",
    "non-ambiguous unlinked identities",
    "summary output directory does not exist",
    "/dev/null",
    "completion summary is not valid JSON",
    "active-profile-strict",
    "identity-only-strict",
    "empty-hints-strict",
    "non-empty safe profile hints",
  ]) {
    requireFragment(erhuaCompletionFinalizerTestPath, test, fragment);
  }
}

const operationsReadmePath = "docs/operations/README.md";
if (!exists(operationsReadmePath)) {
  addError(`${operationsReadmePath}: missing operations index`);
} else {
  const readme = readText(operationsReadmePath);
  for (const fragment of [
    "[xiaoman-production-evidence-runbook.md](xiaoman-production-evidence-runbook.md)",
    "owner-operated Huabaosi canary, QiWe companion verification, real-activity retention,",
    "final completion-manifest sequence.",
    "[erhua-member-recognition-production-runbook.md](erhua-member-recognition-production-runbook.md)",
    "release-local production config observation",
    "release-local QiWe room roster sync",
    "safe profile refresh, coverage checker, sanitized answer-context",
    "canary builder, canary checker, and final completion checker for Erhua member",
    "reviewed one-shot completion finalizer",
    "node tools/deploy/check-xiaoman-production-evidence-chain-local.mjs",
    "repository-local Xiaoman production evidence chain verification bundle",
    "deploy:xiaoman-production-evidence:finalize",
    "pnpm deploy:erhua-member-recognition:finalize -- --room-sync",
  ]) {
    requireFragment(operationsReadmePath, readme, fragment);
  }
}

const deployToolsReadmePath = "tools/deploy/README.md";
if (!exists(deployToolsReadmePath)) {
  addError(`${deployToolsReadmePath}: missing deploy tools README`);
} else {
  const readme = readText(deployToolsReadmePath);
  for (const fragment of [
    "node tools/deploy/check-xiaoman-production-evidence-chain-local.mjs",
    "Use it before any owner-operated Huabaosi production canary, QiWe companion",
    "pnpm deploy:xiaoman-production-evidence:finalize --",
    "--staging-runtime-readiness <staging-runtime-readiness-output.txt>",
    "--production-real-activity <production-evidence-output.txt>",
    "--output <completed-xiaoman-production-completion-evidence.json>",
    "node tools/deploy/check-erhua-room-member-sync.mjs",
    "node tools/deploy/finalize-erhua-member-recognition-coverage.mjs",
    "node tools/deploy/check-erhua-member-recognition-coverage-summary.mjs",
    "deploy/sidecar/scripts/apply-erhua-member-recognition-production-config.sh --apply",
    "erhua-member-recognition-production-config-observation-smoke.sh",
    "ready_for_member_recognition_runbook",
    "QINTOPIA_ERHUA_MEMBER_RECOGNITION_PRODUCTION_CONFIG=approved-production-erhua-member-recognition-config",
    "node tools/deploy/build-erhua-member-safe-identity-payload-template.mjs",
    "node tools/deploy/check-erhua-member-safe-identity-payload.mjs",
    "qintopia-message-sidecar erhua-member-safe-identity",
    "node tools/deploy/build-erhua-member-safe-alias-payload-template.mjs",
    "qintopia-message-sidecar erhua-member-safe-alias",
    "qintopia-message-sidecar erhua-member-speaker-canary-sender-map",
    "node tools/deploy/build-erhua-member-recognition-canary-mcp-input.mjs",
    "node tools/deploy/build-erhua-member-recognition-canary-evidence.mjs",
    "node tools/deploy/check-erhua-member-recognition-canary.mjs",
    "node tools/deploy/finalize-erhua-member-recognition-completion.mjs",
    "node tools/deploy/build-erhua-member-recognition-roster-audit.mjs",
    "--summary-output <sanitized-coverage-summary.json>",
    "--expect-pass",
    "coverage summary is count-only retained evidence",
    "`check-erhua-member-recognition-completion.mjs`",
    "`check-erhua-member-recognition-completion-summary.mjs`",
    "--summary-output <sanitized-completion-summary.json>",
    "node tools/deploy/build-erhua-member-recognition-roster-audit.mjs",
    "--output <sanitized-roster-audit.json>",
    "The roster audit is per-person retained evidence",
    "--require-active-profiles",
    "active `reply_context` profile",
    "`identity_only`",
    "empty safe profile",
    "non-sensitive scope/count fields",
    "explicit no-secret",
    "`qiwe_room_channel_identities_raw_total`",
    "to equal the",
    "applied room-sync",
    "`room_members_discovered`",
    "`linked_people_without_qiwe_platform_identity = 0`",
    "`unsafe_display_unlinked = 0`",
    "`qiwe_speaker_identities`",
    "`platform_identities_missing = 0`",
    "`ambiguous_users = 0`",
    "`answer_context_speaker_canary_specs` resolving every linked current-room person",
    "server-local `/tmp` only",
    "`person_ref` SHA-256",
    "must not contain",
  ]) {
    requireFragment(deployToolsReadmePath, readme, fragment);
  }
}

const erhuaMemberRecognitionRunbookPath =
  "docs/operations/erhua-member-recognition-production-runbook.md";
if (!exists(erhuaMemberRecognitionRunbookPath)) {
  addError(
    `${erhuaMemberRecognitionRunbookPath}: missing Erhua member recognition production runbook`
  );
} else {
  const runbook = readText(erhuaMemberRecognitionRunbookPath);
  for (const fragment of [
    "node tools/deploy/finalize-erhua-member-recognition-completion.mjs",
    "QINTOPIA_ERHUA_MEMBER_RECOGNITION_PRODUCTION_CONFIG=approved-production-erhua-member-recognition-config",
    "deploy/sidecar/scripts/apply-erhua-member-recognition-production-config.sh --apply",
    "erhua-member-recognition-production-config-observation-smoke.sh",
    "ready_for_member_recognition_runbook",
    "does not call QiWe, Postgres, MCP, systemctl, or the network",
    "node tools/deploy/finalize-erhua-member-recognition-coverage.mjs",
    "--summary-output /tmp/erhua-member-recognition-completion-summary.json",
    "--require-active-profiles",
    "`identity_only`",
    "`unsafe_display_unlinked = 0`",
    "`qiwe_speaker_identities.platform_identities_missing = 0`",
    "`qiwe_speaker_identities.ambiguous_users = 0`",
    "non-empty safe profile",
    "/tmp/erhua-member-recognition-coverage-summary.json",
    "node tools/deploy/check-erhua-member-recognition-coverage-summary.mjs",
    "--expect-pass",
    "sanitized count-only evidence",
    "JSONL, checker output, completion summary, finalizer output, and roster audit",
    "node tools/deploy/build-erhua-member-recognition-roster-audit.mjs",
    "--output /tmp/erhua-member-recognition-roster-audit.json",
    "The roster audit is the retained per-person proof",
    "wrong-room, incomplete, or extra sender",
    "map must fail before raw MCP input is emitted",
    "materialized QiWe platform identity",
    "valid `speaker.person_ref`",
    "must not contain `person_id`",
  ]) {
    requireFragment(erhuaMemberRecognitionRunbookPath, runbook, fragment);
  }
}

const releaseCurrentModelPath = "docs/operations/release-current-model.md";
if (exists(releaseCurrentModelPath)) {
  const releaseCurrentModel = readText(releaseCurrentModelPath);
  requireFragment(
    releaseCurrentModelPath,
    releaseCurrentModel,
    "release-acceptance-checklist.md"
  );
  requireFragment(
    releaseCurrentModelPath,
    releaseCurrentModel,
    "exact-head Release Please validation"
  );
  requireFragment(
    releaseCurrentModelPath,
    releaseCurrentModel,
    "qintopia-message-sidecar-qiwe-production-linux-x86_64-gnu"
  );
  requireFragment(
    releaseCurrentModelPath,
    releaseCurrentModel,
    "sidecar-profiles/qiwe-production/"
  );
  requireFragment(
    releaseCurrentModelPath,
    releaseCurrentModel,
    "not a global Huabaosi/QiWe switch"
  );
}

const runtimeBaselinePath = "docs/operations/runtime-baseline.md";
if (exists(runtimeBaselinePath)) {
  const baseline = readText(runtimeBaselinePath);
  for (const fragment of [
    "runtime_artifact_profile=huabaosi-production",
    "sidecar-profiles/qiwe-production",
    "Huabaosi production sidecar SHA-256",
    "QiWe production sidecar SHA-256",
    "Treating them as the same production binary is no longer a valid assumption",
  ]) {
    requireFragment(runtimeBaselinePath, baseline, fragment);
  }
}

const xiaomanProductionCompletionGatePath =
  "docs/plans/active/xiaoman-production-completion-gate.md";
if (exists(xiaomanProductionCompletionGatePath)) {
  const gate = readText(xiaomanProductionCompletionGatePath);
  for (const fragment of [
    "Updated: 2026-07-24",
    "2026-07-24 Xiaoman production evidence chain local verification",
    "That local verification does not satisfy the completion gates below.",
    "Xiaoman production evidence runbook",
    "not a large remaining repository implementation gap",
    "runtime_artifact_profile=huabaosi-production",
    "runtime_artifact_profile=qiwe-production",
    "--production-real-activity <production-evidence-output.txt>",
    "pnpm deploy:xiaoman-production-evidence:finalize --",
    "--staging-runtime-readiness <staging-runtime-readiness-output.txt>",
    "reviewed one-shot",
  ]) {
    requireFragment(xiaomanProductionCompletionGatePath, gate, fragment);
  }
  forbidFragment(
    xiaomanProductionCompletionGatePath,
    gate,
    "--production-real-activity <production-real-activity-output.txt>"
  );
}

const reportsReadmePath = "docs/reports/README.md";
if (exists(reportsReadmePath)) {
  const reportsReadme = readText(reportsReadmePath);
  for (const fragment of [
    "2026-07-24 Xiaoman production evidence PR body",
    "2026-07-24 Xiaoman production evidence PR notes",
    "2026-07-24 Xiaoman production test map",
    "2026-07-24 Xiaoman production evidence chain local verification",
    "single-page HTML handoff view",
    "## Templates",
    "templates/huabaosi-image-production-canary-evidence.md",
    "templates/xiaoman-production-completion-evidence.json",
    "templates/xiaoman-real-activity-production-evidence.md",
    "tools/deploy/finalize-xiaoman-production-completion-evidence.mjs",
    "retained staging and production evidence files",
  ]) {
    requireFragment(reportsReadmePath, reportsReadme, fragment);
  }
}

const xiaomanProductionTestMapPath =
  "docs/reports/2026-07-24-xiaoman-production-test-map.html";
if (exists(xiaomanProductionTestMapPath)) {
  const html = readText(xiaomanProductionTestMapPath);
  for (const fragment of [
    "<title>Xiaoman Production Test Map - 2026-07-24</title>",
    "小满生产测试图",
    "老板版摘要",
    "代码基本完成，当前卡在真实生产证据",
    "差 4 类真实外部动作",
    "先执行，再收尾，不再扩主流程代码",
    "仓库内校验已全绿",
    "Huabaosi 生产 Canary",
    "QiWe 生产 Follow-up Deploy",
    "真实小满活动证据",
    "企微群到达确认",
    "最终 Completion Manifest",
    "pnpm deploy:xiaoman-production-evidence:finalize -- ...",
    "runtime_artifact_profile=huabaosi-production",
    "runtime_artifact_profile=qiwe-production",
    "production-complete",
  ]) {
    requireFragment(xiaomanProductionTestMapPath, html, fragment);
  }
  for (const fragment of [
    "QIWE_TOKEN=",
    "postgres://",
    "postgresql://",
    "tenant_access_token",
    "systemctl enable --now",
  ]) {
    forbidFragment(xiaomanProductionTestMapPath, html, fragment);
  }
}

const xiaomanProductionEvidenceHandoffPath =
  "docs/reports/2026-07-24-xiaoman-production-evidence-handoff.md";
if (exists(xiaomanProductionEvidenceHandoffPath)) {
  const handoff = readText(xiaomanProductionEvidenceHandoffPath);
  for (const fragment of [
    "2026-07-24-xiaoman-production-evidence-pr-body.md",
    "2026-07-24-xiaoman-production-evidence-pr-notes.md",
    "pnpm deploy:xiaoman-production-evidence:finalize --",
  ]) {
    requireFragment(xiaomanProductionEvidenceHandoffPath, handoff, fragment);
  }
}

const xiaomanProductionEvidencePrNotesPath =
  "docs/reports/2026-07-24-xiaoman-production-evidence-pr-notes.md";
if (exists(xiaomanProductionEvidencePrNotesPath)) {
  const notes = readText(xiaomanProductionEvidencePrNotesPath);
  for (const fragment of [
    "feat(deploy): harden Xiaoman production evidence chain handoff",
    "production evidence capture remains",
    "docs/reports/2026-07-24-xiaoman-production-test-map.html",
    "pnpm deploy:xiaoman-production-evidence:finalize -- ...",
  ]) {
    requireFragment(xiaomanProductionEvidencePrNotesPath, notes, fragment);
  }
}

const xiaomanLocalVerificationReportPath =
  "docs/reports/2026-07-24-xiaoman-production-evidence-chain-local-verification.md";
if (exists(xiaomanLocalVerificationReportPath)) {
  const report = readText(xiaomanLocalVerificationReportPath);
  for (const fragment of [
    "repository-local verification only",
    "It does not prove that production evidence has",
    "node tools/deploy/check-xiaoman-production-evidence-chain-local.mjs",
    "node tools/deploy/check-deploy-contracts.mjs",
    "node tools/deploy/check-deploy-runner.mjs",
    "node tools/deploy/test-build-sidecar-artifact.mjs",
    "node tools/deploy/test-build-qiwe-production-sidecar-artifact.mjs",
    "node tools/deploy/test-fetch-cos-artifact-permissions.mjs",
    "runtime_artifact_profile=qiwe-production",
    "artifact-manifest.json",
    "SHA256SUMS",
    "mode `0444`",
    "sidecar binary at `0755`",
    "owner-operated capture of real production evidence",
    "Build and validate the final Xiaoman production completion manifest",
    "finalize-xiaoman-production-completion-evidence.mjs",
    "docs/operations/xiaoman-production-evidence-runbook.md",
  ]) {
    requireFragment(xiaomanLocalVerificationReportPath, report, fragment);
  }
  for (const fragment of [
    "QIWE_TOKEN=",
    "postgres://",
    "postgresql://",
    "gh release create",
    "systemctl enable",
  ]) {
    forbidFragment(xiaomanLocalVerificationReportPath, report, fragment);
  }
}

const xiaomanLocalVerificationScriptPath =
  "tools/deploy/check-xiaoman-production-evidence-chain-local.mjs";
if (!exists(xiaomanLocalVerificationScriptPath)) {
  addError(
    `${xiaomanLocalVerificationScriptPath}: missing Xiaoman production evidence chain local check`
  );
} else {
  const script = readText(xiaomanLocalVerificationScriptPath);
  for (const fragment of [
    '["node", ["tools/deploy/check-deploy-contracts.mjs"]]',
    '["node", ["tools/deploy/check-deploy-runner.mjs"]]',
    '["node", ["tools/deploy/test-build-sidecar-artifact.mjs"]]',
    '["node", ["tools/deploy/test-build-qiwe-production-sidecar-artifact.mjs"]]',
    '["node", ["tools/deploy/test-fetch-cos-artifact-permissions.mjs"]]',
    '["node", ["tools/deploy/test-xiaoman-production-completion-manifest-builder.mjs"]]',
    '["node", ["tools/deploy/test-xiaoman-production-completion-evidence.mjs"]]',
    '"cargo",',
    '"runtime/sidecar/Cargo.toml"',
    "Xiaoman production evidence chain local check passed",
  ]) {
    requireFragment(xiaomanLocalVerificationScriptPath, script, fragment);
  }
}

for (const [packagePath, requiredFragments] of Object.entries(packages)) {
  const readmePath = `${packagePath}/README.md`;
  const manifestPath = `${packagePath}/manifest.yaml`;
  if (!exists(readmePath)) {
    addError(`${packagePath}: missing README.md`);
    continue;
  }
  if (!exists(manifestPath)) {
    addError(`${packagePath}: missing manifest.yaml`);
    continue;
  }

  const readme = readText(readmePath);
  for (const fragment of requiredFragments) {
    if (!readme.includes(fragment)) {
      addError(`${readmePath}: must mention ${fragment}`);
    }
  }

  const manifest = YAML.parse(readText(manifestPath));
  if (manifest.id !== packagePath) {
    addError(`${manifestPath}: id must be ${packagePath}`);
  }
  if (manifest.type !== "deploy") {
    addError(`${manifestPath}: type must be deploy`);
  }
}

const xiaomanPreflightPath =
  "deploy/sidecar/scripts/xiaoman-activity-production-preflight-smoke.sh";
if (!exists(xiaomanPreflightPath)) {
  addError(`${xiaomanPreflightPath}: missing Xiaoman production preflight smoke`);
} else {
  const preflight = readText(xiaomanPreflightPath);
  for (const fragment of [
    "QINTOPIA_XIAOMAN_ACTIVITY_PRODUCTION_PREFLIGHT_ENABLE",
    "QINTOPIA_XIAOMAN_ACTIVITY_SIGNAL_TIMER_OBSERVATION_ENABLE",
    "xiaoman-activity-signal-timer-observation-smoke.sh",
    "QINTOPIA_XIAOMAN_LEGACY_CRON_OBSERVATION_ENABLE",
    "xiaoman-legacy-cron-observation-smoke.sh",
    "QINTOPIA_XIAOMAN_ACTIVITY_PROMOTION_STARTER_TIMER_OBSERVATION_ENABLE",
    "xiaoman-activity-promotion-starter-timer-observation-smoke.sh",
    "QINTOPIA_OPERATIONS_DOWNSTREAM_TIMERS_OBSERVATION_ENABLE",
    "operations-downstream-timers-observation-smoke.sh",
    "QINTOPIA_XIAOMAN_ACTIVITY_DOWNSTREAM_OBSERVATION_ENABLE",
    "xiaoman-activity-downstream-observation-smoke.sh",
    "QINTOPIA_XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_OBSERVATION_ENABLE",
    "xiaoman-activity-image-generation-starter-observation-smoke.sh",
    "QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_OBSERVATION_ENABLE",
    "huabaosi-image-generation-production-observation-smoke.sh",
    "QINTOPIA_XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_OBSERVATION_ENABLE",
    "xiaoman-activity-send-request-starter-observation-smoke.sh",
    "QINTOPIA_OPERATIONS_GROUP_SEND_READY_TIMER_OBSERVATION_ENABLE",
    "operations-group-send-ready-timer-observation-smoke.sh",
    "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_OBSERVATION_ENABLE",
    "qiwe-image-send-production-observation-smoke.sh",
    "QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_OBSERVATION_ENABLE",
    "qiwe-image-callback-bridge-production-observation-smoke.sh",
    'CHILD_PATH="/usr/bin:/bin:/usr/sbin:/sbin"',
    'env -i "${child_env[@]}" "$script_path"',
    '"PATH=${CHILD_PATH}"',
  ]) {
    requireFragment(xiaomanPreflightPath, preflight, fragment);
  }

  for (const fragment of [
    "env QINTOPIA_",
    "QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE=1",
    "QINTOPIA_SIDECAR_ENV_FILE=",
    "SYSTEMCTL=",
    "JOURNALCTL=",
    "_OBSERVATION_TEST_MODE",
    "server-deploy.sh",
    "gh release",
    "release create",
    "release edit",
    "run-group-message-send-worker",
    "send_executed=true",
    "--use-feishu-base",
    "tenant_access_token",
    "QIWE_TOKEN",
    "QIWE_GUID",
  ]) {
    forbidFragment(xiaomanPreflightPath, preflight, fragment);
  }
}

const qiweCallbackBridgeProductionObservationPath =
  "deploy/sidecar/scripts/qiwe-image-callback-bridge-production-observation-smoke.sh";
if (!exists(qiweCallbackBridgeProductionObservationPath)) {
  addError(
    `${qiweCallbackBridgeProductionObservationPath}: missing production observation smoke`
  );
} else {
  const smoke = readText(qiweCallbackBridgeProductionObservationPath);
  for (const fragment of [
    "QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_OBSERVATION_ENABLE",
    "QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_ENABLED",
    "/home/ubuntu/.hermes/profiles/erhua/.env",
    "/home/ubuntu/.hermes/profiles/erhua/plugins/qiwe-platform",
    "/home/ubuntu/qintopia-agent-os-releases/current/sidecar-profiles/qiwe-production/qintopia-message-sidecar",
    "skills/qiwe/image_callback_bridge.py",
    "qiwe_image_callback_bridge_production_observation_state",
    "huabaosi-feishu-mirror-adapter",
    "qiwe-production",
    '"qiwe-production-adapter"',
    'allowlist = {"QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_ENABLED"}',
  ]) {
    requireFragment(qiweCallbackBridgeProductionObservationPath, smoke, fragment);
  }
  for (const fragment of [
    "process-qiwe-image-send-callback",
    "--apply",
    "systemctl enable",
    "systemctl start",
    "source ",
    "eval ",
    "tenant_access_token",
    "raw_body",
    "huabaosi-production-adapter",
    "/current/sidecar/qintopia-message-sidecar",
  ]) {
    forbidFragment(qiweCallbackBridgeProductionObservationPath, smoke, fragment);
  }
}

const qiweCallbackBridgePath = "skills/qiwe/image_callback_bridge.py";
if (!exists(qiweCallbackBridgePath)) {
  addError(`${qiweCallbackBridgePath}: missing QiWe callback bridge`);
} else {
  const bridge = readText(qiweCallbackBridgePath);
  for (const fragment of [
    'Path("sidecar-profiles") / "qiwe-production" / PROCESSOR_BASENAME',
    "if enabled and processor_mode == PROCESSOR_MODE_PRODUCTION:",
    ") = _production_processor_identity()",
    'artifact_dir / "artifact-manifest.json"',
    'artifact_dir / "SHA256SUMS"',
    'manifest.get("commit_sha") != resolved_release.name',
    'validation.get("artifact_profile") != PRODUCTION_ARTIFACT_PROFILE',
    'validation.get("cargo_features") != PRODUCTION_ARTIFACT_FEATURES',
    "checksums.get(PROCESSOR_BASENAME) != expected_sha256",
    "artifact_sha256 = _production_artifact_binary_sha256(resolved_current)",
    "_validate_processor_digest(resolved, expected_sha256)",
    'self.processor_env["QINTOPIA_DEPLOYED_COMMIT_SHA"] = release_sha',
    '"QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA"',
  ]) {
    requireFragment(qiweCallbackBridgePath, bridge, fragment);
  }
}

const qiweCallbackBridgeTestPath = "skills/qiwe/tests/test_image_callback_bridge.py";
if (!exists(qiweCallbackBridgeTestPath)) {
  addError(`${qiweCallbackBridgeTestPath}: missing QiWe callback bridge tests`);
} else {
  const tests = readText(qiweCallbackBridgeTestPath);
  for (const fragment of [
    "test_production_environment_derives_current_companion_identity",
    '"QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_BIN": "/stale/release/qintopia-message-sidecar"',
    '"QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_ROOT": "/stale/release"',
    "self.assertEqual(bridge.processor_sha256, digest)",
    'bridge.processor_env["QINTOPIA_DEPLOYED_COMMIT_SHA"]',
  ]) {
    requireFragment(qiweCallbackBridgeTestPath, tests, fragment);
  }
}

const qiweCallbackBridgeProductionActivationPath =
  "deploy/sidecar/scripts/activate-qiwe-image-callback-bridge-production.sh";
if (!exists(qiweCallbackBridgeProductionActivationPath)) {
  addError(
    `${qiweCallbackBridgeProductionActivationPath}: missing production activation script`
  );
} else {
  const activation = readText(qiweCallbackBridgeProductionActivationPath);
  for (const fragment of [
    "QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_ACTIVATION",
    "approved-production-qiwe-image-callback-bridge",
    "qiwe-image-callback-bridge-production-observation-smoke.sh",
    'RUNUSER_BIN="/usr/sbin/runuser"',
    'HERMES_SYSTEMD_USER="ubuntu"',
    'HERMES_SERVICE="hermes-gateway-erhua.service"',
    'QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_EXPECTED_STATE="$expected_state"',
    "QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_OBSERVATION_ENABLE=1",
    "run_observation enabled",
    "restart_erhua",
    "systemctl --user restart ${HERMES_SERVICE}",
    "systemctl --user is-active --quiet ${HERMES_SERVICE}",
    "env -i",
    "PATH=/usr/bin:/bin:/usr/sbin:/sbin",
  ]) {
    requireFragment(qiweCallbackBridgeProductionActivationPath, activation, fragment);
  }
  for (const fragment of [
    "systemctl enable",
    "systemctl start",
    "process-qiwe-image-send-callback",
    "--apply",
    "source ",
    "eval ",
    "tenant_access_token",
    "QIWE_TOKEN",
    "QIWE_GUID",
    "QINTOPIA_SIDECAR_DATABASE_URL",
    "TEST_MODE",
    "_TEST_MODE",
    "RUNUSER_BIN:-",
    "QINTOPIA_HERMES_SYSTEMD_USER",
    "QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_HERMES_SERVICE",
    "QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_OBSERVATION_TEST_MODE",
    "QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_OBSERVATION_TEST_ROOT",
    "QINTOPIA_RELEASE_CURRENT_DIR",
    "QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_ENV_FILE",
    "QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PLUGIN_PATH",
  ]) {
    forbidFragment(qiweCallbackBridgeProductionActivationPath, activation, fragment);
  }
}

const qiweCallbackBridgeProductionRollbackPath =
  "deploy/sidecar/scripts/rollback-qiwe-image-callback-bridge-production.sh";
if (!exists(qiweCallbackBridgeProductionRollbackPath)) {
  addError(
    `${qiweCallbackBridgeProductionRollbackPath}: missing production rollback script`
  );
} else {
  const rollback = readText(qiweCallbackBridgeProductionRollbackPath);
  for (const fragment of [
    "QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_ROLLBACK",
    "approved-production-qiwe-image-callback-bridge-rollback",
    "qiwe-image-callback-bridge-production-observation-smoke.sh",
    'RUNUSER_BIN="/usr/sbin/runuser"',
    'HERMES_SYSTEMD_USER="ubuntu"',
    'HERMES_SERVICE="hermes-gateway-erhua.service"',
    'QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_EXPECTED_STATE="$expected_state"',
    "QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_OBSERVATION_ENABLE=1",
    "run_observation disabled",
    "restart_erhua",
    "systemctl --user restart ${HERMES_SERVICE}",
    "systemctl --user is-active --quiet ${HERMES_SERVICE}",
    "env -i",
    "PATH=/usr/bin:/bin:/usr/sbin:/sbin",
  ]) {
    requireFragment(qiweCallbackBridgeProductionRollbackPath, rollback, fragment);
  }
  for (const fragment of [
    "systemctl enable",
    "systemctl start",
    "process-qiwe-image-send-callback",
    "--apply",
    "source ",
    "eval ",
    "tenant_access_token",
    "QIWE_TOKEN",
    "QIWE_GUID",
    "QINTOPIA_SIDECAR_DATABASE_URL",
    "TEST_MODE",
    "_TEST_MODE",
    "RUNUSER_BIN:-",
    "QINTOPIA_HERMES_SYSTEMD_USER",
    "QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_HERMES_SERVICE",
    "QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_OBSERVATION_TEST_MODE",
    "QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_OBSERVATION_TEST_ROOT",
    "QINTOPIA_RELEASE_CURRENT_DIR",
    "QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_ENV_FILE",
    "QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PLUGIN_PATH",
  ]) {
    forbidFragment(qiweCallbackBridgeProductionRollbackPath, rollback, fragment);
  }
}

const xiaomanLegacyCronObservationPath =
  "deploy/sidecar/scripts/xiaoman-legacy-cron-observation-smoke.sh";
if (!exists(xiaomanLegacyCronObservationPath)) {
  addError(
    `${xiaomanLegacyCronObservationPath}: missing Xiaoman legacy cron observation`
  );
} else {
  const smoke = readText(xiaomanLegacyCronObservationPath);
  for (const fragment of [
    "QINTOPIA_XIAOMAN_LEGACY_CRON_OBSERVATION_ENABLE",
    "/home/ubuntu/.hermes/profiles/xiaoman",
    "/home/ubuntu/.hermes/profiles/xiaoman/cron/jobs.json",
    "runtime/hermes/cron/reviewed-cron-jobs.json",
    "reviewed_declarations_only",
    "reviewed_decl_count",
    "cron_decl_count",
    "live_profile_modified",
    "external_calls_executed",
    "origin_platform",
    'entry.get("deliver")',
    "Xiaoman legacy cron observation found unreviewed cron job declarations",
  ]) {
    requireFragment(xiaomanLegacyCronObservationPath, smoke, fragment);
  }
  for (const fragment of [
    "systemctl",
    "rm ",
    "mv ",
    "cp ",
    "source ",
    "eval ",
    "run-",
    "send_executed=true",
    "QIWE_TOKEN",
    "tenant_access_token",
  ]) {
    forbidFragment(xiaomanLegacyCronObservationPath, smoke, fragment);
  }
}

const xiaomanPreflightRecordPath =
  "deploy/smoke/docs/xiaoman-production-preflight-record.md";
if (!exists(xiaomanPreflightRecordPath)) {
  addError(`${xiaomanPreflightRecordPath}: missing Xiaoman preflight record template`);
} else {
  const record = readText(xiaomanPreflightRecordPath);
  for (const fragment of [
    "Do not paste secrets, raw chat logs, Feishu Base",
    "QINTOPIA_XIAOMAN_ACTIVITY_PRODUCTION_PREFLIGHT_ENABLE=1",
    "qintopia-agentos-xiaoman-activity-signal-worker.timer",
    "run-xiaoman-activity-signal-worker --once --apply",
    "qintopia-agentos-xiaoman-activity-promotion-starter-worker.timer",
    "run-xiaoman-activity-promotion-starter-worker --once --apply",
    "qintopia-agentos-operations-evidence-worker.timer",
    "run-evidence-worker --once --apply",
    "qintopia-agentos-operations-visual-worker.timer",
    "run-collaboration-worker --work-item-type visual_asset_request --once --apply",
    "qintopia-agentos-xiaoman-activity-image-generation-starter-worker.timer",
    "run-xiaoman-activity-image-generation-starter-worker --once --apply",
    "Huabaosi provider runtime state",
    "run-huabaosi-image-generation-worker --once --dry-run",
    "qintopia-agentos-xiaoman-activity-send-request-starter-worker.timer",
    "run-xiaoman-activity-send-request-starter-worker --once --apply",
    "Secret and external-send scan",
    "send_executed=true",
    "Production boundary",
    "Eligible Xiaoman `event_signals` preview count",
    "Eligible image-generation request preview count",
    "Eligible awaiting publish group message request count",
    "Pass: production observation can continue without executing external adapters",
    "Hold: one or more timers, commands, previews, or boundary checks failed",
    "Passing this preflight does not approve publishing",
  ]) {
    requireFragment(xiaomanPreflightRecordPath, record, fragment);
  }
}

const xiaomanImageStarterObservationPath =
  "deploy/sidecar/scripts/xiaoman-activity-image-generation-starter-observation-smoke.sh";
if (!exists(xiaomanImageStarterObservationPath)) {
  addError(`${xiaomanImageStarterObservationPath}: missing observation smoke`);
} else {
  const smoke = readText(xiaomanImageStarterObservationPath);
  for (const fragment of [
    "QINTOPIA_XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_OBSERVATION_ENABLE",
    "qintopia-agentos-xiaoman-activity-image-generation-starter-worker.service",
    "qintopia-agentos-xiaoman-activity-image-generation-starter-worker.timer",
    "run-xiaoman-activity-image-generation-starter-worker --once --apply",
    "run-xiaoman-activity-image-generation-starter-worker --check-only",
    "OnBootSec=9min",
    "safe_for_chat",
    "QINTOPIA_XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_TIMER_INTERVAL_EXPECTED:-${QINTOPIA_XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_TIMER_INTERVAL:-2min}",
    "--use-feishu-base",
    "send_executed=true",
  ]) {
    requireFragment(xiaomanImageStarterObservationPath, smoke, fragment);
  }
  for (const fragment of [
    "QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE=1",
    "run-huabaosi-image-generation-worker --once --apply",
    "xiaoman-activity shadow-validate",
  ]) {
    forbidFragment(xiaomanImageStarterObservationPath, smoke, fragment);
  }
}

const huabaosiImageProductionObservationPath =
  "deploy/sidecar/scripts/huabaosi-image-generation-production-observation-smoke.sh";
if (!exists(huabaosiImageProductionObservationPath)) {
  addError(`${huabaosiImageProductionObservationPath}: missing observation smoke`);
} else {
  const smoke = readText(huabaosiImageProductionObservationPath);
  for (const fragment of [
    "QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_OBSERVATION_ENABLE",
    "/home/ubuntu/qintopia-agent-os-releases/current",
    "/etc/qintopia/message-sidecar.env",
    'ENV_FILE="/etc/qintopia/message-sidecar.env"',
    'RELEASE_CURRENT_DIR="/home/ubuntu/qintopia-agent-os-releases/current"',
    "sidecar/qintopia-message-sidecar",
    "artifact-manifest.json",
    'PATH="/usr/bin:/bin:/usr/sbin:/sbin"',
    'SYSTEMCTL="/usr/bin/systemctl"',
    '"huabaosi-production-adapter"',
    '"huabaosi-feishu-mirror-adapter"',
    "QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED",
    "QINTOPIA_HUABAOSI_IMAGE_HTTP_TIMEOUT_SECONDS",
    "QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_APPROVAL",
    "QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_DATABASE_URL_SHA256",
    "QINTOPIA_HUABAOSI_MEDIA_MAX_BYTES",
    'PROVIDER_SERVICE_NAME="qintopia-agentos-huabaosi-image-generation-worker.service"',
    'PROVIDER_TIMER_NAME="qintopia-agentos-huabaosi-image-generation-worker.timer"',
    "huabaosi-image-generation-preflight",
    "run-huabaosi-image-generation-worker --once --dry-run",
    "CHILD_ENV",
    "load_observation_env",
    'add_child_env "QINTOPIA_DEPLOYED_COMMIT_SHA" "$RELEASE_SHA"',
    'add_child_env "QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_RELEASE_SHA" "$RELEASE_SHA"',
    'add_child_env "QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA" "$RELEASE_SHA"',
    "env -i",
    'worker_stderr="$tmp_dir/worker-preview.stderr"',
    "worker_status=$?",
    'assert_no_sensitive_output "image worker dry-run stderr"',
    "generation_enabled",
    "adapter_compiled",
    "NextElapseUSecMonotonic",
    "provider timer must have a future trigger",
    "generation_flag//[[:space:]]/",
    "safe_for_chat",
    "contains forbidden sensitive output",
    "--use-feishu-base",
  ]) {
    requireFragment(huabaosiImageProductionObservationPath, smoke, fragment);
  }
  for (const fragment of [
    '"qiwe-production-adapter"',
    "QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_OBSERVATION_TEST_MODE",
    "QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_OBSERVATION_TEST_ROOT",
    "QINTOPIA_RELEASE_CURRENT_DIR",
    "QINTOPIA_SIDECAR_ENV_FILE",
    'SYSTEMCTL="${SYSTEMCTL:-',
    "TEST_MODE=",
    "TEST_ROOT=",
    "run-huabaosi-image-generation-worker --once --apply",
    "generated_image_created",
    "run-group-message-send-worker",
    'source "$ENV_FILE"',
    "QINTOPIA_SIDECAR_SOURCE_DIR",
    "SIDECAR_SOURCE_DIR",
    "SIDECAR_DIR",
    "cargo run",
    "${CARGO:-cargo}",
  ]) {
    forbidFragment(huabaosiImageProductionObservationPath, smoke, fragment);
  }
}

const huabaosiImageProductionCanaryPath =
  "deploy/sidecar/scripts/huabaosi-image-generation-production-canary-smoke.sh";
if (!exists(huabaosiImageProductionCanaryPath)) {
  addError(`${huabaosiImageProductionCanaryPath}: missing production canary command`);
} else {
  const canary = readText(huabaosiImageProductionCanaryPath);
  for (const fragment of [
    "QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_ENABLE",
    "approved-production-image-generation-canary",
    'PREFERRED_REVIEWER_ID="trainer"',
    'REVIEWER_EVIDENCE_ID="allowlisted-production-reviewer"',
    'PRODUCTION_ENV_FILE="/etc/qintopia/message-sidecar.env"',
    'PROVIDER_TIMER="qintopia-agentos-huabaosi-image-generation-worker.timer"',
    'PATH="/usr/bin:/bin:/usr/sbin:/sbin"',
    'SYSTEMCTL="/usr/bin/systemctl"',
    "os.path.realpath",
    "stat.S_ISLNK",
    "load_release_artifact_profile",
    "artifact-manifest.json",
    'artifact_profile != "huabaosi-production"',
    '"huabaosi-production-adapter"',
    '"huabaosi-feishu-mirror-adapter"',
    "requires the immutable Huabaosi production artifact manifest",
    "production canary test mode is forbidden from production release roots",
    "production canary test mode may execute only a temporary fake sidecar",
    'timer_enabled_state="$("$SYSTEMCTL" is-enabled "$PROVIDER_TIMER" 2>/dev/null || true)"',
    '[[ "$timer_enabled_state" != "disabled" ]]',
    "production provider timer must be disabled during one-shot canary",
    'if "$SYSTEMCTL" is-active --quiet "$PROVIDER_TIMER"',
    "production provider timer must be inactive during one-shot canary",
    "production canary sidecar hash does not match",
    "production canary database hash does not match",
    "production environment contains a duplicate canary key",
    "operations-artifact-review-decision --apply",
    '"expected_artifact_type": "poster_brief"',
    '"expected_review_status": "pending"',
    'assert data["artifact_type"] == "poster_brief"',
    'assert data["previous_review_status"] == "pending"',
    'BRIEF_WORK_ITEM_ID="${review_facts[2]}"',
    "run-xiaoman-activity-image-generation-starter-worker --once --apply --work-item-id",
    'assert data["requested_work_item_id"] == sys.argv[1]',
    'assert item["parent_work_item_id"] == sys.argv[1]',
    "run-huabaosi-image-generation-worker --once --work-item-id",
    "huabaosi-feishu-primary-storage-revalidate --artifact-id",
    'assert artifact["review_status"] == "pending"',
    "database_writes_executed",
    "contains sensitive output",
    "one Feishu-backed JPEG remains pending human review",
  ]) {
    requireFragment(huabaosiImageProductionCanaryPath, canary, fragment);
  }
  for (const fragment of [
    'source "$ENV_FILE"',
    "eval ",
    'SYSTEMCTL="systemctl"',
    "systemctl enable",
    "systemctl start",
    'operations-artifact-review-decision --apply --payload-json "{',
    "run-group-message-send-worker",
    "run-qiwe-image-send-worker",
  ]) {
    forbidFragment(huabaosiImageProductionCanaryPath, canary, fragment);
  }
}

const huabaosiWeComGatewayObservationPath =
  "deploy/sidecar/scripts/huabaosi-wecom-gateway-observation-smoke.sh";
if (!exists(huabaosiWeComGatewayObservationPath)) {
  addError(`${huabaosiWeComGatewayObservationPath}: missing observation smoke`);
} else {
  const smoke = readText(huabaosiWeComGatewayObservationPath);
  for (const fragment of [
    "QINTOPIA_HUABAOSI_WECOM_OBSERVATION_ENABLE",
    "hermes-gateway-huabaosi.service",
    '--user is-active "$SERVICE_NAME"',
    '--user show "$SERVICE_NAME" --property=WorkingDirectory --property=ExecStart --property=DropInPaths --property=EnvironmentFiles',
    '--user -u "$SERVICE_NAME"',
    "WorkingDirectory=${PROFILE_DIR}",
    "--profile huabaosi gateway run --replace",
    "/home/ubuntu/.config/systemd/user/hermes-gateway-huabaosi.service.d/env.conf",
    "/home/ubuntu/.hermes/profiles/huabaosi/.env (ignore_errors=no)",
    "single reviewed environment drop-in",
    "fixed profile environment file",
    "busy_input_mode",
    'PATH="/usr/bin:/bin"',
    'SERVICE_NAME="hermes-gateway-huabaosi.service"',
    'PROFILE_DIR="/home/ubuntu/.hermes/profiles/huabaosi"',
    'RELEASE_CURRENT="/home/ubuntu/qintopia-agent-os-releases/current"',
    'SYSTEMCTL="/usr/bin/systemctl"',
    'JOURNALCTL="/usr/bin/journalctl"',
    'JOURNAL_LINES="160"',
    'JOURNAL_SINCE="30 minutes ago"',
    '--since "$JOURNAL_SINCE" -n "$JOURNAL_LINES"',
    "journal_window=30m",
    "internal_filter_count",
    "send_fallback_count",
    "api_timeout_count",
    "contains forbidden sensitive output",
  ]) {
    requireFragment(huabaosiWeComGatewayObservationPath, smoke, fragment);
  }
  for (const fragment of [
    "systemctl restart",
    "systemctl reload",
    "systemctl start",
    "systemctl enable",
    'cat "$SERVICE_NAME"',
    'source "$ENV_FILE"',
    ". /etc/qintopia/message-sidecar.env",
    "run-huabaosi-image-generation-worker",
    "run-group-message-send-worker",
    "--apply",
    "QINTOPIA_HUABAOSI_WECOM_JOURNAL_LINES",
    "QINTOPIA_HUABAOSI_WECOM_JOURNAL_SINCE",
    "QINTOPIA_HUABAOSI_WECOM_SERVICE_NAME",
    "QINTOPIA_HUABAOSI_WECOM_PROFILE_DIR",
    "QINTOPIA_HUABAOSI_WECOM_PROFILE_CONFIG",
    "QINTOPIA_RELEASE_CURRENT_PATH",
    "${SYSTEMCTL:-",
    "${JOURNALCTL:-",
  ]) {
    forbidFragment(huabaosiWeComGatewayObservationPath, smoke, fragment);
  }
}

const huabaosiWeComCanaryObservationPath =
  "deploy/sidecar/scripts/huabaosi-wecom-canary-observation-smoke.sh";
if (!exists(huabaosiWeComCanaryObservationPath)) {
  addError(`${huabaosiWeComCanaryObservationPath}: missing observation smoke`);
} else {
  const smoke = readText(huabaosiWeComCanaryObservationPath);
  for (const fragment of [
    "QINTOPIA_HUABAOSI_WECOM_CANARY_OBSERVATION_ENABLE",
    "qintopia-agentos-huabaosi-wecom-canary-gateway.service",
    "qintopia-agentos-huabaosi-wecom-canary-gateway.timer",
    "huabaosi-wecom-canary-preflight",
    "canary_enabled",
    "staging_adapter_not_compiled",
    "canary_configuration_not_approved",
    "QINTOPIA_HUABAOSI_WECOM_CANARY_TOKEN",
    "${MONOREPO_ROOT}/sidecar/qintopia-message-sidecar",
    "contains forbidden sensitive output",
  ]) {
    requireFragment(huabaosiWeComCanaryObservationPath, smoke, fragment);
  }
  for (const fragment of [
    "huabaosi-wecom-canary-gateway --apply",
    "systemctl restart",
    "systemctl reload",
    "systemctl start",
    "systemctl enable",
    'source "$ENV_FILE"',
    ". /etc/qintopia/message-sidecar.env",
    "run-huabaosi-image-generation-worker",
    "run-group-message-send-worker",
  ]) {
    forbidFragment(huabaosiWeComCanaryObservationPath, smoke, fragment);
  }
}

for (const observationPath of [
  "deploy/sidecar/scripts/operations-downstream-timers-observation-smoke.sh",
  "deploy/sidecar/scripts/operations-group-send-ready-timer-observation-smoke.sh",
  "deploy/sidecar/scripts/xiaoman-activity-downstream-observation-smoke.sh",
  "deploy/sidecar/scripts/xiaoman-activity-image-generation-starter-observation-smoke.sh",
  "deploy/sidecar/scripts/huabaosi-image-generation-production-observation-smoke.sh",
  "deploy/sidecar/scripts/huabaosi-wecom-canary-observation-smoke.sh",
  "deploy/sidecar/scripts/huabaosi-wecom-gateway-observation-smoke.sh",
  "deploy/sidecar/scripts/xiaoman-activity-promotion-starter-timer-observation-smoke.sh",
  "deploy/sidecar/scripts/xiaoman-activity-send-request-starter-observation-smoke.sh",
  "deploy/sidecar/scripts/xiaoman-activity-signal-timer-observation-smoke.sh",
]) {
  const smoke = exists(observationPath) ? readText(observationPath) : "";
  requireFragment(observationPath, smoke, "contains forbidden sensitive output");
  forbidFragment(observationPath, smoke, "leaked forbidden output: ${token}");
}

const aliangStagingSmokePath =
  "deploy/sidecar/scripts/huabaosi-image-generation-staging-smoke.sh";
const aliangStagingReadinessPath =
  "deploy/sidecar/scripts/huabaosi-image-generation-staging-readiness-smoke.sh";
const stagingRuntimePrerequisiteObservationPath =
  "deploy/sidecar/scripts/staging-runtime-prerequisite-observation-smoke.sh";
const stagingRuntimeValuesObservationPath =
  "deploy/sidecar/scripts/staging-runtime-values-observation-smoke.sh";
if (!exists(stagingRuntimeValuesObservationPath)) {
  addError(
    `${stagingRuntimeValuesObservationPath}: missing staging runtime values observation smoke`
  );
} else {
  const observation = readText(stagingRuntimeValuesObservationPath);
  for (const fragment of [
    "QINTOPIA_STAGING_RUNTIME_VALUES_OBSERVATION_ENABLE",
    "QINTOPIA_STAGING_RUNTIME_VALUES_OBSERVATION_TEST_MODE",
    "/etc/qintopia/message-sidecar-staging-values.json",
    "/etc/qintopia/message-sidecar-staging.env",
    "deploy/sidecar/scripts/render-staging-runtime-env.py",
    "staging_runtime_values_observation=",
    "ready_for_render_validation",
    "server-local values file contents are not read",
    "staging env file contents are not read",
    "renderer is not executed",
    "no Postgres, Huabaosi, Feishu, QiWe, provider, media, service, timer, release, or network action",
    "values_file_present",
    "env_file_already_present",
    "path_parent_is_symlink",
    "path_parent_missing",
    "path_group_or_world_writable",
    "path_group_or_world_readable",
  ]) {
    requireFragment(stagingRuntimeValuesObservationPath, observation, fragment);
  }
  for (const fragment of [
    "systemctl",
    "source ",
    'source "$',
    ". /etc/qintopia",
    "env -i",
    "QINTOPIA_SIDECAR_DATABASE_URL",
    "QINTOPIA_HUABAOSI_IMAGE_API_KEY",
    "QIWE_TOKEN",
    "run-huabaosi-image-generation-worker",
    "run-qiwe-image-send-worker",
    "curl ",
    "psql ",
  ]) {
    forbidFragment(stagingRuntimeValuesObservationPath, observation, fragment);
  }
}
if (!exists(stagingRuntimePrerequisiteObservationPath)) {
  addError(
    `${stagingRuntimePrerequisiteObservationPath}: missing staging runtime prerequisite observation smoke`
  );
} else {
  const observation = readText(stagingRuntimePrerequisiteObservationPath);
  for (const fragment of [
    "QINTOPIA_STAGING_RUNTIME_PREREQUISITE_OBSERVATION_ENABLE",
    "QINTOPIA_STAGING_RUNTIME_PREREQUISITE_OBSERVATION_TEST_MODE",
    "/etc/qintopia/message-sidecar-staging.env",
    "/home/ubuntu/qintopia-agent-os-staging-releases",
    "QINTOPIA_STAGING_RUNTIME_PREREQUISITE_RELEASE_SHA",
    "QINTOPIA_STAGING_RUNTIME_PREREQUISITE_SIDECAR_SHA256",
    "staging_runtime_prerequisite_observation=",
    "ready_for_staging_readiness_smokes",
    "staging env file contents are not read",
    "sidecar binary is not executed",
    "no Postgres, Huabaosi, Feishu, QiWe, provider, media, service, timer, release, or network action",
    "path_is_secure",
    "require_executable",
    "os.access(path, os.X_OK)",
    "path_not_executable",
    "reject_owner_writable",
    "path_owner_group_or_world_writable",
    "path_group_or_world_writable",
    "path_is_symlink",
    "path_parent_is_symlink",
    "path_parent_group_or_world_writable",
    "path_parent_unexpected_owner",
    "sidecar_hash_mismatch",
  ]) {
    requireFragment(stagingRuntimePrerequisiteObservationPath, observation, fragment);
  }
  for (const fragment of [
    "systemctl",
    "source ",
    'source "$',
    ". /etc/qintopia",
    "env -i",
    "QINTOPIA_SIDECAR_DATABASE_URL",
    "QINTOPIA_HUABAOSI_IMAGE_API_KEY",
    "QIWE_TOKEN",
    "run-huabaosi-image-generation-worker",
    "huabaosi-image-generation-preflight",
    "curl ",
    "psql ",
  ]) {
    forbidFragment(stagingRuntimePrerequisiteObservationPath, observation, fragment);
  }
}
if (!exists(aliangStagingReadinessPath)) {
  addError(`${aliangStagingReadinessPath}: missing Huabaosi staging readiness smoke`);
} else {
  const readiness = readText(aliangStagingReadinessPath);
  for (const fragment of [
    "QINTOPIA_HUABAOSI_IMAGE_STAGING_READINESS_ENABLE",
    "QINTOPIA_HUABAOSI_IMAGE_STAGING_APPROVAL",
    "approved-staging-image-generation",
    "/etc/qintopia/message-sidecar-staging.env",
    "/home/ubuntu/qintopia-agent-os-staging-releases",
    "QINTOPIA_HUABAOSI_IMAGE_STAGING_RELEASE_SHA",
    "QINTOPIA_HUABAOSI_IMAGE_STAGING_SIDECAR_SHA256",
    "huabaosi_image_generation_staging_readiness=",
    "ready_for_staging_preflight",
    "staging env file contents are not read",
    "sidecar binary is not executed",
    "no Huabaosi, Postgres, Feishu, QiWe, provider, media, service, or timer action",
    "path_is_secure",
    "require_executable",
    "os.access(path, os.X_OK)",
    "path_not_executable",
    "reject_owner_writable",
    "path_owner_group_or_world_writable",
    "path_group_or_world_writable",
    "path_is_symlink",
    "path_parent_is_symlink",
    "path_parent_group_or_world_writable",
    "path_parent_unexpected_owner",
    "path_unexpected_owner",
    "sidecar_hash_mismatch",
  ]) {
    requireFragment(aliangStagingReadinessPath, readiness, fragment);
  }
  for (const fragment of [
    "systemctl",
    "source ",
    'source "$',
    ". /etc/qintopia",
    "env -i",
    "QINTOPIA_SIDECAR_DATABASE_URL",
    "QINTOPIA_HUABAOSI_IMAGE_API_KEY",
    "run-huabaosi-image-generation-worker",
    "huabaosi-image-generation-preflight",
    "subprocess",
    "curl ",
    "psql ",
  ]) {
    forbidFragment(aliangStagingReadinessPath, readiness, fragment);
  }
}

if (!exists(aliangStagingSmokePath)) {
  addError(`${aliangStagingSmokePath}: missing Huabaosi staging smoke`);
} else {
  const smoke = readText(aliangStagingSmokePath);
  for (const fragment of [
    "QINTOPIA_HUABAOSI_IMAGE_STAGING_SMOKE_ENABLE",
    "QINTOPIA_HUABAOSI_IMAGE_STAGING_APPROVAL",
    "approved-staging-image-generation",
    "QINTOPIA_HUABAOSI_IMAGE_STAGING_ENV_FILE",
    "QINTOPIA_HUABAOSI_IMAGE_STAGING_DATABASE_URL_SHA256",
    "QINTOPIA_HUABAOSI_IMAGE_STAGING_RELEASE_SHA",
    "QINTOPIA_HUABAOSI_IMAGE_STAGING_SIDECAR_SHA256",
    "QINTOPIA_HUABAOSI_IMAGE_STAGING_WORK_ITEM_ID",
    "QINTOPIA_HUABAOSI_IMAGE_STAGING_DATABASE_URL_SHA256 must be a canonical SHA-256",
    "QINTOPIA_HUABAOSI_IMAGE_STAGING_RELEASE_SHA must be a 40-character lowercase hex SHA",
    "QINTOPIA_HUABAOSI_IMAGE_STAGING_SIDECAR_SHA256 must be a canonical SHA-256",
    "QINTOPIA_HUABAOSI_IMAGE_STAGING_SMOKE_TEST_MODE must be 0 or 1",
    "Huabaosi staging smoke must run from /home/ubuntu/qintopia-agent-os-staging-releases/<approved 40-hex sha>",
    "Huabaosi staging smoke test mode may read only a temporary fake env file",
    "QINTOPIA_SIDECAR_BIN is test-only and must not override the fixed staging release sidecar",
    "packaged sidecar/qintopia-message-sidecar or QINTOPIA_SIDECAR_BIN is required for Huabaosi staging smoke",
    "verify_sidecar_binary",
    "staging sidecar binary hash changed before",
    "staging sidecar binary must stay under the fixed staging release root before",
    "staging sidecar binary must come from /home/ubuntu/qintopia-agent-os-staging-releases/<approved 40-hex sha> before",
    "candidate_lstat.st_uid == os.geteuid()",
    "staging sidecar binary, parent directory, and release ancestors must not be writable by the staging runner or by group/world before",
    '(release_root_parent, "directory", True)',
    '(root, "directory", True)',
    "sidecar_binary_sha256",
    "STAGING_ENV_KEYS",
    "IGNORED_STAGING_ENV_KEYS",
    "load_staging_env",
    "QINTOPIA_QIWE_IMAGE_SEND_ENABLED",
    "QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS",
    "staging database URL hash does not match the approved command",
    "CHILD_ENV",
    "add_child_env",
    "add_child_env_if_set",
    "env -i",
    'verify_sidecar_binary "$label spawn"',
    'output="$(env -i "${CHILD_ENV[@]}" "$@" 2>&1)"',
    "assert_no_sensitive_text",
    'payload["adapter_compiled"] is True',
    "huabaosi-image-generation-preflight",
    "run-huabaosi-image-generation-worker",
    "generated_image_created",
    "pending",
    "huabaosi_image_generation_staging_evidence=",
    "emit_sanitized_evidence",
    "payload = json.load(sys.stdin)",
    "database_url_sha256",
    "content_hash",
    "mime_type",
    "storage_backend",
    "feishu-base",
    'urlparse(artifact["artifact_uri"]).scheme',
    "generated image storage boundary is not Feishu Base",
    "QINTOPIA_HUABAOSI_IMAGE_STORAGE_BACKEND",
    "QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED",
    "QINTOPIA_HUABAOSI_FEISHU_MIRROR_APPROVAL",
    "QINTOPIA_HUABAOSI_FEISHU_BASE_TOKEN",
    "QINTOPIA_HUABAOSI_FEISHU_ARTIFACT_TABLE_ID",
    'hashlib.sha256(value.encode("utf-8")).hexdigest()',
    "urlparse(value).path",
  ]) {
    requireFragment(aliangStagingSmokePath, smoke, fragment);
  }

  for (const fragment of [
    "systemctl",
    'source "$ENV_FILE"',
    ". /etc/qintopia/message-sidecar-staging.env",
    "mktemp",
    "preflight_output",
    "worker_output",
    '>"$preflight_output"',
    '>"$worker_output"',
    'python3 - "$QINTOPIA_SIDECAR_DATABASE_URL"',
    "run-group-message-send-worker",
    "--use-feishu-base",
    "send-ready",
    "operations-group-message-confirm",
    "SANITIZED_EVIDENCE_PAYLOAD",
    "json.loads(os.environ",
    "QINTOPIA_HUABAOSI_MEDIA_UPLOAD_ENDPOINT",
    "QINTOPIA_HUABAOSI_MEDIA_PUBLIC_BASE_URL",
  ]) {
    forbidFragment(aliangStagingSmokePath, smoke, fragment);
  }
}

const aliangStagingEvidenceCheckPath =
  "tools/deploy/check-huabaosi-image-staging-evidence.mjs";
if (!exists(aliangStagingEvidenceCheckPath)) {
  addError(
    `${aliangStagingEvidenceCheckPath}: missing Huabaosi staging evidence checker`
  );
} else {
  const checker = readText(aliangStagingEvidenceCheckPath);
  for (const fragment of [
    "huabaosi_image_generation_staging_evidence=",
    "Huabaosi image staging evidence check passed.",
    "expected exactly two Huabaosi staging evidence records",
    "expected one preflight and one generation evidence record",
    "generation evidence does not prove one pending final JPEG",
    "artifact_uri",
    "https?:",
    "storage_backend",
    "feishu-base",
    "staging sidecar binary hash is missing or inconsistent",
  ]) {
    requireFragment(aliangStagingEvidenceCheckPath, checker, fragment);
  }
}

const aliangStagingEvidenceTemplatePath =
  "docs/reports/templates/huabaosi-image-generation-staging-evidence.md";
if (!exists(aliangStagingEvidenceTemplatePath)) {
  addError(
    `${aliangStagingEvidenceTemplatePath}: missing Huabaosi staging evidence template`
  );
} else {
  const template = readText(aliangStagingEvidenceTemplatePath);
  for (const fragment of [
    "node tools/deploy/check-huabaosi-image-staging-evidence.mjs <huabaosi-staging-evidence-output.txt>",
    "Repository commit SHA",
    "Packaged sidecar binary SHA-256",
    "Staging database URL SHA-256",
    "Image request work item UUID",
    "Final JPEG `content_hash`",
    "Review status: `pending`",
    "`adapter_config_ready`",
    "`generated_image_created`",
    "External provider call",
    "Feishu Base write",
    "QiWe send",
    "`database_url_sha256`",
    "`sidecar_binary_sha256`",
    "`content_hash`",
    "`mime_type`: `image/jpeg`",
    "`storage_backend`: `feishu-base`",
    "Complete Huabaosi evidence checker passed",
    "QiWe staging send must wait for manual approval revalidation and combined",
    "Feishu-to-QiWe bridge evidence",
    "no QiWe send, production timer, service, Release publish",
    "Do not record provider endpoint, provider response, API key, token, database URL",
  ]) {
    requireFragment(aliangStagingEvidenceTemplatePath, template, fragment);
  }
  for (const fragment of [
    "QINTOPIA_HUABAOSI_IMAGE_API_KEY=",
    "postgres://",
    "postgresql://",
    "https://",
    "artifact_uri",
    "filename",
    "systemctl enable",
    "systemctl start",
    "gh release",
  ]) {
    forbidFragment(aliangStagingEvidenceTemplatePath, template, fragment);
  }
}

const stagingRuntimeProvisioningRunbookPath =
  "docs/operations/staging-runtime-provisioning-runbook.md";
if (!exists(stagingRuntimeProvisioningRunbookPath)) {
  addError(
    `${stagingRuntimeProvisioningRunbookPath}: missing staging runtime provisioning runbook`
  );
} else {
  const runbook = readText(stagingRuntimeProvisioningRunbookPath);
  for (const fragment of [
    "/etc/qintopia/message-sidecar-staging.env",
    "/home/ubuntu/qintopia-agent-os-staging-releases/<40-hex-sha>/sidecar/qintopia-message-sidecar",
    "QINTOPIA_STAGING_RUNTIME_PREREQUISITE_OBSERVATION_ENABLE=1",
    "staging-runtime-prerequisite-observation-smoke.sh",
    "QINTOPIA_STAGING_RUNTIME_VALUES_OBSERVATION_ENABLE=1",
    "staging-runtime-values-observation-smoke.sh",
    "ready_for_render_validation",
    "QINTOPIA_STAGING_RUNTIME_READINESS_EVIDENCE_ENABLE=1",
    "staging-runtime-readiness-evidence-smoke.sh",
    "staging_runtime_readiness_evidence=",
    "ready_for_huabaosi_qiwe_staging_smokes",
    "render-staging-runtime-env.py",
    "staging_runtime_env_render=",
    "/etc/qintopia/message-sidecar-staging-values.json",
    "approved-staging-runtime-env-provision",
    "mode `0600`",
    "requires exactly one isolated staging group id",
    "docs/reports/2026-07-16-staging-runtime-prerequisite-observation.md",
    "staging release SHA",
    "packaged staging sidecar SHA-256",
    "staging database URL SHA-256",
    "Huabaosi staging keys",
    "Downstream QiWe staging keys",
    "QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED",
    "QINTOPIA_HUABAOSI_IMAGE_API_KEY",
    "QINTOPIA_HUABAOSI_IMAGE_STORAGE_BACKEND=feishu-base",
    "QINTOPIA_HUABAOSI_FEISHU_SCHEMA_VERSION=huabaosi-generated-image-v1",
    "QINTOPIA_QIWE_IMAGE_SEND_ENABLED",
    "QIWE_TOKEN",
    "QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS",
    "staging env file already contains the downstream QiWe keys",
    "ignore those keys and must not pass them to its child sidecar process",
    "invalid assignment syntax still fail closed",
    "QINTOPIA_STAGING_SIDECAR_PROVISION_APPROVAL=approved-staging-sidecar-provision",
    "fetch-staging-sidecar-artifact.sh",
    "--sha '<approved staging release sha>'",
    "do not provision the older `37fff8bf...` staging binary",
    "bridge exercise",
    "first build or fetch a successful staging-only artifact",
    "release SHA and record its reviewed sidecar SHA-256",
    "Previous staging artifact evidence, retained only as historical proof",
    "deploy bundle contains `deploy/sidecar/scripts/fetch-staging-sidecar-artifact.sh`",
    "no checked path component is a symlink",
    "no checked path component is group- or world-writable",
    "staging database URL hash is absent",
    "node tools/deploy/check-huabaosi-image-staging-evidence.mjs",
    "docs/reports/templates/huabaosi-image-generation-staging-evidence.md",
    "This runbook is not production enablement",
    "enable a listener",
  ]) {
    requireFragment(stagingRuntimeProvisioningRunbookPath, runbook, fragment);
  }
  for (const fragment of [
    "QINTOPIA_HUABAOSI_IMAGE_API_KEY=",
    "QIWE_TOKEN=",
    "QIWE_GUID=",
    "postgres://",
    "postgresql://",
    "systemctl enable",
    "systemctl start",
    "gh release",
    'source "$',
    ". /etc/qintopia/message-sidecar-staging.env",
  ]) {
    forbidFragment(stagingRuntimeProvisioningRunbookPath, runbook, fragment);
  }
}

const aliangStagingSmokeTestPath = "tools/deploy/test-huabaosi-image-staging-smoke.mjs";
const aliangProductionCanaryTestPath =
  "tools/deploy/test-huabaosi-image-production-canary.mjs";
const huabaosiProductionCanaryEvidenceCheckPath =
  "tools/deploy/check-huabaosi-image-production-canary-evidence.mjs";
const huabaosiProductionCanaryEvidenceTestPath =
  "tools/deploy/test-huabaosi-image-production-canary-evidence.mjs";
const aliangStagingReadinessTestPath =
  "tools/deploy/test-huabaosi-image-staging-readiness.mjs";
const stagingRuntimePrerequisiteObservationTestPath =
  "tools/deploy/test-staging-runtime-prerequisite-observation.mjs";
const stagingRuntimeValuesObservationTestPath =
  "tools/deploy/test-staging-runtime-values-observation.mjs";
const stagingRuntimeReadinessEvidenceTestPath =
  "tools/deploy/test-staging-runtime-readiness-evidence.mjs";
const stagingRuntimeEnvRenderPath =
  "deploy/sidecar/scripts/render-staging-runtime-env.py";
const stagingRuntimeEnvRenderTestPath =
  "tools/deploy/test-staging-runtime-env-render.mjs";
if (!exists(stagingRuntimePrerequisiteObservationTestPath)) {
  addError(
    `${stagingRuntimePrerequisiteObservationTestPath}: missing staging runtime prerequisite observation test`
  );
} else {
  const test = readText(stagingRuntimePrerequisiteObservationTestPath);
  for (const fragment of [
    "staging-runtime-prerequisite-observation-smoke.sh",
    "staging_runtime_prerequisite_observation=",
    "ready_for_staging_readiness_smokes",
    "Staging runtime prerequisite observation smoke test passed.",
    "staging-prereq-secret-must-not-appear",
    "env_file_path_parent_is_symlink",
    "owner-executable observation should not fail",
    "non-executable observation should not fail",
    "sidecar_binary_path_not_executable",
    "sidecar_hash_mismatch",
  ]) {
    requireFragment(stagingRuntimePrerequisiteObservationTestPath, test, fragment);
  }
}
if (!exists(stagingRuntimeValuesObservationTestPath)) {
  addError(
    `${stagingRuntimeValuesObservationTestPath}: missing staging runtime values observation test`
  );
} else {
  const test = readText(stagingRuntimeValuesObservationTestPath);
  for (const fragment of [
    "staging-runtime-values-observation-smoke.sh",
    "staging_runtime_values_observation=",
    "ready_for_render_validation",
    "rendered_env_already_present",
    "Staging runtime values observation smoke test passed.",
    "staging-values-secret-must-not-appear",
    "values_file_path_parent_is_symlink",
    "values_file_path_parent_missing",
    "env_file_path_parent_missing",
    "renderer_path_parent_missing",
    "values_file_path_group_or_world_writable",
    "values_file_path_group_or_world_readable",
    "env_file_path_group_or_world_readable",
  ]) {
    requireFragment(stagingRuntimeValuesObservationTestPath, test, fragment);
  }
}
if (!exists(stagingRuntimeReadinessEvidenceTestPath)) {
  addError(
    `${stagingRuntimeReadinessEvidenceTestPath}: missing staging runtime readiness evidence test`
  );
} else {
  const test = readText(stagingRuntimeReadinessEvidenceTestPath);
  for (const fragment of [
    "staging-runtime-readiness-evidence-smoke.sh",
    "staging_runtime_readiness_evidence=",
    "ready_for_huabaosi_qiwe_staging_smokes",
    "staging-runtime-evidence-secret-must-not-appear",
    "QINTOPIA_STAGING_RUNTIME_DATABASE_URL_SHA256",
    "hash mismatch evidence is invalid",
    "expected missing staging database hash to fail",
    "Staging runtime readiness evidence smoke test passed.",
  ]) {
    requireFragment(stagingRuntimeReadinessEvidenceTestPath, test, fragment);
  }
}
if (!exists(stagingRuntimeEnvRenderPath)) {
  addError(`${stagingRuntimeEnvRenderPath}: missing staging runtime env renderer`);
} else {
  const script = readText(stagingRuntimeEnvRenderPath);
  for (const fragment of [
    "staging_runtime_env_render=",
    "approved-staging-runtime-env-provision",
    "/etc/qintopia/message-sidecar-staging.env",
    "message-sidecar-staging-values.json",
    "QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED",
    "QINTOPIA_HUABAOSI_IMAGE_STORAGE_BACKEND",
    "QINTOPIA_HUABAOSI_FEISHU_SCHEMA_VERSION",
    "QINTOPIA_HUABAOSI_FEISHU_ALLOWED_BASE_TOKENS",
    "QINTOPIA_HUABAOSI_FEISHU_ALLOWED_ARTIFACT_TABLE_IDS",
    "QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS",
    "media_host_count",
    "contains a duplicate host entry",
    "contains a port outside 1-65535",
    "QINTOPIA_QIWE_IMAGE_SEND_WEBHOOK_READY",
    "QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS must contain exactly one isolated group",
    "QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA must match QINTOPIA_DEPLOYED_COMMIT_SHA",
    "QINTOPIA_HUABAOSI_FEISHU_DATABASE_URL_SHA256 must match the approved staging database hash",
    "staging database URL hash does not match approved hash",
    "validate_protected_output_boundary",
    "os.lstat(component)",
    "protected output path component must not be a symlink",
    "protected output path component must be root-owned",
    "reject_existing_output",
    "output parent directory must not be a symlink",
    "output mode is 0600 on apply",
    "server-local values file is never printed",
    "no provider, media, Postgres, Feishu, QiWe, service, timer, or release action",
  ]) {
    requireFragment(stagingRuntimeEnvRenderPath, script, fragment);
  }
  for (const fragment of [
    "print(content)",
    "systemctl",
    "gh release",
    "subprocess",
    "requests",
    "urllib.request",
    "QINTOPIA_HUABAOSI_MEDIA_UPLOAD_ENDPOINT",
    "QINTOPIA_HUABAOSI_MEDIA_PUBLIC_BASE_URL",
  ]) {
    forbidFragment(stagingRuntimeEnvRenderPath, script, fragment);
  }
}
if (!exists(stagingRuntimeEnvRenderTestPath)) {
  addError(
    `${stagingRuntimeEnvRenderTestPath}: missing staging runtime env renderer test`
  );
} else {
  const test = readText(stagingRuntimeEnvRenderTestPath);
  for (const fragment of [
    "render-staging-runtime-env.py",
    "render-secret-must-not-appear",
    "staging_runtime_env_render=",
    "staging_env_render_ready",
    "staging_env_written",
    "unsupported keys",
    "hash does not match",
    "release SHA mismatch failure invalid",
    "duplicate host failure invalid",
    "invalid host port failure invalid",
    "QINTOPIA_HUABAOSI_IMAGE_STORAGE_BACKEND",
    "QINTOPIA_HUABAOSI_FEISHU_SCHEMA_VERSION",
    "QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS=media.example.test,cloud.example.test",
    "report.media_host_count !== 2",
    "non-test output guard invalid",
    "symlink parent guard invalid",
    "Staging runtime env render test passed.",
  ]) {
    requireFragment(stagingRuntimeEnvRenderTestPath, test, fragment);
  }
}
if (!exists(aliangStagingReadinessTestPath)) {
  addError(
    `${aliangStagingReadinessTestPath}: missing Huabaosi staging readiness test`
  );
} else {
  const test = readText(aliangStagingReadinessTestPath);
  for (const fragment of [
    "huabaosi-image-generation-staging-readiness-smoke.sh",
    "huabaosi_image_generation_staging_readiness=",
    "Huabaosi image staging readiness smoke test passed.",
    "readiness smoke exposed staging env contents",
    "expected owner-writable sidecar to fail readiness",
    "expected symlink parent path to fail readiness",
    "expected owner-executable sidecar to pass readiness",
    "expected non-executable sidecar to fail readiness",
    "env_file_path_parent_is_symlink",
    "sidecar_binary_path_not_executable",
    "expected sidecar hash mismatch to fail",
  ]) {
    requireFragment(aliangStagingReadinessTestPath, test, fragment);
  }
}

if (!exists(aliangStagingSmokeTestPath)) {
  addError(`${aliangStagingSmokeTestPath}: missing Huabaosi staging smoke test`);
} else {
  const test = readText(aliangStagingSmokeTestPath);
  for (const fragment of [
    "env file command was executed",
    "ambient secret reached child process",
    "staging-qiwe-token-must-not-reach-huabaosi-child",
    "staging database URL hash does not match the approved command",
    "staging env contains an unsupported key",
    "qintopia-agent-os-staging-releases/<approved 40-hex sha>",
    "contains forbidden sensitive output",
    "staging sidecar binary hash changed before",
    "sidecar_binary_sha256",
    "check-huabaosi-image-staging-evidence.mjs",
    "huabaosi_image_generation_staging_evidence=",
    "raw-huabaosi-staging-evidence.txt",
    "Huabaosi image staging smoke test passed.",
  ]) {
    requireFragment(aliangStagingSmokeTestPath, test, fragment);
  }
}

if (!exists(aliangProductionCanaryTestPath)) {
  addError(
    `${aliangProductionCanaryTestPath}: missing Huabaosi production canary test`
  );
} else {
  const test = readText(aliangProductionCanaryTestPath);
  for (const fragment of [
    "huabaosi-image-generation-production-canary-smoke.sh",
    '"reviewer_id":"trainer"',
    "expected five sidecar commands",
    "timer must be disabled during one-shot canary",
    "timer must be inactive during one-shot canary",
    "masked provider timer must block one-shot production canary",
    "static provider timer must block one-shot production canary",
    "test mode must be rejected from production release roots",
    "test mode must reject non-temporary sidecar paths",
    "test mode must reject symlink sidecar paths",
    "test mode must reject symlink env file paths",
    "ambient QiWe credential reached Huabaosi child",
    "invalid production canary brief UUID must fail closed",
    "production canary should accept an existing allowlisted reviewer",
    "production canary did not redact the allowlisted reviewer",
    "missing artifact manifest must block production canary",
    "wrong artifact manifest profile must block production canary",
    "starter parent work item mismatch must fail before generation",
    "duplicate production canary env key must fail closed",
    "revalidation identity mismatch must block canary completion",
    "sensitive child output must block production canary",
    "one Feishu-backed JPEG remains pending human review",
    "Huabaosi image production canary test passed.",
  ]) {
    requireFragment(aliangProductionCanaryTestPath, test, fragment);
  }
}

if (!exists(huabaosiProductionCanaryEvidenceCheckPath)) {
  addError(
    `${huabaosiProductionCanaryEvidenceCheckPath}: missing Huabaosi production canary evidence checker`
  );
} else {
  const checker = readText(huabaosiProductionCanaryEvidenceCheckPath);
  for (const fragment of [
    "huabaosi_image_generation_production_canary_evidence=",
    "Huabaosi production canary passed: one Feishu-backed JPEG remains pending human review; no generated-image approval, mirror, publish, QiWe, or send was executed",
    "preflight",
    "brief_review",
    "request_intake",
    "generation",
    "revalidation",
    "release_binary_verified",
    "artifact_profile",
    "approved_sidecar_sha256_matched",
    "approved_database_url_sha256_matched",
    'record.artifact_profile !== "huabaosi-production"',
    'reviewer_id !== "allowlisted-production-reviewer"',
    'generation.review_status !== "pending"',
    'generation.storage_backend !== "feishu-base"',
    "preflight.timer_enabled !== false",
    "revalidation.database_writes_executed !== false",
    "revalidation.external_calls_executed !== true",
    "revalidation.sensitive_fields_redacted !== true",
    '"artifact_uri"',
    '"provider_response"',
    '"target_group_id"',
    "authenticated same-byte readback",
    "Huabaosi image production canary evidence check passed.",
  ]) {
    requireFragment(huabaosiProductionCanaryEvidenceCheckPath, checker, fragment);
  }
}

if (!exists(huabaosiProductionCanaryEvidenceTestPath)) {
  addError(
    `${huabaosiProductionCanaryEvidenceTestPath}: missing Huabaosi production canary evidence checker test`
  );
} else {
  const test = readText(huabaosiProductionCanaryEvidenceTestPath);
  for (const fragment of [
    "check-huabaosi-image-production-canary-evidence.mjs",
    "hash-mismatch.txt",
    "raw-secret.txt",
    "missing-phase.txt",
    "request-drift.txt",
    "send-leak.txt",
    "redaction-mismatch.txt",
    "timer-enabled-mismatch.txt",
    "missing-completion.txt",
    "mutable-boundary.txt",
    "sidecar-boundary.txt",
    "database-boundary.txt",
    "profile-boundary.txt",
    'artifact_profile: "huabaosi-production"',
    "authenticated same-byte readback",
    "forbidden sensitive fragment",
    "exactly five fixed phase records",
    "does not bind the approved brief",
    "unexpected key",
    "Huabaosi image production canary evidence test passed.",
  ]) {
    requireFragment(huabaosiProductionCanaryEvidenceTestPath, test, fragment);
  }
}

const aliangProductionActivationPath =
  "deploy/sidecar/scripts/activate-huabaosi-image-generation-production.sh";
if (!exists(aliangProductionActivationPath)) {
  addError(`${aliangProductionActivationPath}: missing production activation command`);
} else {
  const activation = readText(aliangProductionActivationPath);
  for (const fragment of [
    "QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_ACTIVATION",
    "approved-production-image-generation",
    'PATH="/usr/bin:/bin:/usr/sbin:/sbin"',
    'SYSTEMCTL="/usr/bin/systemctl"',
    "qintopia-agentos-huabaosi-image-generation-preflight.service",
    "qintopia-agentos-huabaosi-image-generation-worker.timer",
    '"$SYSTEMCTL" start "$PREFLIGHT_SERVICE"',
    '"$SYSTEMCTL" enable "$WORKER_TIMER"',
    '"$SYSTEMCTL" restart "$WORKER_TIMER"',
    '"$SYSTEMCTL" is-enabled --quiet "$WORKER_TIMER"',
    '"$SYSTEMCTL" is-active --quiet "$WORKER_TIMER"',
    "NextElapseUSecMonotonic",
  ]) {
    requireFragment(aliangProductionActivationPath, activation, fragment);
  }
  for (const fragment of [
    "run-huabaosi-image-generation-worker",
    "--apply",
    "source ",
    "QIWE_",
    "FEISHU_",
    'SYSTEMCTL="${SYSTEMCTL:-systemctl}"',
    'SYSTEMCTL="systemctl"',
  ]) {
    forbidFragment(aliangProductionActivationPath, activation, fragment);
  }
}

const aliangProductionRollbackPath =
  "deploy/sidecar/scripts/rollback-huabaosi-image-generation-production.sh";
if (!exists(aliangProductionRollbackPath)) {
  addError(`${aliangProductionRollbackPath}: missing production rollback command`);
} else {
  const rollback = readText(aliangProductionRollbackPath);
  for (const fragment of [
    "QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_ROLLBACK",
    "approved-production-image-generation-rollback",
    'PATH="/usr/bin:/bin:/usr/sbin:/sbin"',
    'SYSTEMCTL="/usr/bin/systemctl"',
    "qintopia-agentos-huabaosi-image-generation-worker.service",
    "qintopia-agentos-huabaosi-image-generation-worker.timer",
    '"$SYSTEMCTL" disable --now "$WORKER_TIMER"',
  ]) {
    requireFragment(aliangProductionRollbackPath, rollback, fragment);
  }
  for (const fragment of [
    "rm -",
    "source ",
    "QIWE_",
    "FEISHU_",
    'SYSTEMCTL="${SYSTEMCTL:-systemctl}"',
    'SYSTEMCTL="systemctl"',
  ]) {
    forbidFragment(aliangProductionRollbackPath, rollback, fragment);
  }
}

const huabaosiFeishuMirrorProductionObservationPath =
  "deploy/sidecar/scripts/huabaosi-feishu-artifact-mirror-production-observation-smoke.sh";
if (!exists(huabaosiFeishuMirrorProductionObservationPath)) {
  addError(
    `${huabaosiFeishuMirrorProductionObservationPath}: missing Huabaosi Feishu mirror production observation`
  );
} else {
  const observation = readText(huabaosiFeishuMirrorProductionObservationPath);
  for (const fragment of [
    "artifact-manifest.json",
    '"huabaosi-production-adapter"',
    '"huabaosi-feishu-mirror-adapter"',
    "NextElapseUSecMonotonic",
    "production timer must have a future trigger",
  ]) {
    requireFragment(
      huabaosiFeishuMirrorProductionObservationPath,
      observation,
      fragment
    );
  }
  forbidFragment(
    huabaosiFeishuMirrorProductionObservationPath,
    observation,
    '"qiwe-production-adapter"'
  );
}

const huabaosiFeishuMirrorActivationPath =
  "deploy/sidecar/scripts/activate-huabaosi-feishu-artifact-mirror-production.sh";
if (!exists(huabaosiFeishuMirrorActivationPath)) {
  addError(
    `${huabaosiFeishuMirrorActivationPath}: missing Huabaosi Feishu mirror activation command`
  );
} else {
  const activation = readText(huabaosiFeishuMirrorActivationPath);
  for (const fragment of [
    "QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_ACTIVATION",
    "approved-production-huabaosi-feishu-artifact-mirror",
    'ENV_FILE="/etc/qintopia/message-sidecar.env"',
    'PATH="/usr/bin:/bin:/usr/sbin:/sbin"',
    'SYSTEMCTL="/usr/bin/systemctl"',
    "QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED",
    "qintopia-agentos-huabaosi-feishu-artifact-mirror-preflight.service",
    "qintopia-agentos-huabaosi-feishu-artifact-mirror-worker.timer",
    '"$SYSTEMCTL" start "$PREFLIGHT_SERVICE"',
    '"$SYSTEMCTL" enable "$WORKER_TIMER"',
    '"$SYSTEMCTL" restart "$WORKER_TIMER"',
    '"$SYSTEMCTL" is-enabled --quiet "$WORKER_TIMER"',
    '"$SYSTEMCTL" is-active --quiet "$WORKER_TIMER"',
    "NextElapseUSecMonotonic",
    "requires exactly one persistent enablement flag",
  ]) {
    requireFragment(huabaosiFeishuMirrorActivationPath, activation, fragment);
  }
  for (const fragment of [
    "source ",
    'source "$',
    ". /etc/qintopia",
    "eval ",
    "run-huabaosi-feishu-artifact-mirror-worker",
    "--apply",
    "QIWE_",
    "QINTOPIA_SIDECAR_ENV_FILE",
    'SYSTEMCTL="${SYSTEMCTL:-systemctl}"',
    'SYSTEMCTL="systemctl"',
  ]) {
    forbidFragment(huabaosiFeishuMirrorActivationPath, activation, fragment);
  }
}

const qiweImageSendProductionActivationPath =
  "deploy/sidecar/scripts/activate-qiwe-image-send-production.sh";
const qiweImageSendProductionConfigApplyPath =
  "deploy/sidecar/scripts/apply-qiwe-image-send-production-config.py";
const qiweImageSendProductionConfigApplyTestPath =
  "tools/deploy/test_qiwe_image_send_production_config_apply.py";
if (!exists(qiweImageSendProductionConfigApplyPath)) {
  addError(
    `${qiweImageSendProductionConfigApplyPath}: missing production config apply command`
  );
} else {
  const script = readText(qiweImageSendProductionConfigApplyPath);
  for (const fragment of [
    'SIDECAR_ENV_PATH = Path("/etc/qintopia/message-sidecar.env")',
    'RELEASE_ROOT_PATH = Path("/home/ubuntu/qintopia-agent-os-releases")',
    'RELEASE_CURRENT_PATH = Path("/home/ubuntu/qintopia-agent-os-releases/current")',
    "approved-production-qiwe-image-send-config-v1",
    "approved-production-qiwe-image-send",
    "QINTOPIA_QIWE_IMAGE_SEND_ENABLED",
    "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_APPROVAL",
    "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256",
    "QINTOPIA_QIWE_IMAGE_SEND_WEBHOOK_READY",
    "QINTOPIA_HUABAOSI_FEISHU_DATABASE_URL_SHA256",
    "QINTOPIA_HUABAOSI_FEISHU_ALLOWED_BASE_TOKENS",
    "QINTOPIA_HUABAOSI_FEISHU_ALLOWED_ARTIFACT_TABLE_IDS",
    "huabaosi-generated-image-v1",
    "QINTOPIA_SIDECAR_DATABASE_URL",
    "validate_database_url",
    "release/current does not match the approved release SHA",
    "release/current must resolve to the fixed release root",
    "database URL hash does not match the approved production hash",
    "Feishu primary-storage database hash is not approved",
    "Feishu artifact table allowlist is not exact",
    "sidecar env file must be a regular non-symlink file",
    "sidecar env file must not have hard links",
    "sidecar env file must not be group/world writable",
    "sidecar env file owner is not approved",
    "sidecar env file group is not approved",
    "sidecar env file mode is not approved",
    "stage_path.unlink()",
    "FileNotFoundError",
    "external_calls_executed",
    "database_writes_executed",
    "service_changes_executed",
  ]) {
    requireFragment(qiweImageSendProductionConfigApplyPath, script, fragment);
  }
  for (const fragment of [
    "systemctl",
    "curl ",
    "psql ",
    "source ",
    "eval ",
    "QIWE_TOKEN=",
    "print(values",
    "QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA",
    "QINTOPIA_DEPLOYED_COMMIT_SHA",
    "Feishu primary-storage release SHA is not approved",
  ]) {
    forbidFragment(qiweImageSendProductionConfigApplyPath, script, fragment);
  }
}
if (!exists(qiweImageSendProductionConfigApplyTestPath)) {
  addError(
    `${qiweImageSendProductionConfigApplyTestPath}: missing production config apply test`
  );
} else {
  const test = readText(qiweImageSendProductionConfigApplyTestPath);
  for (const fragment of [
    "test_preview_and_apply_enable_without_leaking_sensitive_values",
    "test_disabled_only_flips_enable_flag",
    "test_enable_rejects_unmatched_database_hash_before_mutation",
    "test_enable_allows_persistent_release_identity_to_lag_current",
    "test_enable_rejects_feishu_delivery_drift_before_mutation",
    "test_apply_requires_exact_owner_approval_and_root",
    "test_release_current_must_match_request",
    "test_release_current_must_stay_under_fixed_root",
    "test_env_metadata_must_match_production_boundary",
    "test_failed_commit_removes_secret_stage_file",
  ]) {
    requireFragment(qiweImageSendProductionConfigApplyTestPath, test, fragment);
  }
}
requireFragment(
  "tools/deploy/build-deploy-bundle.mjs",
  readText("tools/deploy/build-deploy-bundle.mjs"),
  qiweImageSendProductionConfigApplyPath
);
requireFragment(
  "package.json",
  readText("package.json"),
  "python3 tools/deploy/test_qiwe_image_send_production_config_apply.py"
);
if (!exists(qiweImageSendProductionActivationPath)) {
  addError(
    `${qiweImageSendProductionActivationPath}: missing production activation command`
  );
} else {
  const activation = readText(qiweImageSendProductionActivationPath);
  for (const fragment of [
    "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_ACTIVATION",
    "approved-production-qiwe-image-send",
    "qiwe-image-send-production-observation-smoke.sh",
    "QINTOPIA_QIWE_IMAGE_SEND_ENABLED",
    "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_APPROVAL",
    "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256",
    "QINTOPIA_SIDECAR_DATABASE_URL",
    'ENV_FILE="/etc/qintopia/message-sidecar.env"',
    'RELEASE_CURRENT_DIR="/home/ubuntu/qintopia-agent-os-releases/current"',
    'PATH="/usr/bin:/bin:/usr/sbin:/sbin"',
    'SYSTEMCTL="/usr/bin/systemctl"',
    'SHA256SUM="/usr/bin/sha256sum"',
    "database URL hash does not match the approved production hash",
    "requires the reviewed QiWe companion artifact",
    "qintopia-agentos-qiwe-image-send-preflight.service",
    "qintopia-agentos-qiwe-image-send-worker.timer",
    "qintopia-agentos-qiwe-image-send-worker.service",
    "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_OBSERVATION_ENABLE=1",
    "QINTOPIA_QIWE_IMAGE_SEND_EXPECTED_STATE=enabled",
    "unsafe env value for",
    "duplicate env key",
    '"$OBSERVATION_SCRIPT" >/dev/null',
    '"$SYSTEMCTL" start "$PREFLIGHT_SERVICE"',
    '"$SYSTEMCTL" enable "$WORKER_TIMER"',
    '"$SYSTEMCTL" restart "$WORKER_TIMER"',
    '"$SYSTEMCTL" is-enabled --quiet "$WORKER_TIMER"',
    '"$SYSTEMCTL" is-active --quiet "$WORKER_TIMER"',
    "NextElapseUSecMonotonic",
    '"$SYSTEMCTL" disable --now "$WORKER_TIMER"',
    '"$SYSTEMCTL" stop "$WORKER_SERVICE"',
    '"$SYSTEMCTL" reset-failed "$WORKER_SERVICE"',
  ]) {
    requireFragment(qiweImageSendProductionActivationPath, activation, fragment);
  }
  for (const fragment of [
    "run-qiwe-image-send-worker",
    "--apply",
    "source ",
    "eval ",
    "QIWE_TOKEN",
    "QINTOPIA_SIDECAR_ENV_FILE",
    'SYSTEMCTL="${SYSTEMCTL:-systemctl}"',
    'SYSTEMCTL="systemctl"',
    "sha256sum | awk",
    'grep -E "^${key}="',
  ]) {
    forbidFragment(qiweImageSendProductionActivationPath, activation, fragment);
  }
}

const qiweImageSendProductionRollbackPath =
  "deploy/sidecar/scripts/rollback-qiwe-image-send-production.sh";
if (!exists(qiweImageSendProductionRollbackPath)) {
  addError(
    `${qiweImageSendProductionRollbackPath}: missing production rollback command`
  );
} else {
  const rollback = readText(qiweImageSendProductionRollbackPath);
  for (const fragment of [
    "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_ROLLBACK",
    "approved-production-qiwe-image-send-rollback",
    "QINTOPIA_QIWE_IMAGE_SEND_ENABLED=0",
    'ENV_FILE="/etc/qintopia/message-sidecar.env"',
    'PATH="/usr/bin:/bin:/usr/sbin:/sbin"',
    'SYSTEMCTL="/usr/bin/systemctl"',
    "qintopia-agentos-qiwe-image-send-worker.service",
    "qintopia-agentos-qiwe-image-send-worker.timer",
    '"$SYSTEMCTL" disable --now "$WORKER_TIMER"',
  ]) {
    requireFragment(qiweImageSendProductionRollbackPath, rollback, fragment);
  }
  for (const fragment of [
    "rm -",
    "source ",
    "eval ",
    "QIWE_TOKEN",
    "QINTOPIA_SIDECAR_ENV_FILE",
    'SYSTEMCTL="${SYSTEMCTL:-systemctl}"',
    'SYSTEMCTL="systemctl"',
  ]) {
    forbidFragment(qiweImageSendProductionRollbackPath, rollback, fragment);
  }
}

const xiaomanDailyCaseReportWorkerPath =
  "deploy/sidecar/scripts/xiaoman-daily-case-report-auto-publish-worker.sh";
const xiaomanDailyCaseReportConfigApplyPath =
  "deploy/sidecar/scripts/apply-xiaoman-daily-case-report-production-config.py";
const xiaomanCreativeProfileCandidatesApplyPath =
  "deploy/sidecar/scripts/apply-xiaoman-creative-profile-candidates-production.sh";
const xiaomanDailyCaseReportObservationPath =
  "deploy/sidecar/scripts/xiaoman-daily-case-report-auto-publish-production-observation-smoke.sh";
const xiaomanDailyCaseReportBackfillPath =
  "deploy/sidecar/scripts/xiaoman-daily-case-report-auto-publish-backfill.sh";
const xiaomanDailyCaseReportActivationPath =
  "deploy/sidecar/scripts/activate-xiaoman-daily-case-report-auto-publish-production.sh";
const xiaomanDailyCaseReportRollbackPath =
  "deploy/sidecar/scripts/rollback-xiaoman-daily-case-report-auto-publish-production.sh";
for (const scriptPath of [
  xiaomanDailyCaseReportConfigApplyPath,
  xiaomanCreativeProfileCandidatesApplyPath,
  xiaomanDailyCaseReportWorkerPath,
  xiaomanDailyCaseReportBackfillPath,
  xiaomanDailyCaseReportObservationPath,
  xiaomanDailyCaseReportActivationPath,
  xiaomanDailyCaseReportRollbackPath,
]) {
  if (!exists(scriptPath)) {
    addError(`${scriptPath}: missing Xiaoman daily case report production script`);
  }
}
if (exists(xiaomanDailyCaseReportConfigApplyPath)) {
  const configApply = readText(xiaomanDailyCaseReportConfigApplyPath);
  for (const fragment of [
    'SIDECAR_ENV_PATH = Path("/etc/qintopia/message-sidecar.env")',
    'RELEASE_CURRENT_PATH = RELEASE_ROOT_PATH / "current"',
    'APPLY_APPROVAL = "approved-production-xiaoman-daily-case-report-config-v1"',
    'PUBLISH_APPROVAL = "approved-production-xiaoman-daily-case-report-auto-publish"',
    '"QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED"',
    '"QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_STORAGE_BACKEND"',
    "FEISHU_STORAGE_BACKEND",
    '"QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED"',
    '"QINTOPIA_HUABAOSI_FEISHU_ALLOWED_BASE_TOKENS"',
    '"QINTOPIA_QIWE_IMAGE_SEND_ENABLED"',
    '"QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS"',
    "Feishu primary-storage database hash is not approved",
    "daily report target group is not allowlisted for QiWe sends",
    "database URL hash does not match the approved production hash",
    "external_calls_executed",
    "database_writes_executed",
    "service_changes_executed",
  ]) {
    requireFragment(xiaomanDailyCaseReportConfigApplyPath, configApply, fragment);
  }
  for (const fragment of [
    "source ",
    "eval ",
    "QIWE_TOKEN",
    "QIWE_GUID",
    "QINTOPIA_SIDECAR_ENV_FILE",
  ]) {
    forbidFragment(xiaomanDailyCaseReportConfigApplyPath, configApply, fragment);
  }
}
if (exists(xiaomanCreativeProfileCandidatesApplyPath)) {
  const applyCreativeProfiles = readText(xiaomanCreativeProfileCandidatesApplyPath);
  for (const fragment of [
    'APPROVAL="approved-production-xiaoman-creative-profile-candidates"',
    'ENV_FILE="/etc/qintopia/message-sidecar.env"',
    'RELEASE_CURRENT="/home/ubuntu/qintopia-agent-os-releases/current"',
    'PAYLOAD_FILE="/home/ubuntu/.local/state/qintopia-agentos/xiaoman-creative-profile-candidates/reviewed-payload.json"',
    "QINTOPIA_XIAOMAN_CREATIVE_PROFILE_CANDIDATES_PAYLOAD_SHA256",
    "reviewed payload SHA-256 mismatch",
    "workflows/xiaoman-daily-case-report/apply_creative_profile_candidates.py",
    '--approval "$APPROVAL"',
  ]) {
    requireFragment(
      xiaomanCreativeProfileCandidatesApplyPath,
      applyCreativeProfiles,
      fragment
    );
  }
  for (const fragment of [
    "eval ",
    "curl ",
    "ssh ",
    "QINTOPIA_XIAOMAN_CREATIVE_PROFILE_CANDIDATES_PAYLOAD_FILE",
    "QINTOPIA_SIDECAR_ENV_FILE",
  ]) {
    forbidFragment(
      xiaomanCreativeProfileCandidatesApplyPath,
      applyCreativeProfiles,
      fragment
    );
  }
}
if (exists(xiaomanDailyCaseReportWorkerPath)) {
  const worker = readText(xiaomanDailyCaseReportWorkerPath);
  for (const fragment of [
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED",
    "workflows/xiaoman-daily-case-report/daily_case_report.py",
    "sidecar-profiles/qiwe-production/qintopia-message-sidecar",
    'PYTHON_BIN="/usr/bin/python3"',
    'PSQL_BIN="/usr/bin/psql"',
    "Pillow is required for xiaoman daily case report local image rendering",
    "operations-daily-case-report-media-upload",
    "operations-daily-case-report-auto-publish-create",
    "--image-format jpeg",
    '--chat-id "$QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_CHAT_ID"',
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_TARGET_GROUP_ID",
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_STORAGE_BACKEND",
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_DATE",
    "approved-production-xiaoman-daily-case-report-auto-publish-backfill",
    'report_date_args=(--date "$QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_DATE")',
    "feishu-base",
    "artifact_uri",
    '"width": uploaded["width"]',
    '"height": uploaded["height"]',
    "media_upload_evidence",
    "media upload did not return media_upload_evidence",
    "requires_human_final_confirmation",
    "external_send_executed",
  ]) {
    requireFragment(xiaomanDailyCaseReportWorkerPath, worker, fragment);
  }
  for (const fragment of [
    "run-qiwe-image-send-worker",
    "reply",
    '"human_final_confirmation"',
    "'human_final_confirmation'",
    "QIWE_TOKEN",
    "QIWE_GUID",
    "source ",
    "eval ",
  ]) {
    forbidFragment(xiaomanDailyCaseReportWorkerPath, worker, fragment);
  }
}
if (exists(xiaomanDailyCaseReportBackfillPath)) {
  const backfill = readText(xiaomanDailyCaseReportBackfillPath);
  for (const fragment of [
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_BACKFILL",
    "approved-production-xiaoman-daily-case-report-auto-publish-backfill",
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_BACKFILL_RELEASE_SHA",
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_BACKFILL_DATE",
    'ENV_FILE="/etc/qintopia/message-sidecar.env"',
    'PATH="/usr/bin:/bin:/usr/sbin:/sbin"',
    'SYSTEMCTL="/usr/bin/systemctl"',
    "qintopia-agentos-xiaoman-daily-case-report-auto-publish.service",
    'require_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED" "1"',
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_PRODUCTION_APPROVAL",
    'require_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_READ_THROUGH_ENABLE" "1"',
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_TARGET_GROUP_ID",
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_DATE=${BACKFILL_DATE}",
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_BACKFILL_APPROVAL=${APPROVAL}",
    '"$SYSTEMCTL" start "$SERVICE_NAME"',
  ]) {
    requireFragment(xiaomanDailyCaseReportBackfillPath, backfill, fragment);
  }
  for (const fragment of [
    "run-qiwe-image-send-worker",
    "reply",
    "QIWE_TOKEN",
    "QIWE_GUID",
    "source ",
    "eval ",
    'SYSTEMCTL="${SYSTEMCTL:-systemctl}"',
    'SYSTEMCTL="systemctl"',
  ]) {
    forbidFragment(xiaomanDailyCaseReportBackfillPath, backfill, fragment);
  }
}
if (exists(xiaomanDailyCaseReportObservationPath)) {
  const observation = readText(xiaomanDailyCaseReportObservationPath);
  for (const fragment of [
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_OBSERVATION_ENABLE",
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_EXPECTED_STATE",
    'ENV_FILE="/etc/qintopia/message-sidecar.env"',
    'SYSTEMCTL="/usr/bin/systemctl"',
    "qintopia-agentos-xiaoman-daily-case-report-auto-publish.service",
    "qintopia-agentos-xiaoman-daily-case-report-auto-publish.timer",
    "OnCalendar=*-*-* 08:00:00",
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_STORAGE_BACKEND",
    "feishu-base",
  ]) {
    requireFragment(xiaomanDailyCaseReportObservationPath, observation, fragment);
  }
  for (const fragment of ["source ", "eval ", "QIWE_TOKEN", "QIWE_GUID"]) {
    forbidFragment(xiaomanDailyCaseReportObservationPath, observation, fragment);
  }
}
if (exists(xiaomanDailyCaseReportActivationPath)) {
  const activation = readText(xiaomanDailyCaseReportActivationPath);
  for (const fragment of [
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_PRODUCTION_ACTIVATION",
    "approved-production-xiaoman-daily-case-report-auto-publish",
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED",
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_PRODUCTION_APPROVAL",
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_READ_THROUGH_ENABLE",
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_STORAGE_BACKEND",
    "feishu-base",
    "QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED",
    "QINTOPIA_HUABAOSI_FEISHU_MIRROR_APPROVAL",
    "QINTOPIA_HUABAOSI_FEISHU_DATABASE_URL_SHA256",
    "QINTOPIA_HUABAOSI_FEISHU_ALLOWED_BASE_TOKENS",
    "QINTOPIA_HUABAOSI_FEISHU_ALLOWED_ARTIFACT_TABLE_IDS",
    "require_feishu_database_hash_match",
    "require_exact_allowlist",
    'ENV_FILE="/etc/qintopia/message-sidecar.env"',
    'SYSTEMCTL="/usr/bin/systemctl"',
    'SHA256SUM="/usr/bin/sha256sum"',
    'PYTHON_BIN="/usr/bin/python3"',
    'PSQL_BIN="/usr/bin/psql"',
    "Pillow is required for xiaoman daily case report activation",
    '"$SYSTEMCTL" enable "$TIMER_NAME"',
    '"$SYSTEMCTL" restart "$TIMER_NAME"',
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_EXPECTED_STATE=enabled",
  ]) {
    requireFragment(xiaomanDailyCaseReportActivationPath, activation, fragment);
  }
  for (const fragment of ["source ", "eval ", "QIWE_TOKEN", "QIWE_GUID"]) {
    forbidFragment(xiaomanDailyCaseReportActivationPath, activation, fragment);
  }
}
if (exists(xiaomanDailyCaseReportRollbackPath)) {
  const rollback = readText(xiaomanDailyCaseReportRollbackPath);
  for (const fragment of [
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_PRODUCTION_ROLLBACK",
    "approved-production-xiaoman-daily-case-report-auto-publish-rollback",
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED=0",
    'ENV_FILE="/etc/qintopia/message-sidecar.env"',
    'SYSTEMCTL="/usr/bin/systemctl"',
    '"$SYSTEMCTL" disable --now "$TIMER_NAME"',
  ]) {
    requireFragment(xiaomanDailyCaseReportRollbackPath, rollback, fragment);
  }
  for (const fragment of ["source ", "eval ", "QIWE_TOKEN", "QIWE_GUID"]) {
    forbidFragment(xiaomanDailyCaseReportRollbackPath, rollback, fragment);
  }
}
const deployBundleBuilderForDailyReport = readText(
  "tools/deploy/build-deploy-bundle.mjs"
);
for (const fragment of [
  xiaomanDailyCaseReportConfigApplyPath,
  xiaomanDailyCaseReportWorkerPath,
  xiaomanDailyCaseReportBackfillPath,
  xiaomanDailyCaseReportObservationPath,
  xiaomanDailyCaseReportActivationPath,
  xiaomanDailyCaseReportRollbackPath,
  "deploy/sidecar/scripts/apply-xiaoman-daily-case-report-hermes-cron.sh",
  "workflows/xiaoman-daily-case-report",
]) {
  requireFragment(
    "tools/deploy/build-deploy-bundle.mjs",
    deployBundleBuilderForDailyReport,
    fragment
  );
}
const xiaomanDailyCaseReportHermesCronApplyPath =
  "deploy/sidecar/scripts/apply-xiaoman-daily-case-report-hermes-cron.sh";
if (!exists(xiaomanDailyCaseReportHermesCronApplyPath)) {
  addError(
    `${xiaomanDailyCaseReportHermesCronApplyPath}: missing daily case report Hermes cron apply`
  );
} else {
  const apply = readText(xiaomanDailyCaseReportHermesCronApplyPath);
  for (const fragment of [
    "approved-production-xiaoman-daily-case-report-hermes-cron",
    "usage: apply-xiaoman-daily-case-report-hermes-cron.sh [--install|--enable]",
    'RELEASE_CURRENT="/home/ubuntu/qintopia-agent-os-releases/current"',
    'CRON_FILE="/home/ubuntu/.hermes/profiles/xiaoman/cron/jobs.json"',
    'PROFILE_ENV_FILE="/home/ubuntu/.hermes/profiles/xiaoman/.env"',
    'WRAPPER_TARGET="${HERMES_SCRIPTS_DIR}/qintopia_xiaoman_daily_case_report.sh"',
    'SNAPSHOT_SYNC="${RELEASE_CURRENT}/deploy/sidecar/scripts/sync-hermes-cron-snapshot.sh"',
    "WECOM_HOME_CHANNEL",
    "origin_chat_id_resolved",
    "verify_installed_wrapper",
    "atomic_replace",
    "external_calls_executed",
    "safe_for_chat",
    "daily_case_report_hermes_cron_installed",
    "daily_case_report_hermes_cron_enabled",
    "daily_case_report_hermes_cron_already_enabled",
    "reviewed daily case report job deliver mode does not match the reviewed declaration",
    "reviewed daily case report job origin platform does not match the reviewed declaration",
    "reviewed daily case report job origin chat id drifted from the Xiaoman profile env",
  ]) {
    requireFragment(xiaomanDailyCaseReportHermesCronApplyPath, apply, fragment);
  }
  for (const fragment of [
    "eval ",
    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_HERMES_CRON_FILE",
    "QINTOPIA_XIAOMAN_PROFILE_DIR",
    "QIWE_TOKEN",
    "tenant_access_token",
    "print(chat_id)",
  ]) {
    forbidFragment(xiaomanDailyCaseReportHermesCronApplyPath, apply, fragment);
  }
}

const xiaomanWeeklyLoopTimers = [
  {
    label: "xiaoman weekly recruitment",
    envPrefix: "QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT",
    approval: "approved-production-xiaoman-weekly-recruitment",
    configApproval: "approved-production-xiaoman-weekly-recruitment-config",
    rollbackApproval: "approved-production-xiaoman-weekly-recruitment-rollback",
    configPath:
      "deploy/sidecar/scripts/apply-xiaoman-weekly-recruitment-production-config.sh",
    workerPath: "deploy/sidecar/scripts/xiaoman-weekly-recruitment-worker.sh",
    observationPath:
      "deploy/sidecar/scripts/xiaoman-weekly-recruitment-production-observation-smoke.sh",
    activationPath:
      "deploy/sidecar/scripts/activate-xiaoman-weekly-recruitment-production.sh",
    rollbackPath:
      "deploy/sidecar/scripts/rollback-xiaoman-weekly-recruitment-production.sh",
    workerName: "xiaoman-weekly-recruitment-worker",
    workDir:
      'WORK_DIR="/home/ubuntu/.local/state/qintopia-agentos/xiaoman-weekly-recruitment"',
    serviceName: "qintopia-agentos-xiaoman-weekly-recruitment.service",
    timerName: "qintopia-agentos-xiaoman-weekly-recruitment.timer",
    mode: "weekly_recruitment_form",
    calendar: "OnCalendar=Sat *-*-* 10:00:00",
  },
  {
    label: "xiaoman weekly plan confirmation",
    envPrefix: "QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION",
    approval: "approved-production-xiaoman-weekly-plan-confirmation",
    configApproval: "approved-production-xiaoman-weekly-plan-confirmation-config",
    rollbackApproval: "approved-production-xiaoman-weekly-plan-confirmation-rollback",
    configPath:
      "deploy/sidecar/scripts/apply-xiaoman-weekly-plan-confirmation-production-config.sh",
    workerPath: "deploy/sidecar/scripts/xiaoman-weekly-plan-confirmation-worker.sh",
    observationPath:
      "deploy/sidecar/scripts/xiaoman-weekly-plan-confirmation-production-observation-smoke.sh",
    activationPath:
      "deploy/sidecar/scripts/activate-xiaoman-weekly-plan-confirmation-production.sh",
    rollbackPath:
      "deploy/sidecar/scripts/rollback-xiaoman-weekly-plan-confirmation-production.sh",
    workerName: "xiaoman-weekly-plan-confirmation-worker",
    workDir:
      'WORK_DIR="/home/ubuntu/.local/state/qintopia-agentos/xiaoman-weekly-plan-confirmation"',
    serviceName: "qintopia-agentos-xiaoman-weekly-plan-confirmation.service",
    timerName: "qintopia-agentos-xiaoman-weekly-plan-confirmation.timer",
    mode: "weekly_plan_confirmation",
    calendar: "OnCalendar=Sun *-*-* 20:00:00",
  },
];
for (const timer of xiaomanWeeklyLoopTimers) {
  for (const scriptPath of [
    timer.configPath,
    timer.workerPath,
    timer.observationPath,
    timer.activationPath,
    timer.rollbackPath,
  ]) {
    if (!exists(scriptPath)) {
      addError(`${scriptPath}: missing ${timer.label} production script`);
    }
  }
  if (exists(timer.configPath)) {
    const config = readText(timer.configPath);
    for (const fragment of [
      timer.configApproval,
      'PYTHON_BIN="/usr/bin/python3"',
      'ENV_FILE="/etc/qintopia/message-sidecar.env"',
      `${timer.envPrefix}_ENABLED`,
      `${timer.envPrefix}_PRODUCTION_APPROVAL`,
      "QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE",
      "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE",
      "QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE",
      "requires exactly one QINTOPIA_SIDECAR_DATABASE_URL",
      "os.chown(tmp_name, stat.st_uid, stat.st_gid)",
    ]) {
      requireFragment(timer.configPath, config, fragment);
    }
    for (const fragment of [
      "QINTOPIA_SIDECAR_ENV_FILE",
      'SYSTEMCTL="${SYSTEMCTL:-systemctl}"',
      "source ",
      ". /etc/qintopia",
      "eval ",
    ]) {
      forbidFragment(timer.configPath, config, fragment);
    }
  }
  if (exists(timer.workerPath)) {
    const worker = readText(timer.workerPath);
    for (const fragment of [
      `${timer.envPrefix}_ENABLED`,
      `${timer.envPrefix}_PRODUCTION_APPROVAL`,
      timer.approval,
      'RELEASE_DIR="/home/ubuntu/qintopia-agent-os-releases/current"',
      'PYTHON_BIN="/usr/bin/python3"',
      timer.workDir,
      `${timer.label} refuses runtime path overrides`,
      "-v QINTOPIA_XIAOMAN_WRAPPER_PATH",
      `require_env "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE"`,
      `require_env "QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE"`,
      `${timer.label} requires Xiaoman activity Feishu Base mode to be enabled`,
      `${timer.label} requires Xiaoman activity read-through to be enabled`,
      "workflows/xiaoman-weekly-loop/weekly_loop.py",
      timer.mode,
      "--json",
      "operator_review_message_path",
      "requires_human_confirmation",
      "external_send_executed",
      "safe_for_member_chat",
      "latest-operator-review-message.txt",
      "latest-summary.json",
    ]) {
      requireFragment(timer.workerPath, worker, fragment);
    }
    for (const fragment of [
      "run-group-message-send-worker",
      "operations-group-message-confirm",
      "operations-work-item-create",
      "QIWE_TOKEN",
      "QIWE_GUID",
      "QINTOPIA_RELEASE_DIR:-",
      "QINTOPIA_XIAOMAN_WRAPPER_PATH:-",
      `${timer.envPrefix}_PYTHON:-`,
      `${timer.envPrefix}_OUTPUT_DIR:-`,
      "source ",
      "eval ",
    ]) {
      forbidFragment(timer.workerPath, worker, fragment);
    }
  }
  if (exists(timer.observationPath)) {
    const observation = readText(timer.observationPath);
    for (const fragment of [
      `${timer.envPrefix}_OBSERVATION_ENABLE`,
      `${timer.envPrefix}_EXPECTED_STATE`,
      `${timer.envPrefix}_PRODUCTION_RELEASE_SHA`,
      `${timer.envPrefix}_PRODUCTION_RELEASE_SHA must be a 40-character lowercase hex SHA`,
      'ENV_FILE="/etc/qintopia/message-sidecar.env"',
      'SYSTEMCTL="/usr/bin/systemctl"',
      "QINTOPIA_DEPLOYED_COMMIT_SHA=${EXPECTED_RELEASE_SHA}",
      timer.serviceName,
      timer.timerName,
      'require_env_line "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE" "1"',
      'require_env_line "QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE" "1"',
      timer.calendar,
    ]) {
      requireFragment(timer.observationPath, observation, fragment);
    }
    for (const fragment of ["source ", "eval ", "QIWE_TOKEN", "QIWE_GUID"]) {
      forbidFragment(timer.observationPath, observation, fragment);
    }
  }
  if (exists(timer.activationPath)) {
    const activation = readText(timer.activationPath);
    for (const fragment of [
      `${timer.envPrefix}_PRODUCTION_ACTIVATION`,
      timer.approval,
      `${timer.envPrefix}_ENABLED`,
      `${timer.envPrefix}_PRODUCTION_APPROVAL`,
      "QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE",
      "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE",
      "QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE",
      "QINTOPIA_XIAOMAN_LEGACY_CRON_OBSERVATION_ENABLE=1",
      `${timer.envPrefix}_PRODUCTION_RELEASE_SHA`,
      'ENV_FILE="/etc/qintopia/message-sidecar.env"',
      'SYSTEMCTL="/usr/bin/systemctl"',
      'if ! "$SYSTEMCTL" enable "$TIMER_NAME"; then',
      'if ! "$SYSTEMCTL" restart "$TIMER_NAME"; then',
      '"$SYSTEMCTL" enable "$TIMER_NAME"',
      '"$SYSTEMCTL" restart "$TIMER_NAME"',
      `${timer.envPrefix}_PRODUCTION_RELEASE_SHA="$EXPECTED_RELEASE_SHA"`,
      `${timer.envPrefix}_EXPECTED_STATE=enabled`,
    ]) {
      requireFragment(timer.activationPath, activation, fragment);
    }
    for (const fragment of ["source ", "eval ", "QIWE_TOKEN", "QIWE_GUID"]) {
      forbidFragment(timer.activationPath, activation, fragment);
    }
  }
  if (exists(timer.rollbackPath)) {
    const rollback = readText(timer.rollbackPath);
    for (const fragment of [
      `${timer.envPrefix}_PRODUCTION_ROLLBACK`,
      timer.rollbackApproval,
      `${timer.envPrefix}_ENABLED=0`,
      'ENV_FILE="/etc/qintopia/message-sidecar.env"',
      'SYSTEMCTL="/usr/bin/systemctl"',
      '"$SYSTEMCTL" disable --now "$TIMER_NAME"',
    ]) {
      requireFragment(timer.rollbackPath, rollback, fragment);
    }
    for (const fragment of ["source ", "eval ", "QIWE_TOKEN", "QIWE_GUID"]) {
      forbidFragment(timer.rollbackPath, rollback, fragment);
    }
    const envDisabledCheck = rollback.indexOf(
      `grep -Fxq "${timer.envPrefix}_ENABLED=0"`
    );
    const timerDisable = rollback.indexOf('"$SYSTEMCTL" disable --now "$TIMER_NAME"');
    if (
      envDisabledCheck === -1 ||
      timerDisable === -1 ||
      envDisabledCheck > timerDisable
    ) {
      addError(
        `${timer.rollbackPath}: must verify persistent disabled flag before mutating systemd`
      );
    }
  }
}
const xiaomanPlanConfirmationHermesCronApplyPath =
  "deploy/sidecar/scripts/apply-xiaoman-weekly-plan-confirmation-hermes-cron.sh";
if (!exists(xiaomanPlanConfirmationHermesCronApplyPath)) {
  addError(
    `${xiaomanPlanConfirmationHermesCronApplyPath}: missing plan confirmation Hermes cron apply`
  );
} else {
  const apply = readText(xiaomanPlanConfirmationHermesCronApplyPath);
  for (const fragment of [
    "approved-production-xiaoman-weekly-plan-confirmation-hermes-cron",
    "usage: apply-xiaoman-weekly-plan-confirmation-hermes-cron.sh --install|--enable",
    'RELEASE_DIR="/home/ubuntu/qintopia-agent-os-releases/current"',
    'CRON_FILE="/home/ubuntu/.hermes/profiles/xiaoman/cron/jobs.json"',
    'PROFILE_ENV="/home/ubuntu/.hermes/profiles/xiaoman/.env"',
    'WRAPPER_DEST="/home/ubuntu/.hermes/scripts/qintopia_xiaoman_weekly_plan_confirmation.sh"',
    'SNAPSHOT_SYNC="${RELEASE_DIR}/deploy/sidecar/scripts/sync-hermes-cron-snapshot.sh"',
    "WECOM_HOME_CHANNEL",
    "origin_chat_id_sha256",
    "os.replace(temp_name, cron_file)",
    "external_calls_executed",
    "safe_for_chat",
    'chmod 0700 "$WRAPPER_DEST"',
  ]) {
    requireFragment(xiaomanPlanConfirmationHermesCronApplyPath, apply, fragment);
  }
  for (const fragment of [
    "source ",
    "eval ",
    "QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_HERMES_CRON_FILE",
    "QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_HERMES_CRON_PROFILE_DIR",
    "QIWE_TOKEN",
    "tenant_access_token",
    "print(chat_id)",
  ]) {
    forbidFragment(xiaomanPlanConfirmationHermesCronApplyPath, apply, fragment);
  }
}
const deployBundleBuilderForWeeklyLoop = readText(
  "tools/deploy/build-deploy-bundle.mjs"
);
for (const fragment of [
  ...xiaomanWeeklyLoopTimers.flatMap((timer) => [
    timer.configPath,
    timer.workerPath,
    timer.observationPath,
    timer.activationPath,
    timer.rollbackPath,
  ]),
  "deploy/sidecar/scripts/apply-xiaoman-weekly-plan-confirmation-hermes-cron.sh",
  "runtime/hermes/cron/xiaoman/weekly-plan-confirmation.job.json",
  "runtime/hermes/scripts/qintopia_xiaoman_weekly_plan_confirmation.sh",
  "workflows/xiaoman-weekly-loop",
]) {
  requireFragment(
    "tools/deploy/build-deploy-bundle.mjs",
    deployBundleBuilderForWeeklyLoop,
    fragment
  );
}
const xiaomanPlanConfirmationWrapperPath =
  "runtime/hermes/scripts/qintopia_xiaoman_weekly_plan_confirmation.sh";
if (!exists(xiaomanPlanConfirmationWrapperPath)) {
  addError(
    `${xiaomanPlanConfirmationWrapperPath}: missing plan confirmation Hermes wrapper`
  );
} else {
  const wrapper = readText(xiaomanPlanConfirmationWrapperPath);
  for (const fragment of [
    'ENV_FILE="/etc/qintopia/message-sidecar.env"',
    'RELEASE_CURRENT="/home/ubuntu/qintopia-agent-os-releases/current"',
    'export PATH="/usr/bin:/bin:/usr/sbin:/sbin"',
    'release_dir="$(cd "$RELEASE_CURRENT" && pwd -P)"',
    'release_sha="${release_dir##*/}"',
    'export QINTOPIA_DEPLOYED_COMMIT_SHA="$release_sha"',
    'WORKER="${release_dir}/deploy/sidecar/scripts/xiaoman-weekly-plan-confirmation-worker.sh"',
  ]) {
    requireFragment(xiaomanPlanConfirmationWrapperPath, wrapper, fragment);
  }
  const envSource = wrapper.indexOf('. "$ENV_FILE"');
  const pathExport = wrapper.lastIndexOf('export PATH="/usr/bin:/bin:/usr/sbin:/sbin"');
  const shaExport = wrapper.indexOf("export QINTOPIA_DEPLOYED_COMMIT_SHA");
  if (envSource === -1 || pathExport === -1 || envSource > pathExport) {
    addError(
      `${xiaomanPlanConfirmationWrapperPath}: must export fixed PATH after sourcing the persistent env`
    );
  }
  if (envSource === -1 || shaExport === -1 || envSource > shaExport) {
    addError(
      `${xiaomanPlanConfirmationWrapperPath}: must export QINTOPIA_DEPLOYED_COMMIT_SHA after sourcing the persistent env`
    );
  }
}
const erhuaMorningBriefHermesCronApplyPath =
  "deploy/sidecar/scripts/apply-erhua-morning-brief-hermes-cron.sh";
if (!exists(erhuaMorningBriefHermesCronApplyPath)) {
  addError(
    `${erhuaMorningBriefHermesCronApplyPath}: missing morning brief Hermes cron apply`
  );
} else {
  const apply = readText(erhuaMorningBriefHermesCronApplyPath);
  for (const fragment of [
    "approved-production-erhua-morning-brief-hermes-cron",
    "usage: apply-erhua-morning-brief-hermes-cron.sh --install|--enable",
    'RELEASE_DIR="/home/ubuntu/qintopia-agent-os-releases/current"',
    'CRON_FILE="/home/ubuntu/.hermes/profiles/erhua/cron/jobs.json"',
    'PROFILE_ENV="/home/ubuntu/.hermes/profiles/erhua/.env"',
    'WRAPPER_DEST="/home/ubuntu/.hermes/scripts/qintopia_erhua_morning_brief.sh"',
    'SNAPSHOT_SYNC="${RELEASE_DIR}/deploy/sidecar/scripts/sync-hermes-cron-snapshot.sh"',
    "WECOM_HOME_CHANNEL",
    "origin_chat_id_sha256",
    "updated_at_preserved",
    "os.replace(temp_name, cron_file)",
    "external_calls_executed",
    "safe_for_chat",
    'chmod 0700 "$WRAPPER_DEST"',
  ]) {
    requireFragment(erhuaMorningBriefHermesCronApplyPath, apply, fragment);
  }
  for (const fragment of [
    "source ",
    "eval ",
    "QINTOPIA_ERHUA_MORNING_BRIEF_HERMES_CRON_FILE",
    "QINTOPIA_ERHUA_MORNING_BRIEF_HERMES_CRON_PROFILE_DIR",
    "QIWE_TOKEN",
    "tenant_access_token",
    "print(chat_id)",
  ]) {
    forbidFragment(erhuaMorningBriefHermesCronApplyPath, apply, fragment);
  }
}
const deployBundleBuilderForErhuaMorningBrief = readText(
  "tools/deploy/build-deploy-bundle.mjs"
);
for (const fragment of [
  "deploy/sidecar/scripts/apply-erhua-morning-brief-hermes-cron.sh",
  "runtime/hermes/cron/erhua/morning-brief.job.json",
  "runtime/hermes/scripts/qintopia_erhua_morning_brief.sh",
]) {
  requireFragment(
    "tools/deploy/build-deploy-bundle.mjs",
    deployBundleBuilderForErhuaMorningBrief,
    fragment
  );
}
const erhuaMorningBriefWrapperPath =
  "runtime/hermes/scripts/qintopia_erhua_morning_brief.sh";
if (!exists(erhuaMorningBriefWrapperPath)) {
  addError(`${erhuaMorningBriefWrapperPath}: missing morning brief Hermes wrapper`);
} else {
  const wrapper = readText(erhuaMorningBriefWrapperPath);
  for (const fragment of [
    'ENV_FILE="/etc/qintopia/message-sidecar.env"',
    'RELEASE_LINK="/home/ubuntu/qintopia-agent-os-releases/current"',
    'export PATH="/usr/bin:/bin:/usr/sbin:/sbin"',
    'release_dir="$(cd "$RELEASE_LINK" && pwd -P)"',
    'release_sha="${release_dir##*/}"',
    'export QINTOPIA_DEPLOYED_COMMIT_SHA="$release_sha"',
    'WORKER="${release_dir}/deploy/sidecar/scripts/erhua-morning-brief-worker.sh"',
  ]) {
    requireFragment(erhuaMorningBriefWrapperPath, wrapper, fragment);
  }
  const envSource = wrapper.indexOf('. "$ENV_FILE"');
  const pathExport = wrapper.lastIndexOf('export PATH="/usr/bin:/bin:/usr/sbin:/sbin"');
  const shaExport = wrapper.indexOf("export QINTOPIA_DEPLOYED_COMMIT_SHA");
  if (envSource === -1 || pathExport === -1 || envSource > pathExport) {
    addError(
      `${erhuaMorningBriefWrapperPath}: must export fixed PATH after sourcing the persistent env`
    );
  }
  if (envSource === -1 || shaExport === -1 || envSource > shaExport) {
    addError(
      `${erhuaMorningBriefWrapperPath}: must export QINTOPIA_DEPLOYED_COMMIT_SHA after sourcing the persistent env`
    );
  }
}
const xiaomanWeeklyLoopWorkflowPath = "workflows/xiaoman-weekly-loop/weekly_loop.py";
if (exists(xiaomanWeeklyLoopWorkflowPath)) {
  const workflow = readText(xiaomanWeeklyLoopWorkflowPath);
  requireFragment(
    xiaomanWeeklyLoopWorkflowPath,
    workflow,
    "cannot locate reviewed xiaoman wrapper"
  );
  forbidFragment(
    xiaomanWeeklyLoopWorkflowPath,
    workflow,
    "QINTOPIA_XIAOMAN_WRAPPER_PATH"
  );
}

const xiaomanWeeklyPreviewConfigApplyPath =
  "deploy/sidecar/scripts/apply-xiaoman-weekly-preview-production-config.sh";
const xiaomanWeeklyPreviewWorkflowPath =
  "workflows/xiaoman-weekly-preview/weekly_preview.py";
const xiaomanWeeklyPreviewWorkerPath =
  "deploy/sidecar/scripts/xiaoman-weekly-preview-worker.sh";
const xiaomanWeeklyPreviewObservationPath =
  "deploy/sidecar/scripts/xiaoman-weekly-preview-production-observation-smoke.sh";
const xiaomanWeeklyPreviewActivationPath =
  "deploy/sidecar/scripts/activate-xiaoman-weekly-preview-production.sh";
const xiaomanWeeklyPreviewRollbackPath =
  "deploy/sidecar/scripts/rollback-xiaoman-weekly-preview-production.sh";
for (const scriptPath of [
  xiaomanWeeklyPreviewConfigApplyPath,
  xiaomanWeeklyPreviewWorkerPath,
  xiaomanWeeklyPreviewObservationPath,
  xiaomanWeeklyPreviewActivationPath,
  xiaomanWeeklyPreviewRollbackPath,
]) {
  if (!exists(scriptPath)) {
    addError(`${scriptPath}: missing Xiaoman weekly preview production script`);
  }
}
if (exists(xiaomanWeeklyPreviewWorkflowPath)) {
  const workflow = readText(xiaomanWeeklyPreviewWorkflowPath);
  for (const fragment of [
    "cannot locate reviewed xiaoman wrapper",
    "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE",
  ]) {
    requireFragment(xiaomanWeeklyPreviewWorkflowPath, workflow, fragment);
  }
  forbidFragment(
    xiaomanWeeklyPreviewWorkflowPath,
    workflow,
    "QINTOPIA_XIAOMAN_WRAPPER_PATH"
  );
}
if (exists(xiaomanWeeklyPreviewConfigApplyPath)) {
  const config = readText(xiaomanWeeklyPreviewConfigApplyPath);
  for (const fragment of [
    "approved-production-xiaoman-weekly-preview-config",
    'PYTHON_BIN="/usr/bin/python3"',
    'ENV_FILE="/etc/qintopia/message-sidecar.env"',
    "QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_ENABLED",
    "QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_PRODUCTION_APPROVAL",
    "QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE",
    "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE",
    "QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE",
    "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE",
    "requires exactly one QINTOPIA_SIDECAR_DATABASE_URL",
    "os.chown(tmp_name, stat.st_uid, stat.st_gid)",
  ]) {
    requireFragment(xiaomanWeeklyPreviewConfigApplyPath, config, fragment);
  }
  for (const fragment of [
    "QINTOPIA_SIDECAR_ENV_FILE",
    'SYSTEMCTL="${SYSTEMCTL:-systemctl}"',
    "source ",
    ". /etc/qintopia",
    "eval ",
  ]) {
    forbidFragment(xiaomanWeeklyPreviewConfigApplyPath, config, fragment);
  }
}
if (exists(xiaomanWeeklyPreviewWorkerPath)) {
  const worker = readText(xiaomanWeeklyPreviewWorkerPath);
  for (const fragment of [
    "QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_ENABLED",
    "QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_PRODUCTION_APPROVAL",
    "approved-production-xiaoman-weekly-preview",
    'RELEASE_DIR="/home/ubuntu/qintopia-agent-os-releases/current"',
    'PYTHON_BIN="/usr/bin/python3"',
    'SIDECAR_BIN="${RELEASE_DIR}/sidecar/qintopia-message-sidecar"',
    'WORK_DIR="/home/ubuntu/.local/state/qintopia-agentos/xiaoman-weekly-preview"',
    "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE",
    "xiaoman weekly preview refuses runtime path overrides",
    "-v QINTOPIA_XIAOMAN_WRAPPER_PATH",
    'export QINTOPIA_XIAOMAN_ACTIVITY_WORKER_BIN="$SIDECAR_BIN"',
    "xiaoman weekly preview requires Xiaoman activity Feishu Base mode to be enabled",
    "xiaoman weekly preview requires the release-local sidecar binary",
    "workflows/xiaoman-weekly-preview/weekly_preview.py",
    "--json",
    "operator_review_message_path",
    "requires_human_confirmation",
    "external_send_executed",
    "safe_for_member_chat",
    "latest-operator-review-message.txt",
    "latest-summary.json",
  ]) {
    requireFragment(xiaomanWeeklyPreviewWorkerPath, worker, fragment);
  }
  for (const fragment of [
    "run-group-message-send-worker",
    "operations-group-message-confirm",
    "operations-work-item-create",
    "QIWE_TOKEN",
    "QIWE_GUID",
    "QINTOPIA_RELEASE_DIR:-",
    "QINTOPIA_XIAOMAN_WRAPPER_PATH:-",
    "QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_PYTHON:-",
    "QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_OUTPUT_DIR:-",
    "source ",
    "eval ",
  ]) {
    forbidFragment(xiaomanWeeklyPreviewWorkerPath, worker, fragment);
  }
}
if (exists(xiaomanWeeklyPreviewObservationPath)) {
  const observation = readText(xiaomanWeeklyPreviewObservationPath);
  for (const fragment of [
    "QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_OBSERVATION_ENABLE",
    "QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_EXPECTED_STATE",
    "QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_PRODUCTION_RELEASE_SHA",
    "QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_PRODUCTION_RELEASE_SHA must be a 40-character lowercase hex SHA",
    'ENV_FILE="/etc/qintopia/message-sidecar.env"',
    'SYSTEMCTL="/usr/bin/systemctl"',
    "QINTOPIA_DEPLOYED_COMMIT_SHA=${EXPECTED_RELEASE_SHA}",
    "qintopia-agentos-xiaoman-weekly-preview.service",
    "qintopia-agentos-xiaoman-weekly-preview.timer",
    "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE",
    "OnCalendar=Mon *-*-* 09:30:00",
    'require_env_line "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE" "1"',
  ]) {
    requireFragment(xiaomanWeeklyPreviewObservationPath, observation, fragment);
  }
  for (const fragment of ["source ", "eval ", "QIWE_TOKEN", "QIWE_GUID"]) {
    forbidFragment(xiaomanWeeklyPreviewObservationPath, observation, fragment);
  }
}
if (exists(xiaomanWeeklyPreviewActivationPath)) {
  const activation = readText(xiaomanWeeklyPreviewActivationPath);
  for (const fragment of [
    "QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_PRODUCTION_ACTIVATION",
    "approved-production-xiaoman-weekly-preview",
    "QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_ENABLED",
    "QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_PRODUCTION_APPROVAL",
    "QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE",
    "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE",
    "QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE",
    "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE",
    "QINTOPIA_XIAOMAN_LEGACY_CRON_OBSERVATION_ENABLE=1",
    "QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_PRODUCTION_RELEASE_SHA",
    'ENV_FILE="/etc/qintopia/message-sidecar.env"',
    'SYSTEMCTL="/usr/bin/systemctl"',
    'if ! "$SYSTEMCTL" enable "$TIMER_NAME"; then',
    'if ! "$SYSTEMCTL" restart "$TIMER_NAME"; then',
    '"$SYSTEMCTL" enable "$TIMER_NAME"',
    '"$SYSTEMCTL" restart "$TIMER_NAME"',
    'QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_PRODUCTION_RELEASE_SHA="$EXPECTED_RELEASE_SHA"',
    "QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_EXPECTED_STATE=enabled",
  ]) {
    requireFragment(xiaomanWeeklyPreviewActivationPath, activation, fragment);
  }
  for (const fragment of ["source ", "eval ", "QIWE_TOKEN", "QIWE_GUID"]) {
    forbidFragment(xiaomanWeeklyPreviewActivationPath, activation, fragment);
  }
}
if (exists(xiaomanWeeklyPreviewRollbackPath)) {
  const rollback = readText(xiaomanWeeklyPreviewRollbackPath);
  for (const fragment of [
    "QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_PRODUCTION_ROLLBACK",
    "approved-production-xiaoman-weekly-preview-rollback",
    "QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_ENABLED=0",
    'ENV_FILE="/etc/qintopia/message-sidecar.env"',
    'SYSTEMCTL="/usr/bin/systemctl"',
    '"$SYSTEMCTL" disable --now "$TIMER_NAME"',
  ]) {
    requireFragment(xiaomanWeeklyPreviewRollbackPath, rollback, fragment);
  }
  for (const fragment of ["source ", "eval ", "QIWE_TOKEN", "QIWE_GUID"]) {
    forbidFragment(xiaomanWeeklyPreviewRollbackPath, rollback, fragment);
  }
  const envDisabledCheck = rollback.indexOf(
    'grep -Fxq "QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_ENABLED=0"'
  );
  const timerDisable = rollback.indexOf('"$SYSTEMCTL" disable --now "$TIMER_NAME"');
  if (
    envDisabledCheck === -1 ||
    timerDisable === -1 ||
    envDisabledCheck > timerDisable
  ) {
    addError(
      `${xiaomanWeeklyPreviewRollbackPath}: must verify persistent disabled flag before mutating systemd`
    );
  }
}
const deployBundleBuilderForWeeklyPreview = readText(
  "tools/deploy/build-deploy-bundle.mjs"
);
for (const fragment of [
  xiaomanWeeklyPreviewConfigApplyPath,
  xiaomanWeeklyPreviewWorkerPath,
  xiaomanWeeklyPreviewObservationPath,
  xiaomanWeeklyPreviewActivationPath,
  xiaomanWeeklyPreviewRollbackPath,
  "deploy/sidecar/scripts/apply-xiaoman-weekly-preview-hermes-cron.sh",
  "workflows/xiaoman-weekly-preview",
]) {
  requireFragment(
    "tools/deploy/build-deploy-bundle.mjs",
    deployBundleBuilderForWeeklyPreview,
    fragment
  );
}
const xiaomanWeeklyPreviewHermesCronApplyPath =
  "deploy/sidecar/scripts/apply-xiaoman-weekly-preview-hermes-cron.sh";
if (!exists(xiaomanWeeklyPreviewHermesCronApplyPath)) {
  addError(
    `${xiaomanWeeklyPreviewHermesCronApplyPath}: missing weekly preview Hermes cron apply`
  );
} else {
  const apply = readText(xiaomanWeeklyPreviewHermesCronApplyPath);
  for (const fragment of [
    "approved-production-xiaoman-weekly-preview-hermes-cron",
    "usage: apply-xiaoman-weekly-preview-hermes-cron.sh [--install|--enable]",
    'RELEASE_CURRENT="/home/ubuntu/qintopia-agent-os-releases/current"',
    'CRON_FILE="/home/ubuntu/.hermes/profiles/xiaoman/cron/jobs.json"',
    'PROFILE_ENV_FILE="/home/ubuntu/.hermes/profiles/xiaoman/.env"',
    'WRAPPER_TARGET="${HERMES_SCRIPTS_DIR}/qintopia_xiaoman_weekly_preview.sh"',
    'SNAPSHOT_SYNC="${RELEASE_CURRENT}/deploy/sidecar/scripts/sync-hermes-cron-snapshot.sh"',
    "WECOM_HOME_CHANNEL",
    "origin_chat_id_resolved",
    "verify_installed_wrapper",
    "atomic_replace",
    "external_calls_executed",
    "safe_for_chat",
    "weekly_preview_hermes_cron_installed",
    "weekly_preview_hermes_cron_enabled",
    "weekly_preview_hermes_cron_already_enabled",
    "reviewed weekly preview job deliver mode does not match the reviewed declaration",
    "reviewed weekly preview job origin platform does not match the reviewed declaration",
    "reviewed weekly preview job origin chat id drifted from the Xiaoman profile env",
  ]) {
    requireFragment(xiaomanWeeklyPreviewHermesCronApplyPath, apply, fragment);
  }
  for (const fragment of [
    "eval ",
    "QINTOPIA_XIAOMAN_WEEKLY_PREVIEW_HERMES_CRON_FILE",
    "QINTOPIA_XIAOMAN_PROFILE_DIR",
    "QIWE_TOKEN",
    "tenant_access_token",
    "print(chat_id)",
  ]) {
    forbidFragment(xiaomanWeeklyPreviewHermesCronApplyPath, apply, fragment);
  }
}
const releaseSystemdInstallerPath = "deploy/runner/install-release-systemd-units.sh";
if (exists(releaseSystemdInstallerPath)) {
  const installer = readText(releaseSystemdInstallerPath);
  for (const fragment of [
    "qintopia-agentos-xiaoman-daily-case-report-auto-publish.service",
    "qintopia-agentos-xiaoman-daily-case-report-auto-publish.timer",
    "qintopia-agentos-xiaoman-weekly-recruitment.service",
    "qintopia-agentos-xiaoman-weekly-recruitment.timer",
    "qintopia-agentos-xiaoman-weekly-plan-confirmation.service",
    "qintopia-agentos-xiaoman-weekly-plan-confirmation.timer",
    "qintopia-agentos-xiaoman-weekly-preview.service",
    "qintopia-agentos-xiaoman-weekly-preview.timer",
    "qintopia-agentos-erhua-morning-brief.service",
    "qintopia-agentos-erhua-morning-brief.timer",
  ]) {
    requireFragment(releaseSystemdInstallerPath, installer, fragment);
  }
  const unitFilesBlock = installer.match(/unit_files=\([\s\S]*?\n\)/)?.[0] ?? "";
  const unitFiles = [
    ...unitFilesBlock.matchAll(/^\s+([^\s]+\.(?:service|timer))$/gm),
  ].map((match) => match[1]);
  const seenUnitFiles = new Set();
  for (const unitFile of unitFiles) {
    if (seenUnitFiles.has(unitFile)) {
      addError(
        `${releaseSystemdInstallerPath}: duplicate unit_files entry ${unitFile}`
      );
    }
    seenUnitFiles.add(unitFile);
  }
  const internalTimersBlock =
    installer.match(/internal_timers=\([\s\S]*?\n\)/)?.[0] ?? "";
  if (
    internalTimersBlock.includes(
      "qintopia-agentos-xiaoman-daily-case-report-auto-publish.timer"
    )
  ) {
    addError(
      `${releaseSystemdInstallerPath}: daily case report timer must not be default-enabled by release install`
    );
  }
  if (internalTimersBlock.includes("qintopia-agentos-erhua-morning-brief.timer")) {
    addError(
      `${releaseSystemdInstallerPath}: Erhua morning brief timer must not be default-enabled by release install`
    );
  }
  if (internalTimersBlock.includes("qintopia-agentos-xiaoman-weekly-preview.timer")) {
    addError(
      `${releaseSystemdInstallerPath}: Xiaoman weekly preview timer must not be default-enabled by release install`
    );
  }
  if (
    internalTimersBlock.includes("qintopia-agentos-xiaoman-weekly-recruitment.timer")
  ) {
    addError(
      `${releaseSystemdInstallerPath}: Xiaoman weekly recruitment timer must not be default-enabled by release install`
    );
  }
  if (
    internalTimersBlock.includes(
      "qintopia-agentos-xiaoman-weekly-plan-confirmation.timer"
    )
  ) {
    addError(
      `${releaseSystemdInstallerPath}: Xiaoman weekly plan confirmation timer must not be default-enabled by release install`
    );
  }
}

const huabaosiFeishuMirrorRollbackPath =
  "deploy/sidecar/scripts/rollback-huabaosi-feishu-artifact-mirror-production.sh";
if (exists(huabaosiFeishuMirrorRollbackPath)) {
  const rollback = readText(huabaosiFeishuMirrorRollbackPath);
  requireFragment(
    huabaosiFeishuMirrorRollbackPath,
    rollback,
    'ENV_FILE="/etc/qintopia/message-sidecar.env"'
  );
  requireFragment(
    huabaosiFeishuMirrorRollbackPath,
    rollback,
    'PATH="/usr/bin:/bin:/usr/sbin:/sbin"'
  );
  requireFragment(
    huabaosiFeishuMirrorRollbackPath,
    rollback,
    'SYSTEMCTL="/usr/bin/systemctl"'
  );
  forbidFragment(
    huabaosiFeishuMirrorRollbackPath,
    rollback,
    "QINTOPIA_SIDECAR_ENV_FILE"
  );
  forbidFragment(
    huabaosiFeishuMirrorRollbackPath,
    rollback,
    'SYSTEMCTL="${SYSTEMCTL:-systemctl}"'
  );
  forbidFragment(huabaosiFeishuMirrorRollbackPath, rollback, 'SYSTEMCTL="systemctl"');
}

const renderSystemdUnitsPath = "deploy/sidecar/scripts/render-systemd-units.sh";
if (!exists(renderSystemdUnitsPath)) {
  addError(`${renderSystemdUnitsPath}: missing systemd unit renderer`);
} else {
  const renderer = readText(renderSystemdUnitsPath);
  for (const fragment of [
    'local huabaosi_feishu_release_environment="QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA=${TARGET_SHA}"',
    'local huabaosi_image_release_environment="QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_RELEASE_SHA=${TARGET_SHA}',
    "ExecStart=/usr/bin/env ${release_environment}",
    '"$huabaosi_image_release_environment"',
    '"$huabaosi_feishu_release_environment"',
    "qintopia-agentos-huabaosi-image-generation-preflight.service",
    "qintopia-agentos-huabaosi-image-generation-worker.service",
    "qintopia-agentos-huabaosi-feishu-artifact-mirror-preflight.service",
    "qintopia-agentos-huabaosi-feishu-artifact-mirror-worker.service",
    "qintopia-agentos-qiwe-image-send-preflight.service",
    "qiwe-image-send-production-preflight",
    "qintopia-agentos-qiwe-image-send-worker.service",
    "run-qiwe-image-send-worker --once --apply",
    "qintopia-agentos-qiwe-image-send-worker.timer",
    "qintopia-agentos-xiaoman-daily-case-report-auto-publish.service",
    "xiaoman-daily-case-report-auto-publish-worker.sh",
    "qintopia-agentos-xiaoman-daily-case-report-auto-publish.timer",
    "qintopia-agentos-xiaoman-weekly-recruitment.service",
    "xiaoman-weekly-recruitment-worker.sh",
    "qintopia-agentos-xiaoman-weekly-recruitment.timer",
    "Sat *-*-* 10:00:00",
    "qintopia-agentos-xiaoman-weekly-plan-confirmation.service",
    "xiaoman-weekly-plan-confirmation-worker.sh",
    "qintopia-agentos-xiaoman-weekly-plan-confirmation.timer",
    "Sun *-*-* 20:00:00",
    "qintopia-agentos-xiaoman-weekly-preview.service",
    "xiaoman-weekly-preview-worker.sh",
    "qintopia-agentos-xiaoman-weekly-preview.timer",
    "OnCalendar=${calendar}",
    "run-xiaoman-feishu-poster-delivery --once --apply --conversation-scope direct",
    "run-xiaoman-feishu-poster-delivery --once --apply --conversation-scope group",
    "xiaoman-feishu-poster-preflight --conversation-scope direct",
    "xiaoman-feishu-poster-preflight --conversation-scope group",
    "render_activation_timer",
    "OnActiveSec=${activation_sec}",
    "render_calendar_timer",
    "OnCalendar=${calendar}",
    "qintopia-agentos-erhua-morning-brief.service",
    "qintopia-agentos-erhua-morning-brief.timer",
    "erhua-morning-brief-worker.sh",
    "QINTOPIA_ERHUA_MORNING_BRIEF_PYTHON=/home/ubuntu/.hermes/hermes-agent/venv/bin/python",
  ]) {
    requireFragment(renderSystemdUnitsPath, renderer, fragment);
  }
  for (const fragment of [
    "Environment=QINTOPIA_DEPLOYED_COMMIT_SHA=${TARGET_SHA}",
    "Environment=QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_RELEASE_SHA=${TARGET_SHA}",
    "Environment=QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA=${TARGET_SHA}",
  ]) {
    forbidFragment(renderSystemdUnitsPath, renderer, fragment);
  }
  forbidFragment(renderSystemdUnitsPath, renderer, '"qiwe-image-send-preflight"');
}

const erhuaMorningBriefScripts = [
  "deploy/sidecar/scripts/erhua-morning-brief-worker.sh",
  "deploy/sidecar/scripts/erhua-morning-brief-timer-observation-smoke.sh",
  "deploy/sidecar/scripts/apply-erhua-morning-brief-production-config.sh",
  "deploy/sidecar/scripts/erhua-morning-brief-one-shot-production.sh",
  "deploy/sidecar/scripts/activate-erhua-morning-brief-production.sh",
  "deploy/sidecar/scripts/rollback-erhua-morning-brief-production.sh",
  "deploy/sidecar/scripts/apply-erhua-morning-brief-hermes-cron.sh",
  "deploy/sidecar/scripts/apply-xiaoman-activity-read-through-production-config.py",
  "deploy/sidecar/scripts/erhua-legacy-cron-observation-smoke.sh",
  "deploy/sidecar/scripts/retire-erhua-legacy-cron-production.sh",
  "deploy/sidecar/scripts/xiaoman-legacy-cron-observation-smoke.sh",
  "deploy/sidecar/scripts/retire-xiaoman-legacy-cron-production.sh",
];
const erhuaMorningBriefWorkflowPath = "workflows/erhua-morning-brief/morning_brief.py";
if (exists(erhuaMorningBriefWorkflowPath)) {
  const workflow = readText(erhuaMorningBriefWorkflowPath);
  for (const fragment of [
    "cannot locate reviewed xiaoman wrapper",
    "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE",
  ]) {
    requireFragment(erhuaMorningBriefWorkflowPath, workflow, fragment);
  }
  forbidFragment(
    erhuaMorningBriefWorkflowPath,
    workflow,
    "QINTOPIA_XIAOMAN_WRAPPER_PATH"
  );
}
for (const scriptPath of erhuaMorningBriefScripts) {
  if (!exists(scriptPath)) {
    addError(`${scriptPath}: missing Erhua morning brief production script`);
    continue;
  }
  const script = readText(scriptPath);
  for (const forbidden of [
    'SYSTEMCTL="${SYSTEMCTL:-systemctl}"',
    "QINTOPIA_SIDECAR_ENV_FILE",
  ]) {
    forbidFragment(scriptPath, script, forbidden);
  }
}
for (const scriptPath of [
  "deploy/sidecar/scripts/activate-erhua-morning-brief-production.sh",
  "deploy/sidecar/scripts/rollback-erhua-morning-brief-production.sh",
]) {
  if (!exists(scriptPath)) {
    continue;
  }
  const script = readText(scriptPath);
  for (const forbidden of [
    "operations-group-message-confirm",
    "run-group-message-send-worker",
    "send_executed=true",
    "/usr/bin/env python3",
  ]) {
    forbidFragment(scriptPath, script, forbidden);
  }
}
if (exists("deploy/sidecar/scripts/erhua-morning-brief-worker.sh")) {
  const worker = readText("deploy/sidecar/scripts/erhua-morning-brief-worker.sh");
  for (const fragment of [
    'DEFAULT_HERMES_PYTHON="/home/ubuntu/.hermes/hermes-agent/venv/bin/python"',
    'PYTHON_VALIDATOR="${RELEASE_DIR}/runtime/hermes/validate_hermes_python.py"',
    "QINTOPIA_ERHUA_MORNING_BRIEF_ENABLED",
    "QINTOPIA_ERHUA_MORNING_BRIEF_PRODUCTION_APPROVAL",
    "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE",
    'SIDECAR_BIN="${RELEASE_DIR}/sidecar/qintopia-message-sidecar"',
    "refuses Xiaoman wrapper path override",
    'export QINTOPIA_XIAOMAN_ACTIVITY_WORKER_BIN="$SIDECAR_BIN"',
    "reviewed primary sidecar binary is missing",
    'QIWE_BIN="${RELEASE_DIR}/sidecar-profiles/qiwe-production/qintopia-message-sidecar"',
    "QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_PUBLISH_ENABLED",
    "approved-production-erhua-morning-brief-auto-publish",
    "QINTOPIA_ERHUA_MORNING_BRIEF_TARGET_GROUP_ID",
    "QINTOPIA_QIWE_TEXT_SEND_PRODUCTION_APPROVAL",
    "--prepare-artifact",
    "--execute-artifact-create",
    "--apply-artifact-create",
    "operations-artifact-review-decision",
    "operations-group-message-confirm",
    "run-group-message-send-worker",
    "run-qiwe-text-send-worker",
    '"external_send_executed": False',
    '"send_request_created": False',
  ]) {
    requireFragment(
      "deploy/sidecar/scripts/erhua-morning-brief-worker.sh",
      worker,
      fragment
    );
  }
  for (const fragment of [
    "QINTOPIA_XIAOMAN_WRAPPER_PATH:-",
    "QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE:=1",
    "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE:=1",
    "QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE:=1",
  ]) {
    forbidFragment(
      "deploy/sidecar/scripts/erhua-morning-brief-worker.sh",
      worker,
      fragment
    );
  }
}
if (exists("deploy/sidecar/scripts/erhua-morning-brief-one-shot-production.sh")) {
  const oneShot = readText(
    "deploy/sidecar/scripts/erhua-morning-brief-one-shot-production.sh"
  );
  for (const fragment of [
    "QINTOPIA_ERHUA_MORNING_BRIEF_ONE_SHOT",
    "approved-production-erhua-morning-brief-one-shot",
    "QINTOPIA_ERHUA_MORNING_BRIEF_ONE_SHOT_RELEASE_SHA",
    'ENV_FILE="/etc/qintopia/message-sidecar.env"',
    'PATH="/usr/bin:/bin:/usr/sbin:/sbin"',
    'SYSTEMCTL="/usr/bin/systemctl"',
    "qintopia-agentos-erhua-morning-brief.service",
    "qintopia-agentos-erhua-morning-brief.timer",
    'require_env_line "QINTOPIA_ERHUA_MORNING_BRIEF_ENABLED" "1"',
    'require_env_line "QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_PUBLISH_ENABLED" "1"',
    'require_env_line "QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_PUBLISH_APPROVAL" "approved-production-erhua-morning-brief-auto-publish"',
    'require_env_line "QINTOPIA_QIWE_TEXT_SEND_ENABLED" "1"',
    'require_env_line "QINTOPIA_QIWE_TEXT_SEND_PRODUCTION_APPROVAL" "approved-production-qiwe-text-send"',
    'require_env_line "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE" "1"',
    "QINTOPIA_ERHUA_MORNING_BRIEF_TARGET_GROUP_ID",
    "QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS",
    '"$SYSTEMCTL" is-enabled "$TIMER_NAME"',
    '"$SYSTEMCTL" is-active "$TIMER_NAME"',
    '"$SYSTEMCTL" start "$SERVICE_NAME"',
  ]) {
    requireFragment(
      "deploy/sidecar/scripts/erhua-morning-brief-one-shot-production.sh",
      oneShot,
      fragment
    );
  }
  for (const fragment of [
    "operations-group-message-confirm",
    "run-group-message-send-worker",
    "run-qiwe-text-send-worker",
    "source ",
    "eval ",
    "QINTOPIA_SIDECAR_ENV_FILE",
    'SYSTEMCTL="${SYSTEMCTL:-systemctl}"',
    'SYSTEMCTL="systemctl"',
  ]) {
    forbidFragment(
      "deploy/sidecar/scripts/erhua-morning-brief-one-shot-production.sh",
      oneShot,
      fragment
    );
  }
}
const xiaomanActivityReadThroughConfigPath =
  "deploy/sidecar/scripts/apply-xiaoman-activity-read-through-production-config.py";
if (!exists(xiaomanActivityReadThroughConfigPath)) {
  addError(
    `${xiaomanActivityReadThroughConfigPath}: missing Xiaoman activity read-through production config script`
  );
} else {
  const config = readText(xiaomanActivityReadThroughConfigPath);
  const expectedReadThroughKeys = [
    "QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_BASE_TOKEN",
    "QINTOPIA_XIAOMAN_ACTIVITY_ALLOWED_FEISHU_BASE_TOKENS",
    "QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_PLAN_TABLE_ID",
    "QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_OCCURRENCE_TABLE_ID",
    "QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_PROFILE_ENV_PATH",
  ];
  for (const fragment of [
    'SIDECAR_ENV_PATH = Path("/etc/qintopia/message-sidecar.env")',
    'PROFILE_ENV_PATH = Path("/home/ubuntu/.hermes/profiles/xiaoman/.env")',
    'RELEASE_CURRENT_PATH = RELEASE_ROOT_PATH / "current"',
    'LOCK_PATH = Path("/run/qintopia-xiaoman-activity-read-through-config.lock")',
    'APPLY_APPROVAL = "approved-production-xiaoman-activity-read-through-config-v1"',
    "expected_sidecar_uid=0",
    "expected_profile_uid=ubuntu_uid",
    "expected_profile_gid=ubuntu_gid",
    "expected_mode=0o640",
    "expected_mode=0o600",
    "Xiaoman profile env path must be the fixed production path",
    "Xiaoman activity Feishu Base token must be explicitly allowlisted",
    'FEISHU_BASE_MODE_KEY = "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE"',
    'FEISHU_BASE_MODE_VALUE = "1"',
    "MANAGED_KEYS = READ_THROUGH_KEYS + (FEISHU_BASE_MODE_KEY,)",
    "copied_key_count",
    "feishu_base_mode_enabled",
    "sensitive_values_redacted",
    "external_calls_executed",
    "service_changes_executed",
  ]) {
    requireFragment(xiaomanActivityReadThroughConfigPath, config, fragment);
  }
  for (const key of expectedReadThroughKeys) {
    requireFragment(xiaomanActivityReadThroughConfigPath, config, `"${key}"`);
  }
  const keyBlock = config.match(/READ_THROUGH_KEYS = \([\s\S]*?\n\)/)?.[0] ?? "";
  const declaredKeys = [...keyBlock.matchAll(/"([^"]+)"/g)].map((match) => match[1]);
  if (
    declaredKeys.length !== expectedReadThroughKeys.length ||
    declaredKeys.some((key) => !expectedReadThroughKeys.includes(key))
  ) {
    addError(
      `${xiaomanActivityReadThroughConfigPath}: must copy exactly the reviewed Xiaoman activity read-through keys`
    );
  }
  for (const fragment of [
    "source ",
    "eval ",
    "QIWE_TOKEN",
    "QIWE_GUID",
    "QINTOPIA_SIDECAR_ENV_FILE",
    "subprocess",
    "urllib",
    "requests",
  ]) {
    forbidFragment(xiaomanActivityReadThroughConfigPath, config, fragment);
  }
}
const xiaomanActivityReadThroughConfigTestPath =
  "tools/deploy/test_xiaoman_activity_read_through_production_config_apply.py";
if (!exists(xiaomanActivityReadThroughConfigTestPath)) {
  addError(
    `${xiaomanActivityReadThroughConfigTestPath}: missing Xiaoman activity read-through production config tests`
  );
} else {
  const test = readText(xiaomanActivityReadThroughConfigTestPath);
  for (const fragment of [
    "test_apply_copies_only_reviewed_read_through_keys_without_leaking_values",
    "test_rejects_release_current_mismatch",
    "test_apply_requires_exact_owner_approval_and_root",
    "test_rejects_bad_env_metadata_before_mutation",
    "test_rejects_duplicate_or_unsafe_profile_values",
    "test_rejects_unallowlisted_token_or_wrong_profile_path",
  ]) {
    requireFragment(xiaomanActivityReadThroughConfigTestPath, test, fragment);
  }
}
requireFragment(
  "package.json",
  readText("package.json"),
  "python3 tools/deploy/test_xiaoman_activity_read_through_production_config_apply.py"
);
if (exists("deploy/sidecar/scripts/erhua-legacy-cron-observation-smoke.sh")) {
  const legacyCronObservation = readText(
    "deploy/sidecar/scripts/erhua-legacy-cron-observation-smoke.sh"
  );
  for (const fragment of [
    "QINTOPIA_ERHUA_LEGACY_CRON_OBSERVATION_ENABLE",
    "/home/ubuntu/.hermes/profiles/erhua",
    "/home/ubuntu/.hermes/profiles/erhua/cron/jobs.json",
    "runtime/hermes/cron/reviewed-cron-jobs.json",
    'PYTHON_BIN="/usr/bin/python3"',
    "reviewed_declarations_only",
    "reviewed_decl_count",
    "cron_decl_count",
    "live_profile_modified",
    "external_calls_executed",
    "origin_platform",
    'entry.get("deliver")',
    "Erhua legacy cron observation found unreviewed cron job declarations",
  ]) {
    requireFragment(
      "deploy/sidecar/scripts/erhua-legacy-cron-observation-smoke.sh",
      legacyCronObservation,
      fragment
    );
  }
  for (const fragment of [
    "systemctl",
    "rm ",
    "mv ",
    "cp ",
    "source ",
    "eval ",
    "run-",
    "send_executed=true",
    "QIWE_TOKEN",
    "tenant_access_token",
    "\npython3 -",
  ]) {
    forbidFragment(
      "deploy/sidecar/scripts/erhua-legacy-cron-observation-smoke.sh",
      legacyCronObservation,
      fragment
    );
  }
}
if (exists("deploy/sidecar/scripts/retire-erhua-legacy-cron-production.sh")) {
  const retirement = readText(
    "deploy/sidecar/scripts/retire-erhua-legacy-cron-production.sh"
  );
  for (const fragment of [
    "QINTOPIA_ERHUA_LEGACY_CRON_RETIREMENT",
    "approved-production-erhua-legacy-cron-retirement",
    'PYTHON_BIN="/usr/bin/python3"',
    "/home/ubuntu/.hermes/profiles/erhua/cron/jobs.json",
    "59edf8abc1602a10a5ffb83120c631395d8c486df66343bfd1591a94da30412c",
    "legacy cron file sha256 does not match the reviewed production observation",
    "previous_decl_count",
    "new_decl_count",
    "backup_created",
    "external_calls_executed",
    "safe_for_chat",
  ]) {
    requireFragment(
      "deploy/sidecar/scripts/retire-erhua-legacy-cron-production.sh",
      retirement,
      fragment
    );
  }
  for (const fragment of [
    "QINTOPIA_ERHUA_PROFILE_DIR",
    "QINTOPIA_ERHUA_LEGACY_CRON_FILE",
    "source ",
    ". /etc/qintopia",
    "systemctl",
    "journalctl",
    "\npython3 -",
    "curl ",
    "psql ",
  ]) {
    forbidFragment(
      "deploy/sidecar/scripts/retire-erhua-legacy-cron-production.sh",
      retirement,
      fragment
    );
  }
}
if (exists("deploy/sidecar/scripts/retire-xiaoman-legacy-cron-production.sh")) {
  const retirement = readText(
    "deploy/sidecar/scripts/retire-xiaoman-legacy-cron-production.sh"
  );
  for (const fragment of [
    "QINTOPIA_XIAOMAN_LEGACY_CRON_RETIREMENT",
    "approved-production-xiaoman-legacy-cron-retirement",
    'PYTHON_BIN="/usr/bin/python3"',
    "/home/ubuntu/.hermes/profiles/xiaoman/cron/jobs.json",
    "2a1619eeabc82bc71e0364eff829877b1fe51be06da13e287b7753f34687eed6",
    "legacy cron file sha256 does not match the reviewed production observation",
    "previous_decl_count",
    "new_decl_count",
    "previous_mode",
    "new_mode",
    "backup_created",
    "external_calls_executed",
    "safe_for_chat",
  ]) {
    requireFragment(
      "deploy/sidecar/scripts/retire-xiaoman-legacy-cron-production.sh",
      retirement,
      fragment
    );
  }
  for (const fragment of [
    "QINTOPIA_XIAOMAN_PROFILE_DIR",
    "QINTOPIA_XIAOMAN_LEGACY_CRON_FILE",
    "source ",
    ". /etc/qintopia",
    "systemctl",
    "journalctl",
    "\npython3 -",
    "curl ",
    "psql ",
  ]) {
    forbidFragment(
      "deploy/sidecar/scripts/retire-xiaoman-legacy-cron-production.sh",
      retirement,
      fragment
    );
  }
}
if (exists("deploy/sidecar/scripts/erhua-morning-brief-timer-observation-smoke.sh")) {
  const timerObservation = readText(
    "deploy/sidecar/scripts/erhua-morning-brief-timer-observation-smoke.sh"
  );
  requireFragment(
    "deploy/sidecar/scripts/erhua-morning-brief-timer-observation-smoke.sh",
    timerObservation,
    'PYTHON_BIN="/usr/bin/python3"'
  );
  requireFragment(
    "deploy/sidecar/scripts/erhua-morning-brief-timer-observation-smoke.sh",
    timerObservation,
    'require_observed_env_value "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE" "1"'
  );
  requireFragment(
    "deploy/sidecar/scripts/erhua-morning-brief-timer-observation-smoke.sh",
    timerObservation,
    "show --property=ActiveEnterTimestamp --value"
  );
  requireFragment(
    "deploy/sidecar/scripts/erhua-morning-brief-timer-observation-smoke.sh",
    timerObservation,
    'JOURNAL_DISABLED_SINCE="30 minutes ago"'
  );
  requireFragment(
    "deploy/sidecar/scripts/erhua-morning-brief-timer-observation-smoke.sh",
    timerObservation,
    '--since "$timer_active_since" -n "$JOURNAL_LINES"'
  );
  requireFragment(
    "deploy/sidecar/scripts/erhua-morning-brief-timer-observation-smoke.sh",
    timerObservation,
    '--since "$JOURNAL_DISABLED_SINCE" -n "$JOURNAL_LINES"'
  );
  for (const fragment of [
    "require_observed_auto_publish_boundary",
    'require_observed_env_value "QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_PUBLISH_ENABLED" "1"',
    'require_observed_env_value "QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_PUBLISH_APPROVAL" "approved-production-erhua-morning-brief-auto-publish"',
    'require_observed_env_value "QINTOPIA_QIWE_TEXT_SEND_ENABLED" "1"',
    'require_observed_env_value "QINTOPIA_QIWE_TEXT_SEND_PRODUCTION_APPROVAL" "approved-production-qiwe-text-send"',
    'require_observed_env_sha256 "QINTOPIA_QIWE_TEXT_SEND_PRODUCTION_DATABASE_URL_SHA256"',
    "QINTOPIA_ERHUA_MORNING_BRIEF_TARGET_GROUP_ID",
    "QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS",
    "QINTOPIA_QIWE_IMAGE_SEND_ALLOWED_HOSTS",
  ]) {
    requireFragment(
      "deploy/sidecar/scripts/erhua-morning-brief-timer-observation-smoke.sh",
      timerObservation,
      fragment
    );
  }
  forbidFragment(
    "deploy/sidecar/scripts/erhua-morning-brief-timer-observation-smoke.sh",
    timerObservation,
    "\npython3 -"
  );
}
if (exists("deploy/sidecar/scripts/activate-erhua-morning-brief-production.sh")) {
  const activation = readText(
    "deploy/sidecar/scripts/activate-erhua-morning-brief-production.sh"
  );
  for (const fragment of [
    "approved-production-erhua-morning-brief",
    'SYSTEMCTL="/usr/bin/systemctl"',
    'ENV_FILE="/etc/qintopia/message-sidecar.env"',
    'RELEASE_CURRENT_DIR="/home/ubuntu/qintopia-agent-os-releases/current"',
    'HERMES_PYTHON="/home/ubuntu/.hermes/hermes-agent/venv/bin/python"',
    "QINTOPIA_ERHUA_LEGACY_CRON_OBSERVATION_ENABLE=1",
    "QINTOPIA_XIAOMAN_LEGACY_CRON_OBSERVATION_ENABLE=1",
    "QINTOPIA_ERHUA_MORNING_BRIEF_TIMER_OBSERVATION_ENABLE=1",
    "show --property=NextElapseUSecRealtime --value",
    'require_env_value "QINTOPIA_ERHUA_MORNING_BRIEF_ENABLED" "1"',
    'require_env_value "QINTOPIA_ERHUA_MORNING_BRIEF_PRODUCTION_APPROVAL" "approved-production-erhua-morning-brief"',
    'require_env_value "QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE" "1"',
    'require_env_value "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE" "1"',
    'require_env_value "QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE" "1"',
    'require_env_value "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE" "1"',
    "require_auto_publish_boundary",
    'require_env_value "QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_PUBLISH_ENABLED" "1"',
    'require_env_value "QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_PUBLISH_APPROVAL" "approved-production-erhua-morning-brief-auto-publish"',
    'require_env_value "QINTOPIA_QIWE_TEXT_SEND_ENABLED" "1"',
    'require_env_value "QINTOPIA_QIWE_TEXT_SEND_PRODUCTION_APPROVAL" "approved-production-qiwe-text-send"',
    'require_env_sha256 "QINTOPIA_QIWE_TEXT_SEND_PRODUCTION_DATABASE_URL_SHA256"',
    "QINTOPIA_ERHUA_MORNING_BRIEF_TARGET_GROUP_ID",
    "QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS",
    "QINTOPIA_QIWE_IMAGE_SEND_ALLOWED_HOSTS",
  ]) {
    requireFragment(
      "deploy/sidecar/scripts/activate-erhua-morning-brief-production.sh",
      activation,
      fragment
    );
  }
}
if (exists("deploy/sidecar/scripts/apply-erhua-morning-brief-production-config.sh")) {
  const config = readText(
    "deploy/sidecar/scripts/apply-erhua-morning-brief-production-config.sh"
  );
  for (const fragment of [
    "approved-production-erhua-morning-brief-config",
    'PYTHON_BIN="/usr/bin/python3"',
    'ENV_FILE="/etc/qintopia/message-sidecar.env"',
    "QINTOPIA_ERHUA_MORNING_BRIEF_ENABLED",
    "QINTOPIA_ERHUA_MORNING_BRIEF_PRODUCTION_APPROVAL",
    "QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE",
    "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE",
    "QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE",
    "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE",
    "requires exactly one QINTOPIA_SIDECAR_DATABASE_URL",
  ]) {
    requireFragment(
      "deploy/sidecar/scripts/apply-erhua-morning-brief-production-config.sh",
      config,
      fragment
    );
  }
  for (const fragment of [
    "QINTOPIA_SIDECAR_ENV_FILE",
    'SYSTEMCTL="${SYSTEMCTL:-systemctl}"',
    "source ",
    ". /etc/qintopia",
  ]) {
    forbidFragment(
      "deploy/sidecar/scripts/apply-erhua-morning-brief-production-config.sh",
      config,
      fragment
    );
  }
}

const qiweImageStagingSmokePath =
  "deploy/sidecar/scripts/qiwe-image-send-staging-smoke.sh";
const qiweImageStagingReadinessPath =
  "deploy/sidecar/scripts/qiwe-image-send-staging-readiness-smoke.sh";
if (!exists(qiweImageStagingReadinessPath)) {
  addError(`${qiweImageStagingReadinessPath}: missing QiWe staging readiness smoke`);
} else {
  const readiness = readText(qiweImageStagingReadinessPath);
  for (const fragment of [
    "QINTOPIA_QIWE_IMAGE_STAGING_READINESS_ENABLE",
    "QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL",
    "approved-staging-qiwe-image-send",
    "/etc/qintopia/message-sidecar-staging.env",
    "/home/ubuntu/qintopia-agent-os-staging-releases",
    "QINTOPIA_QIWE_IMAGE_STAGING_RELEASE_SHA",
    "QINTOPIA_QIWE_IMAGE_STAGING_SIDECAR_SHA256",
    "qiwe_image_send_staging_readiness=",
    "ready_for_staging_preflight",
    "staging env file contents are not read",
    "sidecar binary is not executed",
    "no QiWe, Postgres, Feishu, provider, media, service, or timer action",
    "path_is_secure",
    "require_executable",
    "os.access(path, os.X_OK)",
    "reject_owner_writable",
    "path_not_executable",
    "path_owner_group_or_world_writable",
    "path_group_or_world_writable",
    "path_is_symlink",
    "path_parent_is_symlink",
    "path_parent_group_or_world_writable",
    "path_parent_unexpected_owner",
    "path_unexpected_owner",
    "sidecar_hash_mismatch",
  ]) {
    requireFragment(qiweImageStagingReadinessPath, readiness, fragment);
  }
  for (const fragment of [
    "systemctl",
    "source ",
    'source "$',
    ". /etc/qintopia",
    "env -i",
    "QINTOPIA_SIDECAR_DATABASE_URL",
    "QIWE_TOKEN",
    "QIWE_GUID",
    "run-qiwe-image-send-worker",
    "process-qiwe-image-send-callback",
    "qiwe-image-send-staging-preflight",
    "subprocess",
    "curl ",
    "psql ",
  ]) {
    forbidFragment(qiweImageStagingReadinessPath, readiness, fragment);
  }
}

const qiweImageProductionObservationPath =
  "deploy/sidecar/scripts/qiwe-image-send-production-observation-smoke.sh";
if (!exists(qiweImageProductionObservationPath)) {
  addError(
    `${qiweImageProductionObservationPath}: missing QiWe production observation smoke`
  );
} else {
  const observation = readText(qiweImageProductionObservationPath);
  for (const fragment of [
    "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_OBSERVATION_ENABLE",
    "/home/ubuntu/qintopia-agent-os-releases/current",
    "/etc/qintopia/message-sidecar.env",
    "sidecar-profiles/qiwe-production/qintopia-message-sidecar",
    "artifact-manifest.json",
    "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_OBSERVATION_TEST_MODE",
    "requires the fixed production env file",
    "requires the fixed production release/current path",
    "requires the real systemctl command",
    '"huabaosi-feishu-mirror-adapter"',
    '"qiwe-production-adapter"',
    "qiwe-production",
    "QINTOPIA_QIWE_IMAGE_SEND_ENABLED",
    "parse_send_enablement",
    "expected state must be disabled, enabled, or auto",
    "production timer must not be active",
    "production timer must be active",
    "production timer must be enabled",
    "NextElapseUSecMonotonic",
    "production timer must have a future trigger",
    "qiwe_image_send_production_observation_state=",
  ]) {
    requireFragment(qiweImageProductionObservationPath, observation, fragment);
  }
  for (const fragment of [
    "cargo run",
    'source "$',
    ". /etc/qintopia",
    "eval ",
    "env -i",
    "QINTOPIA_SIDECAR_DATABASE_URL",
    "QIWE_API_URL",
    "QIWE_TOKEN",
    "QIWE_GUID",
    "QINTOPIA_QIWE_IMAGE_SEND_ALLOWED_HOSTS",
    "QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS",
    "QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS",
    "run_sidecar_with_observation_env",
    '"$SIDECAR_BIN" qiwe-image-send-preflight',
    '"$SIDECAR_BIN" run-qiwe-image-send-worker',
    "--apply",
    "process-qiwe-image-send-callback",
    '"huabaosi-production-adapter"',
    '"sidecar", "qintopia-message-sidecar"',
  ]) {
    forbidFragment(qiweImageProductionObservationPath, observation, fragment);
  }
}

const stagingSidecarArtifactBuilderPath =
  "tools/deploy/build-staging-sidecar-artifact.mjs";
if (!exists(stagingSidecarArtifactBuilderPath)) {
  addError(
    `${stagingSidecarArtifactBuilderPath}: missing staging-only sidecar artifact builder`
  );
} else {
  const builder = readText(stagingSidecarArtifactBuilderPath);
  for (const fragment of [
    "qintopia-message-sidecar",
    "huabaosi-image-generation-staging-smoke.sh",
    "qiwe-image-send-staging-smoke.sh",
    "assertContainedArtifactDirBoundary",
    "resolveApprovedTarget",
    "resolveContainedArtifactDir",
    "staging-${targetTriple}",
    '"huabaosi-staging-adapter"',
    '"qiwe-staging-adapter"',
    '"--no-default-features"',
    '"--features"',
    'cargoFeatures.join(",")',
    "manifestSha256",
    "`${bundleSha256}  ${bundleName}`",
    "`${manifestSha256}  artifact-manifest.json`",
    "staging_only: true",
    "production_eligible: false",
    "staging-sidecar-artifact",
    "refusing to build a staging artifact from a dirty or unreadable git worktree",
    "/home/ubuntu/qintopia-agent-os-staging-releases/<approved 40-hex sha>",
  ]) {
    requireFragment(stagingSidecarArtifactBuilderPath, builder, fragment);
  }
  for (const fragment of [
    "huabaosi-production-adapter",
    "huabaosi-feishu-mirror-adapter",
    "qiwe-production-adapter",
    '"--all-features"',
  ]) {
    forbidFragment(stagingSidecarArtifactBuilderPath, builder, fragment);
  }
}

const productionSidecarArtifactBuilderPath = "tools/deploy/build-sidecar-artifact.mjs";
if (exists(productionSidecarArtifactBuilderPath)) {
  const builder = readText(productionSidecarArtifactBuilderPath);
  for (const fragment of [
    "assertContainedArtifactDirBoundary",
    "resolveApprovedTarget",
    "resolveContainedArtifactDir",
    "manifestSha256",
    '"huabaosi-production-adapter"',
    '"huabaosi-feishu-mirror-adapter"',
    "`${bundleSha256}  ${bundleName}`",
    "`${manifestSha256}  artifact-manifest.json`",
  ]) {
    requireFragment(productionSidecarArtifactBuilderPath, builder, fragment);
  }
  for (const fragment of [
    '"qiwe-production-adapter"',
    '"qiwe-staging-adapter"',
    '"huabaosi-staging-adapter"',
  ]) {
    forbidFragment(productionSidecarArtifactBuilderPath, builder, fragment);
  }
}

const productionSidecarArtifactBuilderTestPath =
  "tools/deploy/test-build-sidecar-artifact.mjs";
if (!exists(productionSidecarArtifactBuilderTestPath)) {
  addError(
    `${productionSidecarArtifactBuilderTestPath}: missing production sidecar artifact builder test`
  );
} else {
  const test = readText(productionSidecarArtifactBuilderTestPath);
  for (const fragment of [
    "build-sidecar-artifact.mjs",
    'validation.artifact_profile, "huabaosi-production"',
    "huabaosi-production-adapter",
    "huabaosi-feishu-mirror-adapter",
    "dirty or unreadable git worktree",
    "Production sidecar artifact builder test passed.",
  ]) {
    requireFragment(productionSidecarArtifactBuilderTestPath, test, fragment);
  }
}

const qiweProductionSidecarArtifactBuilderPath =
  "tools/deploy/build-qiwe-production-sidecar-artifact.mjs";
if (exists(qiweProductionSidecarArtifactBuilderPath)) {
  const builder = readText(qiweProductionSidecarArtifactBuilderPath);
  for (const fragment of [
    "assertContainedArtifactDirBoundary",
    "resolveApprovedTarget",
    "resolveContainedArtifactDir",
    'const artifactProfile = "qiwe-production"',
    '"qiwe-production-adapter"',
    '"huabaosi-feishu-mirror-adapter"',
    "manifestSha256",
    "`${bundleSha256}  ${bundleName}`",
    "`${manifestSha256}  artifact-manifest.json`",
  ]) {
    requireFragment(qiweProductionSidecarArtifactBuilderPath, builder, fragment);
  }
  for (const fragment of [
    '"huabaosi-production-adapter"',
    '"qiwe-staging-adapter"',
    '"huabaosi-staging-adapter"',
  ]) {
    forbidFragment(qiweProductionSidecarArtifactBuilderPath, builder, fragment);
  }
}

const qiweProductionSidecarArtifactBuilderTestPath =
  "tools/deploy/test-build-qiwe-production-sidecar-artifact.mjs";
if (!exists(qiweProductionSidecarArtifactBuilderTestPath)) {
  addError(
    `${qiweProductionSidecarArtifactBuilderTestPath}: missing QiWe production sidecar artifact builder test`
  );
} else {
  const test = readText(qiweProductionSidecarArtifactBuilderTestPath);
  for (const fragment of [
    "build-qiwe-production-sidecar-artifact.mjs",
    'validation.artifact_profile, "qiwe-production"',
    '"huabaosi-feishu-mirror-adapter"',
    "dirty or unreadable git worktree",
    "QiWe production sidecar artifact builder test passed.",
  ]) {
    requireFragment(qiweProductionSidecarArtifactBuilderTestPath, test, fragment);
  }
}

const sidecarArtifactBoundaryHelperPath =
  "tools/deploy/sidecar-artifact-build-boundary.mjs";
if (!exists(sidecarArtifactBoundaryHelperPath)) {
  addError(`${sidecarArtifactBoundaryHelperPath}: missing artifact path safety helper`);
} else {
  const helper = readText(sidecarArtifactBoundaryHelperPath);
  for (const fragment of [
    'const approvedTarget = "linux-x86_64-gnu"',
    "artifactNamePattern.test(artifactName)",
    "QINTOPIA_ARTIFACT_TARGET must be",
    'platform !== "linux"',
    'arch !== "x64"',
    "glibcVersionRuntime",
    "linux x64 GNU runners",
    'artifactName.includes("/")',
    'artifactName.includes("\\\\")',
    'artifactName.split("-").includes("..")',
    "fs.lstatSync(currentPath)",
    "stat.isSymbolicLink()",
    "fs.mkdirSync(resolvedRoot, { recursive: true })",
    "fs.realpathSync.native(currentPath)",
    "artifact output path must match its real path",
    "requireTerminalDirectory",
    "artifact output root must be a directory",
    "path.resolve(outputRoot)",
    "!resolvedDir.startsWith(`${resolvedRoot}${path.sep}`)",
  ]) {
    requireFragment(sidecarArtifactBoundaryHelperPath, helper, fragment);
  }
}

const artifactsWorkflowPath = ".github/workflows/artifacts.yml";
if (exists(artifactsWorkflowPath)) {
  const workflow = readText(artifactsWorkflowPath);
  for (const fragment of [
    "build_staging_sidecar",
    "build-staging-sidecar",
    "staging-sidecar-artifact:",
    "github.event_name == 'workflow_dispatch'",
    "node tools/deploy/build-staging-sidecar-artifact.mjs",
    "qintopia-message-sidecar-staging-linux-x86_64-gnu",
    "dist/sidecar-artifacts/qintopia-message-sidecar-staging-linux-x86_64-gnu",
    "Prune old staging sidecar artifacts",
  ]) {
    requireFragment(artifactsWorkflowPath, workflow, fragment);
  }
  const stagingJobStart = workflow.indexOf("  staging-sidecar-artifact:");
  const qiweJobStart = workflow.indexOf("  qiwe-sidecar-artifact:", stagingJobStart);
  const deployBundleJobStart = workflow.indexOf(
    "  deploy-bundle-artifact:",
    stagingJobStart
  );
  const stagingJobEnd =
    qiweJobStart >= 0 && qiweJobStart < deployBundleJobStart
      ? qiweJobStart
      : deployBundleJobStart;
  const stagingJob = workflow.slice(stagingJobStart, stagingJobEnd);
  for (const fragment of [
    "Upload staging sidecar artifact to Tencent COS",
    "Prune old staging sidecar artifacts from Tencent COS",
    "QINTOPIA_SIDECAR_ARTIFACT_PROFILE: combined-staging",
    "TENCENT_COS_SECRET_ID: ${{ secrets.TENCENT_COS_SECRET_ID }}",
    "TENCENT_COS_SECRET_KEY: ${{ secrets.TENCENT_COS_SECRET_KEY }}",
  ]) {
    if (!stagingJob.includes(fragment)) {
      addError(`${artifactsWorkflowPath}: staging job must include ${fragment}`);
    }
  }
  if (
    stagingJob.includes(
      "    env:\n      TENCENT_COS_SECRET_ID: ${{ secrets.TENCENT_COS_SECRET_ID }}"
    ) ||
    stagingJob.includes(
      "    env:\n      TENCENT_COS_SECRET_KEY: ${{ secrets.TENCENT_COS_SECRET_KEY }}"
    )
  ) {
    addError(`${artifactsWorkflowPath}: staging COS secrets must remain step-scoped`);
  }
  for (const fragment of [
    "build_qiwe_sidecar",
    "build-qiwe-sidecar",
    "qiwe-sidecar-artifact:",
    "node tools/deploy/build-qiwe-production-sidecar-artifact.mjs",
    "qintopia-message-sidecar-qiwe-production-linux-x86_64-gnu",
  ]) {
    requireFragment(artifactsWorkflowPath, workflow, fragment);
  }
}

if (!exists(qiweImageStagingSmokePath)) {
  addError(`${qiweImageStagingSmokePath}: missing QiWe image-send staging smoke`);
} else {
  const smoke = readText(qiweImageStagingSmokePath);
  for (const fragment of [
    "QINTOPIA_QIWE_IMAGE_STAGING_SMOKE_ENABLE",
    "QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL",
    "approved-staging-qiwe-image-send",
    "QINTOPIA_QIWE_IMAGE_STAGING_ENV_FILE",
    "QINTOPIA_QIWE_IMAGE_STAGING_DATABASE_URL_SHA256",
    "QINTOPIA_QIWE_IMAGE_STAGING_RELEASE_SHA",
    "QINTOPIA_QIWE_IMAGE_STAGING_SIDECAR_SHA256",
    "QINTOPIA_QIWE_IMAGE_STAGING_WORK_ITEM_ID",
    "QINTOPIA_QIWE_IMAGE_STAGING_PHASE",
    'PHASE" != "preflight"',
    "QINTOPIA_QIWE_IMAGE_STAGING_PHASE must be preflight, upload, or callback",
    "QINTOPIA_QIWE_IMAGE_STAGING_SIDECAR_SHA256 must be a canonical SHA-256",
    "QINTOPIA_QIWE_IMAGE_STAGING_RELEASE_SHA must be a 40-character lowercase hex SHA",
    "QINTOPIA_QIWE_IMAGE_STAGING_SMOKE_TEST_MODE must be 0 or 1",
    "QiWe staging smoke must run from /home/ubuntu/qintopia-agent-os-staging-releases/<approved 40-hex sha>",
    "QiWe staging smoke test mode may read only a temporary fake env file",
    "QiWe staging smoke test mode requires a loopback fake database URL",
    "QiWe staging smoke test mode requires a fake loopback or example.test QiWe API host",
    "packaged sidecar/qintopia-message-sidecar is required for QiWe staging smoke",
    "verify_sidecar_binary",
    "packaged sidecar binary hash changed before",
    "packaged sidecar binary must stay under the fixed staging release root before",
    "packaged sidecar binary must come from /home/ubuntu/qintopia-agent-os-staging-releases/<approved 40-hex sha> before",
    "qintopia-agent-os-staging-releases",
    "packaged sidecar binary, parent directory, release root, and staging releases root must not be symlinks before",
    "packaged sidecar release ancestors, parent directory, and binary must keep the expected file types before",
    "packaged sidecar release ancestors, parent directory, and binary must be executable before",
    "packaged sidecar release ancestors, parent directory, and binary must be owned by root or the staging runner user before",
    "packaged sidecar binary and parent directory must not be owner/group/world writable, and release ancestors must not be group/world writable before",
    "unexpected_owner",
    "os.geteuid()",
    "sidecar_binary_sha256",
    "artifact_content_hash",
    "feishu_delivery_bridge_compiled",
    "qiwe-image-send-staging-preflight",
    "run-qiwe-image-send-worker",
    "process-qiwe-image-send-callback",
    "image_upload_accepted",
    "image_send_completed",
    'payload["external_send_executed"] is True',
    "callback_credential_schema",
    "contains forbidden sensitive output",
    "CHILD_ENV",
    "add_child_env",
    "add_child_env_if_set",
    "env -i",
    'verify_sidecar_binary "$label spawn"',
    'output="$(env -i "${CHILD_ENV[@]}" "$@" 2>&1)"',
    'assert_no_sensitive_text "$label output" "$output"',
    "SANITIZED_OUTPUT",
    "qiwe_image_send_staging_evidence=",
    "emit_sanitized_evidence",
    "payload = json.load(sys.stdin)",
    "fileAesKey",
    "fileAeskey",
    "fileId",
    "fileMd5",
    "fileSize",
    "requestId",
    "STAGING_ENV_KEYS",
    "QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS",
    "load_staging_env",
    "qiwe-image-send-staging-preflight </dev/null",
    "--apply </dev/null",
  ]) {
    requireFragment(qiweImageStagingSmokePath, smoke, fragment);
  }
  for (const fragment of [
    "systemctl",
    "callback.json",
    "run-group-message-send-worker",
    "operations-group-message-confirm",
    "--use-feishu-base",
    'source "$ENV_FILE"',
    '>"$stdout_file"',
    '2>"$stderr_file"',
    "mktemp",
    "report_file",
    "preflight_output",
    "phase_output",
    "--features qiwe-staging-adapter",
    "cargo run",
    "source_fallback",
    "QINTOPIA_SIDECAR_BIN",
    "SANITIZED_EVIDENCE_PAYLOAD",
    "json.loads(os.environ",
  ]) {
    forbidFragment(qiweImageStagingSmokePath, smoke, fragment);
  }
}

const qiweImageStagingSmokeTestPath = "tools/deploy/test-qiwe-image-staging-smoke.mjs";
if (!exists(qiweImageStagingSmokeTestPath)) {
  addError(`${qiweImageStagingSmokeTestPath}: missing QiWe staging smoke test`);
} else {
  const test = readText(qiweImageStagingSmokeTestPath);
  for (const fragment of [
    "QINTOPIA_UNRELATED_RUNTIME_SECRET",
    "ambient secret reached child process",
    "expected source checkout staging smoke to fail closed",
    "source checkout failure did not enforce fixed release root",
    "tamper-after-preflight",
    "expected sidecar tampering before upload spawn to fail",
    "before QiWe staging upload spawn",
    "QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS=media.example.test",
    "QiWe image-send staging smoke test passed.",
  ]) {
    requireFragment(qiweImageStagingSmokeTestPath, test, fragment);
  }
}

const qiweImageStagingReadinessTestPath =
  "tools/deploy/test-qiwe-image-staging-readiness.mjs";
if (!exists(qiweImageStagingReadinessTestPath)) {
  addError(`${qiweImageStagingReadinessTestPath}: missing QiWe staging readiness test`);
} else {
  const test = readText(qiweImageStagingReadinessTestPath);
  for (const fragment of [
    "QiWe image-send staging readiness smoke test passed.",
    "expected missing readiness inputs to fail",
    "expected owner-writable sidecar to fail readiness",
    "expected non-executable sidecar to fail readiness",
    "expected owner-executable sidecar to pass readiness",
    "expected symlink parent path to fail readiness",
    "ready_for_staging_preflight",
    "readiness smoke exposed staging env contents",
    "env_file_path_parent_is_symlink",
    "sidecar_binary_path_not_executable",
    "sidecar_hash_mismatch",
  ]) {
    requireFragment(qiweImageStagingReadinessTestPath, test, fragment);
  }
}

const qiweImageStagingRunbookPath =
  "docs/operations/qiwe-image-send-staging-runbook.md";
if (!exists(qiweImageStagingRunbookPath)) {
  addError(`${qiweImageStagingRunbookPath}: missing QiWe staging runbook`);
} else {
  const runbook = readText(qiweImageStagingRunbookPath);
  for (const fragment of [
    "QINTOPIA_QIWE_IMAGE_STAGING_READINESS_ENABLE=1",
    "QINTOPIA_QIWE_IMAGE_STAGING_SMOKE_ENABLE=1",
    "QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL=approved-staging-qiwe-image-send",
    "QINTOPIA_QIWE_IMAGE_STAGING_RELEASE_SHA='<approved staging release sha>'",
    "deploy/sidecar/scripts/qiwe-image-send-staging-readiness-smoke.sh",
    "does not read the env file contents",
    "execute the sidecar, connect to",
    "Postgres, call QiWe, or touch services",
    "QINTOPIA_QIWE_IMAGE_STAGING_PHASE=preflight",
    "QINTOPIA_QIWE_IMAGE_STAGING_PHASE=upload",
    "QINTOPIA_QIWE_IMAGE_STAGING_PHASE=callback",
    "QINTOPIA_QIWE_IMAGE_STAGING_ENV_FILE=/etc/qintopia/message-sidecar-staging.env",
    "QINTOPIA_QIWE_IMAGE_STAGING_DATABASE_URL_SHA256='<approved staging database URL sha256>'",
    "QINTOPIA_QIWE_IMAGE_STAGING_RELEASE_SHA='<approved staging release sha>'",
    "QINTOPIA_QIWE_IMAGE_STAGING_RELEASE_SHA='<same approved staging release sha>'",
    "QINTOPIA_QIWE_IMAGE_STAGING_SIDECAR_SHA256='<approved staging sidecar binary sha256>'",
    "QINTOPIA_QIWE_IMAGE_STAGING_WORK_ITEM_ID='<approved send-ready UUID>'",
    "QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS",
    "trusted-staging-callback-source |",
    "callback credential schema id",
    "qiwe_image_send_staging_evidence=<json>",
    "node tools/deploy/check-qiwe-image-staging-evidence.mjs <staging-evidence-output.txt>",
    "node tools/deploy/check-qiwe-image-staging-evidence.mjs --preflight-only <preflight-evidence-output.txt>",
    "node tools/deploy/check-xiaoman-image-send-staging-evidence.mjs <huabaosi-staging-evidence-output.txt> <qiwe-staging-evidence-output.txt>",
    "external_send_executed",
    "artifact_content_hash",
    "QINTOPIA_QIWE_IMAGE_SEND_ENABLED=0",
    "Do not add production listener, service, timer, or release activation",
  ]) {
    requireFragment(qiweImageStagingRunbookPath, runbook, fragment);
  }
  for (const fragment of [
    'source "$ENV_FILE"',
    ". /etc/qintopia/message-sidecar-staging.env",
    "callback.json",
    "QIWE_TOKEN=",
    "QIWE_GUID=",
    "systemctl enable",
    "systemctl start",
    "gh release",
  ]) {
    forbidFragment(qiweImageStagingRunbookPath, runbook, fragment);
  }
}

const qiweImageStagingEvidenceCheckPath =
  "tools/deploy/check-qiwe-image-staging-evidence.mjs";
if (!exists(qiweImageStagingEvidenceCheckPath)) {
  addError(
    `${qiweImageStagingEvidenceCheckPath}: missing QiWe staging evidence checker`
  );
} else {
  const checker = readText(qiweImageStagingEvidenceCheckPath);
  for (const fragment of [
    "qiwe_image_send_staging_evidence=",
    "--preflight-only",
    "complete evidence requires preflight, upload, and callback records",
    "upload and callback work_item_id values differ",
    "upload and callback artifact_content_hash values differ",
    "forbidden sensitive fragment appeared in evidence",
    "unexpected non-evidence line",
    "callback_credential_schema",
    "artifact_content_hash",
    "sidecar_binary_sha256",
    "feishu_delivery_bridge_compiled",
    "external_send_executed",
    "image_send_completed",
    "complete evidence records must use the same sidecar binary hash",
  ]) {
    requireFragment(qiweImageStagingEvidenceCheckPath, checker, fragment);
  }
  for (const fragment of ["fetch(", "systemctl", "process.env.QIWE_TOKEN"]) {
    forbidFragment(qiweImageStagingEvidenceCheckPath, checker, fragment);
  }
}

const xiaomanImageSendStagingEvidenceCheckPath =
  "tools/deploy/check-xiaoman-image-send-staging-evidence.mjs";
if (!exists(xiaomanImageSendStagingEvidenceCheckPath)) {
  addError(
    `${xiaomanImageSendStagingEvidenceCheckPath}: missing Xiaoman image-send staging evidence checker`
  );
} else {
  const checker = readText(xiaomanImageSendStagingEvidenceCheckPath);
  for (const fragment of [
    "huabaosi_image_generation_staging_evidence=",
    "qiwe_image_send_staging_evidence=",
    "Huabaosi content_hash and QiWe artifact_content_hash values differ",
    "QiWe upload and callback artifact_content_hash values differ",
    "Huabaosi and QiWe sidecar_binary_sha256 values differ",
    "QiWe preflight evidence does not prove staging send readiness",
    "Huabaosi preflight evidence does not prove staging adapter readiness",
    "forbidden sensitive fragment",
    "image_send_completed",
    "generated_image_created",
    "Xiaoman image-send staging evidence check passed.",
  ]) {
    requireFragment(xiaomanImageSendStagingEvidenceCheckPath, checker, fragment);
  }
  for (const fragment of ["fetch(", "systemctl", "process.env.QIWE_TOKEN"]) {
    forbidFragment(xiaomanImageSendStagingEvidenceCheckPath, checker, fragment);
  }
}

const xiaomanRealActivityProductionEvidenceCheckPath =
  "tools/deploy/check-xiaoman-real-activity-production-evidence.mjs";
if (!exists(xiaomanRealActivityProductionEvidenceCheckPath)) {
  addError(
    `${xiaomanRealActivityProductionEvidenceCheckPath}: missing Xiaoman real activity production evidence checker`
  );
} else {
  const checker = readText(xiaomanRealActivityProductionEvidenceCheckPath);
  for (const fragment of [
    "usage: node tools/deploy/check-xiaoman-real-activity-production-evidence.mjs <production-evidence-output.txt>",
    "xiaoman_real_activity_production_evidence=",
    "signal_intake",
    "image_generation",
    "human_approval",
    "send_ready",
    "qiwe_upload",
    "qiwe_callback_send",
    "sanitized_evidence_retention",
    "artifact_content_hash",
    "runtime_artifact_profile",
    'record.runtime_artifact_profile !== "qiwe-production"',
    "callback_credential_schema",
    "raw_secret_fields_retained",
    "release_binary_verified",
    "approved_sidecar_sha256_matched",
    "approved_database_url_sha256_matched",
    "forbidden sensitive fragment",
    "Xiaoman real activity production evidence check passed.",
  ]) {
    requireFragment(xiaomanRealActivityProductionEvidenceCheckPath, checker, fragment);
  }
  for (const fragment of ["fetch(", "systemctl", "process.env.QIWE_TOKEN"]) {
    forbidFragment(xiaomanRealActivityProductionEvidenceCheckPath, checker, fragment);
  }
}

const xiaomanQiweArrivalConfirmationEvidenceCheckPath =
  "tools/deploy/check-xiaoman-qiwe-group-arrival-confirmation-evidence.mjs";
if (!exists(xiaomanQiweArrivalConfirmationEvidenceCheckPath)) {
  addError(
    `${xiaomanQiweArrivalConfirmationEvidenceCheckPath}: missing Xiaoman QiWe group arrival confirmation evidence checker`
  );
} else {
  const checker = readText(xiaomanQiweArrivalConfirmationEvidenceCheckPath);
  for (const fragment of [
    "usage: node tools/deploy/check-xiaoman-qiwe-group-arrival-confirmation-evidence.mjs <production-evidence-output.txt> <qiwe-group-arrival-confirmation-output.txt>",
    "xiaoman_qiwe_group_arrival_confirmation_evidence=",
    "xiaoman-qiwe-group-arrival-confirmation-evidence-v1",
    "check-xiaoman-real-activity-production-evidence.mjs",
    "<production-evidence-output.txt> <qiwe-group-arrival-confirmation-output.txt>",
    "production real activity evidence failed",
    "human_visible_group_check",
    "community_activity_group",
    "send_ready_work_item_id",
    "generated_image_artifact_id",
    "artifact_content_hash",
    "raw_secret_fields_retained",
    "QiWe group arrival confirmation does not bind to the real activity send",
    "forbidden sensitive fragment",
    "Xiaoman QiWe group arrival confirmation evidence check passed.",
  ]) {
    requireFragment(xiaomanQiweArrivalConfirmationEvidenceCheckPath, checker, fragment);
  }
  forbidFragment(
    xiaomanQiweArrivalConfirmationEvidenceCheckPath,
    checker,
    "<group-arrival-confirmation-output.txt>"
  );
  for (const fragment of ["fetch(", "systemctl", "process.env.QIWE_TOKEN"]) {
    forbidFragment(xiaomanQiweArrivalConfirmationEvidenceCheckPath, checker, fragment);
  }
}

const xiaomanProductionCompletionEvidenceCheckPath =
  "tools/deploy/check-xiaoman-production-completion-evidence.mjs";
if (!exists(xiaomanProductionCompletionEvidenceCheckPath)) {
  addError(
    `${xiaomanProductionCompletionEvidenceCheckPath}: missing Xiaoman production completion evidence checker`
  );
} else {
  const checker = readText(xiaomanProductionCompletionEvidenceCheckPath);
  for (const fragment of [
    "usage: node tools/deploy/check-xiaoman-production-completion-evidence.mjs --manifest <completed-xiaoman-production-completion-evidence.json> --staging-runtime-readiness <readiness-output.txt> --huabaosi-staging <huabaosi-output.txt> --qiwe-staging <qiwe-output.txt> --huabaosi-production-canary <huabaosi-production-canary-output.txt> --production-real-activity <production-evidence-output.txt> --qiwe-group-arrival-confirmation <qiwe-group-arrival-confirmation-output.txt> --daily-case-report-observation <production-observation-deploy-result.json>",
    "xiaoman-production-completion-evidence-v1",
    "check-huabaosi-image-staging-evidence.mjs",
    "check-qiwe-image-staging-evidence.mjs",
    "check-xiaoman-image-send-staging-evidence.mjs",
    "check-xiaoman-real-activity-production-evidence.mjs",
    "check-xiaoman-qiwe-group-arrival-confirmation-evidence.mjs",
    "staging_runtime_readiness_evidence=",
    "huabaosi_image_generation_production_canary_evidence=",
    "--huabaosi-production-canary",
    "--qiwe-group-arrival-confirmation",
    "--daily-case-report-observation",
    "daily_case_report_confirmation",
    "xiaoman-daily-case-report-worker-run",
    "xiaoman-character-universe-v1",
    "daily_case_report_second_pass",
    "raw_messages_included",
    "profile_fact_text_included",
    "xiaoman_qiwe_group_arrival_confirmation_evidence=",
    "does not bind to QiWe group arrival evidence",
    "prerequisite",
    "huabaosi_readiness",
    "qiwe_readiness",
    "isUtcSecondTimestamp",
    "qiwe_production_enablement",
    "release_tag",
    "released_commit_sha",
    "included_in_release_sha",
    "runtime_artifact_profile",
    "release facts do not bind to the deployed production release",
    "listener_service_timer_reviewed",
    "production_feature_boundary_reviewed",
    "huabaosi_production_activation",
    "QiWe real activity production facts do not bind to retained evidence",
    "Huabaosi production activation facts do not bind to canary evidence",
    "--manifest <completed-xiaoman-production-completion-evidence.json>",
    "--production-real-activity <production-evidence-output.txt>",
    "first_record_evidence_retained",
    "brief_review",
    "request_intake",
    "feishu_primary_storage_revalidated",
    "Huabaosi production canary first record does not bind to real activity image",
    "qiwe_group_arrival_confirmed",
    "release_binary_verified",
    "approved_sidecar_sha256_matched",
    "approved_database_url_sha256_matched",
    "Xiaoman production completion evidence check passed.",
  ]) {
    requireFragment(xiaomanProductionCompletionEvidenceCheckPath, checker, fragment);
  }
  forbidFragment(
    xiaomanProductionCompletionEvidenceCheckPath,
    checker,
    "--manifest <completion-manifest.json>"
  );
  forbidFragment(
    xiaomanProductionCompletionEvidenceCheckPath,
    checker,
    "--production-real-activity <production-output.txt>"
  );
  for (const fragment of ["fetch(", "systemctl", "process.env.QIWE_TOKEN"]) {
    forbidFragment(xiaomanProductionCompletionEvidenceCheckPath, checker, fragment);
  }
}

const xiaomanProductionCompletionManifestBuilderPath =
  "tools/deploy/build-xiaoman-production-completion-manifest.mjs";
if (!exists(xiaomanProductionCompletionManifestBuilderPath)) {
  addError(
    `${xiaomanProductionCompletionManifestBuilderPath}: missing Xiaoman production completion manifest builder`
  );
} else {
  const builder = readText(xiaomanProductionCompletionManifestBuilderPath);
  for (const fragment of [
    "usage: node tools/deploy/build-xiaoman-production-completion-manifest.mjs --release-please-pr-number <number> --release-please-head-sha <sha> --release-tag <vX.Y.Z> --released-commit-sha <sha> --qiwe-production-enablement-pr-number <number> --qiwe-production-enablement-head-sha <sha> --huabaosi-production-canary <huabaosi-production-canary-output.txt> --production-real-activity <production-evidence-output.txt> --qiwe-group-arrival-confirmation <qiwe-group-arrival-confirmation-output.txt> --daily-case-report-observation <production-observation-deploy-result.json> [--output <completed-xiaoman-production-completion-evidence.json>]",
    "xiaoman-production-completion-evidence-v1",
    "check-huabaosi-image-production-canary-evidence.mjs",
    "check-xiaoman-real-activity-production-evidence.mjs",
    "check-xiaoman-qiwe-group-arrival-confirmation-evidence.mjs",
    "assertGithubReleaseFacts(options)",
    "gh",
    "pr",
    "view",
    "api",
    "statusCheckRollup",
    "mergeCommit",
    "Release Please validation",
    "assertReleasePleaseChecks",
    "releasePleaseValidationWorkflowRun",
    "releasePleaseValidationRunId",
    "hasSuccessfulJob",
    "Release Please validation workflow run",
    "headSha,status,conclusion,jobs",
    "actions\\/runs",
    "Published GitHub Release",
    "Published Git tag ref",
    "Published annotated Git tag",
    "QiWe production enablement inclusion",
    "releases/tags/${options.releaseTag}",
    "git/ref/tags/${options.releaseTag}",
    "draft",
    "prerelease",
    "pullRequestRevisionIncludedInRelease",
    "commitIncludedInRelease",
    "compare/${candidateSha}...${releasedCommitSha}",
    "head or merge commit",
    "huabaosi_image_generation_production_canary_evidence=",
    "xiaoman_real_activity_production_evidence=",
    "xiaoman_qiwe_group_arrival_confirmation_evidence=",
    "--release-please-pr-number",
    "--release-please-head-sha",
    "--release-tag",
    "--released-commit-sha",
    "--qiwe-production-enablement-pr-number",
    "--qiwe-production-enablement-head-sha",
    "--huabaosi-production-canary <huabaosi-production-canary-output.txt>",
    "--production-real-activity <production-evidence-output.txt>",
    "--qiwe-group-arrival-confirmation <qiwe-group-arrival-confirmation-output.txt>",
    "--daily-case-report-observation <production-observation-deploy-result.json>",
    "daily_case_report_confirmation",
    "extractDailyCaseReportObservation",
    "xiaoman-daily-case-report-worker-run",
    "xiaoman-character-universe-v1",
    "daily_case_report_second_pass",
    "--output <completed-xiaoman-production-completion-evidence.json>",
    "assertNoSensitiveOutput(output)",
    "forbiddenOutputPatterns",
    "released_commit_sha",
    "release_tag",
    "included_in_release_sha",
    "runtime_artifact_profile",
    "qiwe_group_arrival_confirmed",
    "safeDiagnostic",
    "redacted-sensitive-diagnostic",
    "artifact_profile=huabaosi-production",
    "runtime_artifact_profile=qiwe-production",
    "forbiddenOutputPatterns.some",
    "must be merged in GitHub before manifest generation",
    "does not match GitHub state",
    "published GitHub Release tag does not point to the released commit SHA",
    "is not included in the released commit SHA",
  ]) {
    requireFragment(xiaomanProductionCompletionManifestBuilderPath, builder, fragment);
  }
  for (const fragment of [
    "fetch(",
    "systemctl",
    "process.env.QIWE_TOKEN",
    "process.env.DATABASE_URL",
    "child_process.exec",
    "https://",
    "postgres://",
    "postgresql://",
  ]) {
    forbidFragment(xiaomanProductionCompletionManifestBuilderPath, builder, fragment);
  }
  forbidFragment(
    xiaomanProductionCompletionManifestBuilderPath,
    builder,
    "--production-real-activity <output.txt>"
  );
  forbidFragment(
    xiaomanProductionCompletionManifestBuilderPath,
    builder,
    "--qiwe-group-arrival-confirmation <output.txt>"
  );
  forbidFragment(
    xiaomanProductionCompletionManifestBuilderPath,
    builder,
    "[--output <manifest.json>]"
  );
}

const xiaomanTextAnnouncementMvpPath =
  "docs/plans/active/xiaoman-text-announcement-mvp.md";
if (!exists(xiaomanTextAnnouncementMvpPath)) {
  addError(`${xiaomanTextAnnouncementMvpPath}: missing Xiaoman text MVP plan`);
} else {
  const plan = readText(xiaomanTextAnnouncementMvpPath);
  for (const fragment of [
    "No QiWe call, group delivery, publish, or send-ready mutation.",
    "The request",
    "remains `awaiting_publish`",
    "external_send_executed=false",
    "not a production-complete evidence path",
    "does not prove image generation",
    "Huabaosi/Feishu approval, QiWe adapter execution, visible group arrival",
    "production completion retention",
    "check-xiaoman-real-activity-production-evidence.mjs",
    "check-xiaoman-qiwe-group-arrival-confirmation-evidence.mjs",
    "full completion",
    "manifest checker",
    "cannot be reported as Xiaoman production-complete or QiWe group-delivered",
    "evidence.",
  ]) {
    requireFragment(xiaomanTextAnnouncementMvpPath, plan, fragment);
  }
  for (const fragment of [
    "systemctl enable",
    "systemctl start",
    "gh release",
    "QIWE_TOKEN=",
    "QIWE_GUID=",
    "postgres://",
    "postgresql://",
  ]) {
    forbidFragment(xiaomanTextAnnouncementMvpPath, plan, fragment);
  }
}

const xiaomanProductionCompletionFinalizerPath =
  "tools/deploy/finalize-xiaoman-production-completion-evidence.mjs";
if (!exists(xiaomanProductionCompletionFinalizerPath)) {
  addError(
    `${xiaomanProductionCompletionFinalizerPath}: missing Xiaoman production completion evidence finalizer`
  );
} else {
  const finalizer = readText(xiaomanProductionCompletionFinalizerPath);
  for (const fragment of [
    "usage: node tools/deploy/finalize-xiaoman-production-completion-evidence.mjs --release-please-pr-number <number> --release-please-head-sha <sha> --release-tag <vX.Y.Z> --released-commit-sha <sha> --qiwe-production-enablement-pr-number <number> --qiwe-production-enablement-head-sha <sha> --staging-runtime-readiness <staging-runtime-readiness-output.txt> --huabaosi-staging <huabaosi-staging-output.txt> --qiwe-staging <qiwe-staging-output.txt> --huabaosi-production-canary <huabaosi-production-canary-output.txt> --production-real-activity <production-evidence-output.txt> --qiwe-group-arrival-confirmation <qiwe-group-arrival-confirmation-output.txt> --daily-case-report-observation <production-observation-deploy-result.json> --output <completed-xiaoman-production-completion-evidence.json>",
    "build-xiaoman-production-completion-manifest.mjs",
    "check-xiaoman-production-completion-evidence.mjs",
    "--staging-runtime-readiness",
    "--huabaosi-staging",
    "--qiwe-staging",
    "--huabaosi-production-canary",
    "--production-real-activity",
    "--qiwe-group-arrival-confirmation",
    "--daily-case-report-observation",
    "--output",
    "Xiaoman production completion evidence finalized:",
  ]) {
    requireFragment(xiaomanProductionCompletionFinalizerPath, finalizer, fragment);
  }
  for (const fragment of ["fetch(", "gh ", "systemctl", "QIWE_TOKEN="]) {
    forbidFragment(xiaomanProductionCompletionFinalizerPath, finalizer, fragment);
  }
}

const xiaomanRealActivityEvidenceRuntimePath =
  "runtime/sidecar/src/xiaoman_real_activity_evidence.rs";
if (!exists(xiaomanRealActivityEvidenceRuntimePath)) {
  addError(
    `${xiaomanRealActivityEvidenceRuntimePath}: missing Xiaoman production evidence exporter`
  );
} else {
  const source = readText(xiaomanRealActivityEvidenceRuntimePath);
  for (const fragment of [
    "xiaoman_real_activity_production_evidence=",
    "QINTOPIA_DEPLOYED_COMMIT_SHA",
    "QINTOPIA_XIAOMAN_REAL_ACTIVITY_PRODUCTION_SIDECAR_SHA256",
    "QINTOPIA_XIAOMAN_REAL_ACTIVITY_PRODUCTION_DATABASE_URL_SHA256",
    "/home/ubuntu/qintopia-agent-os-releases/current",
    "current_exe",
    "canonicalize",
    "owner-approved SHA-256",
    "configured database URL does not match the owner-approved SHA-256",
    "artifact-manifest.json",
    "validation.artifact_profile",
    "qiwe-production-adapter",
    "reviewed QiWe production artifact",
    "database_url_sha256",
    "release_binary_verified",
    "approved_sidecar_sha256_matched",
    "approved_database_url_sha256_matched",
    "signal_intake",
    "image_generation",
    "human_approval",
    "send_ready",
    "qiwe_upload",
    "qiwe_callback_send",
    "sanitized_evidence_retention",
    "callback_credential_schema",
    "target_group_alias",
    "community_activity_group",
  ]) {
    requireFragment(xiaomanRealActivityEvidenceRuntimePath, source, fragment);
  }
  for (const fragment of [
    "reqwest",
    "HttpClient",
    "systemctl",
    "INSERT INTO",
    "UPDATE qintopia_agent_os",
    'target_group_id":',
    'artifact_uri":',
    'provider_message_id_sha256":',
  ]) {
    forbidFragment(xiaomanRealActivityEvidenceRuntimePath, source, fragment);
  }
}

const qiweImageStagingEvidenceTemplatePath =
  "docs/reports/templates/qiwe-image-send-staging-evidence.md";
if (!exists(qiweImageStagingEvidenceTemplatePath)) {
  addError(
    `${qiweImageStagingEvidenceTemplatePath}: missing QiWe staging evidence template`
  );
} else {
  const template = readText(qiweImageStagingEvidenceTemplatePath);
  for (const fragment of [
    "node tools/deploy/check-qiwe-image-staging-evidence.mjs <staging-evidence-output.txt>",
    "Repository commit SHA",
    "Packaged sidecar binary SHA-256",
    "Staging database URL SHA-256",
    "Work item UUID",
    "Final JPEG `artifact_content_hash`",
    "Target group allowlist: isolated single group confirmed, identifier not recorded.",
    "Rollback owner",
    "Rollback action",
    "External upload requested",
    "External send executed",
    "sidecar_binary_sha256",
    "artifact_content_hash",
    "callback_credential_schema",
    "callback_additional_field_count",
    "Complete evidence checker mode passed",
    "Cross-flow Huabaosi/QiWe hash checker passed",
    "Production enablement PR allowed",
    "Do not record QiWe token, GUID, API secret material, target group id, database URL",
  ]) {
    requireFragment(qiweImageStagingEvidenceTemplatePath, template, fragment);
  }
  for (const fragment of [
    "QIWE_TOKEN=",
    "QIWE_GUID=",
    "postgres://",
    "postgresql://",
    "callback.json",
    "systemctl enable",
    "systemctl start",
    "gh release",
  ]) {
    forbidFragment(qiweImageStagingEvidenceTemplatePath, template, fragment);
  }
}

const xiaomanImageSendStagingEvidenceTemplatePath =
  "docs/reports/templates/xiaoman-image-send-staging-evidence.md";
if (!exists(xiaomanImageSendStagingEvidenceTemplatePath)) {
  addError(
    `${xiaomanImageSendStagingEvidenceTemplatePath}: missing Xiaoman image-send staging evidence template`
  );
} else {
  const template = readText(xiaomanImageSendStagingEvidenceTemplatePath);
  for (const fragment of [
    "node tools/deploy/check-huabaosi-image-staging-evidence.mjs <huabaosi-staging-evidence-output.txt>",
    "node tools/deploy/check-qiwe-image-staging-evidence.mjs <qiwe-staging-evidence-output.txt>",
    "node tools/deploy/check-xiaoman-image-send-staging-evidence.mjs <huabaosi-staging-evidence-output.txt> <qiwe-staging-evidence-output.txt>",
    "Huabaosi image request work item UUID",
    "QiWe send-ready work item UUID",
    "Final JPEG `content_hash`",
    "QiWe `artifact_content_hash`",
    "Huabaosi `sidecar_binary_sha256`",
    "Hash match confirmed by `check-xiaoman-image-send-staging-evidence.mjs`",
    "Huabaosi staging readiness",
    "Huabaosi staging smoke",
    "QiWe staging readiness",
    "QiWe preflight phase",
    "QiWe upload phase",
    "QiWe callback phase",
    "Xiaoman image-send staging evidence check passed.",
    "callback_credential_schema",
    "callback_additional_field_count",
    "external_upload_requested=true",
    "external_send_executed=true",
    "QiWe production enablement PR allowed",
    "no production listener, service, timer, feature build, Feishu write, Release",
    "Do not record QiWe token, GUID, API secret material, target group id, database URL",
  ]) {
    requireFragment(xiaomanImageSendStagingEvidenceTemplatePath, template, fragment);
  }
  for (const fragment of [
    "QIWE_TOKEN=",
    "QIWE_GUID=",
    "postgres://",
    "postgresql://",
    "callback.json",
    "systemctl enable",
    "systemctl start",
    "gh release",
  ]) {
    forbidFragment(xiaomanImageSendStagingEvidenceTemplatePath, template, fragment);
  }
}

const xiaomanRealActivityProductionEvidenceTemplatePath =
  "docs/reports/templates/xiaoman-real-activity-production-evidence.md";
const huabaosiProductionCanaryEvidenceTemplatePath =
  "docs/reports/templates/huabaosi-image-production-canary-evidence.md";
const xiaomanProductionEvidenceRunbookPath =
  "docs/operations/xiaoman-production-evidence-runbook.md";
if (!exists(xiaomanProductionEvidenceRunbookPath)) {
  addError(
    `${xiaomanProductionEvidenceRunbookPath}: missing Xiaoman production evidence runbook`
  );
} else {
  const runbook = readText(xiaomanProductionEvidenceRunbookPath);
  for (const fragment of [
    "node tools/deploy/check-xiaoman-production-evidence-chain-local.mjs",
    "Do not continue to production evidence capture if this local check fails.",
    "runtime_artifact_profile=huabaosi-production",
    "runtime_artifact_profile=qiwe-production",
    "QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_SIDECAR_SHA256=<approved-huabaosi-production-sidecar-sha256>",
    "QINTOPIA_XIAOMAN_REAL_ACTIVITY_PRODUCTION_SIDECAR_SHA256=<approved-qiwe-production-sidecar-sha256>",
    "check-huabaosi-image-production-canary-evidence.mjs",
    "check-xiaoman-real-activity-production-evidence.mjs",
    "check-xiaoman-qiwe-group-arrival-confirmation-evidence.mjs",
    "build-xiaoman-production-completion-manifest.mjs",
    "finalize-xiaoman-production-completion-evidence.mjs",
    "check-xiaoman-production-completion-evidence.mjs",
    "Treating them as the same production binary is invalid.",
    "## 2. QiWe Companion Verification",
    "sidecar-profiles/qiwe-production/qintopia-message-sidecar",
    "does not auto-merge a Release Please PR",
    "does not publish a Release",
  ]) {
    requireFragment(xiaomanProductionEvidenceRunbookPath, runbook, fragment);
  }
  for (const fragment of [
    "QIWE_TOKEN=",
    "postgres://",
    "postgresql://",
    "systemctl enable",
    "gh release create",
  ]) {
    forbidFragment(xiaomanProductionEvidenceRunbookPath, runbook, fragment);
  }
}

if (!exists(huabaosiProductionCanaryEvidenceTemplatePath)) {
  addError(
    `${huabaosiProductionCanaryEvidenceTemplatePath}: missing Huabaosi production canary evidence template`
  );
} else {
  const template = readText(huabaosiProductionCanaryEvidenceTemplatePath);
  for (const fragment of [
    "node tools/deploy/check-huabaosi-image-production-canary-evidence.mjs",
    "Production release SHA",
    "Runtime artifact profile: `huabaosi-production`.",
    "Packaged sidecar binary SHA-256",
    "Production database URL SHA-256",
    "Release-local binary verified",
    "Owner-approved sidecar SHA-256 matched",
    "Owner-approved database URL SHA-256 matched",
    "Generated-image artifact UUID",
    "Final JPEG `content_hash`",
    "`preflight`",
    "`brief_review`",
    "`request_intake`",
    "`generation`",
    "`revalidation`",
    "`artifact_profile`: `huabaosi-production`",
    "`review_status`: `pending`",
    "Huabaosi production canary evidence checker passed: yes/no.",
    "Do not record provider endpoint, provider response, API key, token, database URL",
  ]) {
    requireFragment(huabaosiProductionCanaryEvidenceTemplatePath, template, fragment);
  }
  for (const fragment of [
    "QIWE_TOKEN=",
    "QIWE_GUID=",
    "postgres://",
    "postgresql://",
    "callback.json",
    "systemctl enable",
    "systemctl start",
    "gh release",
  ]) {
    forbidFragment(huabaosiProductionCanaryEvidenceTemplatePath, template, fragment);
  }
}

if (!exists(xiaomanRealActivityProductionEvidenceTemplatePath)) {
  addError(
    `${xiaomanRealActivityProductionEvidenceTemplatePath}: missing Xiaoman real activity production evidence template`
  );
} else {
  const template = readText(xiaomanRealActivityProductionEvidenceTemplatePath);
  for (const fragment of [
    "QINTOPIA_XIAOMAN_REAL_ACTIVITY_PRODUCTION_SIDECAR_SHA256='<approved qiwe production sidecar binary sha256>'",
    "QINTOPIA_XIAOMAN_REAL_ACTIVITY_PRODUCTION_DATABASE_URL_SHA256='<approved production database URL sha256>'",
    "qintopia-message-sidecar xiaoman-real-activity-production-evidence",
    "--workflow-root-id <completed-xiaoman-activity-root-uuid>",
    "node tools/deploy/check-xiaoman-real-activity-production-evidence.mjs <production-evidence-output.txt>",
    "Production release SHA",
    "Runtime artifact profile: `qiwe-production`.",
    "Release-local binary verified",
    "Owner-approved sidecar SHA-256 matched",
    "Production database URL SHA-256",
    "Owner-approved database URL SHA-256 matched",
    "Xiaoman source event signal UUID",
    "Generated-image artifact UUID",
    "Send-ready work item UUID",
    "Final JPEG `artifact_content_hash`",
    "QiWe group arrival confirmed by human operator",
    "signal_intake",
    "`runtime_artifact_profile`: `qiwe-production`",
    "image_generation",
    "human_approval",
    "send_ready",
    "qiwe_upload",
    "qiwe_callback_send",
    "sanitized_evidence_retention",
    "`release_binary_verified`: `true`",
    "`approved_sidecar_sha256_matched`: `true`",
    "`approved_database_url_sha256_matched`: `true`",
    "Do not record QiWe token, GUID, API secret material, target group id, database URL",
  ]) {
    requireFragment(
      xiaomanRealActivityProductionEvidenceTemplatePath,
      template,
      fragment
    );
  }
  for (const fragment of [
    "QIWE_TOKEN=",
    "QIWE_GUID=",
    "postgres://",
    "postgresql://",
    "callback.json",
    "systemctl enable",
    "systemctl start",
    "gh release",
  ]) {
    forbidFragment(
      xiaomanRealActivityProductionEvidenceTemplatePath,
      template,
      fragment
    );
  }
}

const xiaomanQiweArrivalConfirmationTemplatePath =
  "docs/reports/templates/xiaoman-qiwe-group-arrival-confirmation-evidence.md";
if (!exists(xiaomanQiweArrivalConfirmationTemplatePath)) {
  addError(
    `${xiaomanQiweArrivalConfirmationTemplatePath}: missing Xiaoman QiWe group arrival confirmation evidence template`
  );
} else {
  const template = readText(xiaomanQiweArrivalConfirmationTemplatePath);
  for (const fragment of [
    "xiaoman_qiwe_group_arrival_confirmation_evidence=",
    "xiaoman-qiwe-group-arrival-confirmation-evidence-v1",
    "human_visible_group_check",
    "community_activity_group",
    "send_ready_work_item_id",
    "generated_image_artifact_id",
    "artifact_content_hash",
    "raw_secret_fields_retained",
    "Do not record QiWe token, GUID",
  ]) {
    requireFragment(xiaomanQiweArrivalConfirmationTemplatePath, template, fragment);
  }
  for (const fragment of [
    "QIWE_TOKEN=",
    "QIWE_GUID=",
    "postgres://",
    "postgresql://",
    "callback.json",
    "systemctl enable",
    "systemctl start",
    "gh release",
  ]) {
    forbidFragment(xiaomanQiweArrivalConfirmationTemplatePath, template, fragment);
  }
}

const xiaomanImageSendStagingEvidenceTestPath =
  "tools/deploy/test-xiaoman-image-send-staging-evidence.mjs";
if (!exists(xiaomanImageSendStagingEvidenceTestPath)) {
  addError(
    `${xiaomanImageSendStagingEvidenceTestPath}: missing Xiaoman image-send staging evidence checker test`
  );
} else {
  const test = readText(xiaomanImageSendStagingEvidenceTestPath);
  for (const fragment of [
    "check-xiaoman-image-send-staging-evidence.mjs",
    "Huabaosi content_hash and QiWe artifact_content_hash values differ",
    "Huabaosi and QiWe sidecar_binary_sha256 values differ",
    "expected exactly one QiWe preflight evidence record",
    "forbidden sensitive fragment",
    "Xiaoman image-send staging evidence test passed.",
  ]) {
    requireFragment(xiaomanImageSendStagingEvidenceTestPath, test, fragment);
  }
}

const xiaomanQiweArrivalConfirmationEvidenceTestPath =
  "tools/deploy/test-xiaoman-qiwe-group-arrival-confirmation-evidence.mjs";
if (!exists(xiaomanQiweArrivalConfirmationEvidenceTestPath)) {
  addError(
    `${xiaomanQiweArrivalConfirmationEvidenceTestPath}: missing Xiaoman QiWe group arrival confirmation evidence checker test`
  );
} else {
  const test = readText(xiaomanQiweArrivalConfirmationEvidenceTestPath);
  for (const fragment of [
    "check-xiaoman-qiwe-group-arrival-confirmation-evidence.mjs",
    "runtime_artifact_profile:",
    'overrides.runtimeArtifactProfile ?? "qiwe-production"',
    "does not bind to the real activity send",
    "forbidden sensitive fragment",
    "production real activity evidence failed",
    "Xiaoman QiWe group arrival confirmation evidence test passed.",
  ]) {
    requireFragment(xiaomanQiweArrivalConfirmationEvidenceTestPath, test, fragment);
  }
}

for (const relativePath of [
  "deploy/sidecar/README.md",
  "docs/plans/active/xiaoman-qiwe-image-send.md",
]) {
  const text = readText(relativePath);
  for (const fragment of [
    "QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL=approved-staging-qiwe-image-send",
    "QINTOPIA_QIWE_IMAGE_STAGING_PHASE=preflight",
    "QINTOPIA_QIWE_IMAGE_STAGING_PHASE=upload",
    "QINTOPIA_QIWE_IMAGE_STAGING_PHASE=callback",
    "QINTOPIA_QIWE_IMAGE_STAGING_SIDECAR_SHA256='<approved staging sidecar binary sha256>'",
    "trusted-staging-callback-source |",
  ]) {
    requireFragment(relativePath, text, fragment);
  }
}

const xiaomanPosterConfigApplyPath =
  "deploy/sidecar/scripts/apply-xiaoman-feishu-poster-production-config.py";
const xiaomanPosterConfigApplyTestPath =
  "tools/deploy/test_xiaoman_feishu_poster_production_config_apply.py";
const xiaomanPolicyApplyPath =
  "deploy/sidecar/scripts/apply-xiaoman-conversation-policies-production.py";
const xiaomanPolicyApplyTestPath =
  "tools/deploy/test_xiaoman_conversation_policy_production_apply.py";
const xiaomanDbRolloverPath =
  "deploy/sidecar/scripts/xiaoman-shared-db-password-rollover-production.py";
const xiaomanDbRolloverTestPath =
  "tools/deploy/test_xiaoman_shared_db_password_rollover_production.py";
if (!exists(xiaomanPosterConfigApplyPath)) {
  addError(
    `${xiaomanPosterConfigApplyPath}: missing protected production config entrypoint`
  );
} else {
  const script = readText(xiaomanPosterConfigApplyPath);
  for (const fragment of [
    'SIDECAR_ENV_PATH = Path("/etc/qintopia/message-sidecar.env")',
    'HERMES_ENV_PATH = Path("/home/ubuntu/.hermes/profiles/xiaoman/.env")',
    'ERHUA_ENV_PATH = Path("/home/ubuntu/.hermes/profiles/erhua/.env")',
    'RELEASE_CURRENT_PATH = Path("/home/ubuntu/qintopia-agent-os-releases/current")',
    "approved-production-xiaoman-feishu-config-v1",
    "MAX_INPUT_BYTES = 64 * 1024",
    'parser.add_argument("--stdin", action="store_true")',
    'parser.add_argument("--apply", action="store_true")',
    "if os.geteuid() != 0:",
    "metadata.st_uid != os.geteuid()",
    "fcntl.flock(lock_descriptor, fcntl.LOCK_EX)",
    "secrets.token_urlsafe(48)",
    "database_url_sha256_matched",
    "previous_database_url_sha256",
    "cleanup_production_stage_files",
    "staged_secret_files_absent",
    '"external_calls_executed": False',
    '"database_writes_executed": False',
    '"service_changes_executed": False',
  ]) {
    requireFragment(xiaomanPosterConfigApplyPath, script, fragment);
  }
  for (const forbidden of [
    "--test-mode",
    "--output",
    "systemctl",
    "curl ",
    "psql ",
    "source ",
    "eval ",
  ]) {
    forbidFragment(xiaomanPosterConfigApplyPath, script, forbidden);
  }
}
if (!exists(xiaomanPosterConfigApplyTestPath)) {
  addError(
    `${xiaomanPosterConfigApplyTestPath}: missing production config transaction test`
  );
} else {
  const test = readText(xiaomanPosterConfigApplyTestPath);
  for (const fragment of [
    "test_direct_preview_and_apply_generate_one_redacted_hmac",
    "test_database_rotation_updates_url_and_all_present_production_hashes",
    "test_database_rotation_retry_reconciles_interrupted_first_replace",
    "test_orphaned_secret_stages_are_cleaned_and_exact_retry_stages_nothing",
    "test_unsafe_orphaned_config_stage_fails_closed",
    "test_zero_byte_stage_before_metadata_update_is_recoverable",
    "test_group_and_disabled_states_use_the_same_transaction",
    "test_document_commit_restores_all_files_when_third_replace_fails",
    "test_release_owner_and_write_boundaries_match_promoter",
  ]) {
    requireFragment(xiaomanPosterConfigApplyTestPath, test, fragment);
  }
}
requireFragment(
  "tools/deploy/build-deploy-bundle.mjs",
  readText("tools/deploy/build-deploy-bundle.mjs"),
  xiaomanPosterConfigApplyPath
);
requireFragment(
  "package.json",
  readText("package.json"),
  "python3 tools/deploy/test_xiaoman_feishu_poster_production_config_apply.py"
);
if (!exists(xiaomanPolicyApplyPath)) {
  addError(
    `${xiaomanPolicyApplyPath}: missing protected conversation policy entrypoint`
  );
} else {
  const script = readText(xiaomanPolicyApplyPath);
  for (const fragment of [
    'SIDECAR_ENV_PATH = Path("/etc/qintopia/message-sidecar.env")',
    'RELEASE_CURRENT_PATH = Path("/home/ubuntu/qintopia-agent-os-releases/current")',
    "approved-production-xiaoman-conversation-policy-v3",
    '[str(binary), "conversation-policy-apply", "--stdin"]',
    '"PATH": "/usr/bin:/bin"',
    '"PYTHONDONTWRITEBYTECODE": "1"',
    "sys.dont_write_bytecode = True",
    "sensitive_values",
    "validate_policy_report",
    "OPAQUE_REF_RE",
    "release root directory boundary is invalid",
    "release_metadata.st_uid != expected_uid",
    "release sidecar directory boundary is invalid",
    "sidecar_metadata.st_uid != expected_uid",
    "metadata.st_uid != expected_uid",
  ]) {
    requireFragment(xiaomanPolicyApplyPath, script, fragment);
  }
  for (const forbidden of [
    "--test-mode",
    "--output",
    "systemctl",
    "curl ",
    "psql ",
    "source ",
    "eval ",
  ]) {
    forbidFragment(xiaomanPolicyApplyPath, script, forbidden);
  }
}
if (!exists(xiaomanPolicyApplyTestPath)) {
  addError(`${xiaomanPolicyApplyTestPath}: missing production policy entrypoint test`);
} else {
  const test = readText(xiaomanPolicyApplyTestPath);
  for (const fragment of [
    "test_fixed_release_policy_apply_uses_minimal_environment_and_redacted_output",
    "test_approval_database_and_output_boundaries_fail_closed",
    "test_input_and_cli_surface_are_bounded",
    "test_config_helper_import_leaves_no_release_bytecode",
    "test_release_owner_and_write_boundaries_match_promoter",
  ]) {
    requireFragment(xiaomanPolicyApplyTestPath, test, fragment);
  }
}
for (const relativePath of [xiaomanPosterConfigApplyPath, xiaomanPolicyApplyPath]) {
  requireFragment(
    "tools/deploy/build-deploy-bundle.mjs",
    readText("tools/deploy/build-deploy-bundle.mjs"),
    relativePath
  );
}
requireFragment(
  "tools/deploy/build-deploy-bundle.mjs",
  readText("tools/deploy/build-deploy-bundle.mjs"),
  "docs/operations/xiaoman-feishu-poster-production-closeout-runbook.md"
);
for (const fragment of [
  "release_scope=sidecar-runtime,deploy-bundle,hermes-plugins",
  "restart_targets=qintopia-system-services,hermes-erhua",
  '"dry_run_request_id": "<successful-same-sha-dry-run-request-id>"',
  "Stop before password rotation",
  "automatically points `current` to `previous`",
]) {
  requireFragment(
    "docs/operations/xiaoman-feishu-poster-production-closeout-runbook.md",
    readText("docs/operations/xiaoman-feishu-poster-production-closeout-runbook.md"),
    fragment
  );
}
requireFragment(
  "package.json",
  readText("package.json"),
  "python3 tools/deploy/test_xiaoman_conversation_policy_production_apply.py"
);
if (!exists(xiaomanDbRolloverPath)) {
  addError(
    `${xiaomanDbRolloverPath}: missing protected database rollover state machine`
  );
} else {
  const script = readText(xiaomanDbRolloverPath);
  for (const fragment of [
    'STATE_ROOT_PATH = Path("/var/lib/qintopia-xiaoman-db-password-rollover")',
    'DEPLOY_STATE_ROOT_PATH = Path("/var/lib/qintopia-agent-os-deploy")',
    'ERHUA_ENV_PATH = Path("/home/ubuntu/.hermes/profiles/erhua/.env")',
    "approved-production-xiaoman-shared-db-password-rollover-v1",
    '"previous_database_url_sha256"',
    '"unexpected_database_configuration_binding"',
    '"alter_in_flight"',
    '"private_policy_applied"',
    '"secret_cleanup_completed": False',
    "cleanup_temporary_records",
    "cleanup_config_stage_files",
    '"active_database_url_sha256"',
    '"successor_database_url_sha256"',
    "AUTH_REJECTED_RE",
    "TLS_ERROR_MARKERS",
    "SCRAM-SHA-256$",
    "verify_release_boundary",
    "verify_pre_rotation_dry_run",
    "dry_run_request_id",
    'EXPECTED_RELEASE_SCOPE = ["sidecar-runtime", "deploy-bundle", "hermes-plugins"]',
    'EXPECTED_RESTART_TARGETS = ["qintopia-system-services", "hermes-erhua"]',
    '"runtime_artifact_profile": "huabaosi-production"',
    '"PYTHONDONTWRITEBYTECODE": "1"',
    "CONFIG_SCRIPT_RELATIVE_PATH",
    "POLICY_SCRIPT_RELATIVE_PATH",
    '"feishu_calls_executed": False',
    '"service_changes_executed": False',
  ]) {
    requireFragment(xiaomanDbRolloverPath, script, fragment);
  }
  for (const forbidden of [
    'Path("/run/',
    "open-apis",
    "send_as_bot",
    '"start",',
    '"restart",',
    '"enable",',
    "enable --now",
    "curl ",
  ]) {
    forbidFragment(xiaomanDbRolloverPath, script, forbidden);
  }
}
if (!exists(xiaomanDbRolloverTestPath)) {
  addError(
    `${xiaomanDbRolloverTestPath}: missing database rollover state-machine test`
  );
} else {
  const test = readText(xiaomanDbRolloverTestPath);
  for (const fragment of [
    "test_prepare_reconciles_unknown_alter_commit_and_persists_root_only_state",
    "test_pre_rotation_dry_run_gate_accepts_exact_protected_evidence",
    "test_pre_rotation_dry_run_gate_rejects_incomplete_or_mismatched_evidence",
    "test_failed_pre_rotation_gate_creates_no_state_or_password_change",
    "test_pre_rotation_evidence_file_boundaries_fail_closed",
    "test_pre_rotation_evidence_wrong_owner_fails_closed",
    "test_mixed_old_new_configuration_converges_but_third_value_fails_closed",
    "test_persistent_state_is_reentrant_after_process_and_boot_restart",
    "test_post_policy_rollback_keeps_valid_rotated_credential_and_disables_poster",
    "test_terminal_receipt_precedes_secret_cleanup_and_recovers_after_crash",
    "test_sigkill_during_secret_state_replace_cleans_orphan_on_restart",
    "test_unsafe_orphaned_state_record_fails_before_terminal_reconciliation",
    "test_production_config_stage_cleanup_is_release_bound_and_redacted",
    "test_exact_operator_binding_rejects_wrong_database_role_chat_and_actor",
    "test_persisted_state_rederives_targets_and_password_only_rotation",
    "test_production_operations_bind_payload_and_reconciliation_to_erhua",
    "test_protected_python_children_disable_bytecode_writes",
    "test_release_boundary_rejects_symlink_writable_and_digest_drift",
  ]) {
    requireFragment(xiaomanDbRolloverTestPath, test, fragment);
  }
}
requireFragment(
  "tools/deploy/build-deploy-bundle.mjs",
  readText("tools/deploy/build-deploy-bundle.mjs"),
  xiaomanDbRolloverPath
);
requireFragment(
  "package.json",
  readText("package.json"),
  "python3 tools/deploy/test_xiaoman_shared_db_password_rollover_production.py"
);

const xiaomanInternalGroupObservationPath =
  "deploy/sidecar/scripts/xiaoman-feishu-internal-group-production-observation-smoke.sh";
const xiaomanInternalGroupActivationPath =
  "deploy/sidecar/scripts/activate-xiaoman-feishu-internal-group-production.sh";
const xiaomanInternalGroupRollbackPath =
  "deploy/sidecar/scripts/rollback-xiaoman-feishu-internal-group-production.sh";
for (const relativePath of [
  xiaomanInternalGroupObservationPath,
  xiaomanInternalGroupActivationPath,
  xiaomanInternalGroupRollbackPath,
]) {
  if (!exists(relativePath)) {
    addError(`${relativePath}: missing Xiaoman internal-group production control`);
    continue;
  }
  const script = readText(relativePath);
  for (const forbidden of [
    "TEST_MODE",
    "_TEST_MODE",
    "SYSTEMCTL:-",
    "RUNUSER_BIN:-",
    "source ",
    "eval ",
    "curl ",
    "psql ",
  ]) {
    forbidFragment(relativePath, script, forbidden);
  }
}
if (exists(xiaomanInternalGroupObservationPath)) {
  const observation = readText(xiaomanInternalGroupObservationPath);
  for (const fragment of [
    'SYSTEMCTL="/usr/bin/systemctl"',
    'RELEASE_CURRENT_DIR="/home/ubuntu/qintopia-agent-os-releases/current"',
    "huabaosi-production-adapter",
    "xiaoman-feishu-poster-adapter",
    "QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_EXPECTED_STATE",
    "QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_DELIVERY_EXPECTED_STATE",
    "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE",
    "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY",
    "QINTOPIA_XIAOMAN_FEISHU_CALLBACK_ENCRYPT_KEY",
    "delivery_chats != ingress_chats",
    "delivery_users != ingress_users",
    "delivery_users.issubset(reviewer_users)",
    "qintopia-agentos-xiaoman-feishu-poster-delivery.timer",
    "qintopia-agentos-xiaoman-feishu-internal-group-poster-delivery.timer",
    "is-enabled --quiet",
    "is-active --quiet",
  ]) {
    requireFragment(xiaomanInternalGroupObservationPath, observation, fragment);
  }
}
if (exists(xiaomanInternalGroupActivationPath)) {
  const activation = readText(xiaomanInternalGroupActivationPath);
  for (const fragment of [
    "approved-production-xiaoman-feishu-internal-group",
    '"$SYSTEMCTL" start "$GROUP_PREFLIGHT_SERVICE"',
    '"$SYSTEMCTL" enable "$GROUP_DELIVERY_TIMER"',
    '"$SYSTEMCTL" restart "$GROUP_DELIVERY_TIMER"',
    "cleanup_failed_activation",
    '"$SYSTEMCTL" disable --now "$GROUP_DELIVERY_TIMER"',
    '"$SYSTEMCTL" stop "$GROUP_DELIVERY_SERVICE"',
    "restart_xiaoman",
    '"$SYSTEMCTL" restart "$INTAKE_SERVICE"',
    '"$SYSTEMCTL" restart "$CALLBACK_SERVICE"',
    "QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_EXPECTED_STATE=enabled",
  ]) {
    requireFragment(xiaomanInternalGroupActivationPath, activation, fragment);
  }
  forbidFragment(
    xiaomanInternalGroupActivationPath,
    activation,
    'DELIVERY_TIMER="qintopia-agentos-xiaoman-feishu-poster-delivery.timer"'
  );
}
if (exists(xiaomanInternalGroupRollbackPath)) {
  const rollback = readText(xiaomanInternalGroupRollbackPath);
  for (const fragment of [
    "approved-production-xiaoman-feishu-internal-group-rollback",
    '"$SYSTEMCTL" disable --now "$GROUP_DELIVERY_TIMER"',
    '"$SYSTEMCTL" start "$DIRECT_PREFLIGHT_SERVICE"',
    "restart_xiaoman",
    "QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_EXPECTED_STATE=disabled",
    "direct poster services remain active",
  ]) {
    requireFragment(xiaomanInternalGroupRollbackPath, rollback, fragment);
  }
  forbidFragment(
    xiaomanInternalGroupRollbackPath,
    rollback,
    'DELIVERY_TIMER="qintopia-agentos-xiaoman-feishu-poster-delivery.timer"'
  );
}

const xiaomanInternalGroupBundleBuilderPath = "tools/deploy/build-deploy-bundle.mjs";
if (exists(xiaomanInternalGroupBundleBuilderPath)) {
  const bundleBuilder = readText(xiaomanInternalGroupBundleBuilderPath);
  for (const relativePath of [
    xiaomanInternalGroupObservationPath,
    xiaomanInternalGroupActivationPath,
    xiaomanInternalGroupRollbackPath,
  ]) {
    requireFragment(xiaomanInternalGroupBundleBuilderPath, bundleBuilder, relativePath);
  }
}
const xiaomanInternalGroupTestPath =
  "tools/deploy/test-xiaoman-feishu-internal-group-production.mjs";
if (!exists(xiaomanInternalGroupTestPath)) {
  addError(
    `${xiaomanInternalGroupTestPath}: missing fake systemd production control test`
  );
} else {
  const test = readText(xiaomanInternalGroupTestPath);
  for (const fragment of [
    "activation must fail before side effects without owner approval",
    "failed gateway reload must leave delivery stopped",
    "rollback must stop delivery then reject persistent enabled state before reload",
    "observation disclosed sensitive fixture value",
    "Xiaoman Feishu internal-group production control test passed.",
  ]) {
    requireFragment(xiaomanInternalGroupTestPath, test, fragment);
  }
}
for (const relativePath of [
  "deploy/sidecar/scripts/activate-xiaoman-feishu-poster-production.sh",
  "deploy/sidecar/scripts/rollback-xiaoman-feishu-poster-production.sh",
]) {
  const script = readText(relativePath);
  requireFragment(
    relativePath,
    script,
    "QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED"
  );
  requireFragment(
    relativePath,
    script,
    "qintopia-agentos-xiaoman-feishu-internal-group-poster-delivery.timer"
  );
}
requireFragment(
  "tools/deploy/test-xiaoman-feishu-poster-production-activation.mjs",
  readText("tools/deploy/test-xiaoman-feishu-poster-production-activation.mjs"),
  "direct activation must reject internal-group enablement before side effects"
);
requireFragment(
  "package.json",
  readText("package.json"),
  "node tools/deploy/test-xiaoman-feishu-internal-group-production.mjs"
);

const xiaomanProductionCompletionEvidenceTemplatePath =
  "docs/reports/templates/xiaoman-production-completion-evidence.json";
if (!exists(xiaomanProductionCompletionEvidenceTemplatePath)) {
  addError(
    `${xiaomanProductionCompletionEvidenceTemplatePath}: missing Xiaoman production completion evidence manifest template`
  );
} else {
  const template = readText(xiaomanProductionCompletionEvidenceTemplatePath);
  for (const fragment of [
    "xiaoman-production-completion-evidence-v1",
    "release_please_validation",
    "release_tag",
    "released_commit_sha",
    "manual_ci_workflow",
    "release_please_status",
    "qiwe_production_enablement",
    "included_in_release_sha",
    "listener_service_timer_reviewed",
    "observation_reviewed",
    "rollback_reviewed",
    "exact_allowlists_reviewed",
    "production_feature_boundary_reviewed",
    "huabaosi_production_activation",
    "runtime_artifact_profile",
    "<approved-huabaosi-production-sidecar-sha256>",
    "image_generation_observation_passed",
    "feishu_mirror_activation_approved",
    "first_record_evidence_retained",
    "real_activity_confirmation",
    "<approved-qiwe-production-sidecar-sha256>",
    "sidecar_binary_sha256",
    "database_url_sha256",
    "qiwe_group_arrival_confirmed",
    "daily_case_report_confirmation",
    "xiaoman-character-universe-v1",
    "daily_case_report_second_pass",
    "raw_messages_included",
    "profile_fact_text_included",
    "creative_profile_public_surface_allowed",
    "character_universe_people_count",
    "character_universe_meme_count",
    "character_universe_callback_count",
    "character_universe_relationship_count",
    "character_universe_creative_profile_candidate_count",
  ]) {
    requireFragment(
      xiaomanProductionCompletionEvidenceTemplatePath,
      template,
      fragment
    );
  }
  for (const fragment of [
    "QIWE_TOKEN",
    "QIWE_GUID",
    "postgres://",
    "postgresql://",
    "https://",
    "target_group_id",
    "artifact_uri",
  ]) {
    forbidFragment(xiaomanProductionCompletionEvidenceTemplatePath, template, fragment);
  }
}

const hermesCronApplyScripts = [
  "deploy/sidecar/scripts/apply-erhua-morning-brief-hermes-cron.sh",
  "deploy/sidecar/scripts/apply-xiaoman-daily-case-report-hermes-cron.sh",
  "deploy/sidecar/scripts/apply-xiaoman-weekly-recruitment-hermes-cron.sh",
  "deploy/sidecar/scripts/apply-xiaoman-weekly-plan-confirmation-hermes-cron.sh",
  "deploy/sidecar/scripts/apply-xiaoman-weekly-preview-hermes-cron.sh",
];

for (const hermesCronApplyScript of hermesCronApplyScripts) {
  requireExecutable(hermesCronApplyScript);
  if (!exists(hermesCronApplyScript)) {
    continue;
  }
  const apply = readText(hermesCronApplyScript);
  for (const fragment of [
    "set -euo pipefail",
    'PATH="/usr/bin:/bin:/usr/sbin:/sbin"',
    'PYTHON_BIN="/usr/bin/python3"',
    "/home/ubuntu/qintopia-agent-os-releases/current",
    "/home/ubuntu/.hermes/scripts",
    "sync-hermes-cron-snapshot.sh",
    "QINTOPIA_HERMES_CRON_SNAPSHOT",
    "approved-production-hermes-cron-snapshot",
    "qintopia_hermes_cron_apply_safe_failure=",
    "--install",
    "--enable",
    "WECOM_HOME_CHANNEL",
    '"deliver": "origin"',
    '"platform": "wecom"',
    "external_calls_executed",
    "safe_for_chat",
    "os.replace",
  ]) {
    requireFragment(hermesCronApplyScript, apply, fragment);
  }
  for (const fragment of [
    "eval ",
    "QIWE_TOKEN",
    "QIWE_GUID",
    "tenant_access_token",
    "print(chat_id)",
    "QINTOPIA_HERMES_CRON_FILE",
    "QINTOPIA_HERMES_PROFILE_DIR",
  ]) {
    forbidFragment(hermesCronApplyScript, apply, fragment);
  }
}

const hermesCronSnapshotSyncScript =
  "deploy/sidecar/scripts/sync-hermes-cron-snapshot.sh";
requireExecutable(hermesCronSnapshotSyncScript);
if (exists(hermesCronSnapshotSyncScript)) {
  const snapshotSync = readText(hermesCronSnapshotSyncScript);
  for (const fragment of [
    "rev-parse --git-dir",
    "rev-parse --show-toplevel",
    "/usr/sbin/runuser",
    'HOME="$HOME_DIR"',
    'PATH="/usr/bin:/bin"',
    "/home/ubuntu | /home/ubuntu/* | /usr/bin/* | /usr/sbin/*",
    "normalize_snapshot_permissions",
    '-c "%u"',
    '-c "%g"',
    "chown",
    "chmod",
    "snapshot repo must not have a remote",
    "snapshot_commit=skipped-no-changes",
    "snapshot_commit=created",
  ]) {
    requireFragment(hermesCronSnapshotSyncScript, snapshotSync, fragment);
  }
  for (const fragment of ["git remote add", "https://", "QINTOPIA_HERMES_CRON_FILE"]) {
    forbidFragment(hermesCronSnapshotSyncScript, snapshotSync, fragment);
  }
}

if (errors.length > 0) {
  console.error("Deploy contract check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log("Deploy contract check passed.");
