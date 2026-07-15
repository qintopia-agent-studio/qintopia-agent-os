#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { execFileSync } from "node:child_process";
import Ajv2020 from "ajv/dist/2020.js";
import YAML from "yaml";

const repoRoot = process.cwd();
const errors = [];

const exists = (relativePath) => fs.existsSync(path.join(repoRoot, relativePath));
const readText = (relativePath) =>
  fs.readFileSync(path.join(repoRoot, relativePath), "utf8");
const readYaml = (relativePath) => YAML.parse(readText(relativePath));
const addError = (message) => errors.push(message);
const countExactOccurrences = (text, fragment) => text.split(fragment).length - 1;
const stripCommentOnlyLines = (text) =>
  text
    .split("\n")
    .filter((line) => !line.trim().startsWith("#"))
    .join("\n");
const hasDangerousInputInterpolationInRun = (workflowText) => {
  const lines = workflowText.split("\n");
  let inRun = false;
  let runIndent = -1;
  for (const line of lines) {
    const indent = line.match(/^ */)?.[0].length ?? 0;
    if (/^\s*run:\s*/.test(line)) {
      inRun = true;
      runIndent = indent;
      const inlineValue = line.replace(/^\s*run:\s*/, "");
      if (inlineValue.includes("${{ inputs.")) {
        return true;
      }
      continue;
    }
    if (inRun && line.trim() && indent <= runIndent) {
      inRun = false;
    }
    if (inRun && line.includes("${{ inputs.")) {
      return true;
    }
  }
  return false;
};

const requiredFiles = [
  ".github/workflows/deploy-production.yml",
  ".github/workflows/rollback-production.yml",
  "deploy/runner/README.md",
  "deploy/runner/manifest.yaml",
  "deploy/runner/deploy-request.schema.json",
  "deploy/runner/deploy-result.schema.json",
  "deploy/runner/install-release-systemd-units.sh",
  "deploy/runner/qintopia-agent-os-deploy-runner",
  "deploy/runner/poll-deploy-requests.sh",
  "deploy/runner/promote-release.sh",
  "deploy/runner/rollback-release.sh",
  "deploy/runner/smoke-release.sh",
  "deploy/runner/upload-deploy-request.sh",
  "deploy/runner/wait-deploy-result.sh",
  "deploy/restart-target-rules.yaml",
  "deploy/runner/qintopia-agent-os-deploy-runner.service",
  "deploy/runner/qintopia-agent-os-deploy-runner.timer",
  "tools/deploy/create-deploy-request.mjs",
  "tools/deploy/collect-release-deploy-results.mjs",
  "tools/deploy/resolve-release-deploy-base.mjs",
  "tools/deploy/resolve-release-restart-targets.mjs",
  "tools/deploy/resolve-restart-targets.mjs",
  "tools/deploy/test-collect-release-deploy-results.mjs",
  "tools/deploy/test-resolve-release-deploy-base.mjs",
  "tools/deploy/test-resolve-release-restart-targets.mjs",
  "tools/deploy/test-resolve-restart-targets.mjs",
  "tools/deploy/test-deploy-runner-poller.mjs",
  "tools/deploy/test-deploy-runner-promotion.mjs",
];

for (const file of requiredFiles) {
  if (!exists(file)) {
    addError(`${file}: required deploy runner file is missing`);
  }
}

if (exists(".github/workflows/release-please.yml")) {
  const releasePleaseWorkflow = YAML.parse(
    readText(".github/workflows/release-please.yml")
  );
  const releasePleaseText = readText(".github/workflows/release-please.yml");
  const releasePleaseJob = releasePleaseWorkflow?.jobs?.["release-please"];
  if (!releasePleaseWorkflow?.on?.push?.branches?.includes("master")) {
    addError(".github/workflows/release-please.yml: must run from master pushes");
  }
  if (releasePleaseWorkflow?.on?.release) {
    addError(".github/workflows/release-please.yml: must not run from release events");
  }
  if (releasePleaseWorkflow?.permissions?.contents !== "write") {
    addError(
      ".github/workflows/release-please.yml: must be able to update changelog and draft releases"
    );
  }
  if (releasePleaseWorkflow?.permissions?.["pull-requests"] !== "write") {
    addError(
      ".github/workflows/release-please.yml: must be able to update release PRs"
    );
  }
  const releasePleaseStep = releasePleaseJob?.steps?.find((step) =>
    String(step?.uses ?? "").startsWith("googleapis/release-please-action@")
  );
  if (!releasePleaseStep) {
    addError(".github/workflows/release-please.yml: must use release-please-action");
  }
  const releasePleaseUses = String(releasePleaseStep?.uses ?? "");
  if (!/^googleapis\/release-please-action@[0-9a-f]{40}$/.test(releasePleaseUses)) {
    addError(
      ".github/workflows/release-please.yml: release-please-action must be pinned to a full commit SHA"
    );
  }
  if (releasePleaseStep?.with?.["config-file"] !== ".release-please-config.json") {
    addError(
      ".github/workflows/release-please.yml: must use .release-please-config.json"
    );
  }
  if (releasePleaseStep?.with?.["manifest-file"] !== ".release-please-manifest.json") {
    addError(
      ".github/workflows/release-please.yml: must use .release-please-manifest.json"
    );
  }
  if (!releasePleaseText.includes("secrets.RELEASE_PLEASE_TOKEN || github.token")) {
    addError(
      ".github/workflows/release-please.yml: should support RELEASE_PLEASE_TOKEN with github.token fallback"
    );
  }
} else {
  addError(
    ".github/workflows/release-please.yml: missing release preparation workflow"
  );
}

if (exists(".release-please-config.json")) {
  const releasePleaseConfig = JSON.parse(readText(".release-please-config.json"));
  const rootPackage = releasePleaseConfig?.packages?.["."];
  if (rootPackage?.["release-type"] !== "simple") {
    addError(".release-please-config.json: root release type must be simple");
  }
  if (rootPackage?.["bump-patch-for-minor-pre-major"] !== true) {
    addError(
      ".release-please-config.json: 0.x feature releases must stay on the patch line until owner-approved promotion"
    );
  }
  if (rootPackage?.["package-name"] !== "qintopia-agent-os") {
    addError(".release-please-config.json: package-name must be qintopia-agent-os");
  }
  if (rootPackage?.["changelog-path"] !== "CHANGELOG.md") {
    addError(".release-please-config.json: changelog-path must be CHANGELOG.md");
  }
  if (rootPackage?.["draft"] !== true) {
    addError(
      ".release-please-config.json: GitHub Releases must stay draft until owner publishes them"
    );
  }
  if (rootPackage?.["force-tag-creation"] !== true) {
    addError(
      ".release-please-config.json: draft releases must force tag creation so future changelog calculations remain anchored"
    );
  }
  const changelogSections = Array.isArray(rootPackage?.["changelog-sections"])
    ? rootPackage["changelog-sections"]
    : [];
  const sectionByType = new Map(
    changelogSections.map((section) => [section?.type, section])
  );
  for (const [type, section] of [
    ["feat", "Features"],
    ["fix", "Bug Fixes"],
    ["build", "Build System"],
    ["ci", "CI / Deployment"],
    ["docs", "Documentation"],
    ["chore", "Maintenance"],
  ]) {
    const configuredSection = sectionByType.get(type);
    if (configuredSection?.section !== section || configuredSection?.hidden === true) {
      addError(
        `.release-please-config.json: ${type} commits must be visible in the ${section} changelog section`
      );
    }
  }
  for (const type of ["test", "style"]) {
    if (sectionByType.get(type)?.hidden !== true) {
      addError(
        `.release-please-config.json: ${type} commits must stay hidden from release notes`
      );
    }
  }
  if (rootPackage?.["skip-github-release"] === true) {
    addError(
      ".release-please-config.json: must create draft releases so manual Publish remains the production trigger"
    );
  }
} else {
  addError(".release-please-config.json: missing Release Please config");
}

if (exists(".release-please-manifest.json")) {
  const releasePleaseManifest = JSON.parse(readText(".release-please-manifest.json"));
  if (typeof releasePleaseManifest?.["."] !== "string") {
    addError(".release-please-manifest.json: root version must be recorded");
  }
} else {
  addError(".release-please-manifest.json: missing Release Please manifest");
}

const ajv = new Ajv2020({ allErrors: true });
ajv.addFormat("date-time", true);
let deployRequestSchema = null;
if (exists("deploy/runner/deploy-request.schema.json")) {
  deployRequestSchema = JSON.parse(
    readText("deploy/runner/deploy-request.schema.json")
  );
  const validateRequest = ajv.compile(deployRequestSchema);
  const sampleRequest = {
    schema_version: 1,
    request_id: "deploy-20260706T000000Z-0123456789ab",
    environment: "production",
    repository: "qintopia-agent-studio/qintopia-agent-os",
    requested_by: "codex",
    created_at: "2026-07-06T00:00:00Z",
    expires_at: "2026-07-06T01:00:00Z",
    commit_sha: "0123456789abcdef0123456789abcdef01234567",
    runtime_sha: "0123456789abcdef0123456789abcdef01234567",
    deploy_bundle_sha: "abcdef0123456789abcdef0123456789abcdef01",
    release_sha: "abcdef0123456789abcdef0123456789abcdef01",
    release_scope: ["deploy-bundle", "hermes-plugins"],
    restart_targets: ["qintopia-system-services"],
    rollback_on_smoke_failure: true,
    dry_run: true,
    cos: {
      bucket: "qintopia-agent-os-artifacts-1305166808",
      region: "ap-shanghai",
      prefix: "qintopia-agent-os",
      request_key:
        "qintopia-agent-os/deploy-requests/production/requests/deploy-20260706T000000Z-0123456789ab.json",
      result_key:
        "qintopia-agent-os/deploy-results/production/deploy-20260706T000000Z-0123456789ab.json",
    },
    signature: {
      algorithm: "hmac-sha256",
      issuer: "github-actions",
      key_id: "production",
      signed_at: "2026-07-06T00:00:00Z",
      value: "a".repeat(64),
    },
  };
  if (!validateRequest(sampleRequest)) {
    addError(
      `deploy/runner/deploy-request.schema.json: sample request failed validation ${JSON.stringify(
        validateRequest.errors
      )}`
    );
  }
}

if (exists("deploy/runner/deploy-result.schema.json")) {
  const resultSchema = JSON.parse(readText("deploy/runner/deploy-result.schema.json"));
  const validateResult = ajv.compile(resultSchema);
  const sampleResult = {
    schema_version: 1,
    request_id: "deploy-20260706T000000Z-0123456789ab",
    environment: "production",
    status: "dry_run_succeeded",
    started_at: "2026-07-06T00:00:00Z",
    finished_at: "2026-07-06T00:01:00Z",
    release_sha: "abcdef0123456789abcdef0123456789abcdef01",
    previous_sha: "0123456789abcdef0123456789abcdef01234567",
    current_target: "/home/ubuntu/qintopia-agent-os-releases/current",
    restart_targets: ["qintopia-system-services"],
    checks: [{ name: "deploy-runner", status: "passed" }],
    rollback: { attempted: false, status: "not_needed" },
  };
  if (!validateResult(sampleResult)) {
    addError(
      `deploy/runner/deploy-result.schema.json: sample result failed validation ${JSON.stringify(
        validateResult.errors
      )}`
    );
  }
}

if (exists(".github/workflows/deploy-production.yml")) {
  const workflow = YAML.parse(readText(".github/workflows/deploy-production.yml"));
  if (!workflow?.on?.workflow_dispatch) {
    addError(".github/workflows/deploy-production.yml: must use workflow_dispatch");
  }
  if (!workflow?.on?.release?.types?.includes("published")) {
    addError(
      ".github/workflows/deploy-production.yml: must deploy from published GitHub releases"
    );
  }
  const job = workflow?.jobs?.["request-deploy"];
  if (job?.environment !== "production") {
    addError(
      ".github/workflows/deploy-production.yml: request-deploy must use production environment"
    );
  }
  if (job?.permissions?.contents !== "read") {
    addError(
      ".github/workflows/deploy-production.yml: request-deploy must keep contents permission read-only"
    );
  }
  if (job?.permissions?.actions !== "read") {
    addError(
      ".github/workflows/deploy-production.yml: request-deploy must be able to read deploy workflow run logs"
    );
  }
  const buildAssetsJob = workflow?.jobs?.["build-release-artifacts"];
  if (!buildAssetsJob) {
    addError(
      ".github/workflows/deploy-production.yml: must build Release artifacts before production deployment"
    );
  }
  if (buildAssetsJob?.permissions?.contents !== "read") {
    addError(
      ".github/workflows/deploy-production.yml: build-release-artifacts must keep contents permission read-only"
    );
  }
  const requestDeployNeeds = Array.isArray(job?.needs) ? job.needs : [];
  for (const neededJob of ["build-release-artifacts"]) {
    if (!requestDeployNeeds.includes(neededJob)) {
      addError(
        `.github/workflows/deploy-production.yml: request-deploy must depend on ${neededJob}`
      );
    }
  }
  const uploadJobNames = Object.entries(workflow?.jobs || {})
    .filter(([, candidateJob]) => candidateJob?.permissions?.contents === "write")
    .map(([jobName]) => jobName);
  if (uploadJobNames.length !== 0) {
    addError(
      ".github/workflows/deploy-production.yml: production deploy must not require contents: write"
    );
  }
  const workflowText = readText(".github/workflows/deploy-production.yml");
  if (
    job?.if !==
    "${{ always() && (github.ref == 'refs/heads/master' || (github.event_name == 'release' && !github.event.release.prerelease && needs.build-release-artifacts.result == 'success')) }}"
  ) {
    addError(
      ".github/workflows/deploy-production.yml: request-deploy must require built Release artifacts before Release deploy requests"
    );
  }
  if (workflowText.includes("TENCENT_COS_PREFIX")) {
    addError(
      ".github/workflows/deploy-production.yml: deploy request prefix must be fixed to qintopia-agent-os"
    );
  }
  if (workflowText.includes("secrets.") && workflowText.includes("== ''")) {
    addError(
      ".github/workflows/deploy-production.yml: secrets must be validated in shell env, not in if expressions"
    );
  }
  if (hasDangerousInputInterpolationInRun(workflowText)) {
    addError(
      ".github/workflows/deploy-production.yml: workflow_dispatch inputs must not be interpolated directly inside run scripts"
    );
  }
  if (workflowText.includes("notes<<NOTES")) {
    addError(
      ".github/workflows/deploy-production.yml: notes output must not use a fixed delimiter"
    );
  }
  const requestDeployBlock =
    workflowText.split(/\n  request-deploy:/)[1]?.split(/\n  [a-zA-Z0-9_-]+:/)[0] || "";
  if (requestDeployBlock.includes("gh release upload")) {
    addError(
      ".github/workflows/deploy-production.yml: request-deploy must not upload GitHub Release assets with production secrets in scope"
    );
  }
  if (workflowText.includes("gh release upload")) {
    addError(
      ".github/workflows/deploy-production.yml: production deploy must not upload GitHub Release assets"
    );
  }
  if (workflowText.includes("upload-github-release-assets")) {
    addError(
      ".github/workflows/deploy-production.yml: GitHub Release assets must not be part of the production deploy workflow"
    );
  }
  if (workflowText.includes("dist/release-assets")) {
    addError(
      ".github/workflows/deploy-production.yml: production deploy artifacts must use COS, not dist/release-assets"
    );
  }
  if (!requestDeployBlock.includes("path: dist")) {
    addError(
      ".github/workflows/deploy-production.yml: request-deploy must download release build artifacts to dist"
    );
  }
  for (const fragment of [
    "Resolve release or manual deploy inputs",
    "ref: master",
    "release:\n    types:\n      - published",
    "require_single_line()",
    "normalize_boolean()",
    "normalize_csv_allowlist()",
    'if [[ "$GITHUB_EVENT_NAME" == "release" ]]',
    "Deploy Production must be run from refs/heads/master",
    "Pre-releases must not trigger production deployment.",
    "Release tag must point to current origin/master HEAD.",
    'release_scope="$(normalize_csv_allowlist',
    'restart_targets="$(normalize_csv_allowlist',
    'dry_run="$(normalize_boolean "dry_run" "$dry_run")',
    'rollback_on_smoke_failure="$(normalize_boolean',
    "build-release-artifacts:",
    "Download release build artifact",
    "Build release sidecar artifact",
    "Build release deploy bundle",
    "Upload release sidecar artifact to Tencent COS",
    "Upload release deploy bundle to Tencent COS",
    "Wait for server deploy result",
    "previous_release_tag",
    "repos/${GITHUB_REPOSITORY}/releases?per_page=100",
    "gh api --paginate --slurp",
    "repos/${GITHUB_REPOSITORY}/actions/workflows/deploy-production.yml/runs?per_page=100",
    "collect-release-deploy-results.mjs",
    "deploy-results.json",
    "resolve-release-restart-targets.mjs",
    "--deploy-results-file",
    "RELEASE_DEPLOY_RESTART_TARGETS_OVERRIDE",
    'notes_delimiter="deploy_notes_$(uuidgen',
    'echo "notes<<${notes_delimiter}"',
    "create-deploy-request.mjs",
    "upload-deploy-request.sh",
    "wait-deploy-result.sh",
    "git merge-base --is-ancestor",
    "pnpm deploy:runner:check",
    "DEPLOY_COMMIT_SHA",
    "DEPLOY_REQUEST_SIGNING_KEY",
    "DEPLOY_REQUEST_SIGNING_KEY_ID: production",
    "RELEASE_DEPLOY_DRY_RUN: ${{ vars.RELEASE_DEPLOY_DRY_RUN || 'true' }}",
    "WAIT_FOR_SERVER_DEPLOY_RESULT: ${{ vars.WAIT_FOR_SERVER_DEPLOY_RESULT || 'false' }}",
    "TENCENT_COS_SECRET_ID: ${{ secrets.TENCENT_COS_SECRET_ID }}",
    "TENCENT_COS_SECRET_KEY: ${{ secrets.TENCENT_COS_SECRET_KEY }}",
  ]) {
    if (!workflowText.includes(fragment)) {
      addError(`.github/workflows/deploy-production.yml: missing ${fragment}`);
    }
  }
  if (workflowText.includes("git checkout --detach")) {
    addError(
      ".github/workflows/deploy-production.yml: workflow must not execute scripts from an older target SHA"
    );
  }
  if (
    workflowText.includes("TENCENT_COS_SECRET_ID: ${{ env.TENCENT_COS_SECRET_ID }}")
  ) {
    addError(
      ".github/workflows/deploy-production.yml: upload step must receive COS secrets directly from production secrets"
    );
  }
}

if (exists(".github/workflows/rollback-production.yml")) {
  const workflow = YAML.parse(readText(".github/workflows/rollback-production.yml"));
  const workflowText = readText(".github/workflows/rollback-production.yml");
  const job = workflow?.jobs?.["request-rollback"];
  const resolveStep = job?.steps?.find(
    (step) => step?.name === "Resolve rollback target"
  );
  const resolveRun = resolveStep?.run;
  const executableResolveRun =
    typeof resolveRun === "string" ? stripCommentOnlyLines(resolveRun) : "";
  const releaseTagInput = workflow?.on?.workflow_dispatch?.inputs?.release_tag;
  const restartTargetsInput = workflow?.on?.workflow_dispatch?.inputs?.restart_targets;
  const releaseTagOptions = releaseTagInput?.options ?? [];
  if (releaseTagInput?.type !== "choice") {
    addError(
      ".github/workflows/rollback-production.yml: release_tag must use a choice input"
    );
  }
  if (releaseTagOptions.length !== 1) {
    addError(
      ".github/workflows/rollback-production.yml: release_tag options must be narrowed to exactly one verified candidate"
    );
  }
  if (!releaseTagOptions.every((tag) => /^v[0-9]+\.[0-9]+\.[0-9]+$/.test(tag))) {
    addError(
      ".github/workflows/rollback-production.yml: release_tag options must be semver-style vX.Y.Z tags"
    );
  }
  if (releaseTagInput?.default !== "v0.2.0" || !releaseTagOptions.includes("v0.2.0")) {
    addError(
      ".github/workflows/rollback-production.yml: release_tag must default to verified candidate v0.2.0"
    );
  }
  if (!releaseTagOptions.every((tag) => tag === "v0.2.0")) {
    addError(
      ".github/workflows/rollback-production.yml: release_tag options must be exactly [v0.2.0] after v0.2.3 rollback audit"
    );
  }
  if (typeof resolveRun !== "string") {
    addError(
      ".github/workflows/rollback-production.yml: Resolve rollback target step must have a run script"
    );
  }
  const releaseTagGuardBlock = `if [[ "$INPUT_RELEASE_TAG" != "v0.2.0" ]]; then
  echo "Rollback target must be v0.2.0 (verified candidate after v0.2.3 audit)." >&2
  exit 2
fi`;
  if (countExactOccurrences(executableResolveRun, releaseTagGuardBlock) !== 1) {
    addError(
      ".github/workflows/rollback-production.yml: Resolve rollback target must contain exactly one executable INPUT_RELEASE_TAG guard for audited candidate v0.2.0"
    );
  }
  const targetShaGuardBlock = `if [[ "$target_sha" != "b24c3f714b19962c5a7b57a486f7aa18c4ae3e86" ]]; then
  echo "Rollback target SHA must match the audited v0.2.0 release commit." >&2
  exit 2
fi`;
  if (countExactOccurrences(executableResolveRun, targetShaGuardBlock) !== 1) {
    addError(
      ".github/workflows/rollback-production.yml: Resolve rollback target must contain exactly one executable target_sha guard for audited v0.2.0 commit b24c3f714b19962c5a7b57a486f7aa18c4ae3e86"
    );
  }
  if (restartTargetsInput?.type !== "choice") {
    addError(
      ".github/workflows/rollback-production.yml: restart_targets must use a choice input"
    );
  }
  if (!restartTargetsInput?.options?.includes("all-hermes-and-system")) {
    addError(
      ".github/workflows/rollback-production.yml: restart_targets must include all-hermes-and-system"
    );
  }
  if (job?.environment !== "production") {
    addError(
      ".github/workflows/rollback-production.yml: request-rollback must use production environment"
    );
  }
  if (job?.permissions?.contents !== "read") {
    addError(
      ".github/workflows/rollback-production.yml: request-rollback must keep contents permission read-only"
    );
  }
  if (hasDangerousInputInterpolationInRun(workflowText)) {
    addError(
      ".github/workflows/rollback-production.yml: workflow_dispatch inputs must not be interpolated directly inside run scripts"
    );
  }
  for (const forbidden of ["ssh ", "git checkout --detach", "gh release upload"]) {
    if (workflowText.includes(forbidden)) {
      addError(`.github/workflows/rollback-production.yml: forbidden ${forbidden}`);
    }
  }
  for (const fragment of [
    "workflow_dispatch:",
    "type: choice",
    "Resolve rollback target",
    'gh api "repos/${GITHUB_REPOSITORY}/releases/tags/${INPUT_RELEASE_TAG}"',
    "Rollback target must be a published non-prerelease GitHub Release.",
    "git merge-base --is-ancestor",
    "Validate rollback artifacts in Tencent COS",
    "fetch-cos-artifact.sh",
    "ROLLBACK_TARGET_SHA",
    "DEPLOY_RELEASE_SCOPE: sidecar-runtime,deploy-bundle,hermes-plugins",
    "DEPLOY_ROLLBACK_ON_SMOKE_FAILURE: false",
    "create-deploy-request.mjs",
    "upload-deploy-request.sh",
    "wait-deploy-result.sh",
    "DEPLOY_REQUEST_SIGNING_KEY",
    "DEPLOY_REQUEST_SIGNING_KEY_ID: production",
    "environment: production",
    "qintopia-agent-os-rollback-request",
  ]) {
    if (!workflowText.includes(fragment)) {
      addError(`.github/workflows/rollback-production.yml: missing ${fragment}`);
    }
  }
}

const runnerText = exists("deploy/runner/qintopia-agent-os-deploy-runner")
  ? readText("deploy/runner/qintopia-agent-os-deploy-runner")
  : "";
for (const forbidden of ["eval ", 'bash -c "$', "ssh ", "git fetch", "git checkout"]) {
  if (runnerText.includes(forbidden)) {
    addError(
      `deploy/runner/qintopia-agent-os-deploy-runner: forbidden fragment ${forbidden}`
    );
  }
}
if (runnerText.includes("${dry_run:+--dry-run}")) {
  addError(
    "deploy/runner/qintopia-agent-os-deploy-runner: dry-run flag must be conditional on dry_run == true"
  );
}
if (!runnerText.includes('if [[ "$dry_run" == "true" ]]')) {
  addError(
    "deploy/runner/qintopia-agent-os-deploy-runner: must explicitly guard dry-run promotion"
  );
}
for (const fragment of [
  "validate_request",
  "hmac.new",
  "signing_envelope",
  "signature verification failed",
  "DEPLOY_REQUEST_SIGNING_KEY is required",
  "DEPLOY_REQUEST_SIGNING_KEY_ID",
  "signature key_id mismatch",
  "request is expired",
  "repository mismatch",
  "cos.prefix must be qintopia-agent-os",
  "deploy-requests/production/requests",
  "cos.bucket does not match runner environment",
  'if [[ -e "${RELEASE_ROOT}/previous" || -L "${RELEASE_ROOT}/previous" ]]',
  'if [[ -e "${RELEASE_ROOT}/current" || -L "${RELEASE_ROOT}/current" ]]',
  'previous_sha="${previous_target##*/}"',
  "promoted_current=true",
  'promote-release.sh \\\n    "${promote_args[@]}" || return $?',
  'smoke-release.sh --restart-targets "$restart_targets" || return $?',
  "run_promotion\n  status=$?",
  'if [[ "$promoted_current" == "true"',
  "rollback failed",
  "rollback succeeded",
]) {
  if (!runnerText.includes(fragment)) {
    addError(`deploy/runner/qintopia-agent-os-deploy-runner: missing ${fragment}`);
  }
}

const promoteText = exists("deploy/runner/promote-release.sh")
  ? readText("deploy/runner/promote-release.sh")
  : "";
for (const fragment of [
  "existing release manifest",
  "staging_dir/manifest.json",
  '"runtime_sha"',
  '"deploy_bundle_sha"',
  '"commit_sha"',
  '"release_scope"',
  '"restart_targets"',
]) {
  if (!promoteText.includes(fragment)) {
    addError(`deploy/runner/promote-release.sh: missing ${fragment}`);
  }
}
for (const forbidden of [
  'manifest.get("release_sha") != sys.argv[2]',
  'python3 - "$release_dir/manifest.json" "$release_sha"',
]) {
  if (promoteText.includes(forbidden)) {
    addError(`deploy/runner/promote-release.sh: forbidden fragment ${forbidden}`);
  }
}

const pollerText = exists("deploy/runner/poll-deploy-requests.sh")
  ? readText("deploy/runner/poll-deploy-requests.sh")
  : "";
for (const fragment of [
  'prefix="qintopia-agent-os"',
  'pointer_key="${prefix}/deploy-requests/production/current.json"',
  "pointer_identity",
  "require_env DEPLOY_REQUEST_SIGNING_KEY",
  "require_env DEPLOY_REQUEST_SIGNING_KEY_ID",
  "request_id_pattern",
  "actual_request_key",
  "request_key == actual_request_key",
  "deploy request key or identity is invalid",
  "is_object_missing_error",
  "No deploy request pointer found; idle",
  "Deploy request pointer download failed",
  "Deploy request result already exists; idle",
  "Deploy request result probe failed",
  "Deploy request already processed; idle",
  "Deploy request already failed; idle",
  "/failed",
  "deploy request failed before promotion result was written",
]) {
  if (!pollerText.includes(fragment)) {
    addError(`deploy/runner/poll-deploy-requests.sh: missing ${fragment}`);
  }
}
for (const forbidden of [
  'coscli_path" ls',
  "$NF ~ /\\.json$/",
  "pending_prefix",
  "deploy request was already consumed",
  "archive_key=",
  '"$coscli_path" rm "cos://${bucket_alias}/${request_key}"',
  "awk '/\\\\.json$/",
  'request_id="$parsed_request_id"',
  'result_key="$parsed_result_key"',
]) {
  if (pollerText.includes(forbidden)) {
    addError(`deploy/runner/poll-deploy-requests.sh: forbidden fragment ${forbidden}`);
  }
}

const createRequestText = exists("tools/deploy/create-deploy-request.mjs")
  ? readText("tools/deploy/create-deploy-request.mjs")
  : "";
for (const fragment of [
  'const fixedCosPrefix = "qintopia-agent-os"',
  "signRequest",
  "signingEnvelope",
  "canonicalJson",
  "signature",
  "DEPLOY_REQUEST_SIGNING_KEY_ID",
  "requireSha",
  "forbidCosPrefixOverride",
]) {
  if (!createRequestText.includes(fragment)) {
    addError(`tools/deploy/create-deploy-request.mjs: missing ${fragment}`);
  }
}

const smokeText = exists("deploy/runner/smoke-release.sh")
  ? readText("deploy/runner/smoke-release.sh")
  : "";
for (const fragment of [
  "restart_hermes_service",
  "runuser -l",
  "hermes-gateway-erhua.service",
  "hermes-gateway-wenyuange.service",
  "hermes-gateway-xiaoman.service",
  "hermes-gateway-silaoshi.service",
  "hermes-gateway-huabaosi.service",
  "hermes-gateway-guanerye.service",
  "unsupported restart target",
]) {
  if (!smokeText.includes(fragment)) {
    addError(`deploy/runner/smoke-release.sh: missing ${fragment}`);
  }
}
if (
  smokeText.includes('echo "Smoke checks passed') &&
  !smokeText.includes("restart_hermes_service")
) {
  addError(
    "deploy/runner/smoke-release.sh: must not report Hermes smoke without restart checks"
  );
}

const uploadRequestText = exists("deploy/runner/upload-deploy-request.sh")
  ? readText("deploy/runner/upload-deploy-request.sh")
  : "";
for (const fragment of [
  "pointer_key",
  "deploy-requests/production/current.json",
  "Uploaded deploy request pointer",
]) {
  if (!uploadRequestText.includes(fragment)) {
    addError(`deploy/runner/upload-deploy-request.sh: missing ${fragment}`);
  }
}
if (
  uploadRequestText.includes(
    '${TENCENT_COS_SESSION_TOKEN:+--session_token "$TENCENT_COS_SESSION_TOKEN"}'
  )
) {
  addError(
    "deploy/runner/upload-deploy-request.sh: session token must use an auth_args array"
  );
}

const waitResultText = exists("deploy/runner/wait-deploy-result.sh")
  ? readText("deploy/runner/wait-deploy-result.sh")
  : "";
for (const fragment of [
  "DEPLOY_RESULT_TIMEOUT_SECONDS",
  "DEPLOY_RESULT_POLL_SECONDS",
  "qintopia-agent-os/deploy-results/production",
  "succeeded|dry_run_succeeded",
  "failed|rolled_back",
  "Timed out after",
  "print_sanitized_coscli_output",
]) {
  if (!waitResultText.includes(fragment)) {
    addError(`deploy/runner/wait-deploy-result.sh: missing ${fragment}`);
  }
}
if (waitResultText.includes("ssh ")) {
  addError("deploy/runner/wait-deploy-result.sh: must not SSH to production");
}

const restartRules = exists("deploy/restart-target-rules.yaml")
  ? readYaml("deploy/restart-target-rules.yaml")
  : {};
const ruleTargets = new Set((restartRules.rules ?? []).map((rule) => rule.target));
const allowedRuleTargets = new Set(restartRules.allowed_targets ?? []);
const schemaTargets = new Set(
  deployRequestSchema?.properties?.restart_targets?.items?.enum ?? []
);

for (const target of allowedRuleTargets) {
  if (!schemaTargets.has(target)) {
    addError(`deploy/restart-target-rules.yaml: target ${target} missing from schema`);
  }
  if (!ruleTargets.has(target)) {
    addError(`deploy/restart-target-rules.yaml: target ${target} has no path rule`);
  }
}
for (const target of schemaTargets) {
  if (!allowedRuleTargets.has(target)) {
    addError(
      `deploy/runner/deploy-request.schema.json: target ${target} missing from restart rules`
    );
  }
}

const agentRegistry = exists("registry/agents.yaml")
  ? readYaml("registry/agents.yaml")
  : { entries: [] };
for (const entry of agentRegistry.entries ?? []) {
  if (entry.id === "agents/default") {
    continue;
  }
  if (!entry.manifest || !exists(entry.manifest)) {
    continue;
  }
  const agentManifest = readYaml(entry.manifest);
  const target = agentManifest.runtime?.restart_target;
  const service = agentManifest.runtime?.systemd_user_service;
  if (!target || !service) {
    addError(`${entry.manifest}: runtime restart target and service are required`);
    continue;
  }
  if (!schemaTargets.has(target)) {
    addError(`${entry.manifest}: runtime.restart_target ${target} missing from schema`);
  }
  if (!allowedRuleTargets.has(target) || !ruleTargets.has(target)) {
    addError(
      `${entry.manifest}: runtime.restart_target ${target} missing from restart rules`
    );
  }
  if (!smokeText.includes(`${target})`)) {
    addError(`deploy/runner/smoke-release.sh: missing case for ${target}`);
  }
  if (!smokeText.includes(service)) {
    addError(`deploy/runner/smoke-release.sh: missing service ${service}`);
  }
}

const resolverText = exists("tools/deploy/resolve-restart-targets.mjs")
  ? readText("tools/deploy/resolve-restart-targets.mjs")
  : "";
for (const fragment of [
  "deploy/restart-target-rules.yaml",
  "RELEASE_DEPLOY_RESTART_TARGETS_OVERRIDE",
  "Restart Impact",
  "unmatched production-adjacent",
  "latestPreviousReleaseTag",
  "--github-output",
]) {
  if (!resolverText.includes(fragment)) {
    addError(`tools/deploy/resolve-restart-targets.mjs: missing ${fragment}`);
  }
}

for (const script of requiredFiles.filter((file) =>
  file.startsWith("deploy/runner/")
)) {
  if (!exists(script)) {
    continue;
  }
  if (
    script.endsWith(".json") ||
    script.endsWith(".yaml") ||
    script.endsWith(".md") ||
    script.endsWith(".service") ||
    script.endsWith(".timer")
  ) {
    continue;
  }
  const mode = fs.statSync(path.join(repoRoot, script)).mode & 0o111;
  if (mode === 0) {
    addError(`${script}: must be executable`);
  }
}

try {
  execFileSync("bash", ["-n", "deploy/runner/qintopia-agent-os-deploy-runner"], {
    cwd: repoRoot,
  });
  execFileSync("bash", ["-n", "deploy/runner/poll-deploy-requests.sh"], {
    cwd: repoRoot,
  });
  execFileSync("bash", ["-n", "deploy/runner/promote-release.sh"], { cwd: repoRoot });
  execFileSync("bash", ["-n", "deploy/runner/install-release-systemd-units.sh"], {
    cwd: repoRoot,
  });
  execFileSync("bash", ["-n", "deploy/runner/rollback-release.sh"], { cwd: repoRoot });
  execFileSync("bash", ["-n", "deploy/runner/smoke-release.sh"], { cwd: repoRoot });
  execFileSync("bash", ["-n", "deploy/runner/upload-deploy-request.sh"], {
    cwd: repoRoot,
  });
  execFileSync("bash", ["-n", "deploy/runner/wait-deploy-result.sh"], {
    cwd: repoRoot,
  });
  execFileSync("node", ["tools/deploy/test-resolve-restart-targets.mjs"], {
    cwd: repoRoot,
  });
  execFileSync("node", ["tools/deploy/test-collect-release-deploy-results.mjs"], {
    cwd: repoRoot,
  });
  execFileSync("node", ["tools/deploy/test-resolve-release-restart-targets.mjs"], {
    cwd: repoRoot,
  });
  execFileSync("node", ["tools/deploy/test-resolve-release-deploy-base.mjs"], {
    cwd: repoRoot,
  });
  execFileSync("node", ["tools/deploy/test-deploy-runner-poller.mjs"], {
    cwd: repoRoot,
  });
  execFileSync("node", ["tools/deploy/test-deploy-runner-promotion.mjs"], {
    cwd: repoRoot,
  });
  execFileSync("node", ["tools/deploy/test-release-systemd-install.mjs"], {
    cwd: repoRoot,
  });
  execFileSync(
    "node",
    ["tools/deploy/test-huabaosi-image-production-observation.mjs"],
    { cwd: repoRoot }
  );
  execFileSync("node", ["tools/deploy/test-huabaosi-wecom-observation.mjs"], {
    cwd: repoRoot,
  });
  execFileSync("node", ["tools/deploy/test-huabaosi-wecom-canary-observation.mjs"], {
    cwd: repoRoot,
  });
  execFileSync("node", ["tools/deploy/test-qiwe-image-staging-smoke.mjs"], {
    cwd: repoRoot,
  });
  execFileSync(
    "node",
    ["tools/deploy/test-xiaoman-production-observation-contracts.mjs"],
    { cwd: repoRoot }
  );
} catch (error) {
  addError(`deploy runner shell syntax check failed: ${error.message}`);
}

const packageJson = JSON.parse(readText("package.json"));
if (!packageJson.scripts?.["deploy:runner:check"]) {
  addError("package.json: missing deploy:runner:check");
}
if (!packageJson.scripts?.["check:light"]?.includes("pnpm deploy:runner:check")) {
  addError("package.json: check:light must include pnpm deploy:runner:check");
}

if (exists("tools/deploy/build-deploy-bundle.mjs")) {
  const builder = readText("tools/deploy/build-deploy-bundle.mjs");
  for (const fragment of [
    "deploy/runner/qintopia-agent-os-deploy-runner",
    "deploy/runner/poll-deploy-requests.sh",
    "deploy/runner/install-release-systemd-units.sh",
    "deploy/runner/deploy-request.schema.json",
    "deploy/runner/wait-deploy-result.sh",
    "deploy/restart-target-rules.yaml",
    "tools/deploy/collect-release-deploy-results.mjs",
    "tools/deploy/resolve-release-deploy-base.mjs",
    "tools/deploy/resolve-release-restart-targets.mjs",
    "tools/deploy/resolve-restart-targets.mjs",
    "deploy/sidecar/scripts/fetch-cos-artifact.sh",
    "deploy/sidecar/scripts/huabaosi-image-generation-staging-smoke.sh",
    "deploy/sidecar/scripts/huabaosi-image-generation-production-observation-smoke.sh",
    "deploy/sidecar/scripts/huabaosi-wecom-canary-observation-smoke.sh",
    "deploy/sidecar/scripts/huabaosi-wecom-gateway-observation-smoke.sh",
    "deploy/sidecar/scripts/install-coscli.sh",
    "deploy/sidecar/scripts/qiwe-image-send-staging-smoke.sh",
    "deploy/sidecar/scripts/operations-downstream-timers-observation-smoke.sh",
    "deploy/sidecar/scripts/operations-group-send-ready-timer-observation-smoke.sh",
    "deploy/sidecar/scripts/xiaoman-activity-downstream-observation-smoke.sh",
    "deploy/sidecar/scripts/xiaoman-activity-image-generation-starter-observation-smoke.sh",
    "deploy/sidecar/scripts/xiaoman-activity-production-preflight-smoke.sh",
    "deploy/sidecar/scripts/xiaoman-activity-promotion-starter-timer-observation-smoke.sh",
    "deploy/sidecar/scripts/xiaoman-activity-send-request-starter-observation-smoke.sh",
    "deploy/sidecar/scripts/xiaoman-activity-signal-timer-observation-smoke.sh",
  ]) {
    if (!builder.includes(fragment)) {
      addError(`tools/deploy/build-deploy-bundle.mjs: must package ${fragment}`);
    }
  }
}

if (exists("deploy/runner/qintopia-agent-os-deploy-runner")) {
  const runner = readText("deploy/runner/qintopia-agent-os-deploy-runner");
  for (const fragment of [
    "install-release-systemd-units.sh",
    '--release-root "$RELEASE_ROOT"',
    '--release-sha "$release_sha"',
  ]) {
    if (!runner.includes(fragment)) {
      addError(`deploy runner must install release systemd units (${fragment})`);
    }
  }
}

if (exists("deploy/runner/install-release-systemd-units.sh")) {
  const installer = readText("deploy/runner/install-release-systemd-units.sh");
  for (const fragment of [
    "render-systemd-units.sh",
    "qintopia-agentos-xiaoman-activity-signal-worker.timer",
    "qintopia-agentos-xiaoman-activity-promotion-starter-worker.timer",
    "qintopia-agentos-xiaoman-activity-image-generation-starter-worker.timer",
    "qintopia-agentos-xiaoman-activity-send-request-starter-worker.timer",
    "qintopia-agentos-operations-group-send-ready.timer",
    '"$systemctl_bin" daemon-reload',
  ]) {
    if (!installer.includes(fragment)) {
      addError(`release systemd installer is missing ${fragment}`);
    }
  }
  for (const forbidden of ["eval ", "bash -c", "ssh "]) {
    if (installer.includes(forbidden)) {
      addError(`release systemd installer must not contain ${forbidden}`);
    }
  }
}

if (errors.length > 0) {
  console.error("Deploy runner check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log("Deploy runner check passed.");
