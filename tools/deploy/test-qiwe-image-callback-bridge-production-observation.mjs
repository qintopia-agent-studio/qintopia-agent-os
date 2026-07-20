#!/usr/bin/env node

import crypto from "node:crypto";
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
const envLine = (key, value) => `${key}=${value}`;
const sha256 = (value) => crypto.createHash("sha256").update(value).digest("hex");

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
  fs.writeFileSync(
    path.join(releaseDir, "sidecar", "artifact-manifest.json"),
    `${JSON.stringify(
      {
        commit_sha: releaseSha,
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
  fs.symlinkSync(releaseDir, currentRelease);
  fs.mkdirSync(path.dirname(plugin), { recursive: true });
  fs.symlinkSync(path.join(currentRelease, "skills", "qiwe"), plugin);

  const sidecarHash = sha256(fs.readFileSync(sidecar));
  const databaseHash = sha256(fixtureSecrets[0]);
  const qiweConfigLines = [
    envLine("QINTOPIA_SIDECAR_DATABASE_URL", fixtureSecrets[0]),
    envLine("QIWE_API_URL", "https://manager.qiweapi.com/qiwe/api/qw/doApi"),
    envLine("QIWE_TOKEN", fixtureSecrets[1]),
    envLine("QIWE_GUID", fixtureSecrets[2]),
    envLine("QINTOPIA_QIWE_IMAGE_SEND_ALLOWED_HOSTS", "manager.qiweapi.com"),
    envLine("QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS", fixtureSecrets[4]),
    envLine("QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS", fixtureSecrets[3]),
    envLine("QINTOPIA_QIWE_IMAGE_SEND_ENABLED", "1"),
    envLine("QINTOPIA_QIWE_IMAGE_SEND_WEBHOOK_READY", "1"),
    envLine(
      "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_APPROVAL",
      "approved-production-qiwe-image-send"
    ),
    envLine("QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256", databaseHash),
  ];
  const callbackLines = [
    envLine("QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_ENABLED", "1"),
    envLine("QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_MODE", "production"),
    envLine(
      "QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_BIN",
      path.join(currentRelease, "sidecar", "qintopia-message-sidecar")
    ),
    envLine("QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_ROOT", currentRelease),
    envLine("QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_SHA256", sidecarHash),
    envLine("QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_TIMEOUT_SECONDS", "30"),
  ];

  const disabledEnv = path.join(tmpRoot, "disabled.env");
  const enabledEnv = path.join(tmpRoot, "enabled.env");
  const hashMismatchEnv = path.join(tmpRoot, "hash-mismatch.env");
  const duplicateEnv = path.join(tmpRoot, "duplicate.env");
  const unsafeEnv = path.join(tmpRoot, "unsafe.env");
  const secretEnv = path.join(tmpRoot, "secret.env");
  fs.writeFileSync(
    disabledEnv,
    [
      "QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_ENABLED=0",
      envLine("QINTOPIA_IGNORED_RUNTIME_VALUE", fixtureSecrets[5]),
      "",
    ].join("\n"),
    "utf8"
  );
  fs.writeFileSync(
    enabledEnv,
    [...callbackLines, ...qiweConfigLines, ""].join("\n"),
    "utf8"
  );
  fs.writeFileSync(
    hashMismatchEnv,
    [
      ...callbackLines,
      ...qiweConfigLines.filter(
        (line) =>
          !line.startsWith("QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256=")
      ),
      envLine(
        "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256",
        "a".repeat(64)
      ),
      "",
    ].join("\n"),
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
      "QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_ENABLED=1",
      envLine(
        "QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_BIN",
        `$(touch ${commandSubstitutionMarker})`
      ),
      "",
    ].join("\n"),
    "utf8"
  );
  fs.writeFileSync(
    secretEnv,
    [
      "QINTOPIA_QIWE_IMAGE_CALLBACK_PROCESSOR_ENABLED=0",
      envLine("FEISHU_APP_SECRET", fixtureSecrets[5]),
      envLine("QINTOPIA_SIDECAR_DATABASE_URL", fixtureSecrets[0]),
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
  if (unsafe.status === 0 || fs.existsSync(commandSubstitutionMarker)) {
    throw new Error(
      "callback bridge observation accepted or executed an unsafe env value"
    );
  }

  const hashMismatch = run({
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_ENV_FILE: hashMismatchEnv,
  });
  if (
    hashMismatch.status === 0 ||
    !hashMismatch.stderr.includes("production observation env is invalid")
  ) {
    throw new Error("callback bridge observation accepted a database hash mismatch");
  }
  for (const secret of fixtureSecrets) {
    if (`${hashMismatch.stdout}\n${hashMismatch.stderr}`.includes(secret)) {
      throw new Error("hash mismatch diagnostic leaked a fixture secret");
    }
  }

  const enabled = run({
    QINTOPIA_QIWE_IMAGE_CALLBACK_BRIDGE_ENV_FILE: enabledEnv,
  });
  if (enabled.status !== 0) {
    throw new Error(`enabled observation failed\n${enabled.stdout}\n${enabled.stderr}`);
  }
  if (
    !enabled.stdout.includes(
      "qiwe_image_callback_bridge_production_observation_state=enabled"
    )
  ) {
    throw new Error("enabled observation did not report enabled state");
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
