#!/usr/bin/env node

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import YAML from "yaml";

const workflowPath = ".github/workflows/low-risk-release-eligibility.yml";
const workflowText = fs.readFileSync(workflowPath, "utf8");
const workflow = YAML.parse(workflowText);
const pullRequestTarget = workflow?.on?.pull_request_target;
const workflowRun = workflow?.on?.workflow_run;
const workflowDispatch = workflow?.on?.workflow_dispatch;
const job = workflow?.jobs?.advance;

assert.deepEqual(
  Object.keys(workflow.on),
  ["pull_request_target", "workflow_run", "workflow_dispatch"],
  "auto-release may wake only from trusted base workflows, PR metadata, or a manual retry"
);
assert.deepEqual(pullRequestTarget?.types, [
  "opened",
  "reopened",
  "synchronize",
  "labeled",
  "ready_for_review",
]);
assert.deepEqual(workflowRun?.workflows, ["CI", "PR Agent", "Release Please"]);
assert.deepEqual(workflowRun?.types, ["completed"]);
assert.equal(workflowDispatch, null);
assert.equal(workflow?.concurrency?.group, "low-risk-auto-release");
assert.equal(workflow?.concurrency?.["cancel-in-progress"], false);

assert.deepEqual(workflow.permissions, {
  actions: "write",
  checks: "read",
  contents: "write",
  issues: "read",
  "pull-requests": "write",
  statuses: "read",
});
assert.deepEqual(Object.keys(workflow.jobs), ["advance"]);
assert.ok(job, "workflow must expose only the state-machine job");
assert.equal(job.permissions, undefined, "job must not broaden workflow permissions");
assert.equal(job.environment, undefined, "auto-release must not enter an environment");

assert.ok(
  job.steps.every((step) => step.uses === undefined),
  "the privileged lane must not execute external Actions"
);
const checkout = job.steps.find(
  (step) => step.name === "Checkout trusted master without external actions"
);
assert.equal(checkout?.shell, "bash");
for (const fragment of [
  'readonly EXPECTED_REPOSITORY="qintopia-agent-studio/qintopia-agent-os"',
  'checkout_home="${RUNNER_TEMP}/low-risk-checkout-home"',
  'git remote add origin "https://github.com/${EXPECTED_REPOSITORY}.git"',
  "git fetch --no-tags origin refs/heads/master",
  'fetched_sha="$(git rev-parse FETCH_HEAD)"',
  'git checkout --detach "$fetched_sha"',
  '[[ "$(git rev-parse HEAD)" == "$fetched_sha" ]]',
]) {
  assert.ok(checkout?.run?.includes(fragment), `shell checkout is missing ${fragment}`);
}

const advance = job.steps.find(
  (step) => step.name === "Advance the pre-authorized low-risk lane"
);
assert.equal(advance?.shell, "bash");
assert.ok(advance?.run, "state-machine shell script is missing");

const syntax = spawnSync("bash", ["-n"], {
  input: advance.run,
  encoding: "utf8",
});
assert.equal(
  syntax.status,
  0,
  `embedded auto-release shell is invalid:\n${syntax.stderr}`
);

const embeddedValidators = [
  ...advance.run.matchAll(/<<'NODE'\n([\s\S]*?)\nNODE\n/g),
].map((match) => match[1]);
assert.equal(embeddedValidators.length, 2, "both embedded validators are required");
const [metadataValidator, releaseContractValidator] = embeddedValidators;
const releaseValidationStart = advance.run.indexOf("check_release_validation() {");
const releaseValidationEnd = advance.run.indexOf(
  "\n\nassert_label_provenance()",
  releaseValidationStart
);
assert.ok(
  releaseValidationStart >= 0 && releaseValidationEnd > releaseValidationStart,
  "Release Please validation helper is missing"
);
const releaseValidationFunction = advance.run.slice(
  releaseValidationStart,
  releaseValidationEnd
);

const runGit = (repoRoot, args) => {
  const result = spawnSync("git", args, {
    cwd: repoRoot,
    encoding: "utf8",
  });
  assert.equal(result.status, 0, `git ${args.join(" ")} failed:\n${result.stderr}`);
  return result.stdout.trim();
};

const writeFixture = (repoRoot, relativePath, content, mode = 0o644) => {
  const filePath = path.join(repoRoot, relativePath);
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content, { mode });
  fs.chmodSync(filePath, mode);
};

const VALIDATION_HEAD = "d".repeat(40);
const validationStatus = {
  id: 555,
  context: "Release Please validation",
  state: "success",
  created_at: "2026-08-14T10:00:00Z",
  creator: { login: "github-actions[bot]" },
  target_url:
    "https://github.com/qintopia-agent-studio/qintopia-agent-os/actions/runs/123",
};
const validationRun = {
  name: "CI",
  path: ".github/workflows/ci.yml",
  event: "workflow_dispatch",
  status: "completed",
  conclusion: "success",
  head_sha: VALIDATION_HEAD,
  head_branch: "release-please--branches--master--components--qintopia-agent-os",
  repository: { full_name: "qintopia-agent-studio/qintopia-agent-os" },
};
const requiredValidationJobs = [
  "changes",
  "check",
  "Rust quality baseline",
  "Xiaoman PostgreSQL integration",
  "Release Please validation",
].map((name) => ({ name, status: "completed", conclusion: "success" }));

const runReleaseValidationCase = ({
  statuses = [[validationStatus]],
  run = validationRun,
  jobs = [{ total_count: 5, jobs: requiredValidationJobs }],
} = {}) => {
  const fixtureRoot = fs.mkdtempSync(
    path.join(os.tmpdir(), "qintopia-release-validation-")
  );
  try {
    const fakeGh = path.join(fixtureRoot, "gh");
    fs.writeFileSync(
      fakeGh,
      `#!/bin/sh
set -eu
case "$*" in
  *"/statuses?per_page=100"*) cat "$FAKE_STATUSES" ;;
  *"/jobs?per_page=100"*) cat "$FAKE_JOBS" ;;
  *"/actions/runs/123"*) cat "$FAKE_RUN" ;;
  *) exit 99 ;;
esac
`,
      { mode: 0o755 }
    );
    const statusesFile = path.join(fixtureRoot, "statuses.json");
    const runFile = path.join(fixtureRoot, "run.json");
    const jobsFile = path.join(fixtureRoot, "jobs.json");
    const outputFile = path.join(fixtureRoot, "evidence.json");
    fs.writeFileSync(statusesFile, JSON.stringify(statuses));
    fs.writeFileSync(runFile, JSON.stringify(run));
    fs.writeFileSync(jobsFile, JSON.stringify(jobs));
    const shell = `${releaseValidationFunction}
check_release_validation "$EXPECTED_HEAD" "$OUTPUT_FILE"
status=$?
printf '%s' "$status"
exit 0
`;
    const result = spawnSync("bash", ["-c", shell], {
      encoding: "utf8",
      env: {
        ...process.env,
        PATH: `${fixtureRoot}:${process.env.PATH}`,
        EXPECTED_REPOSITORY: "qintopia-agent-studio/qintopia-agent-os",
        RELEASE_BRANCH:
          "release-please--branches--master--components--qintopia-agent-os",
        EXPECTED_HEAD: VALIDATION_HEAD,
        OUTPUT_FILE: outputFile,
        FAKE_STATUSES: statusesFile,
        FAKE_RUN: runFile,
        FAKE_JOBS: jobsFile,
      },
    });
    assert.equal(result.status, 0, result.stderr);
    return {
      status: Number(result.stdout),
      evidence: fs.existsSync(outputFile)
        ? JSON.parse(fs.readFileSync(outputFile, "utf8"))
        : null,
    };
  } finally {
    fs.rmSync(fixtureRoot, { recursive: true, force: true });
  }
};

{
  const result = runReleaseValidationCase();
  assert.equal(result.status, 0);
  assert.equal(result.evidence.head_sha, VALIDATION_HEAD);
  assert.equal(result.evidence.run_id, "123");
}
assert.equal(runReleaseValidationCase({ statuses: [[]] }).status, 3);
assert.equal(
  runReleaseValidationCase({
    statuses: [[{ ...validationStatus, state: "pending" }]],
  }).status,
  8
);
for (const input of [
  {
    statuses: [[{ ...validationStatus, creator: { login: "other-bot[bot]" } }]],
  },
  { run: { ...validationRun, path: ".github/workflows/other.yml" } },
  {
    jobs: [
      {
        total_count: 6,
        jobs: [requiredValidationJobs[0], ...requiredValidationJobs],
      },
    ],
  },
  {
    jobs: [
      {
        total_count: 5,
        jobs: requiredValidationJobs.map((job) =>
          job.name === "check" ? { ...job, conclusion: "failure" } : job
        ),
      },
    ],
  },
]) {
  assert.equal(runReleaseValidationCase(input).status, 1);
}

const baseChangelog = `# Changelog

All notable changes are documented here.

## [0.2.133](https://github.com/qintopia-agent-studio/qintopia-agent-os/compare/v0.2.132...v0.2.133) (2026-08-13)

### Bug Fixes

* previous release
`;

const generatedChangelog = (version = "0.2.134") => `# Changelog

All notable changes are documented here.

## [${version}](https://github.com/qintopia-agent-studio/qintopia-agent-os/compare/v0.2.133...v${version}) (2026-08-14)

### Features

* add bounded provider event mapping

## [0.2.133](https://github.com/qintopia-agent-studio/qintopia-agent-os/compare/v0.2.132...v0.2.133) (2026-08-13)

### Bug Fixes

* previous release
`;

const runMetadataCase = ({
  manifestVersion = "0.2.134",
  manifestText = null,
  changelog = generatedChangelog(manifestVersion),
  extraFiles = {},
  manifestMode = 0o644,
}) => {
  const repoRoot = fs.mkdtempSync(
    path.join(os.tmpdir(), "qintopia-low-risk-release-metadata-")
  );
  try {
    runGit(repoRoot, ["init"]);
    runGit(repoRoot, ["config", "user.name", "Low Risk Test"]);
    runGit(repoRoot, ["config", "user.email", "low-risk@example.invalid"]);
    runGit(repoRoot, ["config", "commit.gpgsign", "false"]);
    writeFixture(repoRoot, ".release-please-manifest.json", '{\n  ".": "0.2.133"\n}\n');
    writeFixture(repoRoot, "CHANGELOG.md", baseChangelog);
    runGit(repoRoot, ["add", "--all"]);
    runGit(repoRoot, ["commit", "-m", "chore: base release metadata"]);
    const baseSha = runGit(repoRoot, ["rev-parse", "HEAD"]);

    writeFixture(
      repoRoot,
      ".release-please-manifest.json",
      manifestText === null ? `{\n  ".": "${manifestVersion}"\n}\n` : manifestText,
      manifestMode
    );
    writeFixture(repoRoot, "CHANGELOG.md", changelog);
    for (const [relativePath, content] of Object.entries(extraFiles)) {
      writeFixture(repoRoot, relativePath, content);
    }
    runGit(repoRoot, ["add", "--all"]);
    runGit(repoRoot, ["commit", "-m", "chore: release metadata candidate"]);
    const headSha = runGit(repoRoot, ["rev-parse", "HEAD"]);

    return spawnSync(process.execPath, ["-", baseSha, headSha, "v0.2.133"], {
      cwd: repoRoot,
      input: metadataValidator,
      encoding: "utf8",
      env: {
        ...process.env,
        GITHUB_REPOSITORY: "qintopia-agent-studio/qintopia-agent-os",
      },
    });
  } finally {
    fs.rmSync(repoRoot, { recursive: true, force: true });
  }
};

{
  const result = runMetadataCase({});
  assert.equal(result.status, 0, result.stderr);
  const evidence = JSON.parse(result.stdout);
  assert.equal(evidence.eligible, true);
  assert.equal(evidence.previous_tag, "v0.2.133");
  assert.equal(evidence.next_tag, "v0.2.134");
}

for (const testCase of [
  {
    name: "runtime smuggling",
    input: { extraFiles: { "runtime/sidecar/src/smuggled.rs": "unsafe {}\n" } },
    error: "exactly two files",
  },
  {
    name: "minor version escalation",
    input: { manifestVersion: "0.3.0" },
    error: "one patch bump",
  },
  {
    name: "historical changelog rewrite",
    input: {
      changelog: generatedChangelog().replace("* previous release", "* rewritten"),
    },
    error: "only add generated lines",
  },
  {
    name: "executable release manifest",
    input: { manifestMode: 0o755 },
    error: "non-executable regular file",
  },
  {
    name: "duplicate release manifest key",
    input: { manifestText: '{".":"0.2.134",".":"0.2.134"}\n' },
    error: "fixed one-entry root-package JSON shape",
  },
]) {
  const result = runMetadataCase(testCase.input);
  assert.notEqual(result.status, 0, `${testCase.name} unexpectedly passed`);
  assert.match(result.stderr, new RegExp(testCase.error));
}

{
  const repoRoot = fs.mkdtempSync(
    path.join(os.tmpdir(), "qintopia-low-risk-draft-release-")
  );
  try {
    runGit(repoRoot, ["init"]);
    runGit(repoRoot, ["config", "user.name", "Low Risk Test"]);
    runGit(repoRoot, ["config", "user.email", "low-risk@example.invalid"]);
    runGit(repoRoot, ["config", "commit.gpgsign", "false"]);
    const changelog = generatedChangelog();
    writeFixture(repoRoot, "CHANGELOG.md", changelog);
    runGit(repoRoot, ["add", "CHANGELOG.md"]);
    runGit(repoRoot, ["commit", "-m", "chore: release changelog"]);
    const masterSha = runGit(repoRoot, ["rev-parse", "HEAD"]);
    const sectionStart = changelog.indexOf("## [0.2.134]");
    const sectionEnd = changelog.indexOf("\n## [0.2.133]", sectionStart);
    const expectedBody = changelog.slice(sectionStart, sectionEnd).replace(/\n+$/, "");
    const releaseFile = path.join(repoRoot, "release.json");

    const baseRelease = {
      id: 12345,
      tag_name: "v0.2.134",
      name: "v0.2.134",
      target_commitish: "master",
      body: expectedBody,
      author: { login: "github-actions[bot]" },
      assets: [],
      draft: true,
      prerelease: false,
      published_at: null,
    };
    const runReleaseCase = ({
      release = baseRelease,
      state = "draft",
      expectedDigest = "",
    } = {}) => {
      fs.writeFileSync(releaseFile, `${JSON.stringify(release)}\n`);
      return spawnSync(
        process.execPath,
        ["-", releaseFile, masterSha, "v0.2.134", state, expectedDigest],
        {
          cwd: repoRoot,
          input: releaseContractValidator,
          encoding: "utf8",
          env: {
            ...process.env,
            GITHUB_REPOSITORY: "qintopia-agent-studio/qintopia-agent-os",
          },
        }
      );
    };

    const initial = runReleaseCase();
    assert.equal(initial.status, 0, initial.stderr);
    const initialEvidence = JSON.parse(initial.stdout);
    assert.equal(initialEvidence.eligible, true);
    assert.match(initialEvidence.identity_digest, /^[0-9a-f]{64}$/);
    assert.equal(initialEvidence.canonical_summary.asset_count, 0);

    for (const testCase of [
      {
        name: "wrong author",
        release: { ...baseRelease, author: { login: "other-bot[bot]" } },
        error: "identity is outside the fixed contract",
      },
      {
        name: "wrong name",
        release: { ...baseRelease, name: "Release v0.2.134" },
        error: "identity is outside the fixed contract",
      },
      {
        name: "wrong target",
        release: { ...baseRelease, target_commitish: "release-candidate" },
        error: "identity is outside the fixed contract",
      },
      {
        name: "wrong body",
        release: { ...baseRelease, body: `${expectedBody}\nextra` },
        error: "exact changelog release section",
      },
      {
        name: "attached asset",
        release: { ...baseRelease, assets: [{ id: 1 }] },
        error: "identity is outside the fixed contract",
      },
    ]) {
      const result = runReleaseCase({ release: testCase.release });
      assert.notEqual(result.status, 0, `${testCase.name} unexpectedly passed`);
      assert.match(result.stderr, new RegExp(testCase.error));
    }

    const summaryDrift = runReleaseCase({
      release: { ...baseRelease, target_commitish: masterSha },
      expectedDigest: initialEvidence.identity_digest,
    });
    assert.notEqual(summaryDrift.status, 0, "canonical summary drift passed");
    assert.match(summaryDrift.stderr, /canonical summary changed/);

    const published = runReleaseCase({
      release: {
        ...baseRelease,
        draft: false,
        published_at: "2026-08-14T12:00:00Z",
      },
      state: "published",
      expectedDigest: initialEvidence.identity_digest,
    });
    assert.equal(published.status, 0, published.stderr);
    assert.equal(JSON.parse(published.stdout).state, "published");
  } finally {
    fs.rmSync(repoRoot, { recursive: true, force: true });
  }
}

for (const fragment of [
  "QINTOPIA_LOW_RISK_AUTO_RELEASE_ENABLED",
  "QINTOPIA_LOW_RISK_AUTO_RELEASE_OWNER_ACKNOWLEDGEMENT",
  "QINTOPIA_LOW_RISK_AUTO_RELEASE_ACTOR",
  "approved-low-risk-auto-release-v1",
  "the dedicated low-risk release token is missing",
  "the dedicated token does not belong to the fixed automation actor",
  'token_actor="$(gh api user --jq \'.login // ""\')"',
  "qintopia-low-risk-auto",
  "^qintopia-programming-agent/[0-9a-f]{32}$",
  "feat(qiwe): add bounded provider event mapping",
  "the low-risk label was not applied by the fixed automation actor",
  "candidate must contain exactly one commit",
  "candidate master tree differs from the reviewed PR head",
  "candidate base is not the latest published commit",
  "candidate is not the sole unpublished commit",
  "candidate squash base is not the latest published commit",
  "candidate squash is not the only unpublished commit",
  "Release Please base is not the sole candidate squash",
  "publication range must contain candidate and metadata squashes only",
  "candidate-complete-unpublished-range-final.json",
  "release-pr-complete-unpublished-range-final.json",
  "publish-complete-unpublished-range-final.json",
  "candidate-required-checks-final.json",
  "release-pr-required-checks-final.json",
  "publish-required-checks-final.json",
  "release-pr-validation.json",
  "release-pr-validation-final.json",
  "release-pr-validation-pre-merge.json",
  "publish-release-validation.json",
  "publish-release-validation-final.json",
  "publish-release-validation-pre-publish.json",
  "check_release_validation",
  '.creator.login // ""',
  '.path == ".github/workflows/ci.yml"',
  "[.[] | .jobs[]] as $jobs",
  '"Rust quality baseline"',
  '"Xiaoman PostgreSQL integration"',
  '"Release Please validation"',
  "assert_latest_published_unchanged",
  "LATEST_PUBLISHED_ID",
  "LATEST_PUBLISHED_SHA",
  'git rev-list -n 1 "${LATEST_PUBLISHED_TAG}^{commit}"',
  "published Release state changed during the audit",
  "Release Please diff must contain exactly two files",
  "Release Please diff is not the exact metadata allowlist",
  'expectedPaths = [".release-please-manifest.json", "CHANGELOG.md"]',
  "Release Please changelog may only add generated lines",
  "low-risk Release Please metadata must be one patch bump",
  "Release Please PR must contain exactly one metadata commit",
  "current master is not the exact Release Please metadata squash",
  "draft Release tag does not point to current master",
  "draft tag moved before publication",
  "master changed after the final candidate audit",
  "master changed after the final Release Please audit",
  "master changed after the final publication audit",
  "draft tag moved after the final publication audit",
  "draft Release changed after the final publication audit",
  "draft Release identity is outside the fixed contract",
  "draft Release body is not the exact changelog release section",
  "Release canonical summary changed during the audit",
  "draft-release-contract-final.json",
  "published-release-contract.json",
  "published Release tag moved after publication",
  'gh pr checks "$pr_number"',
  "--required",
  'all(.bucket == "pass")',
  "--match-head-commit",
  "gh workflow run ci.yml",
  "gh workflow run pr-agent.yml",
  'gh release edit "$expected_tag"',
  "--draft=false",
  "--verify-tag",
]) {
  assert.ok(workflowText.includes(fragment), `workflow is missing ${fragment}`);
}

assert.equal(
  workflowText.split("gh workflow run ").length - 1,
  2,
  "only the exact-head CI and PR-Agent dispatches are allowed"
);
assert.equal(
  workflowText.split("gh pr merge ").length - 1,
  2,
  "only the candidate and Release Please merges are allowed"
);
assert.equal(
  workflowText.split("gh release edit ").length - 1,
  1,
  "only publication of the already-audited draft is allowed"
);
assert.equal(
  workflowText.split("node tools/ci/classify-low-risk-change.mjs").length - 1,
  1,
  "all classification calls must go through one fixed helper"
);
assert.equal(
  workflowText.split("wait_or_fail_release_validation \\").length - 1,
  6,
  "Release Please validation must be checked before and immediately before both mutations"
);

const candidateFinalAudit = workflowText.indexOf(
  "candidate-complete-unpublished-range-final.json"
);
const candidateMerge = workflowText.indexOf('gh pr merge "$candidate_number"');
const releaseFinalAudit = workflowText.indexOf(
  "release-pr-complete-unpublished-range-final.json"
);
const releaseMerge = workflowText.indexOf('gh pr merge "$release_number"');
const publishFinalAudit = workflowText.indexOf(
  "publish-complete-unpublished-range-final.json"
);
const publishRelease = workflowText.indexOf('gh release edit "$expected_tag"');
const releaseValidationPreMerge = workflowText.indexOf(
  "release-pr-validation-pre-merge.json"
);
const draftContractFinal = workflowText.indexOf("draft-release-contract-final.json");
const publishValidationPrePublish = workflowText.indexOf(
  "publish-release-validation-pre-publish.json"
);
assert.ok(candidateFinalAudit >= 0 && candidateFinalAudit < candidateMerge);
assert.ok(releaseFinalAudit >= 0 && releaseFinalAudit < releaseMerge);
assert.ok(publishFinalAudit >= 0 && publishFinalAudit < publishRelease);
assert.ok(releaseValidationPreMerge >= 0 && releaseValidationPreMerge < releaseMerge);
assert.ok(draftContractFinal >= 0 && draftContractFinal < publishRelease);
assert.ok(
  publishValidationPrePublish >= 0 && publishValidationPrePublish < publishRelease
);

const secretReferences = workflowText.match(/secrets\.[A-Z0-9_]+/g) ?? [];
assert.deepEqual(
  [...new Set(secretReferences)],
  ["secrets.QINTOPIA_LOW_RISK_RELEASE_TOKEN"],
  "the lane may use only its dedicated repository-scoped token"
);

for (const forbidden of [
  "push:",
  "release:\n",
  "pull_request:\n",
  "schedule:",
  "pull-requests: read",
  "id-token: write",
  "packages: write",
  "deployments: write",
  "environment:",
  "github.event.pull_request.head",
  "actions/checkout@v7\n        with:\n          ref: ${{",
  "corepack ",
  "pnpm install",
  'require("yaml")',
  "pnpm install --frozen-lockfile\n",
  "gh pr merge --auto",
  "gh pr merge --admin",
  "gh release create",
  "gh release delete",
  "gh release upload",
  "gh release edit $",
  "gh workflow run deploy",
  "deploy-production.yml",
  "repository_dispatch",
  "/dispatches",
  "create-deploy-request",
  "upload-deploy-request",
  "fetch-cos-artifact",
  "TENCENT_COS_",
  "DEPLOY_REQUEST_SIGNING_KEY",
  "--method POST",
  "--method PUT",
  "--method PATCH",
  "--method DELETE",
  "ssh ",
  "curl ",
  "workflow_call:",
  "secrets.QINTOPIA_LOW_RISK_RELEASE_TOKEN || github.token",
]) {
  assert.equal(
    workflowText.includes(forbidden),
    false,
    `auto-release workflow contains forbidden fragment ${forbidden}`
  );
}

assert.equal(
  /^\s*uses:/m.test(workflowText),
  false,
  "the privileged lane must contain no uses steps"
);

console.log("Low-risk auto-release workflow contract passed.");
