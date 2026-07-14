#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import YAML from "yaml";

const repoRoot = process.cwd();
const readmePath = "tools/ci/README.md";
const packagePath = "package.json";
const errors = [];

const readText = (relativePath) =>
  fs.readFileSync(path.join(repoRoot, relativePath), "utf8");

if (!fs.existsSync(path.join(repoRoot, readmePath))) {
  errors.push(`${readmePath}: missing CI tool contract`);
} else {
  const readme = readText(readmePath);
  for (const fragment of [
    "docs-only",
    "required checks",
    "production-adjacent",
    "secrets",
    "commit message",
  ]) {
    if (!readme.includes(fragment)) {
      errors.push(`${readmePath}: must mention ${fragment}`);
    }
  }
}

const packageJson = JSON.parse(readText(packagePath));
for (const scriptName of [
  "check:light",
  "registry:check",
  "secrets:check",
  "commitlint:check",
  "pr:check-body",
  "release-please:check",
  "pr:doctor",
  "pr:bootstrap",
  "pr:create",
  "pr:tools:check",
]) {
  if (!packageJson.scripts?.[scriptName]) {
    errors.push(`${packagePath}: missing ${scriptName}`);
  }
}

for (const requiredPath of [
  "commitlint.config.mjs",
  ".husky/commit-msg",
  "tools/ci/check-commit-messages.mjs",
  "tools/ci/check-pr-body.mjs",
  "tools/ci/check-release-please-pr.mjs",
  "tools/agents/pr-body.mjs",
  "tools/agents/pr-doctor.mjs",
  "tools/agents/pr-bootstrap.mjs",
  "tools/agents/create-pr.mjs",
  "tools/agents/run-command.mjs",
  "tools/agents/check-pr-tools.mjs",
]) {
  if (!fs.existsSync(path.join(repoRoot, requiredPath))) {
    errors.push(`${requiredPath}: required CI or PR gate file is missing`);
  }
}

const prBodyCheck = fs.existsSync(path.join(repoRoot, "tools/ci/check-pr-body.mjs"))
  ? readText("tools/ci/check-pr-body.mjs")
  : "";
for (const requiredFragment of [
  "isReleasePleasePullRequest",
  'headRef.startsWith("release-please--branches--")',
  "releasePleaseAuthors",
  "github-actions[bot]",
  "app/github-actions",
  "This PR was generated with [Release Please]",
  "PR body check skipped for Release Please generated release PR.",
]) {
  if (prBodyCheck && !prBodyCheck.includes(requiredFragment)) {
    errors.push(`tools/ci/check-pr-body.mjs: must include ${requiredFragment}`);
  }
}

if (!packageJson.scripts?.["check:light"]?.includes("pnpm commitlint:check")) {
  errors.push("package.json: check:light must include pnpm commitlint:check");
}

const commitMessageCheck = fs.existsSync(
  path.join(repoRoot, "tools/ci/check-commit-messages.mjs")
)
  ? readText("tools/ci/check-commit-messages.mjs")
  : "";
for (const requiredFragment of [
  "GITHUB_EVENT_PATH",
  "pull_request?.base?.sha",
  "pull_request?.head?.sha",
  'eventName === "push"',
  "event.before",
  "event.after",
  "refs/pull/${prNumber}/head",
  'git", ["cat-file", "-e"',
  "--format=%H%x00%P%x00%s",
]) {
  if (commitMessageCheck && !commitMessageCheck.includes(requiredFragment)) {
    errors.push(`tools/ci/check-commit-messages.mjs: must include ${requiredFragment}`);
  }
}

const ciWorkflow = fs.existsSync(path.join(repoRoot, ".github/workflows/ci.yml"))
  ? readText(".github/workflows/ci.yml")
  : "";
const prAgentWorkflow = fs.existsSync(
  path.join(repoRoot, ".github/workflows/pr-agent.yml")
)
  ? readText(".github/workflows/pr-agent.yml")
  : "";
if (ciWorkflow && !ciWorkflow.includes("fetch-depth: 0")) {
  errors.push(
    ".github/workflows/ci.yml: checkouts must keep enough history for commitlint"
  );
}

if (ciWorkflow && !ciWorkflow.includes("pnpm pr:check-body")) {
  errors.push(".github/workflows/ci.yml: pull_request checks must validate PR body");
}

if (ciWorkflow && !ciWorkflow.includes("      - edited")) {
  errors.push(
    ".github/workflows/ci.yml: pull_request body edits must trigger fresh validation"
  );
}

if (ciWorkflow && !ciWorkflow.includes("release-please-pr")) {
  errors.push(".github/workflows/ci.yml: must detect Release Please PRs");
}

for (const requiredFragment of [
  "workflow_dispatch:",
  "release_please_pr_number:",
  "DISPATCH_RELEASE_PLEASE_PR_NUMBER",
  "workflow_dispatch ref must resolve to the exact release PR head SHA",
  "workflow_dispatch PR is not an authentic Release Please PR",
]) {
  if (ciWorkflow && !ciWorkflow.includes(requiredFragment)) {
    errors.push(
      `.github/workflows/ci.yml: Release Please manual validation must include ${requiredFragment}`
    );
  }
}

if (ciWorkflow && !ciWorkflow.includes("Restart impact preview")) {
  errors.push(
    ".github/workflows/ci.yml: pull_request checks must preview restart impact"
  );
}

if (ciWorkflow && !ciWorkflow.includes("Fetch restart preview commits")) {
  errors.push(".github/workflows/ci.yml: must fetch restart preview commits");
}

if (ciWorkflow && !ciWorkflow.includes("HEAD_REPOSITORY")) {
  errors.push(".github/workflows/ci.yml: restart preview must handle fork PR heads");
}

if (ciWorkflow && !ciWorkflow.includes("tools/deploy/resolve-restart-targets.mjs")) {
  errors.push(".github/workflows/ci.yml: must run restart target resolver");
}

if (ciWorkflow && !ciWorkflow.includes("node tools/ci/check-release-please-pr.mjs")) {
  errors.push(
    ".github/workflows/ci.yml: Release Please PRs must run the release metadata check"
  );
}

const releasePleaseCheck = fs.existsSync(
  path.join(repoRoot, "tools/ci/check-release-please-pr.mjs")
)
  ? readText("tools/ci/check-release-please-pr.mjs")
  : "";
for (const requiredFragment of [
  "release-please--branches--",
  "github-actions[bot]",
  "app/github-actions",
  "This PR was generated with [Release Please]",
  ".release-please-manifest.json",
  "CHANGELOG.md",
]) {
  if (releasePleaseCheck && !releasePleaseCheck.includes(requiredFragment)) {
    errors.push(
      `tools/ci/check-release-please-pr.mjs: must include ${requiredFragment}`
    );
  }
}

if (ciWorkflow) {
  try {
    const parsedWorkflow = YAML.parse(ciWorkflow);
    const workflowDispatch = parsedWorkflow?.on?.workflow_dispatch;
    if (!workflowDispatch?.inputs?.release_please_pr_number) {
      errors.push(
        ".github/workflows/ci.yml: workflow_dispatch must require an explicit Release Please PR number input contract"
      );
    }
    for (const jobName of ["changes", "check"]) {
      const permission =
        parsedWorkflow?.jobs?.[jobName]?.permissions?.["pull-requests"];
      if (permission !== "read") {
        errors.push(
          `.github/workflows/ci.yml: jobs.${jobName} needs pull-requests: read for Release Please dispatch validation`
        );
      }
    }
    if (parsedWorkflow?.jobs?.check?.permissions?.statuses !== "write") {
      errors.push(
        ".github/workflows/ci.yml: jobs.check needs statuses: write to publish the authenticated Release Please result"
      );
    }
    if (parsedWorkflow?.jobs?.changes?.permissions?.statuses) {
      errors.push(
        ".github/workflows/ci.yml: jobs.changes must not receive commit status write permission"
      );
    }
    const checkSteps = parsedWorkflow?.jobs?.check?.steps;
    if (!Array.isArray(checkSteps)) {
      errors.push(".github/workflows/ci.yml: jobs.check.steps must be a step list");
    } else {
      const lightCheckStep = checkSteps.find((step) => step?.name === "Light check");
      if (!lightCheckStep) {
        errors.push(
          ".github/workflows/ci.yml: Light check must be in jobs.check.steps"
        );
      } else {
        const runScript = String(lightCheckStep.run ?? "");
        if (!runScript.includes("pnpm pr:check-body")) {
          errors.push(
            ".github/workflows/ci.yml: Light check must run pnpm pr:check-body for PRs"
          );
        }
        if (!runScript.includes("pnpm check:light")) {
          errors.push(
            ".github/workflows/ci.yml: Light check must run pnpm check:light"
          );
        }
      }
      const releasePleaseCheckStep = checkSteps.find(
        (step) => step?.name === "Release Please PR check"
      );
      if (!releasePleaseCheckStep) {
        errors.push(
          ".github/workflows/ci.yml: Release Please PR check must be in jobs.check.steps"
        );
      } else {
        const runScript = String(releasePleaseCheckStep.run ?? "");
        const condition = String(releasePleaseCheckStep.if ?? "");
        if (!condition.includes("release-please-pr == 'true'")) {
          errors.push(
            ".github/workflows/ci.yml: Release Please PR check must run only for Release Please PRs"
          );
        }
        if (!runScript.includes("tools/ci/check-release-please-pr.mjs")) {
          errors.push(
            ".github/workflows/ci.yml: Release Please PR check must run tools/ci/check-release-please-pr.mjs"
          );
        }
        for (const requiredFragment of [
          "gh api",
          "GITHUB_EVENT_NAME=pull_request",
          "GITHUB_EVENT_PATH",
        ]) {
          if (!runScript.includes(requiredFragment)) {
            errors.push(
              `.github/workflows/ci.yml: Release Please dispatch check must include ${requiredFragment}`
            );
          }
        }
      }
      const releaseStatusStep = checkSteps.find(
        (step) => step?.name === "Publish Release Please validation status"
      );
      if (!releaseStatusStep) {
        errors.push(
          ".github/workflows/ci.yml: must publish a Release Please validation commit status"
        );
      } else {
        const runScript = String(releaseStatusStep.run ?? "");
        const condition = String(releaseStatusStep.if ?? "");
        for (const requiredFragment of [
          "always()",
          "workflow_dispatch",
          "release-please-pr == 'true'",
        ]) {
          if (!condition.includes(requiredFragment)) {
            errors.push(
              `.github/workflows/ci.yml: Release Please status condition must include ${requiredFragment}`
            );
          }
        }
        for (const requiredFragment of [
          "statuses/${HEAD_SHA}",
          'context="Release Please validation"',
          'state="$state"',
          'target_url="$RUN_URL"',
        ]) {
          if (!runScript.includes(requiredFragment)) {
            errors.push(
              `.github/workflows/ci.yml: Release Please status publisher must include ${requiredFragment}`
            );
          }
        }
      }
    }

    const qualityJob = parsedWorkflow?.jobs?.["rust-quality-baseline"];
    if (!qualityJob) {
      errors.push(".github/workflows/ci.yml: missing rust-quality-baseline job");
    } else {
      const qualitySteps = qualityJob.steps ?? [];
      for (const requiredStep of [
        "Rust coverage baseline",
        "All-feature staging adapter tests",
        "Clippy baseline",
        "Upload Rust quality baseline",
      ]) {
        if (!qualitySteps.some((step) => step?.name === requiredStep)) {
          errors.push(
            `.github/workflows/ci.yml: rust-quality-baseline must include ${requiredStep}`
          );
        }
      }
      const coverageStepIndex = qualitySteps.findIndex(
        (step) => step?.name === "Rust coverage baseline"
      );
      const allFeatureTestStepIndex = qualitySteps.findIndex(
        (step) => step?.name === "All-feature staging adapter tests"
      );
      const allFeatureTestStep = qualitySteps[allFeatureTestStepIndex];
      const allFeatureTestCommand = String(allFeatureTestStep?.run ?? "");
      if (allFeatureTestStep?.["continue-on-error"] === true) {
        errors.push(
          ".github/workflows/ci.yml: all-feature staging adapter tests must block on failures"
        );
      }
      for (const requiredFragment of [
        "cargo nextest run",
        "--manifest-path runtime/sidecar/Cargo.toml",
        "--all-features",
        "--no-fail-fast",
      ]) {
        if (!allFeatureTestCommand.includes(requiredFragment)) {
          errors.push(
            `.github/workflows/ci.yml: all-feature staging adapter tests must include ${requiredFragment}`
          );
        }
      }
      if (
        ["--run-ignored", "--include-ignored", "-- --ignored"].some((fragment) =>
          allFeatureTestCommand.includes(fragment)
        )
      ) {
        errors.push(
          ".github/workflows/ci.yml: all-feature staging adapter tests must leave ignored PostgreSQL tests to the disposable integration job"
        );
      }
      if (
        coverageStepIndex !== -1 &&
        allFeatureTestStepIndex !== -1 &&
        allFeatureTestStepIndex <= coverageStepIndex
      ) {
        errors.push(
          ".github/workflows/ci.yml: all-feature staging adapter tests must run after the default coverage suite"
        );
      }
      const clippyStep = qualitySteps.find((step) => step?.name === "Clippy baseline");
      if (clippyStep?.["continue-on-error"] === true) {
        errors.push(
          ".github/workflows/ci.yml: Clippy baseline must block on lint failures"
        );
      }
      if (
        !String(clippyStep?.run ?? "").includes(
          "cargo clippy --manifest-path runtime/sidecar/Cargo.toml --all-targets --no-default-features -- -D warnings"
        )
      ) {
        errors.push(
          ".github/workflows/ci.yml: Clippy baseline must deny warnings for the default production feature set"
        );
      }
      if (
        !String(clippyStep?.run ?? "").includes(
          "cargo clippy --manifest-path runtime/sidecar/Cargo.toml --all-targets --all-features -- -D warnings"
        )
      ) {
        errors.push(
          ".github/workflows/ci.yml: Clippy baseline must deny all warnings for every sidecar target"
        );
      }
    }

    const postgresJob = parsedWorkflow?.jobs?.["xiaoman-postgres-integration"];
    if (!postgresJob) {
      errors.push(".github/workflows/ci.yml: missing xiaoman-postgres-integration job");
    } else {
      const postgres = postgresJob.services?.postgres;
      if (postgres?.image !== "pgvector/pgvector:pg16") {
        errors.push(
          ".github/workflows/ci.yml: Xiaoman integration must use the temporary PostgreSQL 16 service with the vector extension"
        );
      }
      if (postgresJob.env?.QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE !== "1") {
        errors.push(
          ".github/workflows/ci.yml: Xiaoman integration must explicitly enable its disposable apply smoke"
        );
      }
      if (
        !String(postgresJob.env?.QINTOPIA_SIDECAR_DATABASE_URL ?? "").includes(
          "127.0.0.1:5432/qintopia_test"
        )
      ) {
        errors.push(
          ".github/workflows/ci.yml: Xiaoman integration must target only the disposable qintopia_test database"
        );
      }
      if (
        !postgresJob.steps?.some(
          (step) =>
            step?.name === "Xiaoman downstream apply smoke" &&
            String(step.run ?? "").includes("operations-control-plane-apply-smoke.sh")
        )
      ) {
        errors.push(
          ".github/workflows/ci.yml: Xiaoman integration must run the guarded apply smoke"
        );
      }
      const groupSendIntegrationStep = postgresJob.steps?.find(
        (step) => step?.name === "Rust group send-ready PostgreSQL integration"
      );
      const groupSendIntegrationCommand = String(groupSendIntegrationStep?.run ?? "");
      for (const requiredFragment of [
        "cargo test --manifest-path runtime/sidecar/Cargo.toml",
        "--features postgres-integration-tests",
        "group_message_send::tests::postgres_send_ready_is_idempotent_and_fails_closed",
        "-- --ignored --exact",
      ]) {
        if (!groupSendIntegrationCommand.includes(requiredFragment)) {
          errors.push(
            `.github/workflows/ci.yml: Xiaoman integration Rust send-ready test must include ${requiredFragment}`
          );
        }
      }
    }
  } catch (error) {
    errors.push(`.github/workflows/ci.yml: workflow YAML must parse: ${error.message}`);
  }
}

if (prAgentWorkflow) {
  let parsedPrAgentWorkflow;
  try {
    parsedPrAgentWorkflow = YAML.parse(prAgentWorkflow);
  } catch (error) {
    errors.push(
      `.github/workflows/pr-agent.yml: workflow YAML must parse: ${error.message}`
    );
  }
  const prAgentEnv =
    parsedPrAgentWorkflow?.jobs?.["pr-agent"]?.steps?.find((step) =>
      String(step?.uses ?? "").includes("pr-agent")
    )?.env ?? {};
  const prAgentSteps = parsedPrAgentWorkflow?.jobs?.["pr-agent"]?.steps ?? [];
  const detectReleasePleaseStep = prAgentSteps.find(
    (step) => step?.name === "Detect Release Please PR"
  );
  if (!detectReleasePleaseStep) {
    errors.push(
      ".github/workflows/pr-agent.yml: must detect and skip Release Please generated PRs"
    );
  } else {
    const runScript = String(detectReleasePleaseStep.run ?? "");
    for (const requiredFragment of [
      "release-please--branches--",
      "github-actions[bot]",
      "app/github-actions",
      "This PR was generated with [Release Please]",
      "generated=${generated}",
    ]) {
      if (!runScript.includes(requiredFragment)) {
        errors.push(
          `.github/workflows/pr-agent.yml: Release Please detector must include ${requiredFragment}`
        );
      }
    }
  }
  const runPrAgentStep = prAgentSteps.find((step) =>
    String(step?.uses ?? "").includes("pr-agent")
  );
  if (
    runPrAgentStep &&
    !String(runPrAgentStep.if ?? "").includes(
      "steps.release-please.outputs.generated != 'true'"
    )
  ) {
    errors.push(
      ".github/workflows/pr-agent.yml: PR-Agent must skip Release Please generated PRs"
    );
  }
  if (prAgentEnv["pr_description.add_original_user_description"] !== "false") {
    errors.push(
      ".github/workflows/pr-agent.yml: PR-Agent describe output must not repeat the original PR body"
    );
  }
  if (prAgentEnv["github_action_config.auto_describe"] !== "false") {
    errors.push(
      ".github/workflows/pr-agent.yml: PR-Agent must not automatically replace the required PR body"
    );
  }
}

const agentToolFiles = [
  "tools/agents/create-pr.mjs",
  "tools/agents/pr-bootstrap.mjs",
  "tools/agents/pr-doctor.mjs",
  "tools/agents/run-command.mjs",
];
for (const toolFile of agentToolFiles) {
  const source = readText(toolFile);
  if (/execFileSync\([\s\S]*?\)\.trim\(\)/.test(source)) {
    errors.push(`${toolFile}: execFileSync output must handle null before trim`);
  }
}

if (errors.length > 0) {
  console.error("CI contract check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log("CI contract check passed.");
