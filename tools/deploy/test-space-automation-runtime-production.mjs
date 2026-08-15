#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpParent = fs.existsSync("/private/tmp") ? "/private/tmp" : os.tmpdir();
const tmpRoot = fs.mkdtempSync(
  path.join(tmpParent, "qintopia-space-automation-runtime-")
);
const activationScript = path.join(
  repoRoot,
  "deploy/sidecar/scripts/activate-space-automation-runtime-production.sh"
);
const natsAclPreflightScript = path.join(
  repoRoot,
  "deploy/sidecar/scripts/space-automation-nats-acl-preflight.py"
);
const natsAclProtocolTest = path.join(
  repoRoot,
  "tools/deploy/test_space_automation_nats_acl_preflight.py"
);
const rollbackScript = path.join(
  repoRoot,
  "deploy/sidecar/scripts/rollback-space-automation-runtime-production.sh"
);
const observationScript = path.join(
  repoRoot,
  "deploy/sidecar/scripts/space-automation-runtime-production-observation-smoke.sh"
);
const fixedEnvFile = "/etc/qintopia/message-sidecar.env";
const fixedReleaseCurrent = "/home/ubuntu/qintopia-agent-os-releases/current";
const fixedUnitDir = "/etc/systemd/system";
const fixedSystemctl = "/usr/bin/systemctl";
const fixedSha256sum = "/usr/bin/sha256sum";
const approval = "approved-production-space-automation-runtime";
const rollbackApproval = "approved-production-space-automation-runtime-rollback";
const executionApproval = "approved-production-space-automation-execution";
const databaseUrl = "postgres://fixture-user:fixture-password@127.0.0.1:55432/qintopia";
const databaseHash = crypto.createHash("sha256").update(databaseUrl).digest("hex");
const releaseSha = "0123456789abcdef0123456789abcdef01234567";

const natsAclProtocolResult = spawnSync("python3", [natsAclProtocolTest], {
  cwd: repoRoot,
  env: {
    ...process.env,
    PYTHONDONTWRITEBYTECODE: "1",
  },
  encoding: "utf8",
  timeout: 20_000,
});
if (natsAclProtocolResult.status !== 0) {
  throw new Error(
    `NATS ACL protocol test failed\n${natsAclProtocolResult.stdout}\n${natsAclProtocolResult.stderr}`
  );
}

const writeExecutable = (filePath, content) => {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content, "utf8");
  fs.chmodSync(filePath, 0o755);
};

try {
  const releaseRoot = path.join(tmpRoot, "releases");
  const releaseDir = path.join(releaseRoot, releaseSha);
  const releaseCurrent = path.join(releaseRoot, "current");
  const primaryBin = path.join(releaseDir, "sidecar", "qintopia-message-sidecar");
  const companionDir = path.join(releaseDir, "sidecar-profiles", "qiwe-production");
  const companionBin = path.join(companionDir, "qintopia-message-sidecar");
  const manifestPath = path.join(companionDir, "artifact-manifest.json");
  const envFile = path.join(tmpRoot, "message-sidecar.env");
  const unitDir = path.join(tmpRoot, "systemd");
  const systemctl = path.join(tmpRoot, "bin", "systemctl");
  const sha256sum = path.join(tmpRoot, "bin", "sha256sum");
  const systemctlLog = path.join(tmpRoot, "systemctl.log");
  const commandSubstitutionMarker = path.join(tmpRoot, "command-substitution-ran");
  const procRoot = path.join(tmpRoot, "proc");
  const workerPid = "4242";

  for (const [scriptPath, source] of [
    [activationScript, fs.readFileSync(activationScript, "utf8")],
    [rollbackScript, fs.readFileSync(rollbackScript, "utf8")],
  ]) {
    for (const required of [
      `ENV_FILE="${fixedEnvFile}"`,
      'PATH="/usr/bin:/bin:/usr/sbin:/sbin"',
      `SYSTEMCTL="${fixedSystemctl}"`,
    ]) {
      if (!source.includes(required)) {
        throw new Error(`${path.basename(scriptPath)} is missing ${required}`);
      }
    }
    for (const forbidden of [
      "QINTOPIA_SIDECAR_ENV_FILE",
      'SYSTEMCTL="${SYSTEMCTL:-',
      "eval ",
      "source ",
    ]) {
      if (source.includes(forbidden)) {
        throw new Error(`${path.basename(scriptPath)} exposes ${forbidden}`);
      }
    }
  }
  const activationSource = fs.readFileSync(activationScript, "utf8");
  const natsAclPreflightSource = fs.readFileSync(natsAclPreflightScript, "utf8");
  for (const required of [
    `RELEASE_CURRENT_DIR="${fixedReleaseCurrent}"`,
    `SHA256SUM="${fixedSha256sum}"`,
    "QINTOPIA_SPACE_AUTOMATION_EXECUTION_ENABLED",
    "QINTOPIA_SPACE_AUTOMATION_EXECUTION_APPROVAL",
    "QINTOPIA_SPACE_AUTOMATION_EXECUTION_DATABASE_URL_SHA256",
    "QINTOPIA_SPACE_AGENT_TURN_RUNTIME_READY",
    "QINTOPIA_SPACE_AUTOMATION_QIWE_ALLOWED_HOSTS",
    "QIWE_SPACE_TURN_POLICY_ENFORCEMENT_ENABLED",
    "QIWE_NATS_CAPTURE_ENABLED",
    "QIWE_SYSTEM_EVENT_DURABLE_CAPTURE_ENABLED",
    "QIWE_NATS_AUTH_FILE",
    "QINTOPIA_SIDECAR_NATS_AUTH_FILE",
    "QINTOPIA_SIDECAR_RAW_SUBJECT",
    "QINTOPIA_SIDECAR_MESSAGE_SUBJECT",
    "QINTOPIA_SIDECAR_TRUST_AUTHENTICATED_RAW_SUBJECT",
    "QINTOPIA_SIDECAR_CONSUMER",
    "space-automation-nats-acl-preflight.py",
    "manager.qiweapi.com",
    "qiwe-production-adapter",
    "huabaosi-feishu-mirror-adapter",
  ]) {
    if (!activationSource.includes(required)) {
      throw new Error(`activation is missing ${required}`);
    }
  }
  for (const required of [
    'NATS_HOST = "127.0.0.1"',
    "NATS_PORT = 4222",
    'TRUSTED_SUBJECT = "qintopia.qiwe.raw.authenticated"',
    'EXPECTED_STREAM = "QINTOPIA_QIWE_MESSAGES"',
    'PRODUCER_AUTH_FILE = "/etc/qintopia/nats/qiwe-adapter.json"',
    'CONSUMER_AUTH_FILE = "/etc/qintopia/nats/message-sidecar.json"',
    "space_scoped",
    "_assert_publish_denied",
    "_publish_with_ack",
    "_receive_probe",
  ]) {
    if (!natsAclPreflightSource.includes(required)) {
      throw new Error(`NATS ACL preflight is missing ${required}`);
    }
  }

  writeExecutable(primaryBin, "#!/usr/bin/env bash\nexit 0\n");
  writeExecutable(companionBin, "#!/usr/bin/env bash\nexit 0\n");
  fs.symlinkSync(releaseDir, releaseCurrent);
  fs.mkdirSync(path.join(procRoot, workerPid), { recursive: true });
  fs.symlinkSync(companionBin, path.join(procRoot, workerPid, "exe"));

  const writeManifest = (profile = "qiwe-production") =>
    fs.writeFileSync(
      manifestPath,
      `${JSON.stringify(
        {
          commit_sha: releaseSha,
          validation: {
            artifact_profile: profile,
            cargo_features: [
              "qiwe-production-adapter",
              "huabaosi-feishu-mirror-adapter",
            ],
          },
        },
        null,
        2
      )}\n`,
      "utf8"
    );
  writeManifest();

  const writeEnv = (enabled, overrides = {}) => {
    const values = {
      QINTOPIA_SPACE_AUTOMATION_EXECUTION_ENABLED: enabled,
      QINTOPIA_SPACE_AUTOMATION_EXECUTION_APPROVAL: executionApproval,
      QINTOPIA_SPACE_AUTOMATION_EXECUTION_DATABASE_URL_SHA256: databaseHash,
      QINTOPIA_SPACE_AGENT_TURN_RUNTIME_READY: "0",
      QINTOPIA_SPACE_AUTOMATION_QIWE_ALLOWED_HOSTS: "manager.qiweapi.com",
      QIWE_SPACE_TURN_POLICY_ENFORCEMENT_ENABLED: "1",
      QIWE_NATS_CAPTURE_ENABLED: "1",
      QIWE_SYSTEM_EVENT_DURABLE_CAPTURE_ENABLED: "1",
      QIWE_NATS_URL: "nats://127.0.0.1:4222",
      QIWE_NATS_AUTH_FILE: "/etc/qintopia/nats/qiwe-adapter.json",
      QIWE_NATS_AUTHENTICATED_RAW_SUBJECT: "qintopia.qiwe.raw.authenticated",
      QINTOPIA_SIDECAR_NATS_URL: "nats://127.0.0.1:4222",
      QINTOPIA_SIDECAR_NATS_AUTH_FILE: "/etc/qintopia/nats/message-sidecar.json",
      QINTOPIA_SIDECAR_RAW_SUBJECT: "qintopia.qiwe.raw",
      QINTOPIA_SIDECAR_AUTHENTICATED_RAW_SUBJECT: "qintopia.qiwe.raw.authenticated",
      QINTOPIA_SIDECAR_MESSAGE_SUBJECT: "qintopia.qiwe.message",
      QINTOPIA_SIDECAR_TRUST_AUTHENTICATED_RAW_SUBJECT: "true",
      QINTOPIA_SIDECAR_NATS_STREAM: "QINTOPIA_QIWE_MESSAGES",
      QINTOPIA_SIDECAR_CONSUMER: "qintopia-message-sidecar",
      QINTOPIA_SIDECAR_DATABASE_URL: databaseUrl,
      QINTOPIA_SIDECAR_MIGRATIONS_DIR: path.join(tmpRoot, "stale-migrations"),
      ...overrides,
    };
    fs.writeFileSync(
      envFile,
      `${Object.entries(values)
        .map(([key, value]) => `${key}=${value}`)
        .join("\n")}\n`,
      "utf8"
    );
  };

  const writeUnits = () => {
    fs.mkdirSync(unitDir, { recursive: true });
    fs.writeFileSync(
      path.join(unitDir, "qintopia-agentos-automation-dispatcher.service"),
      `[Unit]
Description=Qintopia AgentOS Space automation dispatcher
After=network-online.target postgresql.service
Wants=network-online.target

[Service]
Type=oneshot
User=ubuntu
Group=ubuntu
WorkingDirectory=${releaseDir}
EnvironmentFile=${envFile}
Environment=QINTOPIA_SIDECAR_MIGRATIONS_DIR=${releaseDir}/runtime/postgres/migrations
ExecStart=/usr/bin/env QINTOPIA_DEPLOYED_COMMIT_SHA=${releaseSha} QINTOPIA_SIDECAR_MIGRATIONS_DIR=${releaseDir}/runtime/postgres/migrations ${primaryBin} run-automation-dispatcher --once --apply
NoNewPrivileges=true
PrivateTmp=true
`,
      "utf8"
    );
    fs.writeFileSync(
      path.join(unitDir, "qintopia-agentos-automation-dispatcher.timer"),
      `[Unit]
Description=Run Qintopia AgentOS Space automation dispatcher

[Timer]
OnBootSec=1min
OnUnitActiveSec=1min
AccuracySec=30s
Persistent=true
Unit=qintopia-agentos-automation-dispatcher.service

[Install]
WantedBy=timers.target
`,
      "utf8"
    );
    fs.writeFileSync(
      path.join(unitDir, "qintopia-agentos-space-automation-execution-worker.service"),
      `[Unit]
Description=Qintopia AgentOS Space automation execution worker
After=network-online.target postgresql.service
Wants=network-online.target

[Service]
Type=simple
User=ubuntu
Group=ubuntu
WorkingDirectory=${releaseDir}
EnvironmentFile=${envFile}


Environment=QINTOPIA_SIDECAR_MIGRATIONS_DIR=${releaseDir}/runtime/postgres/migrations
# EnvironmentFile values override Environment values. Bind immutable release identity
# and migrations at the final exec boundary so stale persistent values cannot shadow
# this release.
ExecStart=/usr/bin/env QINTOPIA_DEPLOYED_COMMIT_SHA=${releaseSha} QINTOPIA_SIDECAR_MIGRATIONS_DIR=${releaseDir}/runtime/postgres/migrations ${companionBin} run-space-automation-execution-worker --apply
Restart=always
RestartSec=10
NoNewPrivileges=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
`,
      "utf8"
    );
  };
  writeUnits();

  writeExecutable(
    systemctl,
    `#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"${systemctlLog}"
unit="\${@: -1}"
enabled_state=""
active_state=""
default_enabled="0"
default_active="0"
case "$unit" in
  qintopia-agentos-automation-dispatcher.timer)
    enabled_state="${path.join(tmpRoot, "dispatcher-enabled.state")}"
    active_state="${path.join(tmpRoot, "dispatcher-active.state")}"
    default_enabled="\${FAKE_DISPATCHER_ENABLED:-0}"
    default_active="\${FAKE_DISPATCHER_ACTIVE:-0}"
    ;;
  qintopia-agentos-space-automation-execution-worker.service)
    enabled_state="${path.join(tmpRoot, "worker-enabled.state")}"
    active_state="${path.join(tmpRoot, "worker-active.state")}"
    default_enabled="\${FAKE_WORKER_ENABLED:-0}"
    default_active="\${FAKE_WORKER_ACTIVE:-0}"
    ;;
  qintopia-agentos-automation-dispatcher.service)
    active_state="${path.join(tmpRoot, "dispatcher-service-active.state")}"
    default_active="\${FAKE_DISPATCHER_SERVICE_ACTIVE:-0}"
    ;;
esac
if [[ "\${FAKE_SYSTEMCTL_FAIL_COMMAND:-}" == "$*" ]]; then
  exit 75
fi
case "$1" in
  enable)
    printf '1\n' >"$enabled_state"
    ;;
  disable)
    printf '0\n' >"$enabled_state"
    if [[ "$*" == *" --now "* ]]; then printf '0\n' >"$active_state"; fi
    ;;
  restart|start)
    printf '1\n' >"$active_state"
    ;;
  stop)
    printf '0\n' >"$active_state"
    ;;
  reset-failed)
    ;;
  is-enabled)
    value="$default_enabled"
    if [[ -f "$enabled_state" ]]; then value="$(<"$enabled_state")"; fi
    [[ "$value" == "1" ]]
    ;;
  is-active)
    value="$default_active"
    if [[ -f "$active_state" ]]; then value="$(<"$active_state")"; fi
    [[ "$value" == "1" ]]
    ;;
  show)
    case "$2" in
      --property=LoadState)
        printf 'loaded\n'
        ;;
      --property=UnitFileState)
        value="$default_enabled"
        if [[ -f "$enabled_state" ]]; then value="$(<"$enabled_state")"; fi
        if [[ "$value" == "1" ]]; then printf 'enabled\n'; else printf 'disabled\n'; fi
        ;;
      --property=ActiveState)
        value="$default_active"
        if [[ -f "$active_state" ]]; then value="$(<"$active_state")"; fi
        if [[ "$value" == "1" ]]; then printf 'active\n'; else printf 'inactive\n'; fi
        ;;
      --property=NextElapseUSecMonotonic)
        printf '%s\n' "\${FAKE_TIMER_NEXT_ELAPSE:-infinity}"
        ;;
      --property=MainPID)
        printf '%s\n' "\${FAKE_WORKER_PID:-0}"
        ;;
      --property=ExecMainStartTimestampMonotonic)
        printf '%s\n' "\${FAKE_WORKER_STARTED_MONOTONIC:-0}"
        ;;
      *) exit 64 ;;
    esac
    ;;
  *) exit 64 ;;
esac
`
  );
  writeExecutable(
    sha256sum,
    `#!/usr/bin/env bash
set -euo pipefail
input="$(cat)"
if [[ "$input" != "${databaseUrl}" ]]; then exit 2; fi
printf '%s  -\n' "${databaseHash}"
`
  );

  const observationEnv = (extra = {}) => ({
    ...process.env,
    QINTOPIA_SPACE_AUTOMATION_RUNTIME_OBSERVATION_ENABLE: "1",
    QINTOPIA_SPACE_AUTOMATION_RUNTIME_OBSERVATION_TEST_MODE: "1",
    QINTOPIA_SPACE_AUTOMATION_RUNTIME_OBSERVATION_TEST_ROOT: tmpRoot,
    QINTOPIA_SPACE_AUTOMATION_RUNTIME_OBSERVATION_TEST_PROC_ROOT: procRoot,
    QINTOPIA_SPACE_AUTOMATION_RUNTIME_EXPECTED_STATE: "auto",
    QINTOPIA_SIDECAR_ENV_FILE: envFile,
    QINTOPIA_RELEASE_CURRENT_DIR: releaseCurrent,
    QINTOPIA_SYSTEMD_UNIT_DIR: unitDir,
    SYSTEMCTL: systemctl,
    FAKE_WORKER_PID: workerPid,
    FAKE_WORKER_STARTED_MONOTONIC: "123456",
    ...extra,
  });
  const runObservation = (extra = {}) =>
    spawnSync("bash", [observationScript], {
      cwd: repoRoot,
      env: observationEnv(extra),
      encoding: "utf8",
    });

  const productionOverride = spawnSync("bash", [observationScript], {
    cwd: repoRoot,
    env: {
      ...process.env,
      QINTOPIA_SPACE_AUTOMATION_RUNTIME_OBSERVATION_ENABLE: "1",
      QINTOPIA_SIDECAR_ENV_FILE: envFile,
    },
    encoding: "utf8",
  });
  if (
    productionOverride.status === 0 ||
    !productionOverride.stderr.includes("fixed production paths")
  ) {
    throw new Error("production observation accepted an unreviewed path override");
  }

  writeEnv("1");
  fs.writeFileSync(systemctlLog, "", "utf8");
  const enabledObservation = runObservation({
    FAKE_DISPATCHER_ENABLED: "1",
    FAKE_DISPATCHER_ACTIVE: "1",
    FAKE_WORKER_ENABLED: "1",
    FAKE_WORKER_ACTIVE: "1",
    FAKE_TIMER_NEXT_ELAPSE: "1min",
  });
  if (
    enabledObservation.status !== 0 ||
    !enabledObservation.stdout.includes(
      "space_automation_runtime_observation_state=enabled"
    ) ||
    `${enabledObservation.stdout}\n${enabledObservation.stderr}`.includes(databaseUrl)
  ) {
    throw new Error(
      `enabled observation failed\n${enabledObservation.stdout}\n${enabledObservation.stderr}`
    );
  }

  writeEnv("1", { QINTOPIA_SPACE_AGENT_TURN_RUNTIME_READY: "1" });
  const unreviewedAgentTurnObservation = runObservation({
    FAKE_DISPATCHER_ENABLED: "1",
    FAKE_DISPATCHER_ACTIVE: "1",
    FAKE_WORKER_ENABLED: "1",
    FAKE_WORKER_ACTIVE: "1",
    FAKE_TIMER_NEXT_ELAPSE: "1min",
  });
  if (
    unreviewedAgentTurnObservation.status === 0 ||
    !unreviewedAgentTurnObservation.stderr.includes(
      "agent-turn readiness to remain disabled"
    )
  ) {
    throw new Error(
      "observation accepted agent-turn readiness without a dedicated runtime"
    );
  }
  writeEnv("1");

  const noFutureTrigger = runObservation({
    FAKE_DISPATCHER_ENABLED: "1",
    FAKE_DISPATCHER_ACTIVE: "1",
    FAKE_WORKER_ENABLED: "1",
    FAKE_WORKER_ACTIVE: "1",
    FAKE_TIMER_NEXT_ELAPSE: "infinity",
  });
  if (noFutureTrigger.status === 0) {
    throw new Error("enabled observation accepted a timer without a schedule value");
  }

  const wrongWorkerProcess = runObservation({
    FAKE_DISPATCHER_ENABLED: "1",
    FAKE_DISPATCHER_ACTIVE: "1",
    FAKE_WORKER_ENABLED: "1",
    FAKE_WORKER_ACTIVE: "1",
    FAKE_TIMER_NEXT_ELAPSE: "1min",
    FAKE_WORKER_PID: "9999",
  });
  if (
    wrongWorkerProcess.status === 0 ||
    !wrongWorkerProcess.stderr.includes("current reviewed companion binary")
  ) {
    throw new Error("enabled observation accepted an unverified worker process");
  }

  const workerUnit = path.join(
    unitDir,
    "qintopia-agentos-space-automation-execution-worker.service"
  );
  fs.appendFileSync(workerUnit, "ExecStartPost=/bin/false\n", "utf8");
  const alteredUnit = runObservation({
    FAKE_DISPATCHER_ENABLED: "1",
    FAKE_DISPATCHER_ACTIVE: "1",
    FAKE_WORKER_ENABLED: "1",
    FAKE_WORKER_ACTIVE: "1",
    FAKE_TIMER_NEXT_ELAPSE: "1min",
  });
  if (
    alteredUnit.status === 0 ||
    !alteredUnit.stderr.includes("unreviewed systemd unit content")
  ) {
    throw new Error("observation accepted altered systemd unit content");
  }
  writeUnits();

  writeManifest("huabaosi-production");
  const wrongArtifact = runObservation();
  if (wrongArtifact.status === 0) {
    throw new Error("observation accepted the wrong companion artifact profile");
  }
  writeManifest();

  writeEnv("0", {
    QINTOPIA_IGNORED_VALUE: `$(touch ${commandSubstitutionMarker})`,
  });
  const disabledObservation = runObservation();
  if (
    disabledObservation.status !== 0 ||
    !disabledObservation.stdout.includes(
      "space_automation_runtime_observation_state=disabled"
    ) ||
    fs.existsSync(commandSubstitutionMarker)
  ) {
    throw new Error(
      `disabled observation failed\n${disabledObservation.stdout}\n${disabledObservation.stderr}`
    );
  }
  const disabledWithWorker = runObservation({ FAKE_WORKER_ENABLED: "1" });
  if (disabledWithWorker.status === 0) {
    throw new Error("disabled observation accepted an enabled execution worker");
  }

  const fixtureDir = path.join(tmpRoot, "activation-fixture");
  const activationFixture = path.join(
    fixtureDir,
    "activate-space-automation-runtime-production.sh"
  );
  const rollbackFixture = path.join(
    fixtureDir,
    "rollback-space-automation-runtime-production.sh"
  );
  const observationFixture = path.join(
    fixtureDir,
    "space-automation-runtime-production-observation-smoke.sh"
  );
  const natsAclPreflightFixture = path.join(
    fixtureDir,
    "space-automation-nats-acl-preflight.py"
  );
  const rewriteFixedPaths = (source) =>
    source
      .replaceAll(fixedEnvFile, envFile)
      .replaceAll(fixedReleaseCurrent, releaseCurrent)
      .replaceAll(fixedSystemctl, systemctl)
      .replaceAll(fixedSha256sum, sha256sum);
  writeExecutable(activationFixture, rewriteFixedPaths(activationSource));
  writeExecutable(
    rollbackFixture,
    rewriteFixedPaths(fs.readFileSync(rollbackScript, "utf8"))
  );
  writeExecutable(
    observationFixture,
    `#!/usr/bin/env bash
set -euo pipefail
if [[ -n "\${QINTOPIA_UNRELATED_RUNTIME_SECRET:-}" ]]; then
  echo "ambient secret reached observation" >&2
  exit 70
fi
if [[ "\${QINTOPIA_SPACE_AUTOMATION_RUNTIME_OBSERVATION_ENABLE:-}" != "1" ]]; then exit 71; fi
if [[ -f "${path.join(tmpRoot, "force-observation-failure")}" ]]; then exit 72; fi
printf '%s\n' "observation \${QINTOPIA_SPACE_AUTOMATION_RUNTIME_EXPECTED_STATE:-missing}" >>"${systemctlLog}"
`
  );
  writeExecutable(
    natsAclPreflightFixture,
    `#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' 'nats-acl-preflight' >>"${systemctlLog}"
if [[ -f "${path.join(tmpRoot, "force-nats-acl-preflight-failure")}" ]]; then
  exit 73
fi
`
  );

  const runFixture = (scriptPath, extra = {}) =>
    spawnSync("bash", [scriptPath], {
      cwd: repoRoot,
      env: {
        ...process.env,
        QINTOPIA_UNRELATED_RUNTIME_SECRET: "must-not-reach-observation",
        ...extra,
      },
      encoding: "utf8",
    });

  writeManifest();
  writeEnv("1");
  fs.writeFileSync(systemctlLog, "", "utf8");
  const deniedActivation = runFixture(activationFixture);
  if (deniedActivation.status === 0 || fs.readFileSync(systemctlLog, "utf8") !== "") {
    throw new Error("activation must fail before systemctl without owner approval");
  }

  writeEnv("1", { QINTOPIA_SPACE_AGENT_TURN_RUNTIME_READY: "1" });
  fs.writeFileSync(systemctlLog, "", "utf8");
  const unreviewedAgentTurnActivation = runFixture(activationFixture, {
    QINTOPIA_SPACE_AUTOMATION_RUNTIME_ACTIVATION: approval,
  });
  if (
    unreviewedAgentTurnActivation.status === 0 ||
    fs.readFileSync(systemctlLog, "utf8") !== ""
  ) {
    throw new Error(
      "activation must reject agent-turn readiness before broker and runner provisioning"
    );
  }

  writeEnv("1", {
    QINTOPIA_SPACE_AUTOMATION_QIWE_ALLOWED_HOSTS: "example.invalid",
  });
  fs.writeFileSync(systemctlLog, "", "utf8");
  const wrongHost = runFixture(activationFixture, {
    QINTOPIA_SPACE_AUTOMATION_RUNTIME_ACTIVATION: approval,
  });
  if (wrongHost.status === 0 || fs.readFileSync(systemctlLog, "utf8") !== "") {
    throw new Error("activation must reject an unreviewed Qiwe host before systemctl");
  }

  writeEnv("1", {
    QIWE_SPACE_TURN_POLICY_ENFORCEMENT_ENABLED: "0",
  });
  fs.writeFileSync(systemctlLog, "", "utf8");
  const turnPolicyDisabled = runFixture(activationFixture, {
    QINTOPIA_SPACE_AUTOMATION_RUNTIME_ACTIVATION: approval,
  });
  if (turnPolicyDisabled.status === 0 || fs.readFileSync(systemctlLog, "utf8") !== "") {
    throw new Error(
      "activation must require Space turn policy enforcement before systemctl"
    );
  }

  writeEnv("1", {
    QINTOPIA_SPACE_AUTOMATION_EXECUTION_DATABASE_URL_SHA256: "b".repeat(64),
  });
  fs.writeFileSync(systemctlLog, "", "utf8");
  const wrongHash = runFixture(activationFixture, {
    QINTOPIA_SPACE_AUTOMATION_RUNTIME_ACTIVATION: approval,
  });
  if (
    wrongHash.status === 0 ||
    fs.readFileSync(systemctlLog, "utf8") !== "" ||
    `${wrongHash.stdout}\n${wrongHash.stderr}`.includes(databaseUrl)
  ) {
    throw new Error(
      "activation must fail closed without leaking a mismatched database URL"
    );
  }

  writeEnv("1");
  writeManifest("huabaosi-production");
  fs.writeFileSync(systemctlLog, "", "utf8");
  const wrongActivationArtifact = runFixture(activationFixture, {
    QINTOPIA_SPACE_AUTOMATION_RUNTIME_ACTIVATION: approval,
  });
  if (
    wrongActivationArtifact.status === 0 ||
    fs.readFileSync(systemctlLog, "utf8") !== ""
  ) {
    throw new Error(
      "activation must reject the wrong companion artifact before systemctl"
    );
  }
  writeManifest();

  writeEnv("1");
  const natsAclFailureMarker = path.join(tmpRoot, "force-nats-acl-preflight-failure");
  fs.writeFileSync(natsAclFailureMarker, "1\n", "utf8");
  fs.writeFileSync(systemctlLog, "", "utf8");
  const failedNatsAclPreflight = runFixture(activationFixture, {
    QINTOPIA_SPACE_AUTOMATION_RUNTIME_ACTIVATION: approval,
  });
  fs.rmSync(natsAclFailureMarker, { force: true });
  if (
    failedNatsAclPreflight.status === 0 ||
    fs.readFileSync(systemctlLog, "utf8") !== "nats-acl-preflight\n"
  ) {
    throw new Error("activation must fail before systemctl when NATS ACL proof fails");
  }

  const observationFailureMarker = path.join(tmpRoot, "force-observation-failure");
  fs.writeFileSync(observationFailureMarker, "1\n", "utf8");
  fs.writeFileSync(systemctlLog, "", "utf8");
  const failedObservation = runFixture(activationFixture, {
    QINTOPIA_SPACE_AUTOMATION_RUNTIME_ACTIVATION: approval,
    FAKE_DISPATCHER_ENABLED: "1",
    FAKE_DISPATCHER_ACTIVE: "1",
    FAKE_WORKER_ENABLED: "1",
    FAKE_WORKER_ACTIVE: "1",
    FAKE_TIMER_NEXT_ELAPSE: "1min",
  });
  fs.rmSync(observationFailureMarker, { force: true });
  const failedObservationLog = fs.readFileSync(systemctlLog, "utf8");
  if (failedObservation.status === 0) {
    throw new Error("activation must fail when the enabled-state observation fails");
  }
  for (const command of [
    "disable --now qintopia-agentos-automation-dispatcher.timer",
    "disable --now qintopia-agentos-space-automation-execution-worker.service",
    "stop qintopia-agentos-automation-dispatcher.service",
    "reset-failed qintopia-agentos-space-automation-execution-worker.service",
  ]) {
    if (!failedObservationLog.includes(command)) {
      throw new Error(`activation failure cleanup is missing ${command}`);
    }
  }

  fs.writeFileSync(observationFailureMarker, "1\n", "utf8");
  fs.writeFileSync(systemctlLog, "", "utf8");
  const failedActivationCleanup = runFixture(activationFixture, {
    QINTOPIA_SPACE_AUTOMATION_RUNTIME_ACTIVATION: approval,
    FAKE_DISPATCHER_ENABLED: "1",
    FAKE_DISPATCHER_ACTIVE: "1",
    FAKE_WORKER_ENABLED: "1",
    FAKE_WORKER_ACTIVE: "1",
    FAKE_TIMER_NEXT_ELAPSE: "1min",
    FAKE_SYSTEMCTL_FAIL_COMMAND:
      "disable --now qintopia-agentos-automation-dispatcher.timer",
  });
  fs.rmSync(observationFailureMarker, { force: true });
  const failedActivationCleanupLog = fs.readFileSync(systemctlLog, "utf8");
  if (
    failedActivationCleanup.status === 0 ||
    !failedActivationCleanup.stderr.includes("shutdown could not be proven")
  ) {
    throw new Error("activation accepted an unproven failure cleanup");
  }
  for (const command of [
    "disable --now qintopia-agentos-space-automation-execution-worker.service",
    "stop qintopia-agentos-automation-dispatcher.service",
    "stop qintopia-agentos-space-automation-execution-worker.service",
    "is-active --quiet qintopia-agentos-space-automation-execution-worker.service",
  ]) {
    if (!failedActivationCleanupLog.includes(command)) {
      throw new Error(`activation cleanup failure did not attempt ${command}`);
    }
  }

  fs.writeFileSync(systemctlLog, "", "utf8");
  const activated = runFixture(activationFixture, {
    QINTOPIA_SPACE_AUTOMATION_RUNTIME_ACTIVATION: approval,
    FAKE_DISPATCHER_ENABLED: "1",
    FAKE_DISPATCHER_ACTIVE: "1",
    FAKE_WORKER_ENABLED: "1",
    FAKE_WORKER_ACTIVE: "1",
    FAKE_TIMER_NEXT_ELAPSE: "1min",
  });
  if (activated.status !== 0) {
    throw new Error(`activation failed\n${activated.stdout}\n${activated.stderr}`);
  }
  const activationLog = fs.readFileSync(systemctlLog, "utf8");
  if (!activationLog.startsWith("nats-acl-preflight\n")) {
    throw new Error("activation must prove NATS ACL before systemctl mutation");
  }
  for (const command of [
    "enable qintopia-agentos-automation-dispatcher.timer",
    "restart qintopia-agentos-automation-dispatcher.timer",
    "enable qintopia-agentos-space-automation-execution-worker.service",
    "restart qintopia-agentos-space-automation-execution-worker.service",
    "observation enabled",
  ]) {
    if (!activationLog.includes(command)) {
      throw new Error(`activation is missing ${command}`);
    }
  }

  fs.writeFileSync(systemctlLog, "", "utf8");
  const deniedRollback = runFixture(rollbackFixture);
  if (deniedRollback.status === 0 || fs.readFileSync(systemctlLog, "utf8") !== "") {
    throw new Error("rollback must fail before systemctl without owner approval");
  }

  writeEnv("0");
  fs.writeFileSync(systemctlLog, "", "utf8");
  const failedRollbackShutdown = runFixture(rollbackFixture, {
    QINTOPIA_SPACE_AUTOMATION_RUNTIME_ROLLBACK: rollbackApproval,
    FAKE_SYSTEMCTL_FAIL_COMMAND:
      "disable --now qintopia-agentos-automation-dispatcher.timer",
  });
  const failedRollbackShutdownLog = fs.readFileSync(systemctlLog, "utf8");
  if (
    failedRollbackShutdown.status === 0 ||
    !failedRollbackShutdown.stderr.includes("could not prove all runtime units stopped")
  ) {
    throw new Error("rollback accepted an unproven shutdown");
  }
  for (const command of [
    "disable --now qintopia-agentos-space-automation-execution-worker.service",
    "stop qintopia-agentos-automation-dispatcher.service",
    "stop qintopia-agentos-space-automation-execution-worker.service",
    "is-active --quiet qintopia-agentos-space-automation-execution-worker.service",
  ]) {
    if (!failedRollbackShutdownLog.includes(command)) {
      throw new Error(`rollback shutdown failure did not attempt ${command}`);
    }
  }

  writeEnv("1");
  fs.writeFileSync(systemctlLog, "", "utf8");
  const stillEnabledRollback = runFixture(rollbackFixture, {
    QINTOPIA_SPACE_AUTOMATION_RUNTIME_ROLLBACK: rollbackApproval,
  });
  const stillEnabledLog = fs.readFileSync(systemctlLog, "utf8");
  if (
    stillEnabledRollback.status === 0 ||
    !stillEnabledLog.startsWith(
      "disable --now qintopia-agentos-automation-dispatcher.timer\n"
    )
  ) {
    throw new Error(
      "rollback must stop runtime state before requiring persistent disablement"
    );
  }

  writeEnv("0");
  fs.writeFileSync(systemctlLog, "", "utf8");
  const rolledBack = runFixture(rollbackFixture, {
    QINTOPIA_SPACE_AUTOMATION_RUNTIME_ROLLBACK: rollbackApproval,
  });
  if (rolledBack.status !== 0) {
    throw new Error(`rollback failed\n${rolledBack.stdout}\n${rolledBack.stderr}`);
  }
  const rollbackLog = fs.readFileSync(systemctlLog, "utf8");
  for (const command of [
    "disable --now qintopia-agentos-automation-dispatcher.timer",
    "disable --now qintopia-agentos-space-automation-execution-worker.service",
    "stop qintopia-agentos-automation-dispatcher.service",
    "stop qintopia-agentos-space-automation-execution-worker.service",
    "reset-failed qintopia-agentos-automation-dispatcher.service",
    "reset-failed qintopia-agentos-space-automation-execution-worker.service",
    "observation disabled",
  ]) {
    if (!rollbackLog.includes(command)) {
      throw new Error(`rollback is missing ${command}`);
    }
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Space automation runtime production control test passed.");
