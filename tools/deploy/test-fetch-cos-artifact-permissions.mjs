#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "qintopia-cos-fetch-mode-"));
const fixtureRoot = path.join(tmpRoot, "fixture");
const outputRoot = path.join(tmpRoot, "output");
const deployBundleOutputRoot = path.join(tmpRoot, "deploy-bundle-output");
const binRoot = path.join(tmpRoot, "bin");
const tarArgsPath = path.join(tmpRoot, "tar-args.log");
const sha = "0123456789abcdef0123456789abcdef01234567";
const artifactName = "qintopia-message-sidecar-linux-x86_64-gnu";
const deployBundleArtifactName = "qintopia-agent-os-deploy-bundle";
const systemTar = "/usr/bin/tar";

const sha256File = (filePath) => {
  const hash = crypto.createHash("sha256");
  hash.update(fs.readFileSync(filePath));
  return hash.digest("hex");
};

const requireMode = (filePath, expected) => {
  const actual = fs.statSync(filePath).mode & 0o777;
  if (actual !== expected) {
    throw new Error(
      `${path.basename(filePath)} mode ${actual.toString(8)} != ${expected.toString(8)}`
    );
  }
};

try {
  if (!fs.existsSync(systemTar)) {
    throw new Error(`system tar is missing: ${systemTar}`);
  }
  fs.mkdirSync(fixtureRoot, { recursive: true });
  const binaryPath = path.join(fixtureRoot, "qintopia-message-sidecar");
  fs.writeFileSync(binaryPath, "#!/usr/bin/env bash\nexit 0\n", "utf8");
  fs.chmodSync(binaryPath, 0o755);

  const archivePath = path.join(fixtureRoot, "qintopia-message-sidecar.tar.gz");
  const tarResult = spawnSync(
    systemTar,
    ["-czf", archivePath, "-C", fixtureRoot, "qintopia-message-sidecar"],
    { encoding: "utf8" }
  );
  if (tarResult.status !== 0) {
    throw new Error(`failed to build fixture archive: ${tarResult.stderr}`);
  }

  const manifestPath = path.join(fixtureRoot, "artifact-manifest.json");
  fs.writeFileSync(
    manifestPath,
    `${JSON.stringify(
      {
        commit_sha: sha,
        artifact_name: artifactName,
        target: "linux-x86_64-gnu",
        files: [
          {
            path: "qintopia-message-sidecar",
            sha256: sha256File(binaryPath),
          },
        ],
        validation: {
          cargo_features: [
            "huabaosi-production-adapter",
            "huabaosi-feishu-mirror-adapter",
          ],
        },
      },
      null,
      2
    )}\n`,
    "utf8"
  );

  fs.writeFileSync(
    path.join(fixtureRoot, "SHA256SUMS"),
    [
      `${sha256File(binaryPath)}  qintopia-message-sidecar`,
      `${sha256File(archivePath)}  qintopia-message-sidecar.tar.gz`,
      `${sha256File(manifestPath)}  artifact-manifest.json`,
      "",
    ].join("\n"),
    "utf8"
  );

  const fakeCoscli = path.join(tmpRoot, "coscli");
  fs.writeFileSync(
    fakeCoscli,
    `#!/usr/bin/env bash
set -euo pipefail
case "\${1:-}" in
  config)
    exit 0
    ;;
  cp)
    source="\${2:-}"
    destination="\${3:-}"
    /bin/cp "\${FIXTURE_ROOT}/\${source##*/}" "$destination"
    ;;
  *)
    exit 64
    ;;
esac
`,
    "utf8"
  );
  fs.chmodSync(fakeCoscli, 0o755);

  fs.mkdirSync(binRoot, { recursive: true });
  const fakeTar = path.join(binRoot, "tar");
  fs.writeFileSync(
    fakeTar,
    `#!/usr/bin/env bash
set -euo pipefail
printf '%s\\n' "$*" >> "$TAR_ARGS_PATH"
exec ${JSON.stringify(systemTar)} "$@"
`,
    "utf8"
  );
  fs.chmodSync(fakeTar, 0o755);

  const fetchEnv = {
    ...process.env,
    COSCLI_PATH: fakeCoscli,
    FIXTURE_ROOT: fixtureRoot,
    PATH: `${binRoot}${path.delimiter}${process.env.PATH ?? ""}`,
    TAR_ARGS_PATH: tarArgsPath,
    TENCENT_COS_AUTH_MODE: "CvmRole",
    TENCENT_COS_BUCKET: "fixture-bucket-1234567890",
    TENCENT_COS_CVM_ROLE_NAME: "fixture-role",
    TENCENT_COS_REGION: "ap-shanghai",
  };

  const result = spawnSync(
    "bash",
    [
      "deploy/sidecar/scripts/fetch-cos-artifact.sh",
      "--artifact-type",
      "sidecar",
      "--sha",
      sha,
      "--output-dir",
      outputRoot,
    ],
    {
      cwd: repoRoot,
      env: {
        ...fetchEnv,
        ARTIFACT_NAME: artifactName,
        ARTIFACT_TARGET: "linux-x86_64-gnu",
      },
      encoding: "utf8",
    }
  );
  if (result.status !== 0) {
    throw new Error(`COS fetch fixture failed\n${result.stdout}\n${result.stderr}`);
  }

  requireMode(path.join(outputRoot, "qintopia-message-sidecar"), 0o755);
  requireMode(path.join(outputRoot, "artifact-manifest.json"), 0o444);
  requireMode(path.join(outputRoot, "SHA256SUMS"), 0o444);
  requireMode(path.join(outputRoot, "qintopia-message-sidecar.tar.gz"), 0o444);

  const qiweFeatureOutputRoot = path.join(tmpRoot, "qiwe-feature-output");
  fs.writeFileSync(
    manifestPath,
    `${JSON.stringify(
      {
        commit_sha: sha,
        artifact_name: artifactName,
        target: "linux-x86_64-gnu",
        files: [
          {
            path: "qintopia-message-sidecar",
            sha256: sha256File(binaryPath),
          },
        ],
        validation: {
          cargo_features: [
            "huabaosi-production-adapter",
            "huabaosi-feishu-mirror-adapter",
            "qiwe-production-adapter",
          ],
        },
      },
      null,
      2
    )}\n`,
    "utf8"
  );
  fs.writeFileSync(
    path.join(fixtureRoot, "SHA256SUMS"),
    [
      `${sha256File(binaryPath)}  qintopia-message-sidecar`,
      `${sha256File(archivePath)}  qintopia-message-sidecar.tar.gz`,
      `${sha256File(manifestPath)}  artifact-manifest.json`,
      "",
    ].join("\n"),
    "utf8"
  );
  const qiweFeatureResult = spawnSync(
    "bash",
    [
      "deploy/sidecar/scripts/fetch-cos-artifact.sh",
      "--artifact-type",
      "sidecar",
      "--sha",
      sha,
      "--output-dir",
      qiweFeatureOutputRoot,
    ],
    {
      cwd: repoRoot,
      env: {
        ...fetchEnv,
        ARTIFACT_NAME: artifactName,
        ARTIFACT_TARGET: "linux-x86_64-gnu",
      },
      encoding: "utf8",
    }
  );
  if (
    qiweFeatureResult.status === 0 ||
    !`${qiweFeatureResult.stdout}\n${qiweFeatureResult.stderr}`.includes(
      "artifact manifest Cargo features are not approved for production"
    )
  ) {
    throw new Error("COS fetch accepted a QiWe-enabled Huabaosi production artifact");
  }

  const payloadRoot = path.join(fixtureRoot, "payload");
  const contextMcpPath = path.join(
    payloadRoot,
    "deploy/sidecar/scripts/hermes/qintopia-context-mcp"
  );
  const rendererPath = path.join(
    payloadRoot,
    "deploy/sidecar/scripts/render-systemd-units.sh"
  );
  fs.mkdirSync(path.dirname(contextMcpPath), { recursive: true });
  fs.writeFileSync(contextMcpPath, "#!/usr/bin/env bash\nexit 0\n", "utf8");
  fs.mkdirSync(path.dirname(rendererPath), { recursive: true });
  fs.writeFileSync(rendererPath, "#!/usr/bin/env bash\nexit 0\n", "utf8");
  fs.chmodSync(contextMcpPath, 0o755);
  fs.chmodSync(rendererPath, 0o755);

  const deployBundleArchivePath = path.join(
    fixtureRoot,
    "qintopia-agent-os-deploy-bundle.tar.gz"
  );
  const deployBundleTarResult = spawnSync(
    systemTar,
    ["-czf", deployBundleArchivePath, "-C", fixtureRoot, "payload"],
    { encoding: "utf8" }
  );
  if (deployBundleTarResult.status !== 0) {
    throw new Error(
      `failed to build deploy bundle fixture archive: ${deployBundleTarResult.stderr}`
    );
  }

  fs.writeFileSync(
    manifestPath,
    `${JSON.stringify(
      {
        commit_sha: sha,
        artifact_name: deployBundleArtifactName,
        target: "server-operator-files",
        files: [
          {
            path: "qintopia-agent-os-deploy-bundle.tar.gz",
            sha256: sha256File(deployBundleArchivePath),
          },
        ],
      },
      null,
      2
    )}\n`,
    "utf8"
  );
  fs.writeFileSync(
    path.join(fixtureRoot, "SHA256SUMS"),
    [
      `${sha256File(deployBundleArchivePath)}  qintopia-agent-os-deploy-bundle.tar.gz`,
      `${sha256File(manifestPath)}  artifact-manifest.json`,
      "",
    ].join("\n"),
    "utf8"
  );

  const deployBundleResult = spawnSync(
    "bash",
    [
      "deploy/sidecar/scripts/fetch-cos-artifact.sh",
      "--artifact-type",
      "deploy-bundle",
      "--sha",
      sha,
      "--output-dir",
      deployBundleOutputRoot,
    ],
    {
      cwd: repoRoot,
      env: {
        ...fetchEnv,
        ARTIFACT_NAME: deployBundleArtifactName,
        ARTIFACT_TARGET: "server-operator-files",
      },
      encoding: "utf8",
    }
  );
  if (deployBundleResult.status !== 0) {
    throw new Error(
      `COS deploy bundle fetch fixture failed\n${deployBundleResult.stdout}\n${deployBundleResult.stderr}`
    );
  }

  requireMode(
    path.join(deployBundleOutputRoot, "qintopia-agent-os-deploy-bundle.tar.gz"),
    0o444
  );
  requireMode(path.join(deployBundleOutputRoot, "artifact-manifest.json"), 0o444);
  requireMode(path.join(deployBundleOutputRoot, "SHA256SUMS"), 0o444);
  requireMode(
    path.join(
      deployBundleOutputRoot,
      "payload/deploy/sidecar/scripts/hermes/qintopia-context-mcp"
    ),
    0o755
  );
  requireMode(
    path.join(
      deployBundleOutputRoot,
      "payload/deploy/sidecar/scripts/render-systemd-units.sh"
    ),
    0o755
  );

  const tarInvocations = fs.readFileSync(tarArgsPath, "utf8").trim().split("\n");
  if (
    tarInvocations.length !== 3 ||
    tarInvocations.some((args) => !args.includes("--no-same-owner"))
  ) {
    throw new Error(
      `COS fetch extraction must use --no-same-owner for each extraction, got ${JSON.stringify(tarInvocations)}`
    );
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("COS artifact fetch permission test passed.");
