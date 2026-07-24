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
    "risk-tiered",
    "secrets",
    "commit message",
    "check:pr:quick",
    "check:pr:heavy",
    "check:pr:auto",
  ]) {
    if (!readme.includes(fragment)) {
      errors.push(`${readmePath}: must mention ${fragment}`);
    }
  }
}

const packageJson = JSON.parse(readText(packagePath));
for (const scriptName of [
  "check:pr:quick",
  "check:pr:heavy",
  "check:pr:postgres",
  "check:pr:auto",
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
  "tools/ci/run-local-pr-checks.mjs",
  "tools/ci/check-pr-body.mjs",
  "tools/ci/check-release-please-pr.mjs",
  "tools/ci/xiaoman-production-claim-boundary.mjs",
  "tools/ci/test-xiaoman-production-claim-boundary.mjs",
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
  "rust-quality-check",
  "postgres-integration-check",
  ".github/workflows/**",
  "runtime/sidecar/**",
  "runtime/postgres/**",
  "deploy/sidecar/**",
]) {
  if (ciWorkflow && !ciWorkflow.includes(requiredFragment)) {
    errors.push(
      `.github/workflows/ci.yml: heavy checks must include ${requiredFragment}`
    );
  }
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
  "validateXiaomanProductionCompletionClaimBoundary",
]) {
  if (releasePleaseCheck && !releasePleaseCheck.includes(requiredFragment)) {
    errors.push(
      `tools/ci/check-release-please-pr.mjs: must include ${requiredFragment}`
    );
  }
}

const xiaomanProductionClaimBoundary = fs.existsSync(
  path.join(repoRoot, "tools/ci/xiaoman-production-claim-boundary.mjs")
)
  ? readText("tools/ci/xiaoman-production-claim-boundary.mjs")
  : "";
for (const requiredFragment of [
  "requiredXiaomanProductionCompletionEvidenceRefs",
  "docs/plans/active/xiaoman-production-completion-gate.md",
  "tools/deploy/check-xiaoman-production-completion-evidence.mjs",
  "xiaoman-production-completion-evidence-v1",
  "owner-retained evidence",
  "positiveXiaomanProductionCompletionClaimLines",
]) {
  if (
    xiaomanProductionClaimBoundary &&
    !xiaomanProductionClaimBoundary.includes(requiredFragment)
  ) {
    errors.push(
      `tools/ci/xiaoman-production-claim-boundary.mjs: must include ${requiredFragment}`
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
    if (parsedWorkflow?.jobs?.check?.permissions?.statuses) {
      errors.push(
        ".github/workflows/ci.yml: jobs.check must not publish Release Please status before independent heavy jobs finish"
      );
    }
    if (parsedWorkflow?.jobs?.changes?.permissions?.statuses) {
      errors.push(
        ".github/workflows/ci.yml: jobs.changes must not receive commit status write permission"
      );
    }
    const changesOutputs = parsedWorkflow?.jobs?.changes?.outputs ?? {};
    for (const outputName of [
      "full-check",
      "rust-quality-check",
      "postgres-integration-check",
      "release-please-pr",
    ]) {
      if (!changesOutputs[outputName]) {
        errors.push(`.github/workflows/ci.yml: jobs.changes must output ${outputName}`);
      }
    }
    const detectChangesStep = parsedWorkflow?.jobs?.changes?.steps?.find(
      (step) => step?.name === "Detect changed files"
    );
    const detectChangesScript = String(detectChangesStep?.run ?? "");
    if (
      !/if \[\[ "\$release_please_pr" == "true" \]\]; then\s+full_check=true\s+rust_quality_check=true\s+postgres_integration_check=true/.test(
        detectChangesScript
      )
    ) {
      errors.push(
        ".github/workflows/ci.yml: authenticated Release Please validation must force full, Rust, and PostgreSQL checks"
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
      if (
        checkSteps.some(
          (step) => step?.name === "Publish Release Please validation status"
        )
      ) {
        errors.push(
          ".github/workflows/ci.yml: jobs.check must not publish Release Please status before independent heavy jobs finish"
        );
      }
    }

    const releaseValidationJob = parsedWorkflow?.jobs?.["release-please-validation"];
    if (!releaseValidationJob) {
      errors.push(
        ".github/workflows/ci.yml: missing final release-please-validation aggregation job"
      );
    } else {
      const releaseNeeds = Array.isArray(releaseValidationJob.needs)
        ? releaseValidationJob.needs
        : [releaseValidationJob.needs];
      for (const requiredJob of [
        "changes",
        "check",
        "rust-quality-baseline",
        "xiaoman-postgres-integration",
      ]) {
        if (!releaseNeeds.includes(requiredJob)) {
          errors.push(
            `.github/workflows/ci.yml: release-please-validation must wait for ${requiredJob}`
          );
        }
      }
      if (releaseValidationJob.permissions?.statuses !== "write") {
        errors.push(
          ".github/workflows/ci.yml: release-please-validation needs statuses: write"
        );
      }
      const releaseCondition = String(releaseValidationJob.if ?? "");
      for (const requiredFragment of [
        "always()",
        "workflow_dispatch",
        "release-please-pr == 'true'",
      ]) {
        if (!releaseCondition.includes(requiredFragment)) {
          errors.push(
            `.github/workflows/ci.yml: release-please-validation condition must include ${requiredFragment}`
          );
        }
      }
      const releaseStatusStep = releaseValidationJob.steps?.find(
        (step) => step?.name === "Publish Release Please validation status"
      );
      const runScript = String(releaseStatusStep?.run ?? "");
      for (const requiredFragment of [
        "needs.check.result",
        "needs.rust-quality-baseline.result",
        "needs.xiaoman-postgres-integration.result",
        "statuses/${HEAD_SHA}",
        'context="Release Please validation"',
        'state="$state"',
        'target_url="$RUN_URL"',
        '[[ "$state" == "success" ]]',
      ]) {
        if (!runScript.includes(requiredFragment)) {
          errors.push(
            `.github/workflows/ci.yml: final Release Please status publisher must include ${requiredFragment}`
          );
        }
      }
    }

    const qualityJob = parsedWorkflow?.jobs?.["rust-quality-baseline"];
    if (!qualityJob) {
      errors.push(".github/workflows/ci.yml: missing rust-quality-baseline job");
    } else {
      const condition = String(qualityJob.if ?? "");
      if (!condition.includes("rust-quality-check == 'true'")) {
        errors.push(
          ".github/workflows/ci.yml: rust-quality-baseline must be gated by rust-quality-check"
        );
      }
      if (condition.includes("full-check == 'true'")) {
        errors.push(
          ".github/workflows/ci.yml: rust-quality-baseline must not run for every full-check change"
        );
      }
      if (condition.includes("release-please-pr != 'true'")) {
        errors.push(
          ".github/workflows/ci.yml: rust-quality-baseline must not exclude authenticated Release Please validation"
        );
      }
      if (
        qualityJob.env?.CARGO_HTTP_MULTIPLEXING !== "false" ||
        qualityJob.env?.CARGO_NET_RETRY !== "10"
      ) {
        errors.push(
          ".github/workflows/ci.yml: Rust quality tool downloads must disable HTTP/2 multiplexing and retry transient failures"
        );
      }
      const qualitySteps = qualityJob.steps ?? [];
      const prepareToolsStep = qualitySteps.find(
        (step) => step?.name === "Prepare Rust quality tool evidence"
      );
      const prepareToolsCommand = String(prepareToolsStep?.run ?? "");
      for (const requiredFragment of [
        "mkdir -p coverage",
        "coverage/rust-quality-tool-install-strategy.txt",
        "installer=taiki-e/install-action@v2",
        "checksum=true",
        "fallback=none",
      ]) {
        if (!prepareToolsCommand.includes(requiredFragment)) {
          errors.push(
            `.github/workflows/ci.yml: Rust quality evidence setup must retain ${requiredFragment}`
          );
        }
      }
      const installToolsStep = qualitySteps.find(
        (step) => step?.name === "Install Rust quality tools"
      );
      if (installToolsStep?.uses !== "taiki-e/install-action@v2") {
        errors.push(
          ".github/workflows/ci.yml: Rust quality tools must use the prebuilt taiki-e installer"
        );
      }
      if (installToolsStep?.with?.tool !== "nextest@0.9.138,cargo-llvm-cov@0.8.7") {
        errors.push(
          ".github/workflows/ci.yml: Rust quality tools must retain the fixed nextest and cargo-llvm-cov versions"
        );
      }
      if (
        installToolsStep?.with?.checksum !== true ||
        installToolsStep?.with?.fallback !== "none"
      ) {
        errors.push(
          ".github/workflows/ci.yml: Rust quality prebuilt installs must require checksums and disable Cargo fallback"
        );
      }
      if (
        qualitySteps.some((step) =>
          /cargo install (?:cargo-nextest|cargo-llvm-cov)/.test(String(step?.run ?? ""))
        )
      ) {
        errors.push(
          ".github/workflows/ci.yml: Rust quality tools must not compile through cargo install"
        );
      }
      const recordToolsStep = qualitySteps.find(
        (step) => step?.name === "Record Rust quality tool versions"
      );
      const recordToolsCommand = String(recordToolsStep?.run ?? "");
      for (const requiredFragment of [
        "set -o pipefail",
        "cargo nextest --version",
        "coverage/cargo-nextest-version.txt",
        "cargo llvm-cov --version",
        "coverage/cargo-llvm-cov-version.txt",
      ]) {
        if (!recordToolsCommand.includes(requiredFragment)) {
          errors.push(
            `.github/workflows/ci.yml: Rust quality tool version evidence must retain ${requiredFragment}`
          );
        }
      }
      const prepareToolsStepIndex = qualitySteps.indexOf(prepareToolsStep);
      const installToolsStepIndex = qualitySteps.indexOf(installToolsStep);
      const recordToolsStepIndex = qualitySteps.indexOf(recordToolsStep);
      if (
        prepareToolsStepIndex < 0 ||
        installToolsStepIndex <= prepareToolsStepIndex ||
        recordToolsStepIndex <= installToolsStepIndex
      ) {
        errors.push(
          ".github/workflows/ci.yml: Rust quality tool evidence, install, and version steps must remain ordered"
        );
      }
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
      const condition = String(postgresJob.if ?? "");
      if (!condition.includes("postgres-integration-check == 'true'")) {
        errors.push(
          ".github/workflows/ci.yml: xiaoman-postgres-integration must be gated by postgres-integration-check"
        );
      }
      if (condition.includes("full-check == 'true'")) {
        errors.push(
          ".github/workflows/ci.yml: xiaoman-postgres-integration must not run for every full-check change"
        );
      }
      if (condition.includes("release-please-pr != 'true'")) {
        errors.push(
          ".github/workflows/ci.yml: xiaoman-postgres-integration must not exclude authenticated Release Please validation"
        );
      }
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
  const prAgentDispatch = parsedPrAgentWorkflow?.on?.workflow_dispatch;
  if (!prAgentDispatch?.inputs?.release_please_pr_number) {
    errors.push(
      ".github/workflows/pr-agent.yml: workflow_dispatch must accept an explicit Release Please PR number"
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
    const condition = String(detectReleasePleaseStep.if ?? "");
    if (!condition.includes("inputs.release_please_pr_number != ''")) {
      errors.push(
        ".github/workflows/pr-agent.yml: Release Please detector must run for explicit manual validation"
      );
    }
    for (const requiredFragment of [
      "release-please--branches--",
      "github-actions[bot]",
      "app/github-actions",
      "This PR was generated with [Release Please]",
      "DISPATCH_RELEASE_PLEASE_PR_NUMBER",
      "gh api",
      "release PR-Agent dispatch requires an open PR targeting master",
      "PR-Agent workflow_dispatch ref must resolve to the exact release PR head SHA",
      "PR-Agent workflow_dispatch PR is not an authentic Release Please PR",
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
  if (
    String(runPrAgentStep?.if ?? "").includes("github.event_name != 'pull_request'")
  ) {
    errors.push(
      ".github/workflows/pr-agent.yml: manual authenticated Release Please validation must skip external PR-Agent"
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
