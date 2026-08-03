#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const originalUmask = process.umask(0o077);
const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "qintopia-promote-tree-"));
const runnerState = path.join(tmpRoot, "runner-state");
const promoteScript = path.join(tmpRoot, "deploy/runner/promote-release.sh");
const fixtureRoot = path.join(tmpRoot, "fixtures");
const fakeBin = path.join(tmpRoot, "bin");
const sha = "0123456789abcdef0123456789abcdef01234567";
const previousSha = "89abcdef0123456789abcdef0123456789abcdef";
fs.mkdirSync(path.dirname(promoteScript), { recursive: true });
fs.mkdirSync(runnerState, { recursive: true });
fs.copyFileSync(path.join(repoRoot, "deploy/runner/promote-release.sh"), promoteScript);
fs.chmodSync(promoteScript, 0o755);

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
  const requestPath = path.join(tmpRoot, `${runtimeArtifactProfile}-request.json`);
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
        restart_targets: ["hermes-erhua", "qintopia-system-services"],
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
      cwd: runnerState,
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

  const qiweSidecarFixture = path.join(fixtureRoot, "sidecar-qiwe");
  writeFile(
    path.join(qiweSidecarFixture, "qintopia-message-sidecar"),
    "#!/usr/bin/env bash\n# qiwe companion\nexit 0\n",
    0o755
  );
  writeFile(
    path.join(qiweSidecarFixture, "qintopia-message-sidecar.tar.gz"),
    "qiwe sidecar archive fixture\n",
    0o444
  );
  writeFile(
    path.join(qiweSidecarFixture, "artifact-manifest.json"),
    `${JSON.stringify({
      commit_sha: sha,
      artifact_name: "qiwe-sidecar-fixture",
      validation: { artifact_profile: "qiwe-production" },
    })}\n`,
    0o444
  );
  writeChecksums(qiweSidecarFixture, [
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
  ensureDirectory(path.join(deployFixture, "payload/deploy/sidecar"));
  ensureDirectory(path.join(deployFixture, "payload/deploy/sidecar/scripts"));
  for (const relativePath of [
    "deploy/sidecar/scripts/apply-xiaoman-feishu-poster-production-config.py",
    "deploy/sidecar/scripts/apply-xiaoman-conversation-policies-production.py",
    "deploy/sidecar/scripts/xiaoman-shared-db-password-rollover-production.py",
  ]) {
    const sourcePath = path.join(repoRoot, relativePath);
    const targetPath = path.join(deployFixture, "payload", relativePath);
    ensureDirectory(path.dirname(targetPath));
    fs.copyFileSync(sourcePath, targetPath);
    fs.chmodSync(targetPath, 0o755);
  }
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
source_root="$FIXTURE_ROOT/$artifact_type"
if [[ "$artifact_type" == "sidecar" && "\${QINTOPIA_SIDECAR_ARTIFACT_PROFILE:-}" == "qiwe-production" ]]; then
  source_root="$FIXTURE_ROOT/sidecar-qiwe"
fi
cp -a "$source_root/." "$output_dir/"
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
  const promotedManifestMode =
    fs.statSync(path.join(validRoot, sha, "manifest.json")).mode & 0o777;
  if (promotedManifestMode !== 0o444) {
    throw new Error(
      `promoted manifest mode ${promotedManifestMode.toString(8)} != 444`
    );
  }
  if (promotedManifest.runtime_artifact_profile !== "huabaosi-production") {
    throw new Error(
      "promoted manifest did not retain huabaosi runtime_artifact_profile"
    );
  }
  if (
    promotedManifest.companion_runtime_artifact_profiles?.join(",") !==
    "qiwe-production"
  ) {
    throw new Error("promoted manifest did not record the QiWe companion runtime");
  }
  const primaryBinary = fs.readFileSync(
    path.join(validRoot, sha, "sidecar", "qintopia-message-sidecar"),
    "utf8"
  );
  const companionBinary = fs.readFileSync(
    path.join(
      validRoot,
      sha,
      "sidecar-profiles",
      "qiwe-production",
      "qintopia-message-sidecar"
    ),
    "utf8"
  );
  if (
    !primaryBinary.includes("exit 0") ||
    !companionBinary.includes("qiwe companion")
  ) {
    throw new Error("promotion did not keep independent Huabaosi and QiWe binaries");
  }

  const releaseDir = fs.realpathSync(path.join(validRoot, "current"));
  const boundaryProbe = spawnSync(
    "python3",
    [
      "-c",
      `
import hashlib
import importlib.util
import os
import sys
from pathlib import Path

current_path, expected_sha = sys.argv[1:3]

def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise SystemExit(f"cannot load {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module

current = Path(current_path)
config = load(
    "promoted_xiaoman_config",
    current / "deploy/sidecar/scripts/apply-xiaoman-feishu-poster-production-config.py",
)
policy = load(
    "promoted_xiaoman_policy",
    current / "deploy/sidecar/scripts/apply-xiaoman-conversation-policies-production.py",
)
rollover = load(
    "promoted_xiaoman_rollover",
    current / "deploy/sidecar/scripts/xiaoman-shared-db-password-rollover-production.py",
)
if config.resolve_release_sha(current) != expected_sha:
    raise SystemExit("promoted release failed Xiaoman config boundary")
expected_binary = current.resolve() / "sidecar/qintopia-message-sidecar"
if policy.resolve_sidecar_binary(current, expected_sha) != expected_binary:
    raise SystemExit("promoted release failed Xiaoman policy boundary")

def digest(relative):
    return hashlib.sha256((current / relative).read_bytes()).hexdigest()

approved = rollover.ApprovedRequest(
    operation_id="11111111-1111-1111-1111-111111111111",
    release_sha=expected_sha,
    dry_run_request_id="deploy-20260803T000000Z-aaaaaaaaaaaa",
    rollover_script_sha256=digest(rollover.SCRIPT_RELATIVE_PATH),
    config_script_sha256=digest(rollover.CONFIG_SCRIPT_RELATIVE_PATH),
    policy_script_sha256=digest(rollover.POLICY_SCRIPT_RELATIVE_PATH),
    old_database_url_sha256="0" * 64,
    role_ref="sha256:" + "1" * 64,
    conversation_ref="sha256:" + "2" * 64,
    actor_ref="sha256:" + "3" * 64,
)
paths = rollover.RuntimePaths(
    release_current=current,
    sidecar_env=current / "unused-sidecar.env",
    hermes_env=current / "unused-xiaoman.env",
    erhua_env=current / "unused-erhua.env",
    state_root=current / "unused-state",
    self_path=current / rollover.SCRIPT_RELATIVE_PATH,
)
rollover.verify_release_boundary(paths, approved, owner_uid=os.geteuid())
`,
      path.join(validRoot, "current"),
      sha,
    ],
    {
      encoding: "utf8",
      env: { ...process.env, PYTHONDONTWRITEBYTECODE: "1" },
    }
  );
  if (boundaryProbe.status !== 0) {
    throw new Error(
      `promoted release did not satisfy Xiaoman protected entrypoints\nstdout:\n${boundaryProbe.stdout}\nstderr:\n${boundaryProbe.stderr}`
    );
  }
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

  fs.unlinkSync(path.join(validRoot, "current"));
  fs.symlinkSync(previousDir, path.join(validRoot, "current"));
  const recovered = runPromotion(requestFile, validRoot);
  if (recovered.status !== 0) {
    throw new Error(`exact-SHA recovery promotion failed: ${recovered.stderr}`);
  }
  if (fs.realpathSync(path.join(validRoot, "current")) !== releaseDir) {
    throw new Error("exact-SHA recovery did not restore approved current");
  }
  if (
    fs.realpathSync(path.join(validRoot, "previous")) !== fs.realpathSync(previousDir)
  ) {
    throw new Error("exact-SHA recovery did not preserve previous");
  }

  const qiweRequestFile = writeRequest("qiwe-production");
  expectFailure(
    runPromotion(qiweRequestFile, path.join(tmpRoot, "qiwe-primary-releases")),
    "QiWe is installed as a companion runtime"
  );
  const sidecarProfileLog = fs
    .readFileSync(path.join(fixtureRoot, "sidecar-profile.log"), "utf8")
    .trim()
    .split("\n")
    .filter(Boolean);
  if (
    !sidecarProfileLog.includes("huabaosi-production") ||
    !sidecarProfileLog.includes("qiwe-production")
  ) {
    throw new Error(
      `promotion did not fetch both reviewed artifact profiles, got ${JSON.stringify(
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
