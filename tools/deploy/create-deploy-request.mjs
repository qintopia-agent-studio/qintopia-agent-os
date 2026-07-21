#!/usr/bin/env node

import fs from "node:fs";
import crypto from "node:crypto";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import Ajv2020 from "ajv/dist/2020.js";

const repoRoot = process.cwd();
const fixedCosPrefix = "qintopia-agent-os";
const shaPattern = /^[0-9a-f]{40}$/;

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

const requireSha = (name, value) => {
  const normalized = requireValue(name, value);
  if (!shaPattern.test(normalized)) {
    console.error(`${name} must be a lowercase 40-character git SHA`);
    process.exit(2);
  }
  return normalized;
};

const canonicalJson = (value) => {
  if (Array.isArray(value)) {
    return `[${value.map(canonicalJson).join(",")}]`;
  }
  if (value && typeof value === "object") {
    return `{${Object.keys(value)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`)
      .join(",")}}`;
  }
  return JSON.stringify(value);
};

const signingEnvelope = (request, signatureMetadata) => ({
  request,
  signature: signatureMetadata,
});

const signRequest = (request, signatureMetadata, signingKey) =>
  crypto
    .createHmac("sha256", signingKey)
    .update(canonicalJson(signingEnvelope(request, signatureMetadata)))
    .digest("hex");

const forbidCosPrefixOverride = () => {
  const requestedPrefix = argValue(
    "--cos-prefix",
    process.env.TENCENT_COS_PREFIX || ""
  );
  const normalized = requestedPrefix.replace(/^\/+/, "").replace(/\/+$/, "");
  if (normalized && normalized !== fixedCosPrefix) {
    console.error(
      `COS deploy request prefix is fixed to ${fixedCosPrefix}; got ${normalized}`
    );
    process.exit(2);
  }
};

const isoNow = () => new Date().toISOString();
const expiresAt = (minutes) =>
  new Date(Date.now() + Number(minutes) * 60 * 1000).toISOString();

const schemaPath = path.join(repoRoot, "deploy/runner/deploy-request.schema.json");
const schema = JSON.parse(fs.readFileSync(schemaPath, "utf8"));
const ajv = new Ajv2020({ allErrors: true });
ajv.addFormat("date-time", true);
const validate = ajv.compile(schema);

const commitSha = requireSha(
  "--commit-sha",
  argValue(
    "--commit-sha",
    process.env.DEPLOY_COMMIT_SHA || process.env.GITHUB_SHA || ""
  )
);
const runtimeSha = requireSha(
  "--runtime-sha",
  argValue("--runtime-sha", process.env.DEPLOY_RUNTIME_SHA || commitSha)
);
const deployBundleSha = requireSha(
  "--deploy-bundle-sha",
  argValue("--deploy-bundle-sha", process.env.DEPLOY_BUNDLE_SHA || commitSha)
);
const releaseSha = requireSha(
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
forbidCosPrefixOverride();
const prefix = fixedCosPrefix;
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
const profileDryRunRequestId = argValue(
  "--profile-dry-run-request-id",
  process.env.DEPLOY_PROFILE_DRY_RUN_REQUEST_ID || ""
);
const signingKey = requireValue(
  "DEPLOY_REQUEST_SIGNING_KEY",
  argValue("--signing-key", process.env.DEPLOY_REQUEST_SIGNING_KEY || "")
);
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
if (profileDryRunRequestId) {
  request.profile_dry_run_request_id = profileDryRunRequestId;
}

const signatureMetadata = {
  algorithm: "hmac-sha256",
  issuer: "github-actions",
  key_id: process.env.DEPLOY_REQUEST_SIGNING_KEY_ID || "production",
  signed_at: isoNow(),
};
request.signature = {
  ...signatureMetadata,
  value: signRequest(request, signatureMetadata, signingKey),
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
