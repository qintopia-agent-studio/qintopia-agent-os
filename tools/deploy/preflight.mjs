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
  "agents:profile-bundles:check",
  "collaboration:check",
  "skills:postgres-context:check",
  "skills:feishu-base:check",
  "policy:check",
  "secrets:check",
  "deploy:preflight:ci",
  "deploy:github-app-git:check",
  "deploy:cos:check",
  "deploy:postgres:schema:preflight",
  "deploy:systemd:check",
  "deploy:release-model:check",
  "artifact:sidecar",
  "artifact:deploy-bundle",
  "artifact:prune:sidecar",
  "artifact:prune:deploy-bundle",
  "test:qiwe",
  "test:sidecar",
  "smoke:sidecar",
  "check:light",
  "check:runtime",
  "check",
];

const requiredDocs = [
  "docs/engineering/server-change-policy.md",
  "docs/engineering/programming-agent-guardrails.md",
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
  "deploy/sidecar/scripts/prune-cos-artifacts.sh",
  "deploy/sidecar/scripts/fetch-ci-artifact.sh",
  "deploy/sidecar/scripts/fetch-staging-sidecar-artifact.sh",
  "deploy/sidecar/scripts/staging-runtime-prerequisite-observation-smoke.sh",
  "deploy/sidecar/scripts/huabaosi-image-generation-staging-smoke.sh",
  "deploy/sidecar/scripts/huabaosi-image-generation-production-observation-smoke.sh",
  "deploy/sidecar/scripts/activate-huabaosi-image-generation-production.sh",
  "deploy/sidecar/scripts/rollback-huabaosi-image-generation-production.sh",
  "deploy/sidecar/scripts/huabaosi-feishu-artifact-mirror-production-observation-smoke.sh",
  "deploy/sidecar/scripts/activate-huabaosi-feishu-artifact-mirror-production.sh",
  "deploy/sidecar/scripts/rollback-huabaosi-feishu-artifact-mirror-production.sh",
  "deploy/sidecar/scripts/huabaosi-wecom-canary-observation-smoke.sh",
  "deploy/sidecar/scripts/huabaosi-wecom-gateway-observation-smoke.sh",
  "deploy/sidecar/scripts/xiaoman-activity-image-generation-starter-observation-smoke.sh",
  "deploy/sidecar/scripts/xiaoman-profile-bundle-observation-smoke.sh",
  "deploy/sidecar/scripts/postgres-schema-preflight.sh",
  "deploy/sidecar/scripts/render-systemd-units.sh",
  "deploy/sidecar/scripts/hermes/qintopia-context-mcp",
  "tools/deploy/check-release-model.mjs",
  "tools/policy/check-collaboration.mjs",
  "tools/deploy/build-deploy-bundle.mjs",
  "tools/skills/check-postgres-context.mjs",
  "tools/skills/check-feishu-base.mjs",
  "tools/agents/check-profile-bundles.mjs",
  "agents/xiaoman/profile-bundle/bundle.json",
  "agents/xiaoman/profile-bundle/migrate_values.py",
  "agents/xiaoman/profile-bundle/render.py",
  "agents/xiaoman/profile-bundle/templates/SOUL.md.template",
  "agents/xiaoman/profile-bundle/templates/profile.yaml.template",
  "docs/operations/profile-bundles/m10f-profile-template-plan.md",
  "docs/plans/active/xiaoman-profile-bundle-migration.md",
];

const requiredCheckFragments = ["pnpm check:light", "pnpm check:runtime"];

const requiredLightCheckFragments = [
  "pnpm format:check",
  "pnpm lint:md",
  "pnpm registry:check",
  "pnpm skills:postgres-context:check",
  "pnpm skills:feishu-base:check",
  "pnpm agents:check",
  "pnpm agents:profile-bundles:check",
  "pnpm collaboration:check",
  "pnpm policy:check",
  "pnpm secrets:check",
  "pnpm deploy:preflight:ci",
  "pnpm deploy:github-app-git:check",
  "pnpm deploy:cos:check",
  "pnpm deploy:systemd:check",
  "pnpm deploy:release-model:check",
];

const requiredRuntimeCheckFragments = [
  "pnpm test:qiwe",
  "pnpm fmt:sidecar",
  "pnpm check:sidecar",
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

const gitFileMode = (relativePath) => {
  try {
    const output = git(["ls-files", "-s", "--", relativePath]);
    return output.split(/\s+/)[0] || "";
  } catch {
    return "";
  }
};

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

for (const fragment of requiredLightCheckFragments) {
  if (!scripts["check:light"]?.includes(fragment)) {
    addError(`package.json: check:light script must include '${fragment}'`);
  }
}

for (const fragment of requiredRuntimeCheckFragments) {
  if (!scripts["check:runtime"]?.includes(fragment)) {
    addError(`package.json: check:runtime script must include '${fragment}'`);
  }
}

for (const docPath of requiredDocs) {
  if (!exists(docPath)) {
    addError(`${docPath}: required deploy gate document is missing`);
  }
}

for (const scriptPath of [
  "deploy/sidecar/scripts/upload-cos-artifact.sh",
  "deploy/sidecar/scripts/prune-cos-artifacts.sh",
  "deploy/sidecar/scripts/fetch-cos-artifact.sh",
  "deploy/sidecar/scripts/fetch-ci-artifact.sh",
  "deploy/sidecar/scripts/huabaosi-image-generation-staging-readiness-smoke.sh",
  "deploy/sidecar/scripts/huabaosi-image-generation-staging-smoke.sh",
  "deploy/sidecar/scripts/huabaosi-image-generation-production-observation-smoke.sh",
  "deploy/sidecar/scripts/activate-huabaosi-image-generation-production.sh",
  "deploy/sidecar/scripts/rollback-huabaosi-image-generation-production.sh",
  "deploy/sidecar/scripts/huabaosi-feishu-artifact-mirror-production-observation-smoke.sh",
  "deploy/sidecar/scripts/activate-huabaosi-feishu-artifact-mirror-production.sh",
  "deploy/sidecar/scripts/rollback-huabaosi-feishu-artifact-mirror-production.sh",
  "deploy/sidecar/scripts/huabaosi-wecom-canary-observation-smoke.sh",
  "deploy/sidecar/scripts/huabaosi-wecom-gateway-observation-smoke.sh",
  "deploy/sidecar/scripts/xiaoman-activity-image-generation-starter-observation-smoke.sh",
  "deploy/sidecar/scripts/xiaoman-profile-bundle-observation-smoke.sh",
  "deploy/sidecar/scripts/github-app-git.sh",
  "deploy/sidecar/scripts/postgres-schema-preflight.sh",
  "deploy/sidecar/scripts/render-systemd-units.sh",
]) {
  if (exists(scriptPath) && gitFileMode(scriptPath) !== "100755") {
    addError(`${scriptPath}: must be committed with executable file mode 100755`);
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
    "huabaosi-production-adapter",
  ]) {
    if (!artifactFetchScript.includes(requiredFragment)) {
      addError(
        `deploy/sidecar/scripts/fetch-ci-artifact.sh: must support GitHub App credential path (${requiredFragment})`
      );
    }
  }
}

if (exists("deploy/sidecar/scripts/fetch-staging-sidecar-artifact.sh")) {
  const stagingArtifactFetchScript = readText(
    "deploy/sidecar/scripts/fetch-staging-sidecar-artifact.sh"
  );
  for (const requiredFragment of [
    "QINTOPIA_STAGING_SIDECAR_PROVISION_APPROVAL",
    "approved-staging-sidecar-provision",
    "qintopia-message-sidecar-staging-linux-x86_64-gnu",
    "huabaosi-staging-adapter",
    "qiwe-staging-adapter",
    "staging_only",
    "production_eligible",
    "/home/ubuntu/qintopia-agent-os-staging-releases",
    "--artifact-zip is test-only",
    "sha256sum -c SHA256SUMS",
    "qintopia-message-sidecar.tar.gz",
    "path component is a symlink",
    "path component is group/world writable",
    "path component has unexpected owner",
    "chmod 0555",
  ]) {
    if (!stagingArtifactFetchScript.includes(requiredFragment)) {
      addError(
        `deploy/sidecar/scripts/fetch-staging-sidecar-artifact.sh: must preserve staging artifact provision boundary (${requiredFragment})`
      );
    }
  }
  for (const forbiddenFragment of [
    "huabaosi-production-adapter",
    "huabaosi-feishu-mirror-adapter",
    "systemctl enable",
    "systemctl start",
    "gh release",
  ]) {
    if (stagingArtifactFetchScript.includes(forbiddenFragment)) {
      addError(
        `deploy/sidecar/scripts/fetch-staging-sidecar-artifact.sh: must not cross production or release boundaries (${forbiddenFragment})`
      );
    }
  }
}

for (const cosScriptPath of [
  "deploy/sidecar/scripts/upload-cos-artifact.sh",
  "deploy/sidecar/scripts/fetch-cos-artifact.sh",
  "deploy/sidecar/scripts/prune-cos-artifacts.sh",
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
    if (!script.includes("config set")) {
      addError(
        `${cosScriptPath}: must write authentication mode into the temporary COSCLI config with config set`
      );
    }
    for (const timeoutFragment of [
      "COSCLI_CONFIG_TIMEOUT_SECONDS",
      "COSCLI_TRANSFER_TIMEOUT_SECONDS",
      "COSCLI timed out after",
    ]) {
      if (!script.includes(timeoutFragment)) {
        addError(
          `${cosScriptPath}: must enforce bounded COSCLI execution (${timeoutFragment})`
        );
      }
    }
    if (
      cosScriptPath === "deploy/sidecar/scripts/upload-cos-artifact.sh" &&
      (!script.includes("COSCLI_PART_SIZE_MB") ||
        !script.includes("COSCLI_THREAD_NUM") ||
        !script.includes("--part-size") ||
        !script.includes("--thread-num"))
    ) {
      addError(
        `${cosScriptPath}: must tune COSCLI uploads for small release artifacts with multipart concurrency`
      );
    }
    if (
      cosScriptPath !== "deploy/sidecar/scripts/prune-cos-artifacts.sh" &&
      !script.includes("TENCENT_COS_ARTIFACT_PAYLOAD")
    ) {
      addError(`${cosScriptPath}: must support explicit COS artifact payload mode`);
    }
    for (const endpointFragment of [
      "TENCENT_COS_ENDPOINT",
      'bucket_config_args+=(-e "$TENCENT_COS_ENDPOINT")',
    ]) {
      if (!script.includes(endpointFragment)) {
        addError(
          `${cosScriptPath}: must support optional Tencent COS endpoint configuration (${endpointFragment})`
        );
      }
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
    "qintopia-message-sidecar.tar.gz",
    'tar -xzf "${output_dir}/qintopia-message-sidecar.tar.gz" -C "$output_dir"',
    "qintopia-message-sidecar",
    "huabaosi-production-adapter",
    "sha256sum -c SHA256SUMS",
  ]) {
    if (!cosFetchScript.includes(requiredFragment)) {
      addError(
        `deploy/sidecar/scripts/fetch-cos-artifact.sh: must verify COS artifact downloads (${requiredFragment})`
      );
    }
  }
}

if (exists("deploy/sidecar/scripts/prune-cos-artifacts.sh")) {
  const cosPruneScript = readText("deploy/sidecar/scripts/prune-cos-artifacts.sh");
  for (const requiredFragment of [
    "QINTOPIA_COS_ARTIFACT_KEEP_COUNT",
    "artifact-manifest",
    "cos://${bucket_alias}/${artifact_prefix}/",
    'run_coscli_capture "delete COS ${artifact_type} artifact',
    "HeadBucket and GetBucket permissions",
    "DeleteMultipleObjects",
    "-r",
    "-f",
    "--dry-run",
  ]) {
    if (!cosPruneScript.includes(requiredFragment)) {
      addError(
        `deploy/sidecar/scripts/prune-cos-artifacts.sh: must support bounded COS artifact retention (${requiredFragment})`
      );
    }
  }
  if (cosPruneScript.includes("--include")) {
    addError(
      "deploy/sidecar/scripts/prune-cos-artifacts.sh: must not depend on COSCLI --include filtering for retention discovery"
    );
  }
}

if (exists("tools/deploy/build-sidecar-artifact.mjs")) {
  const buildArtifactScript = readText("tools/deploy/build-sidecar-artifact.mjs");
  const approvedCargoFeatures = [
    "huabaosi-production-adapter",
    "huabaosi-feishu-mirror-adapter",
  ];
  const cargoFeaturesMatch = buildArtifactScript.match(
    /const cargoFeatures = \[([\s\S]*?)\];/
  );
  if (!cargoFeaturesMatch) {
    addError(
      "tools/deploy/build-sidecar-artifact.mjs: missing literal production cargoFeatures array"
    );
  } else {
    const cargoFeaturesSource = cargoFeaturesMatch[1];
    const cargoFeatures = [...cargoFeaturesSource.matchAll(/"([a-z0-9-]+)"/g)].map(
      (match) => match[1]
    );
    const cargoFeaturesResidue = cargoFeaturesSource
      .replace(/"[a-z0-9-]+"/g, "")
      .replace(/[\s,]/g, "");
    if (
      cargoFeaturesResidue ||
      JSON.stringify(cargoFeatures) !== JSON.stringify(approvedCargoFeatures)
    ) {
      addError(
        "tools/deploy/build-sidecar-artifact.mjs: production cargoFeatures must exactly match the approved feature list"
      );
    }
  }
  for (const requiredFragment of [
    "assertContainedArtifactDirBoundary",
    "resolveApprovedTarget",
    "resolveContainedArtifactDir",
    "const bundleName = `${binaryName}.tar.gz`",
    'gitOutput(["status", "--porcelain"], "unknown")',
    "refusing to build a release artifact from a dirty or unreadable git worktree",
    'run("tar", ["-C", artifactDir, "-czf", bundlePath, binaryName])',
    "bundleSha256",
    "manifestSha256",
    "`${bundleSha256}  ${bundleName}`",
    "`${manifestSha256}  artifact-manifest.json`",
    "cargo_features: cargoFeatures",
    '"--features"',
    'cargoFeatures.join(",")',
  ]) {
    if (!buildArtifactScript.includes(requiredFragment)) {
      addError(
        `tools/deploy/build-sidecar-artifact.mjs: must include compressed sidecar bundle support (${requiredFragment})`
      );
    }
  }
  for (const forbiddenFragment of [
    "huabaosi-staging-adapter",
    "qiwe-staging-adapter",
    '"--all-features"',
  ]) {
    if (buildArtifactScript.includes(forbiddenFragment)) {
      addError(
        `tools/deploy/build-sidecar-artifact.mjs: production sidecar artifacts must use only the reviewed production features (${forbiddenFragment})`
      );
    }
  }
}

if (exists("tools/deploy/build-staging-sidecar-artifact.mjs")) {
  const stagingArtifactScript = readText(
    "tools/deploy/build-staging-sidecar-artifact.mjs"
  );
  const approvedStagingCargoFeatures = [
    "huabaosi-staging-adapter",
    "qiwe-staging-adapter",
  ];
  const cargoFeaturesMatch = stagingArtifactScript.match(
    /const cargoFeatures = \[([\s\S]*?)\];/
  );
  if (!cargoFeaturesMatch) {
    addError(
      "tools/deploy/build-staging-sidecar-artifact.mjs: missing literal staging cargoFeatures array"
    );
  } else {
    const cargoFeaturesSource = cargoFeaturesMatch[1];
    const cargoFeatures = [...cargoFeaturesSource.matchAll(/"([a-z0-9-]+)"/g)].map(
      (match) => match[1]
    );
    const cargoFeaturesResidue = cargoFeaturesSource
      .replace(/"[a-z0-9-]+"/g, "")
      .replace(/[\s,]/g, "");
    if (
      cargoFeaturesResidue ||
      JSON.stringify(cargoFeatures) !== JSON.stringify(approvedStagingCargoFeatures)
    ) {
      addError(
        "tools/deploy/build-staging-sidecar-artifact.mjs: staging cargoFeatures must exactly match the approved feature list"
      );
    }
  }
  for (const requiredFragment of [
    "assertContainedArtifactDirBoundary",
    "resolveApprovedTarget",
    "resolveContainedArtifactDir",
    "staging-${targetTriple}",
    '"--no-default-features"',
    '"--features"',
    'cargoFeatures.join(",")',
    "manifestSha256",
    "`${bundleSha256}  ${bundleName}`",
    "`${manifestSha256}  artifact-manifest.json`",
    "staging_only: true",
    "production_eligible: false",
    "refusing to build a staging artifact from a dirty or unreadable git worktree",
    "/home/ubuntu/qintopia-agent-os-staging-releases/<approved 40-hex sha>",
  ]) {
    if (!stagingArtifactScript.includes(requiredFragment)) {
      addError(
        `tools/deploy/build-staging-sidecar-artifact.mjs: must preserve staging artifact boundary (${requiredFragment})`
      );
    }
  }
  for (const forbiddenFragment of [
    "huabaosi-production-adapter",
    "huabaosi-feishu-mirror-adapter",
    '"--all-features"',
  ]) {
    if (stagingArtifactScript.includes(forbiddenFragment)) {
      addError(
        `tools/deploy/build-staging-sidecar-artifact.mjs: staging artifacts must not use production or all features (${forbiddenFragment})`
      );
    }
  }
}

if (exists("tools/deploy/sidecar-artifact-build-boundary.mjs")) {
  const helper = readText("tools/deploy/sidecar-artifact-build-boundary.mjs");
  for (const requiredFragment of [
    'const approvedTarget = "linux-x86_64-gnu"',
    "artifactNamePattern.test(artifactName)",
    "QINTOPIA_ARTIFACT_TARGET must be",
    'platform !== "linux"',
    'arch !== "x64"',
    "glibcVersionRuntime",
    "linux x64 GNU runners",
    'artifactName.includes("/")',
    'artifactName.includes("\\\\")',
    'artifactName.split("-").includes("..")',
    "fs.lstatSync(currentPath)",
    "stat.isSymbolicLink()",
    "fs.mkdirSync(resolvedRoot, { recursive: true })",
    "fs.realpathSync.native(currentPath)",
    "artifact output path must match its real path",
    "requireTerminalDirectory",
    "artifact output root must be a directory",
    "path.resolve(outputRoot)",
    "!resolvedDir.startsWith(`${resolvedRoot}${path.sep}`)",
  ]) {
    if (!helper.includes(requiredFragment)) {
      addError(
        `tools/deploy/sidecar-artifact-build-boundary.mjs: must preserve artifact path and platform safety (${requiredFragment})`
      );
    }
  }
}

if (exists("deploy/sidecar/scripts/server-deploy.sh")) {
  const serverDeployScript = readText("deploy/sidecar/scripts/server-deploy.sh");
  for (const forbiddenFragment of [
    "huabaosi-staging-adapter",
    "qiwe-staging-adapter",
    "--features",
    "--all-features",
  ]) {
    if (serverDeployScript.includes(forbiddenFragment)) {
      addError(
        `deploy/sidecar/scripts/server-deploy.sh: production source builds must use default Cargo features (${forbiddenFragment})`
      );
    }
  }
}

if (exists("tools/deploy/build-deploy-bundle.mjs")) {
  const buildDeployBundleScript = readText("tools/deploy/build-deploy-bundle.mjs");
  for (const requiredFragment of [
    "qintopia-agent-os-deploy-bundle",
    "deploy/sidecar/scripts/hermes/qintopia-context-mcp",
    "deploy/sidecar/scripts/fetch-staging-sidecar-artifact.sh",
    "deploy/sidecar/scripts/huabaosi-image-generation-staging-readiness-smoke.sh",
    "deploy/sidecar/scripts/huabaosi-image-generation-staging-smoke.sh",
    "deploy/sidecar/scripts/activate-huabaosi-image-generation-production.sh",
    "deploy/sidecar/scripts/rollback-huabaosi-image-generation-production.sh",
    "deploy/sidecar/scripts/huabaosi-feishu-artifact-mirror-production-observation-smoke.sh",
    "deploy/sidecar/scripts/activate-huabaosi-feishu-artifact-mirror-production.sh",
    "deploy/sidecar/scripts/rollback-huabaosi-feishu-artifact-mirror-production.sh",
    "deploy/sidecar/scripts/huabaosi-wecom-canary-observation-smoke.sh",
    "deploy/sidecar/scripts/qiwe-image-send-staging-readiness-smoke.sh",
    "deploy/sidecar/scripts/render-systemd-units.sh",
    "runtime/postgres/migrations",
    "skills/qintopia-tools/variants",
    "skills/qintopia-tools/manifest.yaml",
    "skills/qintopia-weather/__init__.py",
    "skills/qintopia-weather/plugin.yaml",
    "skills/qintopia-weather/tests",
    "skills/knowledge-retrieval/__init__.py",
    "skills/knowledge-retrieval/plugin.yaml",
    "skills/knowledge-retrieval/tests",
    "mcp/weather-provider/manifest.yaml",
    "skills/qiwe/adapter.py",
    "skills/qiwe/image_callback_bridge.py",
    "skills/qiwe/plugin.yaml",
    "skills/qiwe/solitaire",
    "skills/feishu-base/__init__.py",
    "skills/feishu-base/plugin.yaml",
    "skills/feishu-base/docs",
    "artifact-manifest.json",
    "SHA256SUMS",
    'run("tar", ["-C", bundleDir, "-czf", archivePath, "payload"])',
  ]) {
    if (!buildDeployBundleScript.includes(requiredFragment)) {
      addError(
        `tools/deploy/build-deploy-bundle.mjs: must build the deploy bundle (${requiredFragment})`
      );
    }
  }
}

if (exists("skills/feishu-base/__init__.py")) {
  const feishuBasePlugin = readText("skills/feishu-base/__init__.py");
  for (const requiredFragment of [
    "QINTOPIA_BASE_READ_XIAOMAN_ACTIVITY_BASE_TOKEN",
    "QINTOPIA_BASE_READ_XIAOMAN_ACTIVITY_PLAN_TABLE_ID",
    "QINTOPIA_BASE_READ_HUABAOSI_DESIGN_BASE_TOKEN",
    "QINTOPIA_BASE_READ_HUABAOSI_POSTER_TABLE_ID",
    "FEISHU_APP_ID",
    "FEISHU_APP_SECRET",
    "required_env",
    "qintopia_xiaoman_activity_record_get",
    "qintopia_huabaosi_design_record_get",
  ]) {
    if (!feishuBasePlugin.includes(requiredFragment)) {
      addError(
        `skills/feishu-base/__init__.py: missing Huabaosi Base read guard (${requiredFragment})`
      );
    }
  }
  for (const forbiddenPattern of [
    /\bcli_[A-Za-z0-9_-]{20,}/,
    /\bbascn[A-Za-z0-9]+/,
    /\btbl[A-Za-z0-9]+/,
    /base_token"\s*:/,
    /table_id"\s*:/,
  ]) {
    if (forbiddenPattern.test(feishuBasePlugin)) {
      addError(
        "skills/feishu-base/__init__.py: must not commit Feishu app ids, Base ids, or echo source identifiers"
      );
    }
  }
}

if (exists("skills/qiwe/adapter.py")) {
  const qiweAdapter = readText("skills/qiwe/adapter.py");
  for (const forbiddenFragment of [
    "/home/ubuntu/qintopia-msg-sidecar/scripts/hermes/qintopia-context-mcp",
    "/home/ubuntu/qintopia-agent-os-monorepo/deploy/sidecar/scripts/hermes/qintopia-context-mcp",
  ]) {
    if (qiweAdapter.includes(forbiddenFragment)) {
      addError(
        `skills/qiwe/adapter.py: default context MCP command must not point to deprecated or diagnostic checkout path (${forbiddenFragment})`
      );
    }
  }
  if (
    !qiweAdapter.includes(
      "/home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/hermes/qintopia-context-mcp"
    )
  ) {
    addError(
      "skills/qiwe/adapter.py: default context MCP command must point through release/current"
    );
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
    "qintopia_agent_os.event_signal_mutations",
    "gap_summary",
    "activity_phase",
    "2026-06-30.007",
    "2026-07-02.001",
    "2026-07-13.002",
    "2026-07-14.001",
    "2026-07-15.001",
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
    "deploy/sidecar/scripts/prune-cos-artifacts.sh",
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
  "changes:",
  "full-check",
  "pnpm check:light",
  "pnpm check:runtime",
  "Docs-only or Markdown-only change detected.",
  'NODE_VERSION: "24"',
  "pnpm/action-setup@v6",
  "actions/checkout@v7",
  "actions/setup-node@v6",
  "actions/setup-python@v6",
  "concurrency:",
  "cancel-in-progress: true",
  "dtolnay/rust-toolchain@1.96.0",
  "components: rustfmt",
]) {
  if (!ciWorkflow.includes(phrase)) {
    addError(`.github/workflows/ci.yml: must include ${phrase}`);
  }
}

const artifactsWorkflow = exists(".github/workflows/artifacts.yml")
  ? readText(".github/workflows/artifacts.yml")
  : "";
for (const phrase of [
  "workflow_dispatch:",
  "build_sidecar:",
  "build_deploy_bundle:",
  "upload_cos:",
  "[publish-artifacts]",
  "sidecar-artifact",
  "deploy-bundle-artifact",
  "actions/upload-artifact@v7",
  "deploy/sidecar/scripts/upload-cos-artifact.sh",
  "deploy/sidecar/scripts/prune-cos-artifacts.sh",
  "qintopia-agent-os-deploy-bundle",
  "qintopia-agent-os-artifacts-1305166808",
  "ap-shanghai",
  "TENCENT_COS_UPLOAD_ENABLED",
  "TENCENT_COS_ENDPOINT",
  "env.TENCENT_COS_BUCKET",
  "env.TENCENT_COS_REGION",
  "secrets.TENCENT_COS_SECRET_ID",
  "actions: write",
  "node tools/deploy/prune-github-artifacts.mjs",
  "QINTOPIA_ARTIFACT_KEEP_COUNT",
  "QINTOPIA_COS_ARTIFACT_KEEP_COUNT",
  "qintopia-message-sidecar-linux-x86_64-gnu",
  "dtolnay/rust-toolchain@1.96.0",
]) {
  if (!artifactsWorkflow.includes(phrase)) {
    addError(`.github/workflows/artifacts.yml: must include ${phrase}`);
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

if (artifactsWorkflow) {
  const artifactsWorkflowYaml = readYaml(".github/workflows/artifacts.yml");
  for (const [jobName, job] of Object.entries(artifactsWorkflowYaml.jobs ?? {})) {
    for (const [stepIndex, step] of (job?.steps ?? []).entries()) {
      if (String(step?.if ?? "").includes("secrets.")) {
        addError(
          `.github/workflows/artifacts.yml: jobs.${jobName}.steps[${stepIndex}].if must use env instead of secrets`
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
