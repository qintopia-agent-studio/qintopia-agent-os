#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const shaPattern = /^[0-9a-f]{40}$/;

const argValue = (name) => {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] || "" : "";
};

const requireSha = (name, value) => {
  if (!shaPattern.test(value)) {
    throw new Error(`${name} must be a lowercase 40-character SHA`);
  }
};

const trustedSuccessfulResult = (result) => {
  if (result?.status !== "succeeded") {
    return false;
  }
  if (result.environment !== "production") {
    return false;
  }
  if (result.runtime_artifact_profile !== "huabaosi-production") {
    return false;
  }
  const identity = [
    result.release_sha,
    result.commit_sha,
    result.runtime_sha,
    result.deploy_bundle_sha,
  ];
  if (identity.some((value) => !shaPattern.test(String(value || "")))) {
    return false;
  }
  return identity.every((value) => value === result.release_sha);
};

const resultTimestamp = (result) =>
  String(
    result?.workflow_run?.run_started_at ||
      result?.workflow_run?.created_at ||
      result?.finished_at ||
      ""
  );

const resultRunId = (result) => Number(result?.workflow_run?.id || 0);

const latestSuccessfulResult = (results) =>
  results
    .filter(trustedSuccessfulResult)
    .sort((left, right) => {
      const byTime = resultTimestamp(left).localeCompare(resultTimestamp(right));
      return byTime !== 0 ? byTime : resultRunId(left) - resultRunId(right);
    })
    .at(-1);

const main = () => {
  const deployResultsFile = argValue("--deploy-results-file");
  const commitSha = argValue("--commit-sha");
  const runtimeSha = argValue("--runtime-sha");
  const deployBundleSha = argValue("--deploy-bundle-sha");
  const releaseSha = argValue("--release-sha");
  const runtimeArtifactProfile = argValue("--runtime-artifact-profile");
  const releaseScope = argValue("--release-scope");
  const restartTargets = argValue("--restart-targets");
  const rollbackOnSmokeFailure = argValue("--rollback-on-smoke-failure");

  if (!deployResultsFile) {
    throw new Error("--deploy-results-file is required");
  }
  for (const [name, value] of [
    ["commit_sha", commitSha],
    ["runtime_sha", runtimeSha],
    ["deploy_bundle_sha", deployBundleSha],
    ["release_sha", releaseSha],
  ]) {
    requireSha(name, value);
  }

  if (runtimeArtifactProfile !== "huabaosi-production") {
    throw new Error("legacy runner bootstrap requires huabaosi-production");
  }
  if (releaseScope !== "deploy-bundle") {
    throw new Error("legacy runner bootstrap requires release_scope=deploy-bundle");
  }
  if (restartTargets !== "qintopia-system-services") {
    throw new Error(
      "legacy runner bootstrap requires restart_targets=qintopia-system-services"
    );
  }
  if (rollbackOnSmokeFailure !== "true") {
    throw new Error("legacy runner bootstrap requires rollback_on_smoke_failure=true");
  }
  if (commitSha !== runtimeSha) {
    throw new Error("legacy runner bootstrap commit_sha must equal runtime_sha");
  }
  if (deployBundleSha === runtimeSha) {
    throw new Error("legacy runner bootstrap requires a newer deploy_bundle_sha");
  }
  if (releaseSha === runtimeSha || releaseSha === deployBundleSha) {
    throw new Error(
      "legacy runner bootstrap release_sha must be distinct from runtime and deploy bundle"
    );
  }

  const results = JSON.parse(
    fs.readFileSync(path.resolve(process.cwd(), deployResultsFile), "utf8")
  );
  if (!Array.isArray(results)) {
    throw new Error("deploy results must be a JSON array");
  }
  const latest = latestSuccessfulResult(results);
  if (!latest) {
    throw new Error("no trusted successful production deploy result is available");
  }
  if (runtimeSha !== latest.runtime_sha || commitSha !== latest.commit_sha) {
    throw new Error(
      "legacy runner bootstrap runtime must match the latest successful production deploy"
    );
  }

  process.stdout.write(
    `${JSON.stringify({
      status: "legacy_runner_bootstrap_ready",
      base_release_sha: latest.release_sha,
      base_runtime_sha: latest.runtime_sha,
      requested_deploy_bundle_sha: deployBundleSha,
      requested_release_sha: releaseSha,
    })}\n`
  );
};

try {
  main();
} catch (error) {
  console.error(error.message);
  process.exit(1);
}
