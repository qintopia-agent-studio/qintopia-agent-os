#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const options = parseArgs(process.argv.slice(2));
const forbiddenOutputPatterns = [
  /https?:\/\//i,
  /postgres(?:ql)?:\/\//i,
  /tenant_access_token/i,
  /base_token/i,
  /api[_-]?key/i,
  /\btoken\b/i,
  /QIWE_TOKEN/,
  /QIWE_GUID/,
  /DATABASE_URL/,
  /target_group_id/i,
  /artifact_uri/i,
  /provider_response/i,
  /raw_chat/i,
  /message_id/i,
  /callback_event_id/i,
  /request_id/i,
  /fileAesKey/i,
  /file_aes_key/i,
  /fileId/i,
  /file_id/i,
  /fileMd5/i,
  /file_md5/i,
  /fileSize/i,
  /file_size/i,
  /filename/i,
];

runChecker("Huabaosi production canary evidence", [
  "tools/deploy/check-huabaosi-image-production-canary-evidence.mjs",
  options.huabaosiProductionCanary,
]);
runChecker("Xiaoman real activity production evidence", [
  "tools/deploy/check-xiaoman-real-activity-production-evidence.mjs",
  options.productionRealActivity,
]);
runChecker("Xiaoman QiWe group arrival confirmation evidence", [
  "tools/deploy/check-xiaoman-qiwe-group-arrival-confirmation-evidence.mjs",
  options.productionRealActivity,
  options.qiweGroupArrivalConfirmation,
]);

const huabaosiCanaryRecords = prefixedRecords(
  options.huabaosiProductionCanary,
  "huabaosi_image_generation_production_canary_evidence="
);
const productionRecords = prefixedRecords(
  options.productionRealActivity,
  "xiaoman_real_activity_production_evidence="
);
const qiweGroupArrivalConfirmation = singlePrefixedRecord(
  options.qiweGroupArrivalConfirmation,
  "xiaoman_qiwe_group_arrival_confirmation_evidence="
);
const huabaosiGeneration = single(
  huabaosiCanaryRecords.filter((record) => record.phase === "generation"),
  "expected one Huabaosi production canary generation record"
);
const productionRetention = single(
  productionRecords.filter((record) => record.phase === "sanitized_evidence_retention"),
  "expected one Xiaoman production retention record"
);

assertReleaseFactsBind(huabaosiGeneration, productionRetention, options);
assertArrivalConfirmation(qiweGroupArrivalConfirmation);
assertGithubReleaseFacts(options);

const manifest = {
  schema: "xiaoman-production-completion-evidence-v1",
  release_please_validation: {
    status: "passed",
    pr_number: options.releasePleasePrNumber,
    head_sha: options.releasePleaseHeadSha,
    release_tag: options.releaseTag,
    released_commit_sha: options.releasedCommitSha,
    manual_ci_workflow: "ci.yml",
    release_please_status: "success",
  },
  qiwe_production_enablement: {
    status: "merged",
    pr_number: options.qiweProductionEnablementPrNumber,
    head_sha: options.qiweProductionEnablementHeadSha,
    included_in_release_sha: options.releasedCommitSha,
    listener_service_timer_reviewed: true,
    observation_reviewed: true,
    rollback_reviewed: true,
    exact_allowlists_reviewed: true,
    production_feature_boundary_reviewed: true,
  },
  huabaosi_production_activation: {
    release_sha: options.releasedCommitSha,
    sidecar_binary_sha256: huabaosiGeneration.sidecar_binary_sha256,
    database_url_sha256: huabaosiGeneration.database_url_sha256,
    image_generation_observation_passed: true,
    image_generation_activation_approved: true,
    feishu_mirror_observation_passed: true,
    feishu_mirror_activation_approved: true,
    first_record_evidence_retained: true,
  },
  real_activity_confirmation: {
    qiwe_group_arrival_confirmed: true,
    confirmed_by: qiweGroupArrivalConfirmation.confirmed_by,
    confirmed_at: qiweGroupArrivalConfirmation.confirmed_at,
  },
};

const output = `${JSON.stringify(manifest, null, 2)}\n`;
assertNoSensitiveOutput(output);
if (options.output) {
  fs.writeFileSync(options.output, output, "utf8");
} else {
  process.stdout.write(output);
}

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

  const required = [
    "release-please-pr-number",
    "release-please-head-sha",
    "release-tag",
    "released-commit-sha",
    "qiwe-production-enablement-pr-number",
    "qiwe-production-enablement-head-sha",
    "huabaosi-production-canary",
    "production-real-activity",
    "qiwe-group-arrival-confirmation",
  ];
  for (const key of required) {
    if (!parsed[key]) {
      usage();
    }
  }

  const fileKeys = [
    "huabaosi-production-canary",
    "production-real-activity",
    "qiwe-group-arrival-confirmation",
  ];
  for (const key of fileKeys) {
    parsed[key] = path.resolve(parsed[key]);
    if (!fs.existsSync(parsed[key])) {
      fail(`${key} file does not exist`);
    }
  }

  if (parsed.output) {
    parsed.output = path.resolve(parsed.output);
  }

  const options = {
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
    huabaosiProductionCanary: parsed["huabaosi-production-canary"],
    productionRealActivity: parsed["production-real-activity"],
    qiweGroupArrivalConfirmation: parsed["qiwe-group-arrival-confirmation"],
    output: parsed.output,
  };

  if (options.qiweProductionEnablementPrNumber > options.releasePleasePrNumber) {
    fail("QiWe production enablement PR must not be newer than the Release Please PR");
  }

  return options;
}

function usage() {
  fail(
    "usage: node tools/deploy/build-xiaoman-production-completion-manifest.mjs --release-please-pr-number <number> --release-please-head-sha <sha> --release-tag <vX.Y.Z> --released-commit-sha <sha> --qiwe-production-enablement-pr-number <number> --qiwe-production-enablement-head-sha <sha> --huabaosi-production-canary <output.txt> --production-real-activity <output.txt> --qiwe-group-arrival-confirmation <output.txt> [--output <manifest.json>]"
  );
}

function runChecker(label, checkerArgs) {
  const result = spawnSync("node", checkerArgs, {
    cwd: repoRoot,
    encoding: "utf8",
  });
  if (result.status !== 0) {
    fail(`${label} failed: ${safeDiagnostic(result.stderr || result.stdout)}`);
  }
}

function runGhJson(label, args) {
  const result = spawnSync("gh", args, {
    cwd: repoRoot,
    encoding: "utf8",
  });
  if (result.status !== 0) {
    fail(
      `${label} GitHub verification failed: ${safeDiagnostic(result.stderr || result.stdout)}`
    );
  }
  try {
    return JSON.parse(result.stdout);
  } catch (error) {
    fail(`${label} GitHub verification returned invalid JSON: ${error.message}`);
  }
}

function assertGithubReleaseFacts(options) {
  assertPublishedReleaseTag(options);

  const releasePlease = runGhJson("Release Please PR", [
    "pr",
    "view",
    String(options.releasePleasePrNumber),
    "--json",
    "number,state,baseRefName,headRefOid,mergeCommit,statusCheckRollup",
  ]);
  assertPullRequestFact(releasePlease, {
    label: "Release Please PR",
    number: options.releasePleasePrNumber,
    headSha: options.releasePleaseHeadSha,
  });
  if (releasePlease.mergeCommit?.oid !== options.releasedCommitSha) {
    fail("Release Please PR merge commit does not match the released commit SHA");
  }
  for (const checkName of ["changes", "check", "Release Please validation"]) {
    if (!hasSuccessfulCheck(releasePlease.statusCheckRollup, checkName)) {
      fail(`Release Please PR is missing successful ${checkName} status`);
    }
  }

  const qiweEnablement = runGhJson("QiWe production enablement PR", [
    "pr",
    "view",
    String(options.qiweProductionEnablementPrNumber),
    "--json",
    "number,state,baseRefName,headRefOid,mergeCommit",
  ]);
  assertPullRequestFact(qiweEnablement, {
    label: "QiWe production enablement PR",
    number: options.qiweProductionEnablementPrNumber,
    headSha: options.qiweProductionEnablementHeadSha,
  });

  const compare = runGhJson("QiWe production enablement inclusion", [
    "api",
    `repos/:owner/:repo/compare/${options.qiweProductionEnablementHeadSha}...${options.releasedCommitSha}`,
  ]);
  if (!["ahead", "identical"].includes(compare.status)) {
    fail(
      "QiWe production enablement PR head is not included in the released commit SHA"
    );
  }
}

function assertPublishedReleaseTag(options) {
  const release = runGhJson("Published GitHub Release", [
    "api",
    `repos/:owner/:repo/releases/tags/${options.releaseTag}`,
  ]);
  if (
    release.tag_name !== options.releaseTag ||
    release.draft !== false ||
    release.prerelease !== false
  ) {
    fail("published GitHub Release must exist and must not be draft or prerelease");
  }

  const tagRef = runGhJson("Published Git tag ref", [
    "api",
    `repos/:owner/:repo/git/ref/tags/${options.releaseTag}`,
  ]);
  const tagCommitSha = resolveTagCommitSha(tagRef, options.releaseTag);
  if (tagCommitSha !== options.releasedCommitSha) {
    fail("published GitHub Release tag does not point to the released commit SHA");
  }
}

function resolveTagCommitSha(tagRef, releaseTag) {
  if (tagRef.ref !== `refs/tags/${releaseTag}`) {
    fail("published Git tag ref does not match the requested release tag");
  }
  const object = tagRef.object ?? {};
  if (object.type === "commit" && isGitSha(object.sha)) {
    return object.sha;
  }
  if (object.type === "tag" && isGitSha(object.sha)) {
    const annotatedTag = runGhJson("Published annotated Git tag", [
      "api",
      `repos/:owner/:repo/git/tags/${object.sha}`,
    ]);
    if (annotatedTag.object?.type === "commit" && isGitSha(annotatedTag.object.sha)) {
      return annotatedTag.object.sha;
    }
  }
  fail("published Git tag does not resolve to a commit SHA");
}

function assertPullRequestFact(record, expected) {
  if (record.number !== expected.number) {
    fail(`${expected.label} number does not match GitHub state`);
  }
  if (record.state !== "MERGED") {
    fail(`${expected.label} must be merged in GitHub before manifest generation`);
  }
  if (record.baseRefName !== "master") {
    fail(`${expected.label} must target master`);
  }
  if (record.headRefOid !== expected.headSha) {
    fail(`${expected.label} head SHA does not match GitHub state`);
  }
}

function hasSuccessfulCheck(statusCheckRollup, expectedName) {
  if (!Array.isArray(statusCheckRollup)) {
    return false;
  }
  return statusCheckRollup.some((check) => {
    const name = check.name ?? check.context ?? "";
    const conclusion = check.conclusion ?? check.state ?? "";
    return name === expectedName && /^(SUCCESS|success)$/i.test(conclusion);
  });
}

function prefixedRecords(file, prefix) {
  return fs
    .readFileSync(file, "utf8")
    .split(/\r?\n/)
    .filter((line) => line.startsWith(prefix))
    .map((line) => JSON.parse(line.slice(prefix.length)));
}

function singlePrefixedRecord(file, prefix) {
  return single(prefixedRecords(file, prefix), `expected exactly one ${prefix} record`);
}

function single(records, message) {
  if (records.length !== 1) {
    fail(message);
  }
  return records[0];
}

function assertReleaseFactsBind(huabaosiGeneration, productionRetention, options) {
  if (
    huabaosiGeneration.release_sha !== options.releasedCommitSha ||
    productionRetention.production_release_sha !== options.releasedCommitSha
  ) {
    fail("evidence release SHA does not match the released commit SHA");
  }
  for (const key of ["sidecar_binary_sha256", "database_url_sha256"]) {
    if (huabaosiGeneration[key] !== productionRetention[key]) {
      fail(`Huabaosi canary and real activity evidence ${key} values differ`);
    }
  }
  if (
    huabaosiGeneration.release_binary_verified !== true ||
    huabaosiGeneration.approved_sidecar_sha256_matched !== true ||
    huabaosiGeneration.approved_database_url_sha256_matched !== true ||
    productionRetention.release_binary_verified !== true ||
    productionRetention.approved_sidecar_sha256_matched !== true ||
    productionRetention.approved_database_url_sha256_matched !== true
  ) {
    fail("production evidence release-local binary/hash boundary is incomplete");
  }
}

function assertArrivalConfirmation(record) {
  if (
    record.schema !== "xiaoman-qiwe-group-arrival-confirmation-evidence-v1" ||
    record.success !== true ||
    record.confirmation_status !== "confirmed" ||
    record.confirmation_method !== "human_visible_group_check" ||
    record.external_send_executed !== true ||
    record.raw_secret_fields_retained !== false ||
    !isSafeLabel(record.confirmed_by) ||
    !isUtcSecondTimestamp(record.confirmed_at)
  ) {
    fail("QiWe group arrival confirmation is incomplete");
  }
}

function positiveInteger(value) {
  if (!/^[1-9][0-9]*$/.test(value)) {
    fail("PR number must be a positive integer");
  }
  return Number.parseInt(value, 10);
}

function gitSha(value) {
  if (!isGitSha(value)) {
    fail("Git SHA must be 40 lowercase hex characters");
  }
  return value;
}

function isGitSha(value) {
  return typeof value === "string" && /^[0-9a-f]{40}$/.test(value);
}

function releaseTag(value) {
  if (!/^v[0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?$/.test(value)) {
    fail("Release tag must be a safe v-prefixed SemVer tag");
  }
  return value;
}

function isSafeLabel(value) {
  return (
    typeof value === "string" &&
    /^[A-Za-z0-9_.:@-]{1,80}$/.test(value) &&
    !/(token|secret|password|key|guid)/i.test(value)
  );
}

function isUtcSecondTimestamp(value) {
  return (
    typeof value === "string" && /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/.test(value)
  );
}

function safeDiagnostic(text) {
  const diagnostic = String(text ?? "");
  if (forbiddenOutputPatterns.some((pattern) => pattern.test(diagnostic))) {
    return "redacted-sensitive-diagnostic";
  }
  return diagnostic
    .replace(/https?:\/\/\S+/g, "redacted-url")
    .replace(/postgres(?:ql)?:\/\/\S+/gi, "redacted-database-url")
    .replace(/(token|secret|password|key)=\S+/gi, "$1=redacted")
    .slice(0, 1000);
}

function assertNoSensitiveOutput(text) {
  for (const pattern of forbiddenOutputPatterns) {
    if (pattern.test(text)) {
      fail("completion manifest output contains a forbidden sensitive fragment");
    }
  }
}

function fail(message) {
  console.error(`Xiaoman production completion manifest build failed: ${message}`);
  process.exit(1);
}
