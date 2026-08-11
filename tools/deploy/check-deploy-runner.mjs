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
  ".github/workflows/activate-production-timers.yml",
  ".github/workflows/observe-production-runtime.yml",
  ".github/workflows/apply-production-hermes-crons.yml",
  ".github/workflows/retire-production-legacy-crons.yml",
  ".github/workflows/run-production-runtime-one-shot.yml",
  "deploy/runner/README.md",
  "deploy/runner/manifest.yaml",
  "deploy/runner/deploy-request.schema.json",
  "deploy/runner/deploy-result.schema.json",
  "deploy/runner/install-release-systemd-units.sh",
  "deploy/runner/qintopia-agent-os-deploy-runner",
  "deploy/runner/activate-erhua-profile.sh",
  "runtime/hermes/validate_hermes_python.py",
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
  "deploy/sidecar/scripts/render-systemd-units.sh",
  "tools/deploy/create-deploy-request.mjs",
  "tools/deploy/collect-release-deploy-results.mjs",
  "tools/deploy/resolve-release-deploy-base.mjs",
  "tools/deploy/validate-legacy-runner-bootstrap.mjs",
  "tools/deploy/resolve-release-restart-targets.mjs",
  "tools/deploy/resolve-restart-targets.mjs",
  "tools/deploy/test-collect-release-deploy-results.mjs",
  "tools/deploy/test-resolve-release-deploy-base.mjs",
  "tools/deploy/test-validate-legacy-runner-bootstrap.mjs",
  "tools/deploy/test-resolve-release-restart-targets.mjs",
  "tools/deploy/test-resolve-restart-targets.mjs",
  "tools/deploy/test-deploy-runner-poller.mjs",
  "tools/deploy/test-deploy-runner-promotion.mjs",
  "tools/deploy/test-production-timer-activation-runner.mjs",
  "tools/deploy/test-production-observation-runner.mjs",
  "tools/deploy/test-production-worker-run-evidence-smoke.mjs",
  "tools/deploy/test-production-hermes-cron-apply-runner.mjs",
  "tools/deploy/test-production-legacy-cron-retirement-runner.mjs",
  "tools/deploy/test-production-runtime-one-shot-runner.mjs",
  "tools/deploy/test-wait-deploy-result.mjs",
  "tools/deploy/test-promote-existing-release-metadata.mjs",
  "tools/deploy/test-promote-release-tree.mjs",
  "tools/deploy/test-fetch-cos-artifact-permissions.mjs",
  "tools/deploy/test-release-systemd-install.mjs",
  "tools/deploy/test-erhua-legacy-cron-observation.mjs",
  "tools/deploy/test-erhua-legacy-cron-retirement.mjs",
  "tools/deploy/test-xiaoman-legacy-cron-retirement.mjs",
  "tools/deploy/test-erhua-morning-brief-production-activation.mjs",
  "tools/deploy/test-xiaoman-profile-bundle-observation.mjs",
  "deploy/sidecar/scripts/xiaoman-profile-bundle-observation-smoke.sh",
  "docs/operations/production-timer-activation-runbook.md",
  "docs/operations/production-runtime-observation-runbook.md",
  "docs/operations/production-hermes-cron-apply-runbook.md",
  "docs/operations/production-legacy-cron-retirement-runbook.md",
  "docs/operations/production-runtime-one-shot-runbook.md",
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
    runtime_artifact_profile: "huabaosi-production",
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
  const profileRequest = {
    ...sampleRequest,
    release_scope: ["hermes-profile-erhua"],
    restart_targets: ["hermes-erhua"],
  };
  if (!validateRequest(profileRequest)) {
    addError("deploy request schema must accept fixed Erhua profile coupling");
  }
  for (const restart_targets of [
    ["hermes-xiaoman"],
    ["hermes-erhua", "hermes-xiaoman"],
  ]) {
    if (validateRequest({ ...profileRequest, restart_targets })) {
      addError("deploy request schema must reject non-exclusive Erhua restart targets");
    }
  }
  if (validateRequest({ ...profileRequest, rollback_on_smoke_failure: false })) {
    addError(
      "deploy request schema must require Erhua profile rollback on smoke failure"
    );
  }
  const profileActivationRequest = {
    ...profileRequest,
    dry_run: false,
    profile_dry_run_request_id: "deploy-20260706T000000Z-0123456789ab",
  };
  if (!validateRequest(profileActivationRequest)) {
    addError("deploy request schema must accept an activation bound to a dry run");
  }
  if (
    validateRequest({
      ...profileActivationRequest,
      profile_dry_run_request_id: undefined,
    })
  ) {
    addError("deploy request schema must bind profile activation to a dry-run request");
  }
  const productionActivationRequest = {
    ...sampleRequest,
    release_scope: ["production-activation"],
    restart_targets: ["qintopia-system-services"],
    dry_run: false,
    rollback_on_smoke_failure: false,
    activation: {
      targets: [
        "erhua-morning-brief",
        "xiaoman-weekly-recruitment",
        "xiaoman-weekly-plan-confirmation",
        "xiaoman-weekly-preview",
      ],
    },
  };
  if (!validateRequest(productionActivationRequest)) {
    addError(
      `deploy request schema must accept fixed production activation requests ${JSON.stringify(
        validateRequest.errors
      )}`
    );
  }
  for (const badRequest of [
    { ...sampleRequest, activation: { targets: ["erhua-morning-brief"] } },
    {
      ...productionActivationRequest,
      release_scope: ["production-activation", "deploy-bundle"],
    },
    {
      ...productionActivationRequest,
      restart_targets: ["hermes-xiaoman"],
    },
    { ...productionActivationRequest, dry_run: true },
    { ...productionActivationRequest, rollback_on_smoke_failure: true },
    {
      ...productionActivationRequest,
      activation: { targets: ["erhua-morning-brief", "erhua-morning-brief"] },
    },
    {
      ...productionActivationRequest,
      activation: { targets: ["unknown-target"] },
    },
  ]) {
    if (validateRequest(badRequest)) {
      addError(
        "deploy request schema must reject unsafe production activation variants"
      );
      break;
    }
  }
  const productionObservationRequest = {
    ...sampleRequest,
    release_scope: ["production-observation"],
    restart_targets: ["qintopia-system-services"],
    dry_run: false,
    rollback_on_smoke_failure: false,
    observation: {
      targets: ["qiwe-image-send", "xiaoman-daily-case-report-auto-publish"],
    },
  };
  if (!validateRequest(productionObservationRequest)) {
    addError(
      `deploy request schema must accept fixed production observation requests ${JSON.stringify(
        validateRequest.errors
      )}`
    );
  }
  const productionWorkerRunObservationRequest = {
    ...productionObservationRequest,
    observation: {
      targets: [
        "erhua-morning-brief-worker-run",
        "xiaoman-daily-case-report-worker-run",
        "xiaoman-weekly-recruitment-worker-run",
        "xiaoman-weekly-plan-confirmation-worker-run",
        "xiaoman-weekly-preview-worker-run",
      ],
    },
  };
  if (!validateRequest(productionWorkerRunObservationRequest)) {
    addError(
      `deploy request schema must accept fixed worker-run observation requests ${JSON.stringify(
        validateRequest.errors
      )}`
    );
  }
  for (const badRequest of [
    { ...sampleRequest, observation: { targets: ["qiwe-image-send"] } },
    {
      ...productionObservationRequest,
      release_scope: ["production-observation", "deploy-bundle"],
    },
    {
      ...productionObservationRequest,
      restart_targets: ["hermes-xiaoman"],
    },
    { ...productionObservationRequest, dry_run: true },
    { ...productionObservationRequest, rollback_on_smoke_failure: true },
    {
      ...productionObservationRequest,
      observation: { targets: ["qiwe-image-send", "qiwe-image-send"] },
    },
    {
      ...productionObservationRequest,
      observation: { targets: ["unknown-target"] },
    },
  ]) {
    if (validateRequest(badRequest)) {
      addError(
        "deploy request schema must reject unsafe production observation variants"
      );
      break;
    }
  }
  const productionRetirementRequest = {
    ...sampleRequest,
    release_scope: ["production-legacy-cron-retirement"],
    restart_targets: ["qintopia-system-services"],
    dry_run: false,
    rollback_on_smoke_failure: false,
    legacy_cron_retirement: {
      targets: ["erhua-legacy-cron", "xiaoman-legacy-cron"],
    },
  };
  if (!validateRequest(productionRetirementRequest)) {
    addError(
      `deploy request schema must accept fixed production legacy cron retirement requests ${JSON.stringify(
        validateRequest.errors
      )}`
    );
  }
  for (const badRequest of [
    { ...sampleRequest, legacy_cron_retirement: { targets: ["erhua-legacy-cron"] } },
    {
      ...productionRetirementRequest,
      release_scope: ["production-legacy-cron-retirement", "deploy-bundle"],
    },
    {
      ...productionRetirementRequest,
      restart_targets: ["hermes-xiaoman"],
    },
    { ...productionRetirementRequest, dry_run: true },
    { ...productionRetirementRequest, rollback_on_smoke_failure: true },
    {
      ...productionRetirementRequest,
      legacy_cron_retirement: {
        targets: ["erhua-legacy-cron", "erhua-legacy-cron"],
      },
    },
    {
      ...productionRetirementRequest,
      legacy_cron_retirement: { targets: ["unknown-target"] },
    },
  ]) {
    if (validateRequest(badRequest)) {
      addError(
        "deploy request schema must reject unsafe production legacy cron retirement variants"
      );
      break;
    }
  }
  const productionHermesCronApplyRequest = {
    ...sampleRequest,
    release_scope: ["production-hermes-cron-apply"],
    restart_targets: ["qintopia-system-services"],
    dry_run: false,
    rollback_on_smoke_failure: false,
    hermes_cron_apply: {
      targets: [
        "erhua-morning-brief",
        "xiaoman-daily-case-report",
        "xiaoman-weekly-recruitment",
        "xiaoman-weekly-plan-confirmation",
        "xiaoman-weekly-preview",
      ],
      mode: "install",
    },
  };
  if (!validateRequest(productionHermesCronApplyRequest)) {
    addError(
      `deploy request schema must accept fixed production Hermes cron apply requests ${JSON.stringify(
        validateRequest.errors
      )}`
    );
  }
  const productionHermesCronEnableRequest = {
    ...productionHermesCronApplyRequest,
    hermes_cron_apply: {
      targets: ["xiaoman-weekly-preview"],
      mode: "enable",
    },
  };
  if (!validateRequest(productionHermesCronEnableRequest)) {
    addError(
      `deploy request schema must accept fixed production Hermes cron enable requests ${JSON.stringify(
        validateRequest.errors
      )}`
    );
  }
  for (const badRequest of [
    {
      ...sampleRequest,
      hermes_cron_apply: {
        targets: ["xiaoman-weekly-preview"],
        mode: "install",
      },
    },
    {
      ...productionHermesCronApplyRequest,
      release_scope: ["production-hermes-cron-apply", "deploy-bundle"],
    },
    {
      ...productionHermesCronApplyRequest,
      restart_targets: ["hermes-xiaoman"],
    },
    { ...productionHermesCronApplyRequest, dry_run: true },
    { ...productionHermesCronApplyRequest, rollback_on_smoke_failure: true },
    {
      ...productionHermesCronApplyRequest,
      hermes_cron_apply: {
        targets: ["xiaoman-weekly-preview", "xiaoman-weekly-preview"],
        mode: "install",
      },
    },
    {
      ...productionHermesCronApplyRequest,
      hermes_cron_apply: {
        targets: ["unknown-target"],
        mode: "install",
      },
    },
    {
      ...productionHermesCronApplyRequest,
      hermes_cron_apply: {
        targets: ["xiaoman-weekly-preview"],
        mode: "run",
      },
    },
  ]) {
    if (validateRequest(badRequest)) {
      addError(
        "deploy request schema must reject unsafe production Hermes cron apply variants"
      );
      break;
    }
  }
  const productionRuntimeOneShotRequest = {
    ...sampleRequest,
    release_scope: ["production-runtime-one-shot"],
    restart_targets: ["qintopia-system-services"],
    dry_run: false,
    rollback_on_smoke_failure: false,
    runtime_one_shot: {
      targets: ["xiaoman-daily-case-report-auto-publish-backfill"],
      backfill_date: "2026-08-10",
      approval: "approved-production-xiaoman-daily-case-report-auto-publish-backfill",
    },
  };
  if (!validateRequest(productionRuntimeOneShotRequest)) {
    addError(
      `deploy request schema must accept fixed production runtime one-shot requests ${JSON.stringify(
        validateRequest.errors
      )}`
    );
  }
  const productionErhuaOneShotRequest = {
    ...productionRuntimeOneShotRequest,
    runtime_one_shot: {
      targets: ["erhua-morning-brief"],
      approval: "approved-production-erhua-morning-brief-one-shot",
    },
  };
  if (!validateRequest(productionErhuaOneShotRequest)) {
    addError(
      `deploy request schema must accept fixed Erhua production runtime one-shot requests ${JSON.stringify(
        validateRequest.errors
      )}`
    );
  }
  for (const badRequest of [
    {
      ...sampleRequest,
      runtime_one_shot: {
        targets: ["erhua-morning-brief"],
        approval: "approved-production-erhua-morning-brief-one-shot",
      },
    },
    {
      ...productionRuntimeOneShotRequest,
      release_scope: ["production-runtime-one-shot", "deploy-bundle"],
    },
    {
      ...productionRuntimeOneShotRequest,
      restart_targets: ["hermes-xiaoman"],
    },
    { ...productionRuntimeOneShotRequest, dry_run: true },
    { ...productionRuntimeOneShotRequest, rollback_on_smoke_failure: true },
    {
      ...productionRuntimeOneShotRequest,
      runtime_one_shot: {
        targets: [
          "erhua-morning-brief",
          "xiaoman-daily-case-report-auto-publish-backfill",
        ],
        backfill_date: "2026-08-10",
        approval: "approved-production-xiaoman-daily-case-report-auto-publish-backfill",
      },
    },
    {
      ...productionRuntimeOneShotRequest,
      runtime_one_shot: {
        targets: ["xiaoman-daily-case-report-auto-publish-backfill"],
        approval: "approved-production-xiaoman-daily-case-report-auto-publish-backfill",
      },
    },
    {
      ...productionRuntimeOneShotRequest,
      runtime_one_shot: {
        targets: ["erhua-morning-brief"],
        backfill_date: "2026-08-10",
        approval: "approved-production-erhua-morning-brief-one-shot",
      },
    },
    {
      ...productionRuntimeOneShotRequest,
      runtime_one_shot: {
        targets: ["unknown-target"],
        approval: "approved-production-erhua-morning-brief-one-shot",
      },
    },
  ]) {
    if (validateRequest(badRequest)) {
      addError(
        "deploy request schema must reject unsafe production runtime one-shot variants"
      );
      break;
    }
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
    commit_sha: "0123456789abcdef0123456789abcdef01234567",
    runtime_sha: "0123456789abcdef0123456789abcdef01234567",
    runtime_artifact_profile: "huabaosi-production",
    deploy_bundle_sha: "abcdef0123456789abcdef0123456789abcdef01",
    release_scope: ["deploy-bundle"],
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
  const productionHermesCronApplyResult = {
    ...sampleResult,
    status: "succeeded",
    release_scope: ["production-hermes-cron-apply"],
    checks: [
      { name: "deploy-runner", status: "passed" },
      {
        name: "production-hermes-cron-apply",
        status: "passed",
        detail:
          '{"schema_version":1,"mode":"install","targets":[{"target":"xiaoman-weekly-preview","mode":"install","status":"passed","detail":"mode=install"}]}',
      },
    ],
  };
  if (!validateResult(productionHermesCronApplyResult)) {
    addError(
      `deploy/runner/deploy-result.schema.json: production Hermes cron apply result failed validation ${JSON.stringify(
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
  const productionJobNames = Object.keys(workflow?.jobs || {}).sort();
  if (
    JSON.stringify(productionJobNames) !==
    JSON.stringify(["build-release-artifacts", "request-deploy"])
  ) {
    addError(
      ".github/workflows/deploy-production.yml: dual-runtime publication must stay within the existing two jobs"
    );
  }
  const runtimeProfileInput =
    workflow?.on?.workflow_dispatch?.inputs?.runtime_artifact_profile;
  const legacyBootstrapInput =
    workflow?.on?.workflow_dispatch?.inputs?.legacy_runner_bootstrap;
  if (
    runtimeProfileInput?.type !== "choice" ||
    runtimeProfileInput?.default !== "huabaosi-production" ||
    JSON.stringify(runtimeProfileInput?.options) !==
      JSON.stringify(["huabaosi-production"])
  ) {
    addError(
      ".github/workflows/deploy-production.yml: primary runtime profile must be fixed to huabaosi-production; QiWe is a companion"
    );
  }
  if (
    legacyBootstrapInput?.type !== "boolean" ||
    legacyBootstrapInput?.default !== false ||
    legacyBootstrapInput?.required !== true
  ) {
    addError(
      ".github/workflows/deploy-production.yml: legacy_runner_bootstrap must be an explicit default-false boolean"
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
    "Release-published production deploys must use runtime_artifact_profile=huabaosi-production.",
    'release_scope="$(normalize_csv_allowlist',
    'restart_targets="$(normalize_csv_allowlist',
    'dry_run="$(normalize_boolean "dry_run" "$dry_run")',
    'rollback_on_smoke_failure="$(normalize_boolean',
    "rollback_on_smoke_failure=true",
    "build-release-artifacts:",
    "Download release build artifact",
    "Build release sidecar artifact",
    "Build release QiWe companion artifact",
    "Build release deploy bundle",
    "Upload release sidecar artifact to Tencent COS",
    "Upload release QiWe companion artifact to Tencent COS",
    "Upload release deploy bundle to Tencent COS",
    "Validate deploy artifacts in Tencent COS",
    "QINTOPIA_SIDECAR_ARTIFACT_PROFILE",
    "RUNTIME_SHA",
    "DEPLOY_BUNDLE_SHA",
    "fetch-cos-artifact.sh",
    "node tools/deploy/build-qiwe-production-sidecar-artifact.mjs",
    "qintopia-message-sidecar-qiwe-production-linux-x86_64-gnu",
    "QINTOPIA_SIDECAR_ARTIFACT_PROFILE: huabaosi-production",
    "QINTOPIA_SIDECAR_ARTIFACT_PROFILE: qiwe-production",
    "sidecar-profiles/qiwe-production",
    "runtime_artifact_profile must be huabaosi-production; QiWe is installed as a companion runtime.",
    "Wait for server deploy result",
    "previous_release_tag",
    "repos/${GITHUB_REPOSITORY}/releases?per_page=100",
    "gh api --paginate --slurp",
    "repos/${GITHUB_REPOSITORY}/actions/workflows/deploy-production.yml/runs?per_page=100",
    "collect-release-deploy-results.mjs",
    "validate-legacy-runner-bootstrap.mjs",
    "legacy_runner_bootstrap",
    'huabaosi_feature_contract="current"',
    'huabaosi_feature_contract="legacy-runner-bootstrap"',
    "QINTOPIA_HUABAOSI_PRODUCTION_FEATURE_CONTRACT",
    "QINTOPIA_LEGACY_RUNNER_BOOTSTRAP_RUNTIME_SHA",
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
  const runtimeArtifactProfileInput =
    workflow?.on?.workflow_dispatch?.inputs?.runtime_artifact_profile;
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
  if (runtimeArtifactProfileInput !== undefined) {
    addError(
      ".github/workflows/rollback-production.yml: runtime_artifact_profile must not be a global rollback input"
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
  const unavailableTargetGuard = `echo "Rollback target v0.2.0 is a legacy single-runtime release and cannot satisfy the required Huabaosi primary plus QiWe companion contract." >&2
echo "No owner-triggered dual-runtime rollback target is currently verified; no deploy request was created." >&2
exit 2`;
  if (countExactOccurrences(executableResolveRun, unavailableTargetGuard) !== 1) {
    addError(
      ".github/workflows/rollback-production.yml: legacy v0.2.0 rollback must fail closed exactly once before artifact access or request creation"
    );
  }
  for (const forbidden of [
    "inputs.runtime_artifact_profile",
    "INPUT_RUNTIME_ARTIFACT_PROFILE",
    "normalize_runtime_artifact_profile",
    "huabaosi-production|qiwe-production",
  ]) {
    if (workflowText.includes(forbidden)) {
      addError(
        `.github/workflows/rollback-production.yml: global runtime profile switch is forbidden (${forbidden})`
      );
    }
  }
  for (const fragment of [
    "workflow_dispatch:",
    "type: choice",
    "Resolve rollback target",
    'gh api "repos/${GITHUB_REPOSITORY}/releases/tags/${INPUT_RELEASE_TAG}"',
    "Rollback target must be a published non-prerelease GitHub Release.",
    "git merge-base --is-ancestor",
    'runtime_artifact_profile="huabaosi-production"',
    "runtime-artifact-profile=${runtime_artifact_profile}",
    "Validate rollback artifacts in Tencent COS",
    "fetch-cos-artifact.sh",
    "QINTOPIA_SIDECAR_ARTIFACT_PROFILE=huabaosi-production",
    "QINTOPIA_SIDECAR_ARTIFACT_PROFILE=qiwe-production",
    "${temp_dir}/sidecar-profiles/qiwe-production",
    "ROLLBACK_TARGET_SHA",
    "QINTOPIA_SIDECAR_ARTIFACT_PROFILE",
    "DEPLOY_RUNTIME_ARTIFACT_PROFILE",
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

if (exists(".github/workflows/activate-production-timers.yml")) {
  const workflow = YAML.parse(
    readText(".github/workflows/activate-production-timers.yml")
  );
  const workflowText = readText(".github/workflows/activate-production-timers.yml");
  const job = workflow?.jobs?.["request-activation"];
  const activationTargetsInput =
    workflow?.on?.workflow_dispatch?.inputs?.activation_targets;
  const releaseShaInput = workflow?.on?.workflow_dispatch?.inputs?.release_sha;

  if (!workflow?.on?.workflow_dispatch) {
    addError(
      ".github/workflows/activate-production-timers.yml: must use workflow_dispatch"
    );
  }
  if (workflow?.concurrency?.group !== "production-deploy") {
    addError(
      ".github/workflows/activate-production-timers.yml: must share production-deploy concurrency"
    );
  }
  if (job?.environment !== "production") {
    addError(
      ".github/workflows/activate-production-timers.yml: request-activation must use production environment"
    );
  }
  if (job?.permissions?.contents !== "read") {
    addError(
      ".github/workflows/activate-production-timers.yml: request-activation must keep contents permission read-only"
    );
  }
  if (releaseShaInput?.required !== true || releaseShaInput?.type !== "string") {
    addError(
      ".github/workflows/activate-production-timers.yml: release_sha must be a required string input"
    );
  }
  if (
    activationTargetsInput?.default !==
    "erhua-morning-brief,xiaoman-weekly-recruitment,xiaoman-weekly-plan-confirmation,xiaoman-weekly-preview"
  ) {
    addError(
      ".github/workflows/activate-production-timers.yml: default activation targets must stay limited to Erhua and Xiaoman weekly loop timers"
    );
  }
  const uploadJobNames = Object.entries(workflow?.jobs || {})
    .filter(([, candidateJob]) => candidateJob?.permissions?.contents === "write")
    .map(([jobName]) => jobName);
  if (uploadJobNames.length !== 0) {
    addError(
      ".github/workflows/activate-production-timers.yml: timer activation must not require contents: write"
    );
  }
  if (hasDangerousInputInterpolationInRun(workflowText)) {
    addError(
      ".github/workflows/activate-production-timers.yml: workflow_dispatch inputs must not be interpolated directly inside run scripts"
    );
  }
  for (const forbidden of ["ssh ", "bash -c", "eval ", "gh release upload"]) {
    if (workflowText.includes(forbidden)) {
      addError(
        `.github/workflows/activate-production-timers.yml: forbidden ${forbidden}`
      );
    }
  }
  for (const fragment of [
    "Activate Production Timers",
    "ref: master",
    "require_single_line()",
    "normalize_csv_allowlist()",
    "release_sha must be a lowercase 40-character git SHA.",
    "git merge-base --is-ancestor",
    "erhua-morning-brief,xiaoman-weekly-recruitment,xiaoman-weekly-plan-confirmation,xiaoman-weekly-preview,xiaoman-daily-case-report-auto-publish",
    "pnpm deploy:runner:check",
    "DEPLOY_RELEASE_SCOPE: production-activation",
    "DEPLOY_RESTART_TARGETS: qintopia-system-services",
    'DEPLOY_DRY_RUN: "false"',
    'DEPLOY_ROLLBACK_ON_SMOKE_FAILURE: "false"',
    "DEPLOY_ACTIVATION_TARGETS",
    "DEPLOY_REQUEST_SIGNING_KEY",
    "DEPLOY_REQUEST_SIGNING_KEY_ID: production",
    "create-deploy-request.mjs",
    "upload-deploy-request.sh",
    "wait-deploy-result.sh",
    "WAIT_FOR_SERVER_DEPLOY_RESULT",
  ]) {
    if (!workflowText.includes(fragment)) {
      addError(`.github/workflows/activate-production-timers.yml: missing ${fragment}`);
    }
  }
}

if (exists(".github/workflows/observe-production-runtime.yml")) {
  const workflow = YAML.parse(
    readText(".github/workflows/observe-production-runtime.yml")
  );
  const workflowText = readText(".github/workflows/observe-production-runtime.yml");
  const job = workflow?.jobs?.["request-observation"];
  const observationTargetsInput =
    workflow?.on?.workflow_dispatch?.inputs?.observation_targets;
  const releaseShaInput = workflow?.on?.workflow_dispatch?.inputs?.release_sha;

  if (!workflow?.on?.workflow_dispatch) {
    addError(
      ".github/workflows/observe-production-runtime.yml: must use workflow_dispatch"
    );
  }
  if (workflow?.concurrency?.group !== "production-deploy") {
    addError(
      ".github/workflows/observe-production-runtime.yml: must share production-deploy concurrency"
    );
  }
  if (job?.environment !== "production") {
    addError(
      ".github/workflows/observe-production-runtime.yml: request-observation must use production environment"
    );
  }
  if (job?.permissions?.contents !== "read") {
    addError(
      ".github/workflows/observe-production-runtime.yml: request-observation must keep contents permission read-only"
    );
  }
  if (releaseShaInput?.required !== true || releaseShaInput?.type !== "string") {
    addError(
      ".github/workflows/observe-production-runtime.yml: release_sha must be a required string input"
    );
  }
  if (
    observationTargetsInput?.default !==
    "qiwe-image-send,xiaoman-daily-case-report-auto-publish"
  ) {
    addError(
      ".github/workflows/observe-production-runtime.yml: default observation targets must stay limited to QiWe image-send and Xiaoman daily report"
    );
  }
  const uploadJobNames = Object.entries(workflow?.jobs || {})
    .filter(([, candidateJob]) => candidateJob?.permissions?.contents === "write")
    .map(([jobName]) => jobName);
  if (uploadJobNames.length !== 0) {
    addError(
      ".github/workflows/observe-production-runtime.yml: observation must not require contents: write"
    );
  }
  if (hasDangerousInputInterpolationInRun(workflowText)) {
    addError(
      ".github/workflows/observe-production-runtime.yml: workflow_dispatch inputs must not be interpolated directly inside run scripts"
    );
  }
  for (const forbidden of ["ssh ", "bash -c", "eval ", "gh release upload"]) {
    if (workflowText.includes(forbidden)) {
      addError(
        `.github/workflows/observe-production-runtime.yml: forbidden ${forbidden}`
      );
    }
  }
  for (const fragment of [
    "Observe Production Runtime",
    "ref: master",
    "require_single_line()",
    "normalize_csv_allowlist()",
    "release_sha must be a lowercase 40-character git SHA.",
    "git merge-base --is-ancestor",
    "qiwe-image-send,xiaoman-daily-case-report-auto-publish",
    "erhua-morning-brief-worker-run",
    "pnpm deploy:runner:check",
    "DEPLOY_RELEASE_SCOPE: production-observation",
    "DEPLOY_RESTART_TARGETS: qintopia-system-services",
    'DEPLOY_DRY_RUN: "false"',
    'DEPLOY_ROLLBACK_ON_SMOKE_FAILURE: "false"',
    "DEPLOY_OBSERVATION_TARGETS",
    "DEPLOY_REQUEST_SIGNING_KEY",
    "DEPLOY_REQUEST_SIGNING_KEY_ID: production",
    "create-deploy-request.mjs",
    "upload-deploy-request.sh",
    "wait-deploy-result.sh",
    "WAIT_FOR_SERVER_DEPLOY_RESULT",
  ]) {
    if (!workflowText.includes(fragment)) {
      addError(`.github/workflows/observe-production-runtime.yml: missing ${fragment}`);
    }
  }
}

if (exists(".github/workflows/apply-production-hermes-crons.yml")) {
  const workflow = YAML.parse(
    readText(".github/workflows/apply-production-hermes-crons.yml")
  );
  const workflowText = readText(".github/workflows/apply-production-hermes-crons.yml");
  const job = workflow?.jobs?.["request-hermes-cron-apply"];
  const targetsInput =
    workflow?.on?.workflow_dispatch?.inputs?.hermes_cron_apply_targets;
  const modeInput = workflow?.on?.workflow_dispatch?.inputs?.apply_mode;
  const releaseShaInput = workflow?.on?.workflow_dispatch?.inputs?.release_sha;

  if (!workflow?.on?.workflow_dispatch) {
    addError(
      ".github/workflows/apply-production-hermes-crons.yml: must use workflow_dispatch"
    );
  }
  if (workflow?.concurrency?.group !== "production-deploy") {
    addError(
      ".github/workflows/apply-production-hermes-crons.yml: must share production-deploy concurrency"
    );
  }
  if (job?.environment !== "production") {
    addError(
      ".github/workflows/apply-production-hermes-crons.yml: request-hermes-cron-apply must use production environment"
    );
  }
  if (job?.permissions?.contents !== "read") {
    addError(
      ".github/workflows/apply-production-hermes-crons.yml: request-hermes-cron-apply must keep contents permission read-only"
    );
  }
  if (releaseShaInput?.required !== true || releaseShaInput?.type !== "string") {
    addError(
      ".github/workflows/apply-production-hermes-crons.yml: release_sha must be a required string input"
    );
  }
  if (
    modeInput?.required !== true ||
    modeInput?.type !== "choice" ||
    modeInput?.default !== "install"
  ) {
    addError(
      ".github/workflows/apply-production-hermes-crons.yml: apply_mode must be a required install-default choice"
    );
  }
  const modeOptions = modeInput?.options || [];
  for (const expectedMode of ["install", "enable"]) {
    if (!modeOptions.includes(expectedMode)) {
      addError(
        `.github/workflows/apply-production-hermes-crons.yml: missing mode option ${expectedMode}`
      );
    }
  }
  if (
    targetsInput?.default !==
    "erhua-morning-brief,xiaoman-daily-case-report,xiaoman-weekly-recruitment,xiaoman-weekly-plan-confirmation,xiaoman-weekly-preview"
  ) {
    addError(
      ".github/workflows/apply-production-hermes-crons.yml: default targets must stay limited to the five reviewed Hermes cron jobs"
    );
  }
  const uploadJobNames = Object.entries(workflow?.jobs || {})
    .filter(([, candidateJob]) => candidateJob?.permissions?.contents === "write")
    .map(([jobName]) => jobName);
  if (uploadJobNames.length !== 0) {
    addError(
      ".github/workflows/apply-production-hermes-crons.yml: Hermes cron apply must not require contents: write"
    );
  }
  if (hasDangerousInputInterpolationInRun(workflowText)) {
    addError(
      ".github/workflows/apply-production-hermes-crons.yml: workflow_dispatch inputs must not be interpolated directly inside run scripts"
    );
  }
  for (const forbidden of ["ssh ", "bash -c", "eval ", "gh release upload"]) {
    if (workflowText.includes(forbidden)) {
      addError(
        `.github/workflows/apply-production-hermes-crons.yml: forbidden ${forbidden}`
      );
    }
  }
  for (const fragment of [
    "Apply Production Hermes Crons",
    "ref: master",
    "require_single_line()",
    "normalize_csv_allowlist()",
    "release_sha must be a lowercase 40-character git SHA.",
    "git merge-base --is-ancestor",
    "apply_mode must be install or enable.",
    "erhua-morning-brief,xiaoman-daily-case-report,xiaoman-weekly-recruitment,xiaoman-weekly-plan-confirmation,xiaoman-weekly-preview",
    "pnpm deploy:runner:check",
    "DEPLOY_RELEASE_SCOPE: production-hermes-cron-apply",
    "DEPLOY_RESTART_TARGETS: qintopia-system-services",
    'DEPLOY_DRY_RUN: "false"',
    'DEPLOY_ROLLBACK_ON_SMOKE_FAILURE: "false"',
    "DEPLOY_HERMES_CRON_APPLY_TARGETS",
    "DEPLOY_HERMES_CRON_APPLY_MODE",
    "DEPLOY_REQUEST_SIGNING_KEY",
    "DEPLOY_REQUEST_SIGNING_KEY_ID: production",
    "create-deploy-request.mjs",
    "upload-deploy-request.sh",
    "wait-deploy-result.sh",
    "WAIT_FOR_SERVER_DEPLOY_RESULT",
  ]) {
    if (!workflowText.includes(fragment)) {
      addError(
        `.github/workflows/apply-production-hermes-crons.yml: missing ${fragment}`
      );
    }
  }
}

if (exists(".github/workflows/retire-production-legacy-crons.yml")) {
  const workflow = YAML.parse(
    readText(".github/workflows/retire-production-legacy-crons.yml")
  );
  const workflowText = readText(".github/workflows/retire-production-legacy-crons.yml");
  const job = workflow?.jobs?.["request-retirement"];
  const targetsInput =
    workflow?.on?.workflow_dispatch?.inputs?.legacy_cron_retirement_targets;
  const releaseShaInput = workflow?.on?.workflow_dispatch?.inputs?.release_sha;

  if (!workflow?.on?.workflow_dispatch) {
    addError(
      ".github/workflows/retire-production-legacy-crons.yml: must use workflow_dispatch"
    );
  }
  if (workflow?.concurrency?.group !== "production-deploy") {
    addError(
      ".github/workflows/retire-production-legacy-crons.yml: must share production-deploy concurrency"
    );
  }
  if (job?.environment !== "production") {
    addError(
      ".github/workflows/retire-production-legacy-crons.yml: request-retirement must use production environment"
    );
  }
  if (job?.permissions?.contents !== "read") {
    addError(
      ".github/workflows/retire-production-legacy-crons.yml: request-retirement must keep contents permission read-only"
    );
  }
  if (releaseShaInput?.required !== true || releaseShaInput?.type !== "string") {
    addError(
      ".github/workflows/retire-production-legacy-crons.yml: release_sha must be a required string input"
    );
  }
  if (targetsInput?.default !== "erhua-legacy-cron,xiaoman-legacy-cron") {
    addError(
      ".github/workflows/retire-production-legacy-crons.yml: default targets must stay limited to Erhua and Xiaoman legacy crons"
    );
  }
  const uploadJobNames = Object.entries(workflow?.jobs || {})
    .filter(([, candidateJob]) => candidateJob?.permissions?.contents === "write")
    .map(([jobName]) => jobName);
  if (uploadJobNames.length !== 0) {
    addError(
      ".github/workflows/retire-production-legacy-crons.yml: legacy cron retirement must not require contents: write"
    );
  }
  if (hasDangerousInputInterpolationInRun(workflowText)) {
    addError(
      ".github/workflows/retire-production-legacy-crons.yml: workflow_dispatch inputs must not be interpolated directly inside run scripts"
    );
  }
  for (const forbidden of ["ssh ", "bash -c", "eval ", "gh release upload"]) {
    if (workflowText.includes(forbidden)) {
      addError(
        `.github/workflows/retire-production-legacy-crons.yml: forbidden ${forbidden}`
      );
    }
  }
  for (const fragment of [
    "Retire Production Legacy Crons",
    "ref: master",
    "require_single_line()",
    "normalize_csv_allowlist()",
    "release_sha must be a lowercase 40-character git SHA.",
    "git merge-base --is-ancestor",
    "erhua-legacy-cron,xiaoman-legacy-cron",
    "pnpm deploy:runner:check",
    "DEPLOY_RELEASE_SCOPE: production-legacy-cron-retirement",
    "DEPLOY_RESTART_TARGETS: qintopia-system-services",
    'DEPLOY_DRY_RUN: "false"',
    'DEPLOY_ROLLBACK_ON_SMOKE_FAILURE: "false"',
    "DEPLOY_LEGACY_CRON_RETIREMENT_TARGETS",
    "DEPLOY_REQUEST_SIGNING_KEY",
    "DEPLOY_REQUEST_SIGNING_KEY_ID: production",
    "create-deploy-request.mjs",
    "upload-deploy-request.sh",
    "wait-deploy-result.sh",
    "WAIT_FOR_SERVER_DEPLOY_RESULT",
  ]) {
    if (!workflowText.includes(fragment)) {
      addError(
        `.github/workflows/retire-production-legacy-crons.yml: missing ${fragment}`
      );
    }
  }
}

if (exists(".github/workflows/run-production-runtime-one-shot.yml")) {
  const workflow = YAML.parse(
    readText(".github/workflows/run-production-runtime-one-shot.yml")
  );
  const workflowText = readText(
    ".github/workflows/run-production-runtime-one-shot.yml"
  );
  const job = workflow?.jobs?.["request-runtime-one-shot"];
  const targetInput = workflow?.on?.workflow_dispatch?.inputs?.runtime_one_shot_target;
  const releaseShaInput = workflow?.on?.workflow_dispatch?.inputs?.release_sha;
  const approvalInput = workflow?.on?.workflow_dispatch?.inputs?.approval;

  if (!workflow?.on?.workflow_dispatch) {
    addError(
      ".github/workflows/run-production-runtime-one-shot.yml: must use workflow_dispatch"
    );
  }
  if (workflow?.concurrency?.group !== "production-deploy") {
    addError(
      ".github/workflows/run-production-runtime-one-shot.yml: must share production-deploy concurrency"
    );
  }
  if (job?.environment !== "production") {
    addError(
      ".github/workflows/run-production-runtime-one-shot.yml: request-runtime-one-shot must use production environment"
    );
  }
  if (job?.permissions?.contents !== "read") {
    addError(
      ".github/workflows/run-production-runtime-one-shot.yml: request-runtime-one-shot must keep contents permission read-only"
    );
  }
  if (releaseShaInput?.required !== true || releaseShaInput?.type !== "string") {
    addError(
      ".github/workflows/run-production-runtime-one-shot.yml: release_sha must be a required string input"
    );
  }
  if (approvalInput?.required !== true || approvalInput?.type !== "string") {
    addError(
      ".github/workflows/run-production-runtime-one-shot.yml: approval must be a required string input"
    );
  }
  if (targetInput?.default !== "xiaoman-daily-case-report-auto-publish-backfill") {
    addError(
      ".github/workflows/run-production-runtime-one-shot.yml: default target must stay limited to Xiaoman daily report backfill"
    );
  }
  const targetOptions = targetInput?.options || [];
  for (const expectedTarget of [
    "xiaoman-daily-case-report-auto-publish-backfill",
    "erhua-morning-brief",
  ]) {
    if (!targetOptions.includes(expectedTarget)) {
      addError(
        `.github/workflows/run-production-runtime-one-shot.yml: missing target option ${expectedTarget}`
      );
    }
  }
  const uploadJobNames = Object.entries(workflow?.jobs || {})
    .filter(([, candidateJob]) => candidateJob?.permissions?.contents === "write")
    .map(([jobName]) => jobName);
  if (uploadJobNames.length !== 0) {
    addError(
      ".github/workflows/run-production-runtime-one-shot.yml: runtime one-shot must not require contents: write"
    );
  }
  if (hasDangerousInputInterpolationInRun(workflowText)) {
    addError(
      ".github/workflows/run-production-runtime-one-shot.yml: workflow_dispatch inputs must not be interpolated directly inside run scripts"
    );
  }
  for (const forbidden of ["ssh ", "bash -c", "eval ", "gh release upload"]) {
    if (workflowText.includes(forbidden)) {
      addError(
        `.github/workflows/run-production-runtime-one-shot.yml: forbidden ${forbidden}`
      );
    }
  }
  for (const fragment of [
    "Run Production Runtime One-Shot",
    "ref: master",
    "require_single_line()",
    "require_allowed_value()",
    "release_sha must be a lowercase 40-character git SHA.",
    "git merge-base --is-ancestor",
    "xiaoman-daily-case-report-auto-publish-backfill,erhua-morning-brief",
    "approved-production-xiaoman-daily-case-report-auto-publish-backfill",
    "approved-production-erhua-morning-brief-one-shot",
    "pnpm deploy:runner:check",
    "DEPLOY_RELEASE_SCOPE: production-runtime-one-shot",
    "DEPLOY_RESTART_TARGETS: qintopia-system-services",
    'DEPLOY_DRY_RUN: "false"',
    'DEPLOY_ROLLBACK_ON_SMOKE_FAILURE: "false"',
    "DEPLOY_RUNTIME_ONE_SHOT_TARGETS",
    "DEPLOY_RUNTIME_ONE_SHOT_BACKFILL_DATE",
    "DEPLOY_RUNTIME_ONE_SHOT_APPROVAL",
    "DEPLOY_REQUEST_SIGNING_KEY",
    "DEPLOY_REQUEST_SIGNING_KEY_ID: production",
    "create-deploy-request.mjs",
    "upload-deploy-request.sh",
    "wait-deploy-result.sh",
    "WAIT_FOR_SERVER_DEPLOY_RESULT",
  ]) {
    if (!workflowText.includes(fragment)) {
      addError(
        `.github/workflows/run-production-runtime-one-shot.yml: missing ${fragment}`
      );
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
  'RUNNER_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"',
  '"${RUNNER_DIR}/promote-release.sh"',
  '"${RUNNER_DIR}/install-release-systemd-units.sh"',
  '"${RUNNER_DIR}/smoke-release.sh"',
  '"${RUNNER_DIR}/rollback-release.sh"',
  'deploy_stage="install-release-systemd-units"',
  'deploy_stage="smoke-release"',
  '"failure_stage": deploy_failure_stage',
  '"exit_status": int(deploy_failure_status or "0")',
  "run_promotion\n  status=$?",
  'deploy_failure_stage="$deploy_stage"',
  'deploy_failure_status="$status"',
  'if [[ "$promoted_current" == "true"',
  "rollback failed",
  "rollback succeeded",
  "hermes-profile-erhua requires exactly hermes-erhua",
  "production-activation must be the only release scope",
  "production-activation requires exactly qintopia-system-services",
  "production-activation requires rollback_on_smoke_failure=false",
  "activation metadata is only allowed for production-activation",
  "production-observation must be the only release scope",
  "production-observation requires exactly qintopia-system-services",
  "production-observation requires rollback_on_smoke_failure=false",
  "observation metadata is only allowed for production-observation",
  "production-hermes-cron-apply must be the only release scope",
  "production-hermes-cron-apply requires exactly qintopia-system-services",
  "production-hermes-cron-apply requires rollback_on_smoke_failure=false",
  "hermes_cron_apply metadata is only allowed for production-hermes-cron-apply",
  "production-legacy-cron-retirement must be the only release scope",
  "production-legacy-cron-retirement requires exactly qintopia-system-services",
  "production-legacy-cron-retirement requires rollback_on_smoke_failure=false",
  "legacy_cron_retirement metadata is only allowed for production-legacy-cron-retirement",
  "production-runtime-one-shot must be the only release scope",
  "production-runtime-one-shot requires exactly qintopia-system-services",
  "production-runtime-one-shot requires rollback_on_smoke_failure=false",
  "runtime_one_shot metadata is only allowed for production-runtime-one-shot",
  "validate_current_activation_release",
  "validate_current_observation_release",
  "validate_current_hermes_cron_apply_release",
  "validate_current_retirement_release",
  "validate_current_runtime_one_shot_release",
  "observe_erhua_legacy_cron",
  "observe_xiaoman_legacy_cron",
  "observe_enabled_erhua_morning_brief_timer_for_one_shot",
  "observe_enabled_xiaoman_daily_case_report_timer_for_one_shot",
  "run_production_activation",
  "run_production_observation",
  "run_production_hermes_cron_apply",
  "run_production_legacy_cron_retirement",
  "run_production_runtime_one_shot",
  "production-timer-activation",
  "production-observation",
  "production-hermes-cron-apply",
  "production-legacy-cron-retirement",
  "production-runtime-one-shot",
  "runtime_one_shot.backfill_date",
  "activate-erhua-morning-brief-production.sh",
  "erhua-morning-brief-one-shot-production.sh",
  "retire-erhua-legacy-cron-production.sh",
  "retire-xiaoman-legacy-cron-production.sh",
  "activate-xiaoman-weekly-recruitment-production.sh",
  "activate-xiaoman-weekly-plan-confirmation-production.sh",
  "activate-xiaoman-weekly-preview-production.sh",
  "xiaoman-daily-case-report-auto-publish-backfill.sh",
  "activate-xiaoman-daily-case-report-auto-publish-production.sh",
  "qiwe-image-send-production-observation-smoke.sh",
  "xiaoman-daily-case-report-auto-publish-production-observation-smoke.sh",
  "production-worker-run-evidence-smoke.sh",
  "apply-erhua-morning-brief-hermes-cron.sh",
  "apply-xiaoman-daily-case-report-hermes-cron.sh",
  "apply-xiaoman-weekly-recruitment-hermes-cron.sh",
  "apply-xiaoman-weekly-plan-confirmation-hermes-cron.sh",
  "apply-xiaoman-weekly-preview-hermes-cron.sh",
  "approved-production-erhua-morning-brief-hermes-cron",
  "approved-production-xiaoman-daily-case-report-hermes-cron",
  "approved-production-xiaoman-weekly-recruitment-hermes-cron",
  "approved-production-xiaoman-weekly-plan-confirmation-hermes-cron",
  "approved-production-xiaoman-weekly-preview-hermes-cron",
  "hermes_cron_apply_failure_detail",
  "qintopia_hermes_cron_apply_safe_failure=",
  "hermes_cron_apply.mode",
  "run_worker_run_evidence_observation",
  "QINTOPIA_PRODUCTION_WORKER_RUN_EVIDENCE_ENABLE",
  "validate_current_profile_release",
  "activate-erhua-profile.sh",
  "rollback-erhua-profile.sh",
  "assert_current_profile_release",
  ".smoke.json",
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
  "validate_release_tree",
  "release tree owner mismatch",
  "release tree path is group/world writable",
  "release tree directory is not group/world accessible",
  "release tree contains unsupported file type",
  "release tree mode mismatch",
  'install -d -m 0755 "$release_root"',
  'validate_release_tree "$staging_dir"',
  'validate_release_tree "$release_dir"',
  'current_target" != "$release_target',
  "staging_dir/manifest.json",
  '"runtime_sha"',
  '"runtime_artifact_profile"',
  'if [[ "$runtime_artifact_profile" != "huabaosi-production" ]]',
  'companion_runtime_artifact_profile="qiwe-production"',
  'companion_relative_dir="sidecar-profiles/${companion_runtime_artifact_profile}"',
  'QINTOPIA_SIDECAR_ARTIFACT_PROFILE="huabaosi-production"',
  'QINTOPIA_SIDECAR_ARTIFACT_PROFILE="$companion_runtime_artifact_profile"',
  '--output-dir "${staging_dir}/${companion_relative_dir}"',
  '"companion_runtime_artifact_profiles": ["qiwe-production"]',
  'test -x "${staging_dir}/${companion_relative_dir}/qintopia-message-sidecar"',
  "companion_install_active=true",
  '"${release_dir}/sidecar-profiles/${companion_runtime_artifact_profile}"',
  '"deploy_bundle_sha"',
  '"commit_sha"',
  '"release_scope"',
  '"restart_targets"',
  "existing release sidecar artifact manifest profile is unavailable",
  'repair_existing_release_metadata "$release_dir" "$staging_dir"',
  "existing release content differs from freshly verified artifacts",
  'PROMOTER_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"',
  'FETCH_COS_ARTIFACT="${REPOSITORY_ROOT}/deploy/sidecar/scripts/fetch-cos-artifact.sh"',
  '"$FETCH_COS_ARTIFACT"',
  'python_bytecode_relative_dir="runtime/hermes/__pycache__"',
  "validate_existing_python_bytecode_cache",
  "existing Hermes bytecode cache contents are invalid",
  "quarantine_existing_python_bytecode_cache",
  'chown -hR root:root "$existing_dir"',
  "sha256sum -c SHA256SUMS",
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

const rollbackReadmeText = exists("deploy/rollback/README.md")
  ? readText("deploy/rollback/README.md")
  : "";
for (const fragment of [
  "No owner-triggered dual-runtime rollback target is currently verified",
  "fails before artifact",
  "Huabaosi primary sidecar, QiWe companion sidecar, and deploy",
  "`runtime_artifact_profile=huabaosi-production`",
  "global profile choice.",
]) {
  if (!rollbackReadmeText.includes(fragment)) {
    addError(`deploy/rollback/README.md: missing ${fragment}`);
  }
}

const fetchCosArtifactText = exists("deploy/sidecar/scripts/fetch-cos-artifact.sh")
  ? readText("deploy/sidecar/scripts/fetch-cos-artifact.sh")
  : "";
for (const fragment of [
  'tar --no-same-owner -xzf "${output_dir}/qintopia-message-sidecar.tar.gz"',
  'tar --no-same-owner -xzf "${output_dir}/qintopia-agent-os-deploy-bundle.tar.gz"',
  "chmod 0444 artifact-manifest.json SHA256SUMS",
  "chmod 0444 qintopia-message-sidecar.tar.gz",
  "chmod 0444 qintopia-agent-os-deploy-bundle.tar.gz",
  "chmod 0755 qintopia-message-sidecar",
  "QINTOPIA_HUABAOSI_PRODUCTION_FEATURE_CONTRACT",
  "QINTOPIA_LEGACY_RUNNER_BOOTSTRAP_RUNTIME_SHA",
  "legacy-runner-bootstrap",
  "legacy runner bootstrap requires an exact deployed runtime SHA binding",
]) {
  if (!fetchCosArtifactText.includes(fragment)) {
    addError(`deploy/sidecar/scripts/fetch-cos-artifact.sh: missing ${fragment}`);
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
  "DEPLOY_OBSERVATION_TARGETS",
  "request.observation",
  "DEPLOY_HERMES_CRON_APPLY_TARGETS",
  "DEPLOY_HERMES_CRON_APPLY_MODE",
  "request.hermes_cron_apply",
  "DEPLOY_RUNTIME_ONE_SHOT_TARGETS",
  "DEPLOY_RUNTIME_ONE_SHOT_BACKFILL_DATE",
  "DEPLOY_RUNTIME_ONE_SHOT_APPROVAL",
  "request.runtime_one_shot",
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
  "smoke_erhua_profile",
  "--profile erhua doctor",
  "not a recognized provider",
  "unknown provider",
  "--evidence-output",
  '"doctor_succeeded"',
  "verify_runtime_provider.py",
  "verify-activated",
  "export PYTHONDONTWRITEBYTECODE=1",
]) {
  if (!smokeText.includes(fragment)) {
    addError(`deploy/runner/smoke-release.sh: missing ${fragment}`);
  }
}

const activateErhuaText = exists("deploy/runner/activate-erhua-profile.sh")
  ? readText("deploy/runner/activate-erhua-profile.sh")
  : "";
for (const fragment of [
  "--release-sha",
  "profile-dry-runs",
  "matching Erhua dry-run evidence is required before activation",
  "Erhua runtime state changed after the reviewed dry run",
  "requires PyYAML for the root Python runtime",
  "--expected-config-sha",
  "--dry-run-request-id",
  "older than 24 hours",
  "verify_runtime_provider.py",
  "validate_hermes_python.py",
  'runtime_verify_dir="$(mktemp -d)"',
  "/usr/sbin/runuser -u ubuntu -- /usr/bin/env -i",
  "HOME=/home/ubuntu",
  "PATH=/usr/local/bin:/usr/bin:/bin",
]) {
  if (activateErhuaText && !activateErhuaText.includes(fragment)) {
    addError(`deploy/runner/activate-erhua-profile.sh: missing ${fragment}`);
  }
}
for (const forbidden of [
  'chmod 0711 "$work_dir"',
  "/usr/sbin/runuser -u ubuntu -- /usr/bin/env PYTHONDONTWRITEBYTECODE=1",
]) {
  if (activateErhuaText.includes(forbidden)) {
    addError(`deploy/runner/activate-erhua-profile.sh: forbidden ${forbidden}`);
  }
}
const rollbackReleaseText = exists("deploy/runner/rollback-release.sh")
  ? readText("deploy/runner/rollback-release.sh")
  : "";
for (const fragment of [
  "os.replace",
  "os.fsync",
  "rollback current target verification failed",
]) {
  if (rollbackReleaseText && !rollbackReleaseText.includes(fragment)) {
    addError(`deploy/runner/rollback-release.sh: missing ${fragment}`);
  }
}

const runnerServiceText = exists(
  "deploy/runner/qintopia-agent-os-deploy-runner.service"
)
  ? readText("deploy/runner/qintopia-agent-os-deploy-runner.service")
  : "";
const runnerReadWritePaths = runnerServiceText
  .split("\n")
  .filter((line) => line.startsWith("ReadWritePaths="))
  .flatMap((line) =>
    line.slice("ReadWritePaths=".length).trim().split(/\s+/).filter(Boolean)
  )
  .map((pathToken) => pathToken.replace(/\/+$/, "") || "/");
if (
  runnerServiceText &&
  !runnerReadWritePaths.includes("/home/ubuntu/.hermes/profiles/erhua")
) {
  addError("deploy runner service must explicitly allow governed Erhua profile writes");
}
if (
  runnerServiceText &&
  !runnerReadWritePaths.includes("/home/ubuntu/.hermes/profiles/xiaoman/cron")
) {
  addError(
    "deploy runner service must explicitly allow fixed Xiaoman cron retirement writes"
  );
}
if (
  runnerServiceText &&
  !runnerReadWritePaths.includes("/home/ubuntu/.hermes/scripts")
) {
  addError(
    "deploy runner service must explicitly allow fixed Hermes cron wrapper installs"
  );
}
if (
  runnerServiceText &&
  runnerReadWritePaths.includes("/home/ubuntu/.hermes/profiles/xiaoman")
) {
  addError("deploy runner service must not allow whole Xiaoman profile writes");
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
  "deploy result {key} mismatch",
  "deploy-request-validation",
  '"runtime_artifact_profile"',
  '"deploy_bundle_sha"',
  "deploy result release_scope mismatch",
  "deploy result restart_targets mismatch",
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
const erhuaRestartRule = (restartRules.rules ?? []).find(
  (rule) => rule.target === "hermes-erhua"
);
const hermesPythonRestartTargets = (restartRules.rules ?? [])
  .filter((rule) =>
    Array.isArray(rule?.paths)
      ? rule.paths.includes("runtime/hermes/validate_hermes_python.py")
      : false
  )
  .map((rule) => rule.target);
if (
  !Array.isArray(erhuaRestartRule?.paths) ||
  hermesPythonRestartTargets.length !== 1 ||
  hermesPythonRestartTargets[0] !== "hermes-erhua"
) {
  addError(
    "deploy/restart-target-rules.yaml: validate_hermes_python.py must belong only to hermes-erhua paths"
  );
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
  execFileSync("bash", ["-n", "deploy/runner/activate-erhua-profile.sh"], {
    cwd: repoRoot,
  });
  execFileSync("bash", ["-n", "deploy/runner/rollback-erhua-profile.sh"], {
    cwd: repoRoot,
  });
  for (const scriptPath of [
    "deploy/sidecar/scripts/erhua-legacy-cron-observation-smoke.sh",
    "deploy/sidecar/scripts/retire-erhua-legacy-cron-production.sh",
    "deploy/sidecar/scripts/retire-xiaoman-legacy-cron-production.sh",
    "deploy/sidecar/scripts/erhua-morning-brief-timer-observation-smoke.sh",
    "deploy/sidecar/scripts/erhua-morning-brief-worker.sh",
    "deploy/sidecar/scripts/erhua-morning-brief-one-shot-production.sh",
    "deploy/sidecar/scripts/activate-erhua-morning-brief-production.sh",
    "deploy/sidecar/scripts/rollback-erhua-morning-brief-production.sh",
  ]) {
    execFileSync("bash", ["-n", scriptPath], { cwd: repoRoot });
  }
  execFileSync("bash", ["-n", "deploy/runner/wait-deploy-result.sh"], {
    cwd: repoRoot,
  });
  execFileSync("node", ["tools/deploy/test-resolve-restart-targets.mjs"], {
    cwd: repoRoot,
  });
  execFileSync("node", ["tools/deploy/test-collect-release-deploy-results.mjs"], {
    cwd: repoRoot,
  });
  execFileSync("node", ["tools/deploy/test-wait-deploy-result.mjs"], {
    cwd: repoRoot,
  });
  execFileSync("node", ["tools/deploy/test-resolve-release-restart-targets.mjs"], {
    cwd: repoRoot,
  });
  execFileSync("node", ["tools/deploy/test-resolve-release-deploy-base.mjs"], {
    cwd: repoRoot,
  });
  execFileSync("node", ["tools/deploy/test-validate-legacy-runner-bootstrap.mjs"], {
    cwd: repoRoot,
  });
  execFileSync("node", ["tools/deploy/test-deploy-runner-poller.mjs"], {
    cwd: repoRoot,
  });
  execFileSync("node", ["tools/deploy/test-deploy-runner-promotion.mjs"], {
    cwd: repoRoot,
  });
  execFileSync("node", ["tools/deploy/test-production-timer-activation-runner.mjs"], {
    cwd: repoRoot,
  });
  execFileSync("node", ["tools/deploy/test-production-observation-runner.mjs"], {
    cwd: repoRoot,
  });
  execFileSync("node", ["tools/deploy/test-production-worker-run-evidence-smoke.mjs"], {
    cwd: repoRoot,
  });
  execFileSync("node", ["tools/deploy/test-production-hermes-cron-apply-runner.mjs"], {
    cwd: repoRoot,
  });
  execFileSync(
    "node",
    ["tools/deploy/test-production-legacy-cron-retirement-runner.mjs"],
    {
      cwd: repoRoot,
    }
  );
  execFileSync("node", ["tools/deploy/test-production-runtime-one-shot-runner.mjs"], {
    cwd: repoRoot,
  });
  execFileSync("node", ["tools/deploy/test-promote-release-tree.mjs"], {
    cwd: repoRoot,
  });
  execFileSync("node", ["tools/deploy/test-promote-existing-release-metadata.mjs"], {
    cwd: repoRoot,
  });
  execFileSync("node", ["tools/deploy/test-fetch-cos-artifact-permissions.mjs"], {
    cwd: repoRoot,
  });
  execFileSync("node", ["tools/deploy/test-release-systemd-install.mjs"], {
    cwd: repoRoot,
  });
  execFileSync("node", ["tools/deploy/test-erhua-legacy-cron-observation.mjs"], {
    cwd: repoRoot,
  });
  execFileSync("node", ["tools/deploy/test-erhua-legacy-cron-retirement.mjs"], {
    cwd: repoRoot,
  });
  execFileSync("node", ["tools/deploy/test-xiaoman-legacy-cron-retirement.mjs"], {
    cwd: repoRoot,
  });
  execFileSync(
    "node",
    ["tools/deploy/test-erhua-morning-brief-production-activation.mjs"],
    { cwd: repoRoot }
  );
  execFileSync(
    "node",
    ["tools/deploy/test-staging-runtime-prerequisite-observation.mjs"],
    { cwd: repoRoot }
  );
  execFileSync("node", ["tools/deploy/test-staging-runtime-values-observation.mjs"], {
    cwd: repoRoot,
  });
  execFileSync(
    "node",
    ["tools/deploy/test-huabaosi-image-production-observation.mjs"],
    { cwd: repoRoot }
  );
  execFileSync("node", ["tools/deploy/test-huabaosi-image-production-canary.mjs"], {
    cwd: repoRoot,
  });
  execFileSync(
    "node",
    ["tools/deploy/test-huabaosi-image-production-canary-evidence.mjs"],
    { cwd: repoRoot }
  );
  execFileSync("node", ["tools/deploy/test-huabaosi-image-staging-readiness.mjs"], {
    cwd: repoRoot,
  });
  execFileSync("node", ["tools/deploy/test-huabaosi-image-production-activation.mjs"], {
    cwd: repoRoot,
  });
  execFileSync(
    "node",
    ["tools/deploy/test-huabaosi-feishu-mirror-production-observation.mjs"],
    { cwd: repoRoot }
  );
  execFileSync(
    "node",
    ["tools/deploy/test-huabaosi-feishu-mirror-production-activation.mjs"],
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
  execFileSync("node", ["tools/deploy/test-qiwe-image-production-observation.mjs"], {
    cwd: repoRoot,
  });
  execFileSync("node", ["tools/deploy/test-xiaoman-image-send-staging-evidence.mjs"], {
    cwd: repoRoot,
  });
  execFileSync(
    "node",
    ["tools/deploy/test-xiaoman-production-observation-contracts.mjs"],
    { cwd: repoRoot }
  );
  execFileSync("node", ["tools/deploy/test-xiaoman-profile-bundle-observation.mjs"], {
    cwd: repoRoot,
  });
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
    "agents/erhua/config.template.yaml",
    "runtime/hermes/render_profile_overlay.py",
    "runtime/hermes/migrate_erhua_livecool_env.py",
    "runtime/hermes/profile_transaction.py",
    "runtime/hermes/verify_runtime_provider.py",
    "deploy/runner/wait-deploy-result.sh",
    "deploy/restart-target-rules.yaml",
    "tools/deploy/collect-release-deploy-results.mjs",
    "tools/deploy/resolve-release-deploy-base.mjs",
    "tools/deploy/resolve-release-restart-targets.mjs",
    "tools/deploy/resolve-restart-targets.mjs",
    "deploy/sidecar/scripts/fetch-cos-artifact.sh",
    "deploy/sidecar/scripts/staging-runtime-prerequisite-observation-smoke.sh",
    "deploy/sidecar/scripts/huabaosi-image-generation-staging-readiness-smoke.sh",
    "deploy/sidecar/scripts/huabaosi-image-generation-staging-smoke.sh",
    "deploy/sidecar/scripts/huabaosi-image-generation-production-observation-smoke.sh",
    "deploy/sidecar/scripts/huabaosi-feishu-artifact-mirror-production-observation-smoke.sh",
    "deploy/sidecar/scripts/activate-huabaosi-feishu-artifact-mirror-production.sh",
    "deploy/sidecar/scripts/rollback-huabaosi-feishu-artifact-mirror-production.sh",
    "deploy/sidecar/scripts/apply-xiaoman-feishu-poster-production-config.py",
    "deploy/sidecar/scripts/apply-xiaoman-conversation-policies-production.py",
    "deploy/sidecar/scripts/xiaoman-shared-db-password-rollover-production.py",
    "deploy/sidecar/scripts/huabaosi-wecom-canary-observation-smoke.sh",
    "deploy/sidecar/scripts/huabaosi-wecom-gateway-observation-smoke.sh",
    "deploy/sidecar/scripts/install-coscli.sh",
    "deploy/sidecar/scripts/qiwe-image-send-staging-readiness-smoke.sh",
    "deploy/sidecar/scripts/qiwe-image-send-staging-smoke.sh",
    "deploy/sidecar/scripts/qiwe-image-send-production-observation-smoke.sh",
    "deploy/sidecar/scripts/activate-qiwe-image-callback-bridge-production.sh",
    "deploy/sidecar/scripts/rollback-qiwe-image-callback-bridge-production.sh",
    "deploy/sidecar/scripts/erhua-legacy-cron-observation-smoke.sh",
    "deploy/sidecar/scripts/retire-erhua-legacy-cron-production.sh",
    "deploy/sidecar/scripts/retire-xiaoman-legacy-cron-production.sh",
    "deploy/sidecar/scripts/erhua-morning-brief-timer-observation-smoke.sh",
    "deploy/sidecar/scripts/erhua-morning-brief-worker.sh",
    "deploy/sidecar/scripts/erhua-morning-brief-one-shot-production.sh",
    "deploy/sidecar/scripts/activate-erhua-morning-brief-production.sh",
    "deploy/sidecar/scripts/rollback-erhua-morning-brief-production.sh",
    "deploy/sidecar/scripts/activate-qiwe-image-send-production.sh",
    "deploy/sidecar/scripts/rollback-qiwe-image-send-production.sh",
    "deploy/sidecar/scripts/apply-xiaoman-activity-read-through-production-config.py",
    "deploy/sidecar/scripts/apply-xiaoman-daily-case-report-production-config.py",
    "deploy/sidecar/scripts/xiaoman-daily-case-report-auto-publish-worker.sh",
    "deploy/sidecar/scripts/xiaoman-daily-case-report-auto-publish-backfill.sh",
    "deploy/sidecar/scripts/xiaoman-daily-case-report-auto-publish-production-observation-smoke.sh",
    "deploy/sidecar/scripts/activate-xiaoman-daily-case-report-auto-publish-production.sh",
    "deploy/sidecar/scripts/rollback-xiaoman-daily-case-report-auto-publish-production.sh",
    "deploy/sidecar/scripts/production-worker-run-evidence-smoke.sh",
    "deploy/sidecar/scripts/apply-xiaoman-weekly-recruitment-production-config.sh",
    "deploy/sidecar/scripts/xiaoman-weekly-recruitment-worker.sh",
    "deploy/sidecar/scripts/xiaoman-weekly-recruitment-production-observation-smoke.sh",
    "deploy/sidecar/scripts/activate-xiaoman-weekly-recruitment-production.sh",
    "deploy/sidecar/scripts/rollback-xiaoman-weekly-recruitment-production.sh",
    "deploy/sidecar/scripts/apply-xiaoman-weekly-plan-confirmation-production-config.sh",
    "deploy/sidecar/scripts/xiaoman-weekly-plan-confirmation-worker.sh",
    "deploy/sidecar/scripts/xiaoman-weekly-plan-confirmation-production-observation-smoke.sh",
    "deploy/sidecar/scripts/activate-xiaoman-weekly-plan-confirmation-production.sh",
    "deploy/sidecar/scripts/rollback-xiaoman-weekly-plan-confirmation-production.sh",
    "deploy/sidecar/scripts/operations-downstream-timers-observation-smoke.sh",
    "deploy/sidecar/scripts/operations-group-send-ready-timer-observation-smoke.sh",
    "deploy/sidecar/scripts/erhua-morning-brief-worker.sh",
    "deploy/sidecar/scripts/erhua-morning-brief-timer-observation-smoke.sh",
    "deploy/sidecar/scripts/activate-erhua-morning-brief-production.sh",
    "deploy/sidecar/scripts/rollback-erhua-morning-brief-production.sh",
    "deploy/sidecar/scripts/xiaoman-activity-downstream-observation-smoke.sh",
    "deploy/sidecar/scripts/xiaoman-activity-image-generation-starter-observation-smoke.sh",
    "deploy/sidecar/scripts/xiaoman-activity-production-preflight-smoke.sh",
    "deploy/sidecar/scripts/xiaoman-activity-promotion-starter-timer-observation-smoke.sh",
    "deploy/sidecar/scripts/xiaoman-activity-send-request-starter-observation-smoke.sh",
    "deploy/sidecar/scripts/xiaoman-activity-signal-timer-observation-smoke.sh",
    "deploy/sidecar/scripts/xiaoman-profile-bundle-observation-smoke.sh",
    "agents/xiaoman/profile-bundle",
    "workflows/erhua-morning-brief",
    "workflows/xiaoman-daily-case-report",
    "workflows/xiaoman-weekly-loop",
    "skills/qintopia-weather/scripts/qintopia-erhua-weather-broadcast.py",
    "workflows/erhua-morning-brief",
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
    '--qiwe-artifact-dir "${release_dir}/sidecar-profiles/qiwe-production"',
    "qintopia-agentos-xiaoman-activity-signal-worker.timer",
    "qintopia-agentos-xiaoman-activity-promotion-starter-worker.timer",
    "qintopia-agentos-xiaoman-activity-image-generation-starter-worker.timer",
    "qintopia-agentos-xiaoman-activity-send-request-starter-worker.timer",
    "qintopia-agentos-erhua-morning-brief.service",
    "qintopia-agentos-erhua-morning-brief.timer",
    "qintopia-agentos-operations-intake.service",
    "qintopia-agentos-xiaoman-poster-notification-starter.service",
    "qintopia-agentos-xiaoman-poster-notification-starter.timer",
    "qintopia-agentos-xiaoman-feishu-poster-preflight.service",
    "qintopia-agentos-xiaoman-feishu-poster-delivery.service",
    "qintopia-agentos-xiaoman-feishu-poster-delivery.timer",
    "qintopia-agentos-xiaoman-feishu-internal-group-poster-preflight.service",
    "qintopia-agentos-xiaoman-feishu-internal-group-poster-delivery.service",
    "qintopia-agentos-xiaoman-feishu-internal-group-poster-delivery.timer",
    "qintopia-agentos-xiaoman-poster-review-callback.service",
    "qintopia-agentos-operations-group-send-ready.timer",
    "qintopia-agentos-xiaoman-daily-case-report-auto-publish.service",
    "qintopia-agentos-xiaoman-daily-case-report-auto-publish.timer",
    '"$systemctl_bin" daemon-reload',
    "runner_unit_files=(",
    "qintopia-agent-os-deploy-runner.service",
    "qintopia-agent-os-deploy-runner.timer",
    'source_path="${release_dir}/deploy/runner/${unit_file}"',
    'sidecar_env_file="/etc/qintopia/message-sidecar.env"',
    "normalize_production_sidecar_env_metadata",
    'chown root:ubuntu "$env_file"',
    'chmod 0640 "$env_file"',
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

if (exists("deploy/runner/qintopia-agent-os-deploy-runner.service")) {
  const runnerService = readText(
    "deploy/runner/qintopia-agent-os-deploy-runner.service"
  );
  for (const fragment of [
    "User=root",
    "Group=root",
    "StateDirectory=qintopia-agent-os-deploy",
    "StateDirectoryMode=0700",
    "WorkingDirectory=/var/lib/qintopia-agent-os-deploy",
    "Environment=QINTOPIA_DEPLOY_RUNNER_STATE_DIR=/var/lib/qintopia-agent-os-deploy",
    "/var/lib/qintopia-agent-os-deploy",
  ]) {
    if (!runnerService.includes(fragment)) {
      addError(`deploy runner service is missing ${fragment}`);
    }
  }
  if (
    runnerService.includes(
      "WorkingDirectory=/home/ubuntu/qintopia-agent-os-releases/current"
    )
  ) {
    addError("deploy runner service must not write COSCLI state under release/current");
  }
}

if (exists("deploy/sidecar/scripts/render-systemd-units.sh")) {
  const renderer = readText("deploy/sidecar/scripts/render-systemd-units.sh");
  for (const fragment of [
    'QIWE_BIN="${QIWE_ARTIFACT_DIR}/qintopia-message-sidecar"',
    "qintopia-agentos-qiwe-image-send-preflight.service",
    "qintopia-agentos-qiwe-image-send-worker.service",
    "qintopia-agentos-xiaoman-daily-case-report-auto-publish.service",
    "xiaoman-daily-case-report-auto-publish-worker.sh",
    "qintopia-agentos-xiaoman-daily-case-report-auto-publish.timer",
    "*-*-* 08:00:00",
    "run-xiaoman-feishu-poster-delivery --once --apply --conversation-scope direct",
    "run-xiaoman-feishu-poster-delivery --once --apply --conversation-scope group",
    "xiaoman-feishu-poster-preflight --conversation-scope direct",
    "xiaoman-feishu-poster-preflight --conversation-scope group",
    'grep -F " ${QIWE_BIN} "',
    'grep -F " ${BIN} "',
    "qintopia-agentos-erhua-morning-brief.service",
    "qintopia-agentos-erhua-morning-brief.timer",
    "erhua-morning-brief-worker.sh",
    "QINTOPIA_ERHUA_MORNING_BRIEF_PYTHON=/home/ubuntu/.hermes/hermes-agent/venv/bin/python",
  ]) {
    if (!renderer.includes(fragment)) {
      addError(`release systemd renderer is missing ${fragment}`);
    }
  }
  if (countExactOccurrences(renderer, '    "$QIWE_BIN"') !== 2) {
    addError(
      "release systemd renderer must bind exactly the QiWe preflight and worker services to the companion binary"
    );
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
