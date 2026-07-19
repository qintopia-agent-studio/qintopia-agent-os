#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const promoteScript = path.join(repoRoot, "deploy/runner/promote-release.sh");
const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "qintopia-promote-tree-"));
const sha = "0123456789abcdef0123456789abcdef01234567";
const previousSha = "89abcdef0123456789abcdef0123456789abcdef";

const mode = (filePath) => fs.statSync(filePath).mode & 0o777;

const writeExecutable = (filePath, content) => {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content, "utf8");
  fs.chmodSync(filePath, 0o755);
};

const writeRequest = () => {
  const request = {
    release_sha: sha,
    runtime_sha: sha,
    deploy_bundle_sha: sha,
    commit_sha: sha,
    request_id: "deploy-20260719T000000Z-0123456789ab",
    release_scope: ["sidecar-runtime", "deploy-bundle", "hermes-plugins"],
    restart_targets: ["qintopia-system-services", "hermes-erhua"],
    dry_run: false,
  };
  const requestFile = path.join(tmpRoot, "request.json");
  fs.writeFileSync(requestFile, `${JSON.stringify(request, null, 2)}\n`, "utf8");
  return { request, requestFile };
};

const writeExistingRelease = (releaseRoot, request, metadataMode) => {
  const releaseDir = path.join(releaseRoot, sha);
  const previousDir = path.join(releaseRoot, previousSha);
  fs.mkdirSync(path.join(releaseDir, "sidecar"), { recursive: true });
  fs.mkdirSync(path.join(releaseDir, "deploy"), { recursive: true });
  fs.mkdirSync(previousDir, { recursive: true });
  writeExecutable(
    path.join(releaseDir, "sidecar", "qintopia-message-sidecar"),
    "#!/usr/bin/env bash\nexit 0\n"
  );
  for (const name of ["artifact-manifest.json", "SHA256SUMS"]) {
    const filePath = path.join(releaseDir, "sidecar", name);
    fs.writeFileSync(filePath, "{}\n", "utf8");
    fs.chmodSync(filePath, metadataMode);
  }
  fs.writeFileSync(
    path.join(releaseDir, "manifest.json"),
    `${JSON.stringify(
      {
        release_sha: request.release_sha,
        runtime_sha: request.runtime_sha,
        deploy_bundle_sha: request.deploy_bundle_sha,
        commit_sha: request.commit_sha,
        release_scope: request.release_scope,
        restart_targets: request.restart_targets,
      },
      null,
      2
    )}\n`,
    "utf8"
  );
  fs.symlinkSync(releaseDir, path.join(releaseRoot, "current"));
  fs.symlinkSync(previousDir, path.join(releaseRoot, "previous"));
  return { releaseDir, previousDir };
};

const runPromotion = (requestFile, releaseRoot) =>
  spawnSync(
    "bash",
    [promoteScript, "--request-file", requestFile, "--release-root", releaseRoot],
    {
      cwd: tmpRoot,
      env: process.env,
      encoding: "utf8",
    }
  );

try {
  const fakeFetch = path.join(tmpRoot, "deploy/sidecar/scripts/fetch-cos-artifact.sh");
  writeExecutable(
    fakeFetch,
    `#!/usr/bin/env bash
set -euo pipefail
artifact_type=""
output_dir=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --artifact-type) artifact_type="$2"; shift 2 ;;
    --sha) shift 2 ;;
    --output-dir) output_dir="$2"; shift 2 ;;
    *) exit 2 ;;
  esac
done
mkdir -p "$output_dir"
printf '{}\n' > "$output_dir/artifact-manifest.json"
printf '{}\n' > "$output_dir/SHA256SUMS"
chmod 0444 "$output_dir/artifact-manifest.json" "$output_dir/SHA256SUMS"
if [[ "$artifact_type" == "sidecar" ]]; then
  printf '#!/usr/bin/env bash\nexit 0\n' > "$output_dir/qintopia-message-sidecar"
  chmod 0755 "$output_dir/qintopia-message-sidecar"
elif [[ "$artifact_type" == "deploy-bundle" ]]; then
  mkdir -p "$output_dir/payload/deploy"
else
  exit 2
fi
`
  );

  const { request, requestFile } = writeRequest();
  const existingRoot = path.join(tmpRoot, "existing-releases");
  fs.mkdirSync(existingRoot, { recursive: true });
  const { releaseDir, previousDir } = writeExistingRelease(
    existingRoot,
    request,
    0o640
  );
  fs.chmodSync(releaseDir, 0o777);

  const rejected = runPromotion(requestFile, existingRoot);
  if (rejected.status === 0) {
    throw new Error("invalid existing release tree must be rejected");
  }
  if (!rejected.stderr.includes("release tree path is group/world writable")) {
    throw new Error(`unexpected rejection error: ${rejected.stderr}`);
  }
  if (
    fs.realpathSync(path.join(existingRoot, "previous")) !==
    fs.realpathSync(previousDir)
  ) {
    throw new Error("failed same-SHA promotion changed previous");
  }

  fs.chmodSync(releaseDir, 0o700);
  const inaccessible = runPromotion(requestFile, existingRoot);
  if (inaccessible.status === 0) {
    throw new Error("inaccessible existing release tree must be rejected");
  }
  if (
    !inaccessible.stderr.includes(
      "release tree directory is not group/world accessible"
    )
  ) {
    throw new Error(`unexpected accessibility rejection: ${inaccessible.stderr}`);
  }

  fs.chmodSync(releaseDir, 0o755);
  const unreadableMetadata = runPromotion(requestFile, existingRoot);
  if (unreadableMetadata.status === 0) {
    throw new Error("existing release with unreadable metadata must be rejected");
  }
  if (!unreadableMetadata.stderr.includes("release tree mode mismatch")) {
    throw new Error(`unexpected metadata rejection: ${unreadableMetadata.stderr}`);
  }

  fs.chmodSync(path.join(releaseDir, "sidecar", "artifact-manifest.json"), 0o444);
  fs.chmodSync(path.join(releaseDir, "sidecar", "SHA256SUMS"), 0o444);
  const reused = runPromotion(requestFile, existingRoot);
  if (reused.status !== 0) {
    throw new Error(`valid same-SHA reuse failed: ${reused.stderr}`);
  }
  if (
    fs.realpathSync(path.join(existingRoot, "previous")) !==
    fs.realpathSync(previousDir)
  ) {
    throw new Error("valid same-SHA reuse replaced previous with current");
  }

  const newRoot = path.join(tmpRoot, "new-releases");
  const promoted = runPromotion(requestFile, newRoot);
  if (promoted.status !== 0) {
    throw new Error(`new release promotion failed: ${promoted.stderr}`);
  }
  const promotedDir = fs.realpathSync(path.join(newRoot, "current"));
  if (promotedDir !== fs.realpathSync(path.join(newRoot, sha))) {
    throw new Error(`unexpected promoted target: ${promotedDir}`);
  }
  if (
    mode(path.join(promotedDir, "sidecar", "qintopia-message-sidecar")) !== 0o755 ||
    mode(path.join(promotedDir, "sidecar", "artifact-manifest.json")) !== 0o444 ||
    mode(path.join(promotedDir, "sidecar", "SHA256SUMS")) !== 0o444
  ) {
    throw new Error("new release did not preserve reviewed sidecar modes");
  }
  if (fs.statSync(promotedDir).uid !== process.getuid()) {
    throw new Error("new release owner does not match deploy runner uid");
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Promote release tree validation test passed.");
