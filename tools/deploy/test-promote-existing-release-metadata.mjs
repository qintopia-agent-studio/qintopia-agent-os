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

const writeRequest = (
  requestId,
  runtimeArtifactProfile = "huabaosi-production",
  overrides = {}
) => {
  const requestPath = path.join(tmpRoot, `${requestId}.json`);
  writeFile(
    requestPath,
    `${JSON.stringify(
      {
        request_id: requestId,
        commit_sha: sha,
        runtime_sha: sha,
        runtime_artifact_profile: runtimeArtifactProfile,
        deploy_bundle_sha: sha,
        release_sha: sha,
        release_scope: ["sidecar-runtime", "deploy-bundle", "hermes-plugins"],
        restart_targets: ["qintopia-system-services", "hermes-erhua"],
        dry_run: false,
        ...overrides,
      },
      null,
      2
    )}\n`
  );
  return requestPath;
};

const runPromotion = (requestPath, { dryRun = false, extraEnv = {} } = {}) =>
  spawnSync(
    "bash",
    [
      path.join(repoRoot, "deploy/runner/promote-release.sh"),
      "--request-file",
      requestPath,
      "--release-root",
      releaseRoot,
      ...(dryRun ? ["--dry-run"] : []),
    ],
    {
      cwd: tmpRoot,
      env: {
        ...process.env,
        CHOWN_LOG: chownLog,
        FIXTURE_ROOT: fixtureRoot,
        QINTOPIA_DEPLOY_RUNNER_QUARANTINE_ROOT: path.join(tmpRoot, "quarantine"),
        PATH: `${fakeBin}${path.delimiter}${process.env.PATH ?? ""}`,
        ...extraEnv,
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
    `${JSON.stringify(
      {
        commit_sha: sha,
        artifact_name: "sidecar-fixture",
        validation: { artifact_profile: "huabaosi-production" },
      },
      null,
      2
    )}\n`,
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
    "#!/usr/bin/env bash\n# qiwe profile\nexit 0\n",
    0o755
  );
  writeFile(
    path.join(qiweSidecarFixture, "qintopia-message-sidecar.tar.gz"),
    "qiwe sidecar archive fixture\n",
    0o444
  );
  writeFile(
    path.join(qiweSidecarFixture, "artifact-manifest.json"),
    `${JSON.stringify(
      {
        commit_sha: sha,
        artifact_name: "sidecar-qiwe-fixture",
        validation: { artifact_profile: "qiwe-production" },
      },
      null,
      2
    )}\n`,
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
source_root="$FIXTURE_ROOT/$artifact_type"
if [[ "$artifact_type" == "sidecar" && "\${QINTOPIA_SIDECAR_ARTIFACT_PROFILE:-}" == "qiwe-production" ]]; then
  source_root="$FIXTURE_ROOT/sidecar-qiwe"
fi
cp -a "$source_root/." "$output_dir/"
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
  writeFile(
    path.join(fakeBin, "mv"),
    `#!/usr/bin/env bash
set -euo pipefail
destination="\${@: -1}"
/bin/mv "$@"
if [[ "\${FAIL_AFTER_MANIFEST_INSTALL:-0}" == "1" && "$destination" == */manifest.json ]]; then
  marker="\${FAIL_AFTER_MANIFEST_INSTALL_MARKER:?}"
  if [[ ! -e "$marker" ]]; then
    touch "$marker"
    chmod 0644 "$(dirname "$destination")/sidecar-profiles/qiwe-production/SHA256SUMS"
  fi
fi
`,
    0o755
  );

  const first = runPromotion(writeRequest("deploy-20260719T060000Z-0123456789ab"));
  if (first.status !== 0) {
    throw new Error(`initial promotion failed\n${first.stdout}\n${first.stderr}`);
  }

  const releaseDir = path.join(releaseRoot, sha);
  const manifestPath = path.join(releaseDir, "manifest.json");
  requireMode(manifestPath, 0o444);
  const existingManifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
  delete existingManifest.runtime_artifact_profile;
  fs.chmodSync(manifestPath, 0o640);
  fs.writeFileSync(
    manifestPath,
    `${JSON.stringify(existingManifest, null, 2)}\n`,
    "utf8"
  );

  const staleEvidence = [
    "sidecar/artifact-manifest.json",
    "sidecar/SHA256SUMS",
    "sidecar/qintopia-message-sidecar.tar.gz",
    "sidecar-profiles/qiwe-production/artifact-manifest.json",
    "sidecar-profiles/qiwe-production/SHA256SUMS",
    "sidecar-profiles/qiwe-production/qintopia-message-sidecar.tar.gz",
    "deploy-bundle/artifact-manifest.json",
    "deploy-bundle/SHA256SUMS",
    "deploy-bundle/qintopia-agent-os-deploy-bundle.tar.gz",
  ];
  fs.writeFileSync(
    path.join(releaseDir, "deploy/runner-fixture.sh"),
    "#!/usr/bin/env bash\nexit 99\n",
    "utf8"
  );
  const driftedMissingProfile = runPromotion(
    writeRequest("deploy-20260719T060050Z-0123456789ab")
  );
  if (
    driftedMissingProfile.status === 0 ||
    !driftedMissingProfile.stderr.includes(
      "existing release content differs from freshly verified artifacts"
    )
  ) {
    throw new Error(
      `same-SHA drift with missing runtime_artifact_profile must fail before mutation\n${driftedMissingProfile.stdout}\n${driftedMissingProfile.stderr}`
    );
  }
  const unchangedManifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
  if ("runtime_artifact_profile" in unchangedManifest) {
    throw new Error(
      "failed same-SHA drift unexpectedly persisted runtime_artifact_profile"
    );
  }
  if (fs.existsSync(chownLog)) {
    throw new Error("drift with missing profile reached metadata mutation");
  }

  fs.writeFileSync(
    path.join(releaseDir, "deploy/runner-fixture.sh"),
    "#!/usr/bin/env bash\nexit 0\n",
    "utf8"
  );
  const adoptedProfile = runPromotion(
    writeRequest("deploy-20260719T060075Z-0123456789ab")
  );
  if (adoptedProfile.status !== 0) {
    throw new Error(
      `same-SHA runtime_artifact_profile adoption failed\n${adoptedProfile.stdout}\n${adoptedProfile.stderr}`
    );
  }
  const adoptedManifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
  if (adoptedManifest.runtime_artifact_profile !== "huabaosi-production") {
    throw new Error("same-SHA adoption did not restore runtime_artifact_profile");
  }
  requireMode(manifestPath, 0o444);
  fs.rmSync(chownLog, { force: true });

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
  requireMode(manifestPath, 0o444);
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

  fs.rmSync(path.join(releaseRoot, "current"), { force: true });
  fs.rmSync(path.join(releaseRoot, "previous"), { force: true });
  fs.rmSync(releaseDir, { recursive: true, force: true });

  const huabaosiInitial = runPromotion(
    writeRequest("deploy-20260719T060400Z-0123456789ab", "huabaosi-production")
  );
  if (huabaosiInitial.status !== 0) {
    throw new Error(
      `huabaosi initial promotion failed\n${huabaosiInitial.stdout}\n${huabaosiInitial.stderr}`
    );
  }
  const huabaosiManifest = JSON.parse(
    fs.readFileSync(path.join(releaseDir, "manifest.json"), "utf8")
  );
  if (huabaosiManifest.runtime_artifact_profile !== "huabaosi-production") {
    throw new Error(
      "huabaosi initial promotion did not record runtime_artifact_profile"
    );
  }
  const huabaosiBinary = path.join(releaseDir, "sidecar", "qintopia-message-sidecar");
  const huabaosiBinaryHash = sha256File(huabaosiBinary);
  const companionRoot = path.join(releaseDir, "sidecar-profiles");
  fs.rmSync(companionRoot, { recursive: true, force: true });
  delete huabaosiManifest.companion_runtime_artifact_profiles;
  fs.chmodSync(manifestPath, 0o640);
  fs.writeFileSync(
    manifestPath,
    `${JSON.stringify(huabaosiManifest, null, 2)}\n`,
    "utf8"
  );
  const manifestBeforeFailedRepair = fs.readFileSync(manifestPath);

  const coscliOutput = path.join(releaseDir, "coscli_output");
  for (const timestamp of ["20260728_190405", "20260728_190921"]) {
    const diagnosticDir = path.join(coscliOutput, timestamp);
    fs.mkdirSync(diagnosticDir, { recursive: true });
    fs.chmodSync(diagnosticDir, 0o755);
    writeFile(
      path.join(diagnosticDir, "process.log"),
      "bounded COSCLI fixture\n",
      0o644
    );
  }
  fs.chmodSync(coscliOutput, 0o755);

  const compatibleDryRun = runPromotion(
    writeRequest("deploy-20260719T060500Z-0123456789ab"),
    { dryRun: true }
  );
  if (compatibleDryRun.status !== 0) {
    throw new Error(
      `compatible dry-run failed\n${compatibleDryRun.stdout}\n${compatibleDryRun.stderr}`
    );
  }
  if (!fs.existsSync(coscliOutput) || fs.existsSync(companionRoot)) {
    throw new Error("dry-run mutated contamination or installed the companion runtime");
  }

  const scopeMismatchDryRun = runPromotion(
    writeRequest("deploy-20260719T060525Z-0123456789ab", "huabaosi-production", {
      release_scope: ["sidecar-runtime"],
    }),
    { dryRun: true }
  );
  if (
    scopeMismatchDryRun.status === 0 ||
    !scopeMismatchDryRun.stderr.includes(
      "existing release manifest release_scope mismatch"
    )
  ) {
    throw new Error(
      `dry-run must detect manifest identity mismatch\n${scopeMismatchDryRun.stdout}\n${scopeMismatchDryRun.stderr}`
    );
  }

  writeFile(path.join(coscliOutput, "unexpected.txt"), "not COSCLI evidence\n", 0o644);
  const malformedContamination = runPromotion(
    writeRequest("deploy-20260719T060550Z-0123456789ab"),
    { dryRun: true }
  );
  if (
    malformedContamination.status === 0 ||
    !malformedContamination.stderr.includes(
      "existing COSCLI diagnostic directory name is invalid"
    )
  ) {
    throw new Error(
      `arbitrary release contamination must fail\n${malformedContamination.stdout}\n${malformedContamination.stderr}`
    );
  }
  fs.rmSync(path.join(coscliOutput, "unexpected.txt"));

  const failureMarker = path.join(tmpRoot, "failed-after-manifest-install");
  const failedAfterManifestInstall = runPromotion(
    writeRequest("deploy-20260719T060575Z-0123456789ab"),
    {
      extraEnv: {
        FAIL_AFTER_MANIFEST_INSTALL: "1",
        FAIL_AFTER_MANIFEST_INSTALL_MARKER: failureMarker,
      },
    }
  );
  if (
    failedAfterManifestInstall.status === 0 ||
    !failedAfterManifestInstall.stderr.includes(
      "release tree mode mismatch: sidecar-profiles/qiwe-production/SHA256SUMS expected 0444 got 0644"
    )
  ) {
    throw new Error(
      `post-install validation failure was not exercised\n${failedAfterManifestInstall.stdout}\n${failedAfterManifestInstall.stderr}`
    );
  }
  if (!fs.readFileSync(manifestPath).equals(manifestBeforeFailedRepair)) {
    throw new Error("failed same-SHA repair did not restore the original manifest");
  }
  requireMode(manifestPath, 0o640);
  if (fs.existsSync(companionRoot) || !fs.existsSync(coscliOutput)) {
    throw new Error(
      "failed same-SHA repair did not restore the original release shape"
    );
  }
  if (
    fs
      .readdirSync(releaseRoot)
      .some((name) => name.startsWith(".existing-manifest-backup-"))
  ) {
    throw new Error(
      "failed same-SHA repair left a manifest backup in the release root"
    );
  }

  const companionInstall = runPromotion(
    writeRequest("deploy-20260719T060600Z-0123456789ab")
  );
  if (companionInstall.status !== 0) {
    throw new Error(
      `companion installation failed\n${companionInstall.stdout}\n${companionInstall.stderr}`
    );
  }
  const installedCompanionManifest = JSON.parse(
    fs.readFileSync(
      path.join(companionRoot, "qiwe-production", "artifact-manifest.json"),
      "utf8"
    )
  );
  if (installedCompanionManifest.validation?.artifact_profile !== "qiwe-production") {
    throw new Error("same-SHA repair did not install the QiWe companion artifact");
  }
  if (sha256File(huabaosiBinary) !== huabaosiBinaryHash) {
    throw new Error("QiWe companion installation changed the Huabaosi binary");
  }
  requireMode(manifestPath, 0o444);
  if (fs.existsSync(coscliOutput)) {
    throw new Error("successful repair left COSCLI diagnostics inside the release");
  }
  const quarantineRoot = path.join(tmpRoot, "quarantine");
  const quarantines = fs.readdirSync(quarantineRoot);
  if (
    quarantines.length !== 1 ||
    !fs.existsSync(
      path.join(
        quarantineRoot,
        quarantines[0],
        "coscli_output",
        "20260728_190405",
        "process.log"
      )
    )
  ) {
    throw new Error(
      "successful repair did not retain COSCLI diagnostics in quarantine"
    );
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Existing release same-SHA metadata repair test passed.");
