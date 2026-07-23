#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpParent = fs.existsSync("/private/tmp") ? "/private/tmp" : "/tmp";
const tmpRoot = fs.mkdtempSync(
  path.join(tmpParent, "qintopia-qiwe-callback-observation-")
);
const script = path.join(
  repoRoot,
  "deploy/sidecar/scripts/qiwe-image-callback-bridge-production-observation-smoke.sh"
);

const writeExecutable = (filePath, content) => {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content, "utf8");
  fs.chmodSync(filePath, 0o755);
};
try {
  const fixtureSecrets = [
    "postgres://fixture-user:fixture-password@127.0.0.1:55432/qintopia",
    "fixture-qiwe-token-secret",
    "fixture-qiwe-guid-secret",
    "reviewed-group-secret",
    "media.example.secret",
    "fixture-unrelated-secret",
  ];
  const releaseSha = "0123456789abcdef0123456789abcdef01234567";
  const releaseRoot = path.join(tmpRoot, "releases");
  const releaseDir = path.join(releaseRoot, releaseSha);
  const currentRelease = path.join(releaseRoot, "current");
  const sidecar = path.join(releaseDir, "sidecar", "qintopia-message-sidecar");
  const bridge = path.join(releaseDir, "skills", "qiwe", "image_callback_bridge.py");
  const plugin = path.join(
    tmpRoot,
    "home",
    "ubuntu",
    ".hermes",
    "profiles",
    "erhua",
    "plugins",
    "qiwe-platform"
  );
  const commandSubstitutionMarker = path.join(tmpRoot, "command-substitution-ran");

  writeExecutable(
    sidecar,
    `#!/usr/bin/env bash
set -euo pipefail
echo "fake sidecar must not run during callback bridge observation" >&2
exit 70
`
  );
  fs.mkdirSync(path.dirname(bridge), { recursive: true });
  fs.writeFileSync(bridge, "# fixture bridge\n", "utf8");
  const manifestPath = path.join(releaseDir, "sidecar", "artifact-manifest.json");
  const writeManifest = (cargoFeatures) =>
    fs.writeFileSync(
      manifestPath,
      `${JSON.stringify(
        {
          commit_sha: releaseSha,
          validation: { cargo_features: cargoFeatures },
        },
        null,
        2
      )}\n`,
      "utf8"
    );
  const approvedCargoFeatures = [
    "huabaosi-production-adapter",
    "huabaosi-feishu-mirror-adapter",
  ];
  writeManifest(approvedCargoFeatures);
  fs.symlinkSync(releaseDir, currentRelease);
  fs.mkdirSync(path.dirname(plugin), { recursive: true });
  fs.symlinkSync(path.join(currentRelease, "skills", "qiwe"), plugin);

  const disabledEnv = path.join(tmpRoot, "disabled.env");
  const enabledEnv = path.join(tmpRoot, "enabled.env");
  const duplicateEnv = path.join(tmpRoot, "duplicate.env");
  const unsafeEnv = path.join(tmpRoot, "unsafe.env");
  const secretEnv = path.join(tmpRoot, "secret.env");
  fs.writeFileSync(
    disabledEnv,
    [
      "QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_ENABLED=0",
      `QINTOPIA_IGNORED_RUNTIME_VALUE=${fixtureSecrets[5]}`,
      "",
    ].join("\n"),
    "utf8"
  );
  fs.writeFileSync(
    enabledEnv,
    "QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_ENABLED=1\n",
    "utf8"
  );
  fs.writeFileSync(
    duplicateEnv,
    [
      "QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_ENABLED=0",
      "QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_ENABLED=1",
      "",
    ].join("\n"),
    "utf8"
  );
  fs.writeFileSync(
    unsafeEnv,
    [
      "QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_ENABLED=0",
      `QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_BIN=$(touch ${commandSubstitutionMarker})`,
      "",
    ].join("\n"),
    "utf8"
  );
  fs.writeFileSync(
    secretEnv,
    [
      "QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_ENABLED=0",
      `FEISHU_APP_SECRET=${fixtureSecrets[5]}`,
      `QINTOPIA_SIDECAR_DATABASE_URL=${fixtureSecrets[0]}`,
      "",
    ].join("\n"),
    "utf8"
  );

  const run = (extraEnv = {}) =>
    spawnSync("bash", [script], {
      cwd: repoRoot,
      env: {
        ...process.env,
        QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_OBSERVATION_ENABLE: "1",
        QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_OBSERVATION_TEST_MODE: "1",
        QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_OBSERVATION_TEST_ROOT: tmpRoot,
        QINTOPIA_RELEASE_CURRENT_DIR: currentRelease,
        QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_ENV_FILE: disabledEnv,
        QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PLUGIN_PATH: plugin,
        ...extraEnv,
      },
      encoding: "utf8",
    });

  const productionOverride = spawnSync("bash", [script], {
    cwd: repoRoot,
    env: {
      ...process.env,
      QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PRODUCTION_OBSERVATION_ENABLE: "1",
      QINTOPIA_RELEASE_CURRENT_DIR: currentRelease,
      QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_ENV_FILE: disabledEnv,
      QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PLUGIN_PATH: plugin,
    },
    encoding: "utf8",
  });
  if (productionOverride.status === 0) {
    throw new Error(
      "callback bridge observation accepted test overrides without test mode"
    );
  }
  if (!productionOverride.stderr.includes("fixed production release/current path")) {
    throw new Error(
      "production override failure did not explain the fixed release boundary"
    );
  }

  const missingRelease = run({
    QINTOPIA_RELEASE_CURRENT_DIR: path.join(tmpRoot, "missing-current"),
  });
  if (missingRelease.status === 0) {
    throw new Error("callback bridge observation accepted a missing release/current");
  }

  const mutablePlugin = run({
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_PLUGIN_PATH: path.join(
      releaseDir,
      "skills",
      "qiwe"
    ),
  });
  if (mutablePlugin.status === 0) {
    throw new Error("callback bridge observation accepted a non-symlink plugin path");
  }

  writeManifest([...approvedCargoFeatures, "qiwe-production-adapter"]);
  const mixedArtifact = run();
  if (mixedArtifact.status === 0) {
    throw new Error("observation accepted a mixed Huabaosi/QiWe production artifact");
  }
  writeManifest(approvedCargoFeatures);

  const disabled = run();
  if (disabled.status !== 0) {
    throw new Error(
      `disabled observation failed\n${disabled.stdout}\n${disabled.stderr}`
    );
  }
  if (
    !disabled.stdout.includes(
      "qiwe_image_callback_bridge_production_observation_state=disabled"
    )
  ) {
    throw new Error("disabled observation did not report disabled state");
  }

  const secretIgnored = run({
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_ENV_FILE: secretEnv,
  });
  if (secretIgnored.status !== 0) {
    throw new Error(
      `secret-env observation should ignore unrelated secrets\n${secretIgnored.stdout}\n${secretIgnored.stderr}`
    );
  }
  for (const secret of fixtureSecrets) {
    if (`${secretIgnored.stdout}\n${secretIgnored.stderr}`.includes(secret)) {
      throw new Error("callback bridge observation repeated a fixture secret");
    }
  }

  const duplicate = run({
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_ENV_FILE: duplicateEnv,
  });
  if (duplicate.status === 0) {
    throw new Error("callback bridge observation accepted duplicate env keys");
  }

  const unsafe = run({
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_ENV_FILE: unsafeEnv,
  });
  if (unsafe.status !== 0 || fs.existsSync(commandSubstitutionMarker)) {
    throw new Error(
      "callback bridge observation rejected or executed an ignored env value"
    );
  }

  const enabled = run({
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_ENV_FILE: enabledEnv,
  });
  if (
    enabled.status === 0 ||
    !enabled.stderr.includes("requires a separate reviewed QiWe production artifact")
  ) {
    throw new Error("enabled observation accepted the Huabaosi production artifact");
  }

  const expectedDisabled = run({
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_ENV_FILE: enabledEnv,
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_EXPECTED_STATE: "disabled",
  });
  if (expectedDisabled.status === 0) {
    throw new Error("callback bridge observation ignored the expected-state boundary");
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("QiWe image callback bridge production observation test passed.");
