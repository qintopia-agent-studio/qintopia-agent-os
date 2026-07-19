#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "qintopia-same-sha-repair-"));
const fixtureRoot = path.join(tmpRoot, "fixtures");
const releaseRoot = path.join(tmpRoot, "releases");
const fakeBin = path.join(tmpRoot, "bin");
const chownLog = path.join(tmpRoot, "chown.log");
const sha = "0123456789abcdef0123456789abcdef01234567";

const sha256File = (filePath) => {
  const hash = crypto.createHash("sha256");
  hash.update(fs.readFileSync(filePath));
  return hash.digest("hex");
};

const writeFile = (filePath, content, mode = 0o644) => {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content, "utf8");
  fs.chmodSync(filePath, mode);
};

const writeChecksums = (directory, names) => {
  writeFile(
    path.join(directory, "SHA256SUMS"),
    `${names
      .map((name) => `${sha256File(path.join(directory, name))}  ${name}`)
      .join("\n")}\n`,
    0o444
  );
};

const writeRequest = (requestId) => {
  const requestPath = path.join(tmpRoot, `${requestId}.json`);
  writeFile(
    requestPath,
    `${JSON.stringify(
      {
        request_id: requestId,
        commit_sha: sha,
        runtime_sha: sha,
        deploy_bundle_sha: sha,
        release_sha: sha,
        release_scope: ["sidecar-runtime", "deploy-bundle", "hermes-plugins"],
        restart_targets: ["qintopia-system-services", "hermes-erhua"],
        dry_run: false,
      },
      null,
      2
    )}\n`
  );
  return requestPath;
};

const runPromotion = (requestPath) =>
  spawnSync(
    "bash",
    [
      path.join(repoRoot, "deploy/runner/promote-release.sh"),
      "--request-file",
      requestPath,
      "--release-root",
      releaseRoot,
    ],
    {
      cwd: tmpRoot,
      env: {
        ...process.env,
        CHOWN_LOG: chownLog,
        FIXTURE_ROOT: fixtureRoot,
        PATH: `${fakeBin}${path.delimiter}${process.env.PATH ?? ""}`,
      },
      encoding: "utf8",
    }
  );

const requireMode = (filePath, expected) => {
  const actual = fs.statSync(filePath).mode & 0o777;
  if (actual !== expected) {
    throw new Error(
      `${filePath} mode ${actual.toString(8)} != ${expected.toString(8)}`
    );
  }
};

try {
  const sidecarFixture = path.join(fixtureRoot, "sidecar");
  writeFile(
    path.join(sidecarFixture, "qintopia-message-sidecar"),
    "#!/usr/bin/env bash\nexit 0\n",
    0o755
  );
  writeFile(
    path.join(sidecarFixture, "qintopia-message-sidecar.tar.gz"),
    "sidecar archive fixture\n",
    0o444
  );
  writeFile(
    path.join(sidecarFixture, "artifact-manifest.json"),
    `${JSON.stringify({ commit_sha: sha, artifact_name: "sidecar-fixture" })}\n`,
    0o444
  );
  writeChecksums(sidecarFixture, [
    "qintopia-message-sidecar",
    "qintopia-message-sidecar.tar.gz",
    "artifact-manifest.json",
  ]);

  const deployFixture = path.join(fixtureRoot, "deploy-bundle");
  writeFile(
    path.join(deployFixture, "qintopia-agent-os-deploy-bundle.tar.gz"),
    "deploy bundle archive fixture\n",
    0o444
  );
  writeFile(
    path.join(deployFixture, "artifact-manifest.json"),
    `${JSON.stringify({ commit_sha: sha, artifact_name: "deploy-fixture" })}\n`,
    0o444
  );
  writeFile(
    path.join(deployFixture, "payload/deploy/runner-fixture.sh"),
    "#!/usr/bin/env bash\nexit 0\n",
    0o755
  );
  writeChecksums(deployFixture, [
    "qintopia-agent-os-deploy-bundle.tar.gz",
    "artifact-manifest.json",
  ]);

  writeFile(
    path.join(tmpRoot, "deploy/sidecar/scripts/fetch-cos-artifact.sh"),
    `#!/usr/bin/env bash
set -euo pipefail
artifact_type=""
output_dir=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --artifact-type) artifact_type="$2"; shift 2 ;;
    --sha) shift 2 ;;
    --output-dir) output_dir="$2"; shift 2 ;;
    *) exit 64 ;;
  esac
done
mkdir -p "$output_dir"
cp -a "$FIXTURE_ROOT/$artifact_type/." "$output_dir/"
`,
    0o755
  );
  writeFile(
    path.join(fakeBin, "chown"),
    `#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >> "$CHOWN_LOG"
`,
    0o755
  );

  const first = runPromotion(writeRequest("deploy-20260719T060000Z-0123456789ab"));
  if (first.status !== 0) {
    throw new Error(`initial promotion failed\n${first.stdout}\n${first.stderr}`);
  }

  const releaseDir = path.join(releaseRoot, sha);
  const staleEvidence = [
    "sidecar/artifact-manifest.json",
    "sidecar/SHA256SUMS",
    "sidecar/qintopia-message-sidecar.tar.gz",
    "deploy-bundle/artifact-manifest.json",
    "deploy-bundle/SHA256SUMS",
    "deploy-bundle/qintopia-agent-os-deploy-bundle.tar.gz",
  ];
  for (const relative of staleEvidence) {
    fs.chmodSync(path.join(releaseDir, relative), 0o640);
  }
  fs.chmodSync(path.join(releaseDir, "deploy/runner-fixture.sh"), 0o700);

  const followUp = runPromotion(writeRequest("deploy-20260719T060100Z-0123456789ab"));
  if (followUp.status !== 0) {
    throw new Error(
      `same-SHA metadata repair failed\n${followUp.stdout}\n${followUp.stderr}`
    );
  }
  for (const relative of staleEvidence) {
    requireMode(path.join(releaseDir, relative), 0o444);
  }
  requireMode(path.join(releaseDir, "sidecar/qintopia-message-sidecar"), 0o755);
  requireMode(path.join(releaseDir, "deploy/runner-fixture.sh"), 0o755);
  const chownArgs = fs.readFileSync(chownLog, "utf8").trim();
  if (chownArgs !== `-hR root:root ${releaseDir}`) {
    throw new Error(`unexpected metadata repair chown: ${chownArgs}`);
  }

  fs.writeFileSync(
    path.join(releaseDir, "deploy/runner-fixture.sh"),
    "#!/usr/bin/env bash\nexit 99\n",
    "utf8"
  );
  fs.rmSync(chownLog);
  const drifted = runPromotion(writeRequest("deploy-20260719T060200Z-0123456789ab"));
  if (
    drifted.status === 0 ||
    !drifted.stderr.includes(
      "existing release content differs from freshly verified artifacts"
    )
  ) {
    throw new Error(
      `same-SHA content drift must fail before repair\n${drifted.stdout}\n${drifted.stderr}`
    );
  }
  if (fs.existsSync(chownLog)) {
    throw new Error("content drift reached metadata mutation");
  }

  fs.writeFileSync(
    path.join(releaseDir, "deploy/runner-fixture.sh"),
    "#!/usr/bin/env bash\nexit 0\n",
    "utf8"
  );
  const manifestPath = path.join(releaseDir, "manifest.json");
  const outsideManifest = path.join(tmpRoot, "outside-manifest.json");
  fs.writeFileSync(outsideManifest, fs.readFileSync(manifestPath));
  fs.rmSync(manifestPath);
  fs.symlinkSync(outsideManifest, manifestPath);
  const symlinkedManifest = runPromotion(
    writeRequest("deploy-20260719T060300Z-0123456789ab")
  );
  if (
    symlinkedManifest.status === 0 ||
    !symlinkedManifest.stderr.includes(
      "existing release manifest must be a non-symlink regular file"
    )
  ) {
    throw new Error(
      `symlinked existing manifest must fail before repair\n${symlinkedManifest.stdout}\n${symlinkedManifest.stderr}`
    );
  }
  if (fs.existsSync(chownLog)) {
    throw new Error("symlinked manifest reached metadata mutation");
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Existing release same-SHA metadata repair test passed.");
