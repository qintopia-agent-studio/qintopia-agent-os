#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const promoteScript = path.join(repoRoot, "deploy/runner/promote-release.sh");
const originalUmask = process.umask(0o077);
const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "qintopia-promote-tree-"));
const fixtureRoot = path.join(tmpRoot, "fixtures");
const fakeBin = path.join(tmpRoot, "bin");
const sha = "0123456789abcdef0123456789abcdef01234567";
const previousSha = "89abcdef0123456789abcdef0123456789abcdef";

const sha256File = (filePath) => {
  const hash = crypto.createHash("sha256");
  hash.update(fs.readFileSync(filePath));
  return hash.digest("hex");
};

const ensureDirectory = (directory, mode = 0o755) => {
  fs.mkdirSync(directory, { recursive: true });
  fs.chmodSync(directory, mode);
};

const writeFile = (filePath, content, mode = 0o644) => {
  ensureDirectory(path.dirname(filePath));
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

const writeRequest = (runtimeArtifactProfile = "huabaosi-production") => {
  const requestPath = path.join(tmpRoot, "request.json");
  writeFile(
    requestPath,
    `${JSON.stringify(
      {
        release_sha: sha,
        runtime_sha: sha,
        runtime_artifact_profile: runtimeArtifactProfile,
        deploy_bundle_sha: sha,
        commit_sha: sha,
        request_id: "deploy-20260719T000000Z-0123456789ab",
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

const runPromotion = (requestFile, releaseRoot, extraEnv = {}) =>
  spawnSync(
    "bash",
    [promoteScript, "--request-file", requestFile, "--release-root", releaseRoot],
    {
      cwd: tmpRoot,
      env: {
        ...process.env,
        ...extraEnv,
        FIXTURE_ROOT: fixtureRoot,
        PATH: `${fakeBin}${path.delimiter}${process.env.PATH ?? ""}`,
      },
      encoding: "utf8",
    }
  );

const expectFailure = (result, expected) => {
  if (result.status === 0 || !result.stderr.includes(expected)) {
    throw new Error(
      `promotion must fail with ${expected}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`
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
  ensureDirectory(path.join(deployFixture, "payload"));
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
sidecar_profile_log="$FIXTURE_ROOT/sidecar-profile.log"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --artifact-type) artifact_type="$2"; shift 2 ;;
    --sha) shift 2 ;;
    --output-dir) output_dir="$2"; shift 2 ;;
    *) exit 64 ;;
  esac
done
mkdir -p "$output_dir"
chmod 0755 "$output_dir"
if [[ "$artifact_type" == "sidecar" ]]; then
  printf '%s\n' "\${QINTOPIA_SIDECAR_ARTIFACT_PROFILE:-}" >> "$sidecar_profile_log"
fi
cp -a "$FIXTURE_ROOT/$artifact_type/." "$output_dir/"
`,
    0o755
  );
  writeFile(path.join(fakeBin, "chown"), "#!/usr/bin/env bash\nexit 0\n", 0o755);
  writeFile(
    path.join(fakeBin, "id"),
    `#!/usr/bin/env bash
set -euo pipefail
if [[ "\${1:-}" == "-u" ]]; then
  printf '%s\n' "\${FAKE_ID_UID:-${process.getuid()}}"
  exit 0
fi
exec /usr/bin/id "$@"
`,
    0o755
  );

  const requestFile = writeRequest();
  const validRoot = path.join(tmpRoot, "valid-releases");
  const promoted = runPromotion(requestFile, validRoot);
  if (promoted.status !== 0) {
    throw new Error(`new release promotion failed: ${promoted.stderr}`);
  }
  const promotedManifest = JSON.parse(
    fs.readFileSync(path.join(validRoot, sha, "manifest.json"), "utf8")
  );
  if (promotedManifest.runtime_artifact_profile !== "huabaosi-production") {
    throw new Error(
      "promoted manifest did not retain huabaosi runtime_artifact_profile"
    );
  }

  const releaseDir = fs.realpathSync(path.join(validRoot, "current"));
  const previousDir = path.join(validRoot, previousSha);
  ensureDirectory(previousDir);
  fs.symlinkSync(previousDir, path.join(validRoot, "previous"));
  const reused = runPromotion(requestFile, validRoot);
  if (reused.status !== 0) {
    throw new Error(`valid same-SHA reuse failed: ${reused.stderr}`);
  }
  if (
    fs.realpathSync(path.join(validRoot, "previous")) !== fs.realpathSync(previousDir)
  ) {
    throw new Error("valid same-SHA reuse replaced previous with current");
  }
  if (fs.realpathSync(path.join(validRoot, "current")) !== releaseDir) {
    throw new Error("valid same-SHA reuse changed current");
  }

  const qiweRoot = path.join(tmpRoot, "qiwe-releases");
  const qiweRequestFile = writeRequest("qiwe-production");
  const qiwePromoted = runPromotion(qiweRequestFile, qiweRoot);
  if (qiwePromoted.status !== 0) {
    throw new Error(`qiwe promotion failed: ${qiwePromoted.stderr}`);
  }
  const qiweManifest = JSON.parse(
    fs.readFileSync(path.join(qiweRoot, sha, "manifest.json"), "utf8")
  );
  if (qiweManifest.runtime_artifact_profile !== "qiwe-production") {
    throw new Error("qiwe promotion did not retain runtime_artifact_profile");
  }
  const sidecarProfileLog = fs
    .readFileSync(path.join(fixtureRoot, "sidecar-profile.log"), "utf8")
    .trim()
    .split("\n")
    .filter(Boolean);
  if (sidecarProfileLog.at(-1) !== "qiwe-production") {
    throw new Error(
      `qiwe promotion did not pass QINTOPIA_SIDECAR_ARTIFACT_PROFILE, got ${JSON.stringify(
        sidecarProfileLog
      )}`
    );
  }

  fs.chmodSync(sidecarFixture, 0o777);
  expectFailure(
    runPromotion(requestFile, path.join(tmpRoot, "writable-releases")),
    "release tree path is group/world writable"
  );
  fs.chmodSync(sidecarFixture, 0o755);

  const payloadDeploy = path.join(deployFixture, "payload/deploy");
  fs.chmodSync(payloadDeploy, 0o700);
  expectFailure(
    runPromotion(requestFile, path.join(tmpRoot, "inaccessible-releases")),
    "release tree directory is not group/world accessible"
  );
  fs.chmodSync(payloadDeploy, 0o755);

  const sidecarManifest = path.join(sidecarFixture, "artifact-manifest.json");
  fs.chmodSync(sidecarManifest, 0o640);
  expectFailure(
    runPromotion(requestFile, path.join(tmpRoot, "mode-releases")),
    "release tree mode mismatch"
  );
  fs.chmodSync(sidecarManifest, 0o444);

  expectFailure(
    runPromotion(requestFile, path.join(tmpRoot, "owner-releases"), {
      FAKE_ID_UID: String(process.getuid() + 1),
    }),
    "release tree owner mismatch"
  );
} finally {
  process.umask(originalUmask);
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Promote release tree validation test passed.");
