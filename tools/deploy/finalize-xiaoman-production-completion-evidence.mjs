#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const options = parseArgs(process.argv.slice(2));

runNodeScript("build Xiaoman production completion manifest", [
  "tools/deploy/build-xiaoman-production-completion-manifest.mjs",
  "--release-please-pr-number",
  String(options.releasePleasePrNumber),
  "--release-please-head-sha",
  options.releasePleaseHeadSha,
  "--release-tag",
  options.releaseTag,
  "--released-commit-sha",
  options.releasedCommitSha,
  "--qiwe-production-enablement-pr-number",
  String(options.qiweProductionEnablementPrNumber),
  "--qiwe-production-enablement-head-sha",
  options.qiweProductionEnablementHeadSha,
  "--huabaosi-production-canary",
  options.huabaosiProductionCanary,
  "--production-real-activity",
  options.productionRealActivity,
  "--qiwe-group-arrival-confirmation",
  options.qiweGroupArrivalConfirmation,
  "--output",
  options.output,
]);

runNodeScript("check Xiaoman production completion evidence", [
  "tools/deploy/check-xiaoman-production-completion-evidence.mjs",
  "--manifest",
  options.output,
  "--staging-runtime-readiness",
  options.stagingRuntimeReadiness,
  "--huabaosi-staging",
  options.huabaosiStaging,
  "--qiwe-staging",
  options.qiweStaging,
  "--huabaosi-production-canary",
  options.huabaosiProductionCanary,
  "--production-real-activity",
  options.productionRealActivity,
  "--qiwe-group-arrival-confirmation",
  options.qiweGroupArrivalConfirmation,
]);

process.stdout.write(
  `Xiaoman production completion evidence finalized: ${path.relative(repoRoot, options.output)}\n`
);

function parseArgs(argv) {
  const parsed = {};
  for (let index = 0; index < argv.length; index += 2) {
    const key = argv[index];
    const value = argv[index + 1];
    if (!key?.startsWith("--") || !value) {
      usage();
    }
    parsed[key.slice(2)] = value;
  }

  for (const key of [
    "release-please-pr-number",
    "release-please-head-sha",
    "release-tag",
    "released-commit-sha",
    "qiwe-production-enablement-pr-number",
    "qiwe-production-enablement-head-sha",
    "staging-runtime-readiness",
    "huabaosi-staging",
    "qiwe-staging",
    "huabaosi-production-canary",
    "production-real-activity",
    "qiwe-group-arrival-confirmation",
    "output",
  ]) {
    if (!parsed[key]) {
      usage();
    }
  }

  const resolved = {
    releasePleasePrNumber: positiveInteger(parsed["release-please-pr-number"]),
    releasePleaseHeadSha: gitSha(parsed["release-please-head-sha"]),
    releaseTag: releaseTag(parsed["release-tag"]),
    releasedCommitSha: gitSha(parsed["released-commit-sha"]),
    qiweProductionEnablementPrNumber: positiveInteger(
      parsed["qiwe-production-enablement-pr-number"]
    ),
    qiweProductionEnablementHeadSha: gitSha(
      parsed["qiwe-production-enablement-head-sha"]
    ),
    stagingRuntimeReadiness: resolveExistingFile(
      parsed["staging-runtime-readiness"],
      "staging-runtime-readiness"
    ),
    huabaosiStaging: resolveExistingFile(
      parsed["huabaosi-staging"],
      "huabaosi-staging"
    ),
    qiweStaging: resolveExistingFile(parsed["qiwe-staging"], "qiwe-staging"),
    huabaosiProductionCanary: resolveExistingFile(
      parsed["huabaosi-production-canary"],
      "huabaosi-production-canary"
    ),
    productionRealActivity: resolveExistingFile(
      parsed["production-real-activity"],
      "production-real-activity"
    ),
    qiweGroupArrivalConfirmation: resolveExistingFile(
      parsed["qiwe-group-arrival-confirmation"],
      "qiwe-group-arrival-confirmation"
    ),
    output: path.resolve(parsed.output),
  };

  const outputDir = path.dirname(resolved.output);
  if (!fs.existsSync(outputDir)) {
    fail("output directory does not exist");
  }

  return resolved;
}

function resolveExistingFile(filePath, label) {
  const resolved = path.resolve(filePath);
  if (!fs.existsSync(resolved)) {
    fail(`${label} file does not exist`);
  }
  return resolved;
}

function runNodeScript(label, args) {
  const result = spawnSync("node", args, {
    cwd: repoRoot,
    encoding: "utf8",
  });
  if (result.status !== 0) {
    const diagnostic = (result.stderr || result.stdout || "").trim();
    fail(`${label} failed: ${diagnostic || "unknown error"}`);
  }
}

function positiveInteger(value) {
  if (!/^[1-9][0-9]*$/.test(value)) {
    fail("expected a positive integer argument");
  }
  return Number(value);
}

function gitSha(value) {
  if (!/^[0-9a-f]{40}$/i.test(value)) {
    fail("expected a 40-character git SHA");
  }
  return value.toLowerCase();
}

function releaseTag(value) {
  if (!/^v[0-9]+\.[0-9]+\.[0-9]+$/.test(value)) {
    fail("expected a release tag like v0.0.0");
  }
  return value;
}

function usage() {
  fail(
    "usage: node tools/deploy/finalize-xiaoman-production-completion-evidence.mjs --release-please-pr-number <number> --release-please-head-sha <sha> --release-tag <vX.Y.Z> --released-commit-sha <sha> --qiwe-production-enablement-pr-number <number> --qiwe-production-enablement-head-sha <sha> --staging-runtime-readiness <staging-runtime-readiness-output.txt> --huabaosi-staging <huabaosi-staging-output.txt> --qiwe-staging <qiwe-staging-output.txt> --huabaosi-production-canary <huabaosi-production-canary-output.txt> --production-real-activity <production-evidence-output.txt> --qiwe-group-arrival-confirmation <qiwe-group-arrival-confirmation-output.txt> --output <completed-xiaoman-production-completion-evidence.json>"
  );
}

function fail(message) {
  console.error(
    `Xiaoman production completion evidence finalization failed: ${message}`
  );
  process.exit(1);
}
