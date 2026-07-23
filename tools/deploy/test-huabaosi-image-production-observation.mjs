#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpParent = fs.existsSync("/private/tmp") ? "/private/tmp" : os.tmpdir();
const tmpRoot = fs.mkdtempSync(path.join(tmpParent, "qintopia-image-observation-"));
const sourceScript = path.join(
  repoRoot,
  "deploy/sidecar/scripts/huabaosi-image-generation-production-observation-smoke.sh"
);

const bashQuote = (value) => `'${value.replaceAll("'", "'\\''")}'`;

const replaceOnce = (source, search, replacement) => {
  if (!source.includes(search)) {
    throw new Error(`fixture source is missing expected fragment: ${search}`);
  }
  return source.replace(search, replacement);
};

const writeExecutable = (filePath, content) => {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content, "utf8");
  fs.chmodSync(filePath, 0o755);
};

try {
  const systemctlLog = path.join(tmpRoot, "systemctl.log");
  const sidecarLog = path.join(tmpRoot, "sidecar.log");
  const systemctl = path.join(tmpRoot, "bin", "systemctl");
  const observationEnv = path.join(tmpRoot, "observation.env");
  const script = path.join(tmpRoot, "huabaosi-image-observation-fixture.sh");
  const releaseSha = "0123456789abcdef0123456789abcdef01234567";
  const releaseRoot = path.join(tmpRoot, "releases");
  const releaseDir = path.join(releaseRoot, releaseSha);
  const currentRelease = path.join(releaseRoot, "current");
  const sidecar = path.join(releaseDir, "sidecar", "qintopia-message-sidecar");
  const manifestPath = path.join(releaseDir, "sidecar", "artifact-manifest.json");

  const productionSource = fs.readFileSync(sourceScript, "utf8");
  for (const forbidden of [
    "QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_OBSERVATION_TEST_MODE",
    "QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_OBSERVATION_TEST_ROOT",
    "QINTOPIA_RELEASE_CURRENT_DIR",
    "QINTOPIA_SIDECAR_ENV_FILE",
    'SYSTEMCTL="${SYSTEMCTL:-',
  ]) {
    if (productionSource.includes(forbidden)) {
      throw new Error(`production observation script still contains ${forbidden}`);
    }
  }

  writeExecutable(
    systemctl,
    `#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"${systemctlLog}"
if [[ "\${FAKE_PROVIDER_UNIT_PRESENT:-0}" == "1" && "$1" == "cat" ]]; then
  exit 0
fi
if [[ "\${FAKE_PROVIDER_TIMER_ACTIVE:-0}" == "1" && "$1" == "is-active" ]]; then
  exit 0
fi
if [[ "\${FAKE_PROVIDER_TIMER_ENABLED:-0}" == "1" && "$1" == "is-enabled" ]]; then
  exit 0
fi
exit 1
`
  );

  writeExecutable(
    sidecar,
    `#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"${sidecarLog}"
[[ "\${QINTOPIA_DEPLOYED_COMMIT_SHA:-}" == "${releaseSha}" ]]
[[ "\${QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA:-}" == "${releaseSha}" ]]
case "$1" in
  huabaosi-image-generation-preflight)
    if [[ "\${QINTOPIA_HUABAOSI_IMAGE_MODEL:-}" == "production" ]]; then
      printf '%s\n' '{"success":true,"worker":"huabaosi-image-generation-worker","action_status":"adapter_config_ready","generation_enabled":true,"adapter_compiled":true,"adapter_mode":"production","config_valid":true,"media_allowed_host_count":1,"missing_configuration":[],"safe_for_chat":false}'
      exit 0
    fi
    if [[ "\${QINTOPIA_HUABAOSI_IMAGE_MODEL:-}" == "config-valid" ]]; then
      printf '%s\n' '{"success":true,"worker":"huabaosi-image-generation-worker","action_status":"adapter_config_ready","generation_enabled":false,"adapter_compiled":false,"adapter_mode":"disabled","config_valid":true,"media_allowed_host_count":1,"missing_configuration":[],"safe_for_chat":false}'
      exit 0
    fi
    printf '%s\n' '{"success":false,"worker":"huabaosi-image-generation-worker","action_status":"adapter_not_configured","generation_enabled":false,"adapter_compiled":false,"adapter_mode":"disabled","config_valid":false,"media_allowed_host_count":0,"missing_configuration":["QINTOPIA_HUABAOSI_IMAGE_API_KEY"],"safe_for_chat":false}'
    printf '%s\n' 'image adapter preflight configuration is invalid' >&2
    exit 1
    ;;
  run-huabaosi-image-generation-worker)
    [[ "$2" == "--once" && "$3" == "--dry-run" ]]
    if [[ "\${QINTOPIA_HUABAOSI_IMAGE_MODEL:-}" == "leak-stderr" ]]; then
      printf '%s\n' "\${QINTOPIA_HUABAOSI_IMAGE_API_KEY:-}" >&2
      exit 1
    fi
    unexpected=""
    if [[ "\${QINTOPIA_HUABAOSI_IMAGE_MODEL:-}" == "leak-stdout" ]]; then
      unexpected="\${QINTOPIA_HUABAOSI_IMAGE_API_KEY:-}"
    fi
    printf '{"success":true,"dry_run":true,"apply_requested":false,"fixture_mode":false,"worker":"huabaosi-image-generation-worker","action_status":"no_claimable_image_request","work_item_id":null,"artifact_ids":[],"artifact_preview":null,"safe_for_chat":false,"unexpected":"%s"}\n' "$unexpected"
    ;;
  *)
    exit 64
    ;;
esac
`
  );
  const writeManifest = (cargoFeatures) => {
    fs.writeFileSync(
      manifestPath,
      `${JSON.stringify(
        {
          commit_sha: releaseSha,
          validation: {
            cargo_features: cargoFeatures,
          },
        },
        null,
        2
      )}\n`,
      "utf8"
    );
  };
  writeManifest(["huabaosi-production-adapter", "huabaosi-feishu-mirror-adapter"]);
  fs.writeFileSync(sidecarLog, "", "utf8");

  let fixtureSource = productionSource;
  fixtureSource = replaceOnce(
    fixtureSource,
    'MONOREPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"',
    `MONOREPO_ROOT=${bashQuote(repoRoot)}`
  );
  fixtureSource = replaceOnce(
    fixtureSource,
    'ENV_FILE="/etc/qintopia/message-sidecar.env"',
    `ENV_FILE=${bashQuote(observationEnv)}`
  );
  fixtureSource = replaceOnce(
    fixtureSource,
    'RELEASE_CURRENT_DIR="/home/ubuntu/qintopia-agent-os-releases/current"',
    `RELEASE_CURRENT_DIR=${bashQuote(currentRelease)}`
  );
  fixtureSource = replaceOnce(
    fixtureSource,
    'SYSTEMCTL="/usr/bin/systemctl"',
    `SYSTEMCTL=${bashQuote(systemctl)}`
  );
  writeExecutable(script, fixtureSource);

  const writeObservationEnv = (lines = []) => {
    fs.writeFileSync(observationEnv, [...lines, ""].join("\n"), "utf8");
  };

  const runObservation = (extraEnv = {}, envLines = []) => {
    writeObservationEnv(envLines);
    return spawnSync("bash", [script], {
      cwd: repoRoot,
      env: {
        ...process.env,
        QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_OBSERVATION_ENABLE: "1",
        QINTOPIA_SIDECAR_BIN: "",
        ...extraEnv,
      },
      encoding: "utf8",
    });
  };

  const missingRelease = runObservation();
  if (missingRelease.status === 0 || fs.readFileSync(sidecarLog, "utf8") !== "") {
    throw new Error(
      "image observation must fail before execution without release/current"
    );
  }

  fs.symlinkSync(releaseDir, currentRelease);

  writeManifest([
    "huabaosi-production-adapter",
    "huabaosi-feishu-mirror-adapter",
    "qiwe-production-adapter",
  ]);
  fs.writeFileSync(sidecarLog, "", "utf8");
  const qiweFeature = runObservation();
  if (qiweFeature.status === 0 || fs.readFileSync(sidecarLog, "utf8") !== "") {
    throw new Error("image observation accepted a QiWe-enabled production artifact");
  }
  writeManifest(["huabaosi-production-adapter", "huabaosi-feishu-mirror-adapter"]);

  const mutableSidecar = path.join(tmpRoot, "bin", "mutable-sidecar");
  writeExecutable(
    mutableSidecar,
    `#!/usr/bin/env bash
exit 0
`
  );
  const mutableBinary = runObservation({ QINTOPIA_SIDECAR_BIN: mutableSidecar });
  if (mutableBinary.status === 0) {
    throw new Error("image observation accepted a sidecar outside release/current");
  }

  const ignoredOverrides = runObservation({
    QINTOPIA_RELEASE_CURRENT_DIR: path.join(tmpRoot, "missing-release-override"),
    QINTOPIA_SIDECAR_ENV_FILE: path.join(tmpRoot, "invalid-env-override"),
    SYSTEMCTL: mutableSidecar,
  });
  if (ignoredOverrides.status !== 0) {
    throw new Error(
      `image observation honored caller path overrides\nstdout:\n${ignoredOverrides.stdout}\nstderr:\n${ignoredOverrides.stderr}`
    );
  }

  fs.writeFileSync(sidecarLog, "", "utf8");
  const invalidEnv = runObservation({}, ["not a valid env line"]);
  if (invalidEnv.status === 0 || fs.readFileSync(sidecarLog, "utf8") !== "") {
    throw new Error("image observation must fail before execution on invalid env");
  }

  for (const model of ["", "config-valid"]) {
    fs.writeFileSync(sidecarLog, "", "utf8");
    const envLines = model ? [`QINTOPIA_HUABAOSI_IMAGE_MODEL=${model}`] : [];
    const result = runObservation({}, envLines);
    if (result.status !== 0) {
      throw new Error(
        `expected disabled observation to pass for model=${model}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`
      );
    }
    const log = fs.readFileSync(sidecarLog, "utf8");
    for (const command of [
      "huabaosi-image-generation-preflight",
      "run-huabaosi-image-generation-worker --once --dry-run",
    ]) {
      if (!log.includes(command)) {
        throw new Error(`sidecar log is missing ${command}`);
      }
    }
    if (log.includes("--apply")) {
      throw new Error(
        "production observation must not run image generation with --apply"
      );
    }
  }

  for (const enabledValue of ["1", " 1 "]) {
    const enabled = runObservation({}, [
      `QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED=${enabledValue}`,
      "QINTOPIA_HUABAOSI_IMAGE_API_KEY=observation-secret-must-not-appear",
    ]);
    if (enabled.status === 0) {
      throw new Error(
        `expected generation flag ${JSON.stringify(enabledValue)} to fail production observation`
      );
    }
    if (
      `${enabled.stdout}\n${enabled.stderr}`.includes(
        "observation-secret-must-not-appear"
      )
    ) {
      throw new Error("production observation failure exposed a configured secret");
    }
  }

  const enabled = runObservation(
    {
      FAKE_PROVIDER_UNIT_PRESENT: "1",
      FAKE_PROVIDER_TIMER_ACTIVE: "1",
      FAKE_PROVIDER_TIMER_ENABLED: "1",
    },
    [
      "QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED=1",
      "QINTOPIA_HUABAOSI_IMAGE_MODEL=production",
      `QINTOPIA_DEPLOYED_COMMIT_SHA=${"f".repeat(40)}`,
      `QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA=${"e".repeat(40)}`,
    ]
  );
  if (enabled.status !== 0) {
    throw new Error(
      `expected enabled production observation to pass\nstdout:\n${enabled.stdout}\nstderr:\n${enabled.stderr}`
    );
  }

  for (const missingState of [
    { FAKE_PROVIDER_TIMER_ACTIVE: "0", FAKE_PROVIDER_TIMER_ENABLED: "1" },
    { FAKE_PROVIDER_TIMER_ACTIVE: "1", FAKE_PROVIDER_TIMER_ENABLED: "0" },
  ]) {
    const invalidEnabled = runObservation(
      {
        FAKE_PROVIDER_UNIT_PRESENT: "1",
        ...missingState,
      },
      [
        "QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED=1",
        "QINTOPIA_HUABAOSI_IMAGE_MODEL=production",
      ]
    );
    if (invalidEnabled.status === 0) {
      throw new Error("expected incomplete production timer state to fail observation");
    }
  }

  const leakedValue = "configured-secret-must-be-redacted";
  const leaked = runObservation({}, [
    "QINTOPIA_HUABAOSI_IMAGE_MODEL=leak-stdout",
    `QINTOPIA_HUABAOSI_IMAGE_API_KEY=${leakedValue}`,
  ]);
  if (leaked.status === 0) {
    throw new Error("expected configured secret in worker output to fail observation");
  }
  if (`${leaked.stdout}\n${leaked.stderr}`.includes(leakedValue)) {
    throw new Error("observation failure repeated the configured secret");
  }

  const stderrLeakedValue = "stderr-secret-must-be-redacted";
  const stderrLeaked = runObservation({}, [
    "QINTOPIA_HUABAOSI_IMAGE_MODEL=leak-stderr",
    `QINTOPIA_HUABAOSI_IMAGE_API_KEY=${stderrLeakedValue}`,
  ]);
  if (stderrLeaked.status === 0) {
    throw new Error("expected configured secret in worker stderr to fail observation");
  }
  if (`${stderrLeaked.stdout}\n${stderrLeaked.stderr}`.includes(stderrLeakedValue)) {
    throw new Error("observation failure repeated the configured stderr secret");
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Huabaosi image production observation test passed.");
