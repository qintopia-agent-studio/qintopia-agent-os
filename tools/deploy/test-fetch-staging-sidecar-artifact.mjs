#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const script = path.join(
  repoRoot,
  "deploy/sidecar/scripts/fetch-staging-sidecar-artifact.sh"
);
const scriptText = fs.readFileSync(script, "utf8");
const tmpRoot = fs.mkdtempSync(
  path.join(repoRoot, ".tmp-fetch-staging-sidecar-artifact-")
);
const releaseSha = "0123456789abcdef0123456789abcdef01234567";
const artifactName = "qintopia-message-sidecar-staging-linux-x86_64-gnu";
const binaryName = "qintopia-message-sidecar";

const run = (command, args, options = {}) => {
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? repoRoot,
    env: options.env ?? process.env,
    encoding: "utf8",
  });
  if (result.status !== 0) {
    throw new Error(
      `${command} ${args.join(" ")} failed\nstdout:\n${result.stdout}\nstderr:\n${
        result.stderr
      }`
    );
  }
  return result;
};

const sha256File = (filePath) =>
  crypto.createHash("sha256").update(fs.readFileSync(filePath)).digest("hex");

const writeFixtureArtifact = (
  name,
  {
    cargoFeatures = ["huabaosi-staging-adapter", "qiwe-staging-adapter"],
    stagingOnly = true,
    productionEligible = false,
    includeTarballChecksum = true,
  } = {}
) => {
  const artifactDir = path.join(tmpRoot, name, "artifact");
  fs.mkdirSync(artifactDir, { recursive: true });
  const binaryPath = path.join(artifactDir, binaryName);
  fs.writeFileSync(binaryPath, "#!/usr/bin/env bash\nexit 0\n", {
    encoding: "utf8",
    mode: 0o755,
  });
  fs.chmodSync(binaryPath, 0o755);

  const bundlePath = path.join(artifactDir, `${binaryName}.tar.gz`);
  run("tar", ["-C", artifactDir, "-czf", bundlePath, binaryName]);

  const binarySha = sha256File(binaryPath);
  const bundleSha = sha256File(bundlePath);
  const manifest = {
    schema_version: 1,
    artifact_name: artifactName,
    package_name: binaryName,
    binary_name: binaryName,
    target: "linux-x86_64-gnu",
    commit_sha: releaseSha,
    files: [
      {
        path: binaryName,
        sha256: binarySha,
      },
      {
        path: `${binaryName}.tar.gz`,
        sha256: bundleSha,
      },
    ],
    validation: {
      cargo_features: cargoFeatures,
      staging_only: stagingOnly,
      production_eligible: productionEligible,
    },
  };
  const manifestPath = path.join(artifactDir, "artifact-manifest.json");
  fs.writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
  const manifestSha = sha256File(manifestPath);
  const checksumLines = [`${binarySha}  ${binaryName}`];
  if (includeTarballChecksum) {
    checksumLines.push(`${bundleSha}  ${binaryName}.tar.gz`);
  }
  checksumLines.push(`${manifestSha}  artifact-manifest.json`);
  fs.writeFileSync(
    path.join(artifactDir, "SHA256SUMS"),
    `${checksumLines.join("\n")}\n`
  );

  const zipPath = path.join(tmpRoot, `${name}.zip`);
  run("zip", ["-q", "-r", zipPath, "."], { cwd: artifactDir });
  return { zipPath, binarySha };
};

const writeSymlinkArtifact = (name) => {
  const artifactDir = path.join(tmpRoot, name, "artifact");
  fs.mkdirSync(artifactDir, { recursive: true });
  fs.writeFileSync(path.join(artifactDir, "artifact-manifest.json"), "{}\n");
  fs.writeFileSync(path.join(artifactDir, "SHA256SUMS"), "\n");
  fs.writeFileSync(path.join(artifactDir, `${binaryName}.tar.gz`), "not a tar\n");
  fs.symlinkSync("/etc/passwd", path.join(artifactDir, binaryName));
  const zipPath = path.join(tmpRoot, `${name}.zip`);
  run("zip", ["-q", "-y", "-r", zipPath, "."], { cwd: artifactDir });
  return zipPath;
};

const runProvision = ({ zipPath, releaseRoot, extraEnv = {} }) =>
  spawnSync(
    "bash",
    [
      script,
      "--sha",
      releaseSha,
      "--release-root",
      releaseRoot,
      "--artifact-zip",
      zipPath,
    ],
    {
      cwd: repoRoot,
      env: {
        ...process.env,
        QINTOPIA_STAGING_SIDECAR_PROVISION_TEST_MODE: "1",
        QINTOPIA_STAGING_SIDECAR_PROVISION_APPROVAL:
          "approved-staging-sidecar-provision",
        ...extraEnv,
      },
      encoding: "utf8",
    }
  );

const runProvisionWithoutTestMode = (extraEnv = {}) =>
  spawnSync("bash", [script, "--sha", releaseSha], {
    cwd: repoRoot,
    env: {
      ...process.env,
      QINTOPIA_STAGING_SIDECAR_PROVISION_APPROVAL: "approved-staging-sidecar-provision",
      ...extraEnv,
    },
    encoding: "utf8",
  });

const assertFailed = (result, expectedFragment, label) => {
  if (result.status === 0) {
    throw new Error(`${label} unexpectedly passed\nstdout:\n${result.stdout}`);
  }
  if (!`${result.stdout}\n${result.stderr}`.includes(expectedFragment)) {
    throw new Error(
      `${label} did not report ${expectedFragment}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`
    );
  }
};

try {
  const downloadConfigMatch = scriptText.match(
    /download_curl_config=[\s\S]*?\n  \} >"\$download_curl_config"/
  );
  if (!downloadConfigMatch) {
    throw new Error("download curl config block was not found");
  }
  if (downloadConfigMatch[0].includes("Authorization: Bearer")) {
    throw new Error("download curl config must not include an Authorization header");
  }
  if (
    !scriptText.includes('signed_download_url="$(') ||
    !scriptText.includes('--write-out "%{redirect_url}"')
  ) {
    throw new Error("artifact download must capture a signed redirect URL first");
  }

  const good = writeFixtureArtifact("good");
  const releaseRoot = path.join(tmpRoot, "good", "qintopia-agent-os-staging-releases");
  const goodResult = runProvision({ zipPath: good.zipPath, releaseRoot });
  if (goodResult.status !== 0) {
    throw new Error(
      `expected staging artifact provision to pass\nstdout:\n${goodResult.stdout}\nstderr:\n${goodResult.stderr}`
    );
  }
  const sidecarDir = path.join(releaseRoot, releaseSha, "sidecar");
  const sidecarPath = path.join(sidecarDir, binaryName);
  for (const requiredPath of [
    sidecarPath,
    path.join(sidecarDir, `${binaryName}.tar.gz`),
    path.join(sidecarDir, "artifact-manifest.json"),
    path.join(sidecarDir, "SHA256SUMS"),
  ]) {
    if (!fs.existsSync(requiredPath)) {
      throw new Error(`provision did not install ${requiredPath}`);
    }
  }
  if (sha256File(sidecarPath) !== good.binarySha) {
    throw new Error("installed sidecar hash does not match fixture artifact");
  }
  if ((fs.statSync(sidecarPath).mode & 0o777) !== 0o555) {
    throw new Error("installed sidecar must be mode 0555");
  }
  if ((fs.statSync(path.join(sidecarDir, "SHA256SUMS")).mode & 0o777) !== 0o444) {
    throw new Error("installed checksum file must be mode 0444");
  }
  if ((fs.statSync(sidecarDir).mode & 0o777) !== 0o555) {
    throw new Error("installed sidecar directory must be mode 0555");
  }
  if ((fs.statSync(path.join(releaseRoot, releaseSha)).mode & 0o777) !== 0o555) {
    throw new Error("installed release directory must be mode 0555");
  }
  if (
    !goodResult.stdout.includes(`Release SHA: ${releaseSha}`) ||
    !goodResult.stdout.includes(`Sidecar SHA256: ${good.binarySha}`) ||
    !goodResult.stdout.includes("Run id: local-test")
  ) {
    throw new Error(`provision output is missing reviewed identifiers`);
  }

  assertFailed(
    runProvision({
      zipPath: writeFixtureArtifact("wrong-features", {
        cargoFeatures: ["huabaosi-production-adapter"],
      }).zipPath,
      releaseRoot: path.join(
        tmpRoot,
        "wrong-features",
        "qintopia-agent-os-staging-releases"
      ),
    }),
    "artifact manifest Cargo features are not approved for staging",
    "wrong features"
  );

  assertFailed(
    runProvision({
      zipPath: writeFixtureArtifact("production-eligible", {
        productionEligible: true,
      }).zipPath,
      releaseRoot: path.join(
        tmpRoot,
        "production-eligible",
        "qintopia-agent-os-staging-releases"
      ),
    }),
    "artifact manifest production_eligible must be false",
    "production eligible artifact"
  );

  assertFailed(
    runProvision({
      zipPath: writeFixtureArtifact("missing-tarball-checksum", {
        includeTarballChecksum: false,
      }).zipPath,
      releaseRoot: path.join(
        tmpRoot,
        "missing-tarball-checksum",
        "qintopia-agent-os-staging-releases"
      ),
    }),
    "SHA256SUMS missing entries",
    "missing tarball checksum"
  );

  assertFailed(
    runProvision({
      zipPath: writeSymlinkArtifact("symlink-artifact-entry"),
      releaseRoot: path.join(
        tmpRoot,
        "symlink-artifact-entry",
        "qintopia-agent-os-staging-releases"
      ),
    }),
    "artifact entry must not be a symlink",
    "symlink artifact entry"
  );

  const symlinkParent = path.join(tmpRoot, "symlink-release-root");
  const symlinkTarget = path.join(tmpRoot, "symlink-target");
  fs.mkdirSync(symlinkTarget);
  fs.symlinkSync(symlinkTarget, symlinkParent, "dir");
  assertFailed(
    runProvision({
      zipPath: good.zipPath,
      releaseRoot: symlinkParent,
    }),
    "path component is a symlink",
    "symlink release root"
  );

  const existingTargetRoot = path.join(
    tmpRoot,
    "existing-target",
    "qintopia-agent-os-staging-releases"
  );
  const existingSidecarDir = path.join(existingTargetRoot, releaseSha, "sidecar");
  fs.mkdirSync(existingSidecarDir, { recursive: true });
  assertFailed(
    runProvision({
      zipPath: good.zipPath,
      releaseRoot: existingTargetRoot,
    }),
    "staging sidecar directory already exists",
    "existing sidecar target"
  );

  assertFailed(
    runProvisionWithoutTestMode({
      GITHUB_REPOSITORY: "attacker/example",
    }),
    "GITHUB_REPOSITORY override is not allowed",
    "repository override"
  );

  assertFailed(
    runProvisionWithoutTestMode({
      GITHUB_WORKFLOW: "unreviewed.yml",
    }),
    "GITHUB_WORKFLOW override is not allowed",
    "workflow override"
  );

  assertFailed(
    runProvisionWithoutTestMode({
      GITHUB_API_MAX_TIME: "1\nproxy = http://127.0.0.1:1",
    }),
    "GITHUB_API_MAX_TIME must be a positive integer",
    "API timeout injection"
  );

  assertFailed(
    runProvisionWithoutTestMode({
      GITHUB_DOWNLOAD_MAX_TIME: "0",
    }),
    "GITHUB_DOWNLOAD_MAX_TIME must be between 1 and 3600 seconds",
    "download timeout range"
  );

  console.log("Fetch staging sidecar artifact test passed.");
} finally {
  for (const candidate of fs
    .readdirSync(tmpRoot, { recursive: true })
    .map((entry) => path.join(tmpRoot, entry))
    .sort((a, b) => b.length - a.length)) {
    try {
      if (!fs.lstatSync(candidate).isSymbolicLink()) {
        fs.chmodSync(candidate, 0o755);
      }
    } catch {
      // Best-effort cleanup for immutable fixture directories.
    }
  }
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}
