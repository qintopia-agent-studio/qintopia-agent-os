#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import Ajv2020 from "ajv/dist/2020.js";

const repoRoot = process.cwd();

const argValue = (name, fallback = "") => {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] || "" : fallback;
};

const splitList = (value) =>
  String(value || "")
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);

const requireValue = (name, value) => {
  if (!value) {
    console.error(`${name} is required`);
    process.exit(2);
  }
  return value;
};

const isoNow = () => new Date().toISOString();
const expiresAt = (minutes) =>
  new Date(Date.now() + Number(minutes) * 60 * 1000).toISOString();

const schemaPath = path.join(repoRoot, "deploy/runner/deploy-request.schema.json");
const schema = JSON.parse(fs.readFileSync(schemaPath, "utf8"));
const ajv = new Ajv2020({ allErrors: true });
ajv.addFormat("date-time", true);
const validate = ajv.compile(schema);

const commitSha = requireValue(
  "--commit-sha",
  argValue(
    "--commit-sha",
    process.env.DEPLOY_COMMIT_SHA || process.env.GITHUB_SHA || ""
  )
);
const runtimeSha = requireValue(
  "--runtime-sha",
  argValue("--runtime-sha", process.env.DEPLOY_RUNTIME_SHA || commitSha)
);
const deployBundleSha = requireValue(
  "--deploy-bundle-sha",
  argValue("--deploy-bundle-sha", process.env.DEPLOY_BUNDLE_SHA || commitSha)
);
const releaseSha = requireValue(
  "--release-sha",
  argValue("--release-sha", process.env.DEPLOY_RELEASE_SHA || deployBundleSha)
);
const releaseScope = splitList(
  argValue(
    "--release-scope",
    process.env.DEPLOY_RELEASE_SCOPE || "deploy-bundle,hermes-plugins"
  )
);
const restartTargets = splitList(
  argValue("--restart-targets", process.env.DEPLOY_RESTART_TARGETS || "")
);
const bucket = requireValue(
  "TENCENT_COS_BUCKET",
  argValue("--cos-bucket", process.env.TENCENT_COS_BUCKET || "")
);
const region = requireValue(
  "TENCENT_COS_REGION",
  argValue("--cos-region", process.env.TENCENT_COS_REGION || "")
);
const prefix = (
  argValue("--cos-prefix", process.env.TENCENT_COS_PREFIX || "qintopia-agent-os") ||
  "qintopia-agent-os"
)
  .replace(/^\/+/, "")
  .replace(/\/+$/, "");
const ttlMinutes = argValue(
  "--ttl-minutes",
  process.env.DEPLOY_REQUEST_TTL_MINUTES || "60"
);
const dryRun = argValue("--dry-run", process.env.DEPLOY_DRY_RUN || "false") === "true";
const rollbackOnSmokeFailure =
  argValue(
    "--rollback-on-smoke-failure",
    process.env.DEPLOY_ROLLBACK_ON_SMOKE_FAILURE || "true"
  ) === "true";
const requestedBy = requireValue(
  "requested_by",
  argValue("--requested-by", process.env.GITHUB_ACTOR || os.userInfo().username)
);
const notes = argValue("--notes", process.env.DEPLOY_NOTES || "");
const createdAt = isoNow();
const timestamp = createdAt.replace(/[-:]/g, "").replace(/\.\d{3}Z$/, "Z");
const requestId = `deploy-${timestamp}-${commitSha.slice(0, 12)}`;
const requestKey = `${prefix}/deploy-requests/production/pending/${requestId}.json`;
const resultKey = `${prefix}/deploy-results/production/${requestId}.json`;
const outputPath = path.resolve(
  argValue(
    "--output",
    process.env.DEPLOY_REQUEST_OUTPUT || `dist/deploy-requests/${requestId}.json`
  )
);

const request = {
  schema_version: 1,
  request_id: requestId,
  environment: "production",
  repository: "qintopia-agent-studio/qintopia-agent-os",
  requested_by: requestedBy,
  created_at: createdAt,
  expires_at: expiresAt(ttlMinutes),
  commit_sha: commitSha,
  runtime_sha: runtimeSha,
  deploy_bundle_sha: deployBundleSha,
  release_sha: releaseSha,
  release_scope: releaseScope,
  restart_targets: restartTargets,
  rollback_on_smoke_failure: rollbackOnSmokeFailure,
  dry_run: dryRun,
  cos: {
    bucket,
    region,
    prefix,
    request_key: requestKey,
    result_key: resultKey,
  },
  github: {
    workflow: process.env.GITHUB_WORKFLOW || "",
    run_id: process.env.GITHUB_RUN_ID || "",
    run_attempt: process.env.GITHUB_RUN_ATTEMPT || "",
    ref: process.env.GITHUB_REF || "",
    sha: process.env.GITHUB_SHA || "",
  },
  notes,
};

if (!validate(request)) {
  console.error("Deploy request validation failed:");
  for (const error of validate.errors || []) {
    console.error(`- ${error.instancePath || "/"} ${error.message}`);
  }
  process.exit(1);
}

fs.mkdirSync(path.dirname(outputPath), { recursive: true });
fs.writeFileSync(outputPath, `${JSON.stringify(request, null, 2)}\n`);

console.log(`Deploy request: ${outputPath}`);
console.log(`Request key: ${requestKey}`);
console.log(`Result key: ${resultKey}`);
