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
const sha = "0123456789abcdef0123456789abcdef01234567";
const artifactName = "qintopia-message-sidecar-linux-x86_64-gnu";

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
  fs.mkdirSync(fixtureRoot, { recursive: true });
  const binaryPath = path.join(fixtureRoot, "qintopia-message-sidecar");
  fs.writeFileSync(binaryPath, "#!/usr/bin/env bash\nexit 0\n", "utf8");
  fs.chmodSync(binaryPath, 0o755);

  const archivePath = path.join(fixtureRoot, "qintopia-message-sidecar.tar.gz");
  const tarResult = spawnSync(
    "tar",
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
        ...process.env,
        ARTIFACT_NAME: artifactName,
        ARTIFACT_TARGET: "linux-x86_64-gnu",
        COSCLI_PATH: fakeCoscli,
        FIXTURE_ROOT: fixtureRoot,
        TENCENT_COS_AUTH_MODE: "CvmRole",
        TENCENT_COS_BUCKET: "fixture-bucket-1234567890",
        TENCENT_COS_CVM_ROLE_NAME: "fixture-role",
        TENCENT_COS_REGION: "ap-shanghai",
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
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("COS artifact fetch permission test passed.");
