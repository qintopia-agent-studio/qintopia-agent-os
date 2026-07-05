#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { execFileSync } from "node:child_process";
import YAML from "yaml";

const repoRoot = process.cwd();
const args = new Set(process.argv.slice(2));
const ciMode = args.has("--ci") || process.env.CI === "true";
const errors = [];

const requiredScripts = [
  "format:check",
  "lint:md",
  "registry:check",
  "agents:check",
  "policy:check",
  "secrets:check",
  "deploy:preflight:ci",
  "deploy:github-app-git:check",
  "deploy:cos:check",
  "deploy:postgres:schema:preflight",
  "deploy:systemd:check",
  "deploy:m9f:check",
  "artifact:sidecar",
  "artifact:prune:sidecar",
  "test:qiwe",
  "test:sidecar",
  "smoke:sidecar",
  "check",
];

const requiredDocs = [
  "docs/engineering/server-change-policy.md",
  "docs/engineering/ci-cd-gates.md",
  "deploy/sidecar/docs/monorepo-cutover-plan.md",
  "deploy/sidecar/docs/systemd-cutover-plan.md",
  "deploy/sidecar/docs/m9f-legacy-reference-removal.md",
  "docs/operations/sidecar-ci-artifacts.md",
  "docs/operations/m9-server-cutover-runbook.md",
  "deploy/sidecar/scripts/github-app-git.sh",
  "deploy/sidecar/scripts/install-coscli.sh",
  "deploy/sidecar/scripts/upload-cos-artifact.sh",
  "deploy/sidecar/scripts/fetch-cos-artifact.sh",
  "deploy/sidecar/scripts/fetch-ci-artifact.sh",
  "deploy/sidecar/scripts/postgres-schema-preflight.sh",
  "deploy/sidecar/scripts/render-systemd-units.sh",
  "deploy/sidecar/scripts/hermes/qintopia-context-mcp",
  "tools/deploy/check-m9f-readiness.mjs",
];

const requiredCheckFragments = [
  "pnpm format:check",
  "pnpm lint:md",
  "pnpm registry:check",
  "pnpm agents:check",
  "pnpm policy:check",
  "pnpm secrets:check",
  "pnpm deploy:preflight:ci",
  "pnpm deploy:github-app-git:check",
  "pnpm deploy:cos:check",
  "pnpm deploy:systemd:check",
  "pnpm deploy:m9f:check",
  "pnpm test:qiwe",
  "pnpm test:sidecar",
  "pnpm smoke:sidecar",
];

const addError = (message) => {
  errors.push(message);
};

const exists = (relativePath) => fs.existsSync(path.join(repoRoot, relativePath));

const readText = (relativePath) =>
  fs.readFileSync(path.join(repoRoot, relativePath), "utf8");

const readYaml = (relativePath) => YAML.parse(readText(relativePath));

const git = (args) =>
  execFileSync("git", args, {
    cwd: repoRoot,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  }).trim();

const packageJson = JSON.parse(readText("package.json"));
const scripts = packageJson.scripts ?? {};

for (const scriptName of requiredScripts) {
  if (!scripts[scriptName]) {
    addError(`package.json: missing script ${scriptName}`);
  }
}

for (const fragment of requiredCheckFragments) {
  if (!scripts.check?.includes(fragment)) {
    addError(`package.json: check script must include '${fragment}'`);
  }
}

for (const docPath of requiredDocs) {
  if (!exists(docPath)) {
    addError(`${docPath}: required deploy gate document is missing`);
  }
}

if (exists("deploy/sidecar/scripts/github-app-git.sh")) {
  const githubAppGitScript = readText("deploy/sidecar/scripts/github-app-git.sh");
  for (const unsafeFragment of [
    "x-access-token:",
    "https://x-access-token:",
    'GITHUB_APP_INSTALLATION_TOKEN"',
    "GITHUB_APP_INSTALLATION_TOKEN'",
  ]) {
    if (githubAppGitScript.includes(unsafeFragment)) {
      addError(
        "deploy/sidecar/scripts/github-app-git.sh: installation token must not be passed through git URL, argv, or environment token value"
      );
    }
  }
  for (const requiredFragment of [
    "GIT_ASKPASS",
    "GIT_TERMINAL_PROMPT=0",
    "token_file=",
    'cat "$token_file"',
    "/app/installations/${GITHUB_APP_INSTALLATION_ID}/access_tokens",
    '.permissions.contents == "read" or .permissions.contents == "write"',
  ]) {
    if (!githubAppGitScript.includes(requiredFragment)) {
      addError(
        `deploy/sidecar/scripts/github-app-git.sh: must use GitHub App askpass git auth (${requiredFragment})`
      );
    }
  }
}

if (exists("deploy/sidecar/scripts/fetch-ci-artifact.sh")) {
  const artifactFetchScript = readText("deploy/sidecar/scripts/fetch-ci-artifact.sh");
  for (const unsafeFragment of [
    '-H "Authorization: Bearer',
    "-H 'Authorization: Bearer",
    '--header "Authorization: Bearer',
    "--header 'Authorization: Bearer",
  ]) {
    if (artifactFetchScript.includes(unsafeFragment)) {
      addError(
        "deploy/sidecar/scripts/fetch-ci-artifact.sh: GitHub token must not be passed through curl argv headers"
      );
    }
  }
  if (!artifactFetchScript.includes("curl_config=")) {
    addError(
      "deploy/sidecar/scripts/fetch-ci-artifact.sh: expected a curl config file for GitHub API headers"
    );
  }
  for (const requiredFragment of [
    "GITHUB_APP_ID",
    "GITHUB_APP_INSTALLATION_ID",
    "GITHUB_APP_PRIVATE_KEY_PATH",
    "/app/installations/${GITHUB_APP_INSTALLATION_ID}/access_tokens",
    "openssl",
    "jwt_path",
  ]) {
    if (!artifactFetchScript.includes(requiredFragment)) {
      addError(
        `deploy/sidecar/scripts/fetch-ci-artifact.sh: must support GitHub App credential path (${requiredFragment})`
      );
    }
  }
}

for (const cosScriptPath of [
  "deploy/sidecar/scripts/upload-cos-artifact.sh",
  "deploy/sidecar/scripts/fetch-cos-artifact.sh",
]) {
  if (exists(cosScriptPath)) {
    const script = readText(cosScriptPath);
    if (!script.includes("TENCENT_COS_BUCKET")) {
      addError(`${cosScriptPath}: must use explicit Tencent COS bucket configuration`);
    }
    if (!script.includes('touch "$config_path"')) {
      addError(
        `${cosScriptPath}: must create the temporary COSCLI config file before config add/set`
      );
    }
    const cpCommands = script.matchAll(
      /\b(?:run_coscli\s+"[^"]+"\s+)?cp\s+[\s\S]*?(?=\n(?:done|echo|mkdir|test|\(|[a-zA-Z0-9_]+\(|if\b|for\b)|$)/g
    );
    for (const [cpCommand] of cpCommands) {
      if (cpCommand.includes('"$TENCENT_COS_SECRET_ID"')) {
        addError(
          `${cosScriptPath}: COS SecretId must not be passed through coscli cp arguments`
        );
      }
      if (cpCommand.includes('"$TENCENT_COS_SECRET_KEY"')) {
        addError(
          `${cosScriptPath}: COS SecretKey must not be passed through coscli cp arguments`
        );
      }
      if (
        cpCommand.includes('"${config_auth_args[@]}"') ||
        cpCommand.includes('"${auth_args[@]}"')
      ) {
        addError(
          `${cosScriptPath}: COS transfer commands must use temporary config without auth argument arrays`
        );
      }
    }
  }
}

if (exists("deploy/sidecar/scripts/fetch-cos-artifact.sh")) {
  const cosFetchScript = readText("deploy/sidecar/scripts/fetch-cos-artifact.sh");
  for (const requiredFragment of [
    "TENCENT_COS_AUTH_MODE=CvmRole",
    "artifact-manifest.json",
    "SHA256SUMS",
    "sha256sum -c SHA256SUMS",
  ]) {
    if (!cosFetchScript.includes(requiredFragment)) {
      addError(
        `deploy/sidecar/scripts/fetch-cos-artifact.sh: must verify COS artifact downloads (${requiredFragment})`
      );
    }
  }
}

if (exists("deploy/sidecar/scripts/install-coscli.sh")) {
  const cosInstallScript = readText("deploy/sidecar/scripts/install-coscli.sh");
  if (!cosInstallScript.includes("sha256sum -c - >/dev/null")) {
    addError(
      "deploy/sidecar/scripts/install-coscli.sh: stdout must contain only the installed coscli path"
    );
  }
}

if (exists("deploy/sidecar/scripts/postgres-schema-preflight.sh")) {
  const schemaPreflightScript = readText(
    "deploy/sidecar/scripts/postgres-schema-preflight.sh"
  );
  for (const requiredFragment of [
    "qintopia_agent_os.work_item_events",
    "qintopia_agent_os.capabilities",
    "2026-06-30.007",
    "2026-07-02.001",
    "PGHOST",
    "PGDATABASE",
  ]) {
    if (!schemaPreflightScript.includes(requiredFragment)) {
      addError(
        `deploy/sidecar/scripts/postgres-schema-preflight.sh: must check ${requiredFragment}`
      );
    }
  }
  if (schemaPreflightScript.includes('psql "$database_url"')) {
    addError(
      "deploy/sidecar/scripts/postgres-schema-preflight.sh: database URL must not be passed through psql argv"
    );
  }
}

if (exists("deploy/sidecar/scripts/render-systemd-units.sh")) {
  const systemdRenderScript = readText(
    "deploy/sidecar/scripts/render-systemd-units.sh"
  );
  for (const requiredFragment of [
    'MIGRATIONS_DIR="${QINTOPIA_SIDECAR_MIGRATIONS_DIR:-${MONOREPO_DIR}/runtime/postgres/migrations}"',
    "--migrations-dir",
    "Environment=QINTOPIA_SIDECAR_MIGRATIONS_DIR=${MIGRATIONS_DIR}",
    'grep -F "Environment=QINTOPIA_SIDECAR_MIGRATIONS_DIR=${MIGRATIONS_DIR}"',
  ]) {
    if (!systemdRenderScript.includes(requiredFragment)) {
      addError(
        `deploy/sidecar/scripts/render-systemd-units.sh: must keep migrations env in rendered systemd units (${requiredFragment})`
      );
    }
  }
}

if (exists("deploy/sidecar/scripts/hermes/qintopia-context-mcp")) {
  const mcpContextWrapper = readText(
    "deploy/sidecar/scripts/hermes/qintopia-context-mcp"
  );
  for (const requiredFragment of [
    "QINTOPIA_DEPLOYED_COMMIT_SHA",
    "QINTOPIA_SIDECAR_BIN",
    "/home/ubuntu/qintopia-agent-os-artifacts",
    "/home/ubuntu/qintopia-agent-os-releases/current",
  ]) {
    if (!mcpContextWrapper.includes(requiredFragment)) {
      addError(
        `deploy/sidecar/scripts/hermes/qintopia-context-mcp: must support M9-F artifact/release path (${requiredFragment})`
      );
    }
  }
  if (mcpContextWrapper.includes("/home/ubuntu/qintopia-msg-sidecar")) {
    addError(
      "deploy/sidecar/scripts/hermes/qintopia-context-mcp: must not default to the legacy standalone checkout"
    );
  }
}

const m9Runbook = exists("docs/operations/m9-server-cutover-runbook.md")
  ? readText("docs/operations/m9-server-cutover-runbook.md")
  : "";
if (
  m9Runbook &&
  !m9Runbook.includes("deploy/sidecar/scripts/postgres-schema-preflight.sh")
) {
  addError(
    "docs/operations/m9-server-cutover-runbook.md: must include Postgres schema preflight"
  );
}
if (m9Runbook) {
  for (const requiredFragment of [
    "Tencent COS",
    "deploy/sidecar/scripts/fetch-cos-artifact.sh",
    "TENCENT_COS_BUCKET",
    "QINTOPIA_SIDECAR_MIGRATIONS_DIR",
    "M9-D cut over the approved active service family",
  ]) {
    if (!m9Runbook.includes(requiredFragment)) {
      addError(
        `docs/operations/m9-server-cutover-runbook.md: must document COS artifact download (${requiredFragment})`
      );
    }
  }
}

const artifactDoc = exists("docs/operations/sidecar-ci-artifacts.md")
  ? readText("docs/operations/sidecar-ci-artifacts.md")
  : "";
if (artifactDoc) {
  for (const requiredFragment of [
    "COS Distribution",
    "TENCENT_COS_BUCKET",
    "deploy/sidecar/scripts/fetch-cos-artifact.sh",
    "GitHub Artifact Fallback",
    "fetch-ci-artifact.sh",
  ]) {
    if (!artifactDoc.includes(requiredFragment)) {
      addError(
        `docs/operations/sidecar-ci-artifacts.md: must document COS artifact distribution (${requiredFragment})`
      );
    }
  }
}

const serverPolicy = exists("docs/engineering/server-change-policy.md")
  ? readText("docs/engineering/server-change-policy.md").toLowerCase()
  : "";
for (const phrase of [
  "approved commit sha",
  "smoke check",
  "rollback",
  "server is a deployment target",
  "scp",
]) {
  if (!serverPolicy.includes(phrase)) {
    addError(`docs/engineering/server-change-policy.md: must mention ${phrase}`);
  }
}

if (exists("deploy/sidecar/manifest.yaml")) {
  const deployManifest = readYaml("deploy/sidecar/manifest.yaml");
  if (!deployManifest.tags?.includes("legacy-snapshot")) {
    addError("deploy/sidecar/manifest.yaml: legacy deploy snapshot tag is required");
  }
  if (
    !deployManifest.validation?.commands?.some((command) => command.includes("pnpm"))
  ) {
    addError(
      "deploy/sidecar/manifest.yaml: validation commands must include pnpm gates"
    );
  }
}

const ciWorkflow = exists(".github/workflows/ci.yml")
  ? readText(".github/workflows/ci.yml")
  : "";
for (const phrase of [
  "sidecar-artifact",
  'NODE_VERSION: "24"',
  "pnpm/action-setup@v6",
  "actions/checkout@v7",
  "actions/setup-node@v6",
  "actions/setup-python@v6",
  "actions/upload-artifact@v7",
  "deploy/sidecar/scripts/upload-cos-artifact.sh",
  "qintopia-agent-os-artifacts-1305166808",
  "ap-shanghai",
  "env.TENCENT_COS_BUCKET",
  "env.TENCENT_COS_REGION",
  "secrets.TENCENT_COS_SECRET_ID",
  "actions: write",
  "concurrency:",
  "cancel-in-progress: true",
  "github.event_name == 'push' && github.ref == 'refs/heads/master'",
  "node tools/deploy/prune-github-artifacts.mjs",
  "retention-days: 14",
  "qintopia-message-sidecar-linux-x86_64-gnu",
  "dtolnay/rust-toolchain@1.75.0",
  "components: rustfmt",
]) {
  if (!ciWorkflow.includes(phrase)) {
    addError(`.github/workflows/ci.yml: must include ${phrase}`);
  }
}

if (ciWorkflow) {
  const ciWorkflowYaml = readYaml(".github/workflows/ci.yml");
  for (const [jobName, job] of Object.entries(ciWorkflowYaml.jobs ?? {})) {
    for (const [stepIndex, step] of (job?.steps ?? []).entries()) {
      if (String(step?.if ?? "").includes("secrets.")) {
        addError(
          `.github/workflows/ci.yml: jobs.${jobName}.steps[${stepIndex}].if must use env instead of secrets`
        );
      }
    }
  }
}

if (!ciMode) {
  let branch = "";
  try {
    branch = git(["branch", "--show-current"]);
  } catch {
    addError("git branch check failed");
  }
  if (branch !== "master") {
    addError(
      `deploy preflight must run from master; current branch is ${branch || "unknown"}`
    );
  }

  let status = "";
  try {
    status = git(["status", "--short"]);
  } catch {
    addError("git status check failed");
  }
  if (status) {
    addError("deploy preflight requires a clean worktree");
  }
}

if (errors.length > 0) {
  console.error(
    ciMode ? "Deploy preflight CI gate failed:" : "Deploy preflight failed:"
  );
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log(ciMode ? "Deploy preflight CI gate passed." : "Deploy preflight passed.");
