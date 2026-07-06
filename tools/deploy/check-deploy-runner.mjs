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
const addError = (message) => errors.push(message);

const requiredFiles = [
  ".github/workflows/deploy-production.yml",
  "deploy/runner/README.md",
  "deploy/runner/manifest.yaml",
  "deploy/runner/deploy-request.schema.json",
  "deploy/runner/deploy-result.schema.json",
  "deploy/runner/qintopia-agent-os-deploy-runner",
  "deploy/runner/poll-deploy-requests.sh",
  "deploy/runner/promote-release.sh",
  "deploy/runner/rollback-release.sh",
  "deploy/runner/smoke-release.sh",
  "deploy/runner/upload-deploy-request.sh",
  "deploy/runner/qintopia-agent-os-deploy-runner.service",
  "deploy/runner/qintopia-agent-os-deploy-runner.timer",
  "tools/deploy/create-deploy-request.mjs",
];

for (const file of requiredFiles) {
  if (!exists(file)) {
    addError(`${file}: required deploy runner file is missing`);
  }
}

const ajv = new Ajv2020({ allErrors: true });
ajv.addFormat("date-time", true);
if (exists("deploy/runner/deploy-request.schema.json")) {
  const requestSchema = JSON.parse(
    readText("deploy/runner/deploy-request.schema.json")
  );
  const validateRequest = ajv.compile(requestSchema);
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
        "qintopia-agent-os/deploy-requests/production/pending/deploy-20260706T000000Z-0123456789ab.json",
      result_key:
        "qintopia-agent-os/deploy-results/production/deploy-20260706T000000Z-0123456789ab.json",
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
  const job = workflow?.jobs?.["request-deploy"];
  if (job?.environment !== "production") {
    addError(
      ".github/workflows/deploy-production.yml: request-deploy must use production environment"
    );
  }
  const workflowText = readText(".github/workflows/deploy-production.yml");
  for (const fragment of [
    "create-deploy-request.mjs",
    "upload-deploy-request.sh",
    "git merge-base --is-ancestor",
    "pnpm deploy:runner:check",
  ]) {
    if (!workflowText.includes(fragment)) {
      addError(`.github/workflows/deploy-production.yml: missing ${fragment}`);
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

const uploadRequestText = exists("deploy/runner/upload-deploy-request.sh")
  ? readText("deploy/runner/upload-deploy-request.sh")
  : "";
if (
  uploadRequestText.includes(
    '${TENCENT_COS_SESSION_TOKEN:+--session_token "$TENCENT_COS_SESSION_TOKEN"}'
  )
) {
  addError(
    "deploy/runner/upload-deploy-request.sh: session token must use an auth_args array"
  );
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
  execFileSync("bash", ["-n", "deploy/runner/rollback-release.sh"], { cwd: repoRoot });
  execFileSync("bash", ["-n", "deploy/runner/smoke-release.sh"], { cwd: repoRoot });
  execFileSync("bash", ["-n", "deploy/runner/upload-deploy-request.sh"], {
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
    "deploy/runner/deploy-request.schema.json",
  ]) {
    if (!builder.includes(fragment)) {
      addError(`tools/deploy/build-deploy-bundle.mjs: must package ${fragment}`);
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
