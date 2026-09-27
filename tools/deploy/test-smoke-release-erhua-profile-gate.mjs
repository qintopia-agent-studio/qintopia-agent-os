#!/usr/bin/env node

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.realpathSync(
  fs.mkdtempSync(path.join(os.tmpdir(), "qintopia-smoke-release-"))
);
const releaseRoot = path.join(tmpRoot, "releases");
const releaseDir = path.join(releaseRoot, "a".repeat(40));
const smokePath = path.join(releaseDir, "deploy/runner/smoke-release.sh");
const helperPath = path.join(releaseDir, "runtime/hermes/restart_anan.py");
const requestId = "deploy-20260927T010203Z-abcdef0";
const receiptPath = path.join(tmpRoot, "state/results", `${requestId}.anan-drain.json`);

const writeExecutable = (relativePath, content) => {
  const target = path.join(tmpRoot, relativePath);
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(target, content, { mode: 0o755 });
};

const runSmoke = (extraArgs = [], restartTargets = "hermes-erhua", mockEnv = {}) =>
  spawnSync(
    "bash",
    [
      smokePath,
      "--release-root",
      releaseRoot,
      "--restart-targets",
      restartTargets,
      ...extraArgs,
    ],
    {
      cwd: repoRoot,
      env: {
        ...process.env,
        PATH: `${path.join(tmpRoot, "bin")}${path.delimiter}${process.env.PATH ?? ""}`,
        QINTOPIA_HERMES_SYSTEMD_USER: "ubuntu",
        QINTOPIA_ERHUA_PROFILE_DIR: path.join(tmpRoot, "profiles", "erhua"),
        QINTOPIA_HERMES_BIN: "/home/ubuntu/.local/bin/hermes",
        QINTOPIA_HERMES_PYTHON: "/home/ubuntu/.hermes/hermes-agent/venv/bin/python",
        QINTOPIA_DEPLOY_REQUEST_ID: requestId,
        QINTOPIA_DEPLOY_RUNNER_STATE_DIR: path.join(tmpRoot, "state"),
        ...mockEnv,
      },
      encoding: "utf8",
    }
  );

try {
  fs.mkdirSync(path.join(tmpRoot, "profiles", "erhua"), { recursive: true });
  fs.mkdirSync(path.dirname(smokePath), { recursive: true });
  fs.mkdirSync(path.dirname(helperPath), { recursive: true });
  fs.mkdirSync(path.dirname(receiptPath), { recursive: true });
  fs.copyFileSync(path.join(repoRoot, "deploy/runner/smoke-release.sh"), smokePath);
  fs.chmodSync(smokePath, 0o755);
  fs.copyFileSync(path.join(repoRoot, "runtime/hermes/restart_anan.py"), helperPath);
  fs.symlinkSync(releaseDir, path.join(releaseRoot, "current"));
  writeExecutable(
    "bin/runuser",
    `#!/usr/bin/env bash
joined="$*"
if [[ "$joined" == *"verify_runtime_provider.py"* || "$joined" == *"--profile erhua doctor"* ]]; then
  echo "ordinary restart must not run Erhua provider checks" >&2
  exit 41
fi
exit 0
`
  );

  const ordinaryRestart = runSmoke();
  if (ordinaryRestart.status !== 0) {
    throw new Error(
      `ordinary Erhua restart must not require profile overlay verification\nstdout:\n${ordinaryRestart.stdout}\nstderr:\n${ordinaryRestart.stderr}`
    );
  }

  writeExecutable("bin/id", "#!/usr/bin/env bash\necho 1000\n");
  const ananCalls = path.join(tmpRoot, "anan-calls");
  const ananArgs = path.join(tmpRoot, "anan-args");
  const ananUnit = "qintopia-agent-os-anan-drain-restart.service";
  const ananPython =
    "/home/ubuntu/.local/share/hermes-releases/v2026.9.21/venv/bin/python";
  const invocation = "a".repeat(32);
  writeExecutable(
    "bin/systemctl",
    [
      "#!/usr/bin/env bash",
      'if [[ "$1" == show && "$2" == qintopia-agent-os-anan-drain-restart.service ]]; then',
      '  if [[ "$QINTOPIA_TEST_ANAN_LOAD_STATE" == error ]]; then exit 51; fi',
      '  printf "%s\\n" "$QINTOPIA_TEST_ANAN_LOAD_STATE"',
      "  exit 0",
      "fi",
      'if [[ "$1" == restart && "$2" == qintopia-agentos-daily-digest-publisher.service ]]; then',
      "  exit 77",
      "fi",
      "exit 0",
    ].join("\n") + "\n"
  );
  writeExecutable(
    "bin/systemd-run",
    [
      "#!/usr/bin/env bash",
      'printf "%s\\n" "$*" >> ' + JSON.stringify(ananCalls),
      'printf "%s\\n" "$@" > ' + JSON.stringify(ananArgs),
      'if [[ "$QINTOPIA_TEST_ANAN_MODE" == exit-failure ]]; then exit 44; fi',
      "invocation=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      'if [[ "$QINTOPIA_TEST_ANAN_MODE" == bad-invocation ]]; then invocation=invalid; fi',
      'if [[ "$QINTOPIA_TEST_ANAN_MODE" != missing-invocation ]]; then',
      '  printf "Running as unit: qintopia-agent-os-anan-drain-restart.service; invocation ID: %s\\n" "$invocation"',
      "fi",
      "result=success",
      'if [[ "$QINTOPIA_TEST_ANAN_MODE" == failed-result ]]; then result=exit-code; fi',
      'printf "Finished with result: %s\\n" "$result"',
      "status=0",
      'if [[ "$QINTOPIA_TEST_ANAN_MODE" == failed-exit ]]; then status=1; fi',
      'printf "Main processes terminated with: code=exited/status=%s\\n" "$status"',
    ].join("\n") + "\n"
  );
  const expectedAnanArgs = [
    `--unit=${ananUnit}`,
    "--service-type=oneshot",
    "--wait",
    "--uid=ubuntu",
    "--gid=ubuntu",
    "--property=TimeoutStartSec=infinity",
    "--property=NoNewPrivileges=yes",
    "--property=PrivateTmp=yes",
    "--property=ProtectSystem=strict",
    "--property=ProtectHome=read-only",
    "--property=ReadWritePaths=/home/ubuntu/.hermes/profiles/anan",
    "--property=Environment=XDG_RUNTIME_DIR=/run/user/1000",
    "--property=Environment=DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus",
    "--property=Environment=XDG_CONFIG_HOME=/home/ubuntu/.config",
    "--property=Environment=XDG_DATA_HOME=/home/ubuntu/.local/share",
    "--property=Environment=XDG_STATE_HOME=/home/ubuntu/.local/state",
    "--property=Environment=PYTHONDONTWRITEBYTECODE=1",
    ananPython,
    helperPath,
  ];
  const runAnan = (mode, loadState = "not-found") => {
    for (const target of [ananCalls, ananArgs, receiptPath])
      fs.rmSync(target, { force: true });
    return runSmoke([], "hermes-anan", {
      QINTOPIA_TEST_ANAN_MODE: mode,
      QINTOPIA_TEST_ANAN_LOAD_STATE: loadState,
    });
  };
  const assertAnanInvocation = () => {
    assert.deepEqual(fs.readFileSync(ananCalls, "utf8").trim().split("\n").length, 1);
    assert.deepEqual(
      fs.readFileSync(ananArgs, "utf8").trim().split("\n"),
      expectedAnanArgs
    );
  };
  const ananRestart = runAnan("success");
  assert.equal(ananRestart.status, 0, ananRestart.stderr);
  assertAnanInvocation();
  assert.deepEqual(JSON.parse(fs.readFileSync(receiptPath, "utf8")), {
    unit: ananUnit,
    invocation_id: invocation,
    exec_start: [ananPython, helperPath],
    result: "success",
    exec_main_status: 0,
  });
  assert.match(
    ananRestart.stdout,
    new RegExp(`invocation=${invocation} result=success exit=0`)
  );
  for (const [mode, marker] of [
    ["exit-failure", "drain-or-restart;subject=hermes-gateway-anan.service"],
    ["missing-invocation", `unit-result;subject=${ananUnit}`],
    ["bad-invocation", `unit-result;subject=${ananUnit}`],
    ["failed-result", `unit-result;subject=${ananUnit}`],
    ["failed-exit", `unit-result;subject=${ananUnit}`],
  ]) {
    const result = runAnan(mode);
    assert.notEqual(result.status, 0, mode);
    assertAnanInvocation();
    assert.match(result.stderr, new RegExp(`target=hermes-anan;phase=${marker}`));
    assert.equal(fs.existsSync(receiptPath), false, `${mode}: wrote success receipt`);
  }
  for (const [loadState, marker] of [
    ["loaded", "unit-busy"],
    ["error", "unit-state"],
  ]) {
    const result = runAnan("success", loadState);
    assert.notEqual(result.status, 0, loadState);
    assert.match(
      result.stderr,
      new RegExp(`target=hermes-anan;phase=${marker};subject=${ananUnit}`)
    );
    assert.equal(fs.existsSync(ananCalls), false, `${loadState}: invoked busy unit`);
    assert.equal(fs.existsSync(receiptPath), false);
  }
  const systemServiceRestart = runSmoke([], "qintopia-system-services");
  if (systemServiceRestart.status === 0) {
    throw new Error(
      `expected fixed system service restart failure\nstdout:\n${systemServiceRestart.stdout}\nstderr:\n${systemServiceRestart.stderr}`
    );
  }
  if (
    !systemServiceRestart.stderr.includes(
      "qintopia_smoke_release_safe_failure=target=qintopia-system-services;phase=restart;subject=qintopia-agentos-daily-digest-publisher.service"
    )
  ) {
    throw new Error(
      `system service restart failure did not emit safe marker\nstderr:\n${systemServiceRestart.stderr}`
    );
  }

  const metadataPath = path.join(tmpRoot, "profile-backups", "metadata.json");
  fs.mkdirSync(path.dirname(metadataPath), { recursive: true });
  fs.writeFileSync(metadataPath, "{}\n");
  writeExecutable(
    "bin/python3",
    `#!/usr/bin/env bash
if [[ "$*" == *"verify-activated"* ]]; then
  echo "activated-file verification reached" >&2
  exit 42
fi
exec /usr/bin/python3 "$@"
`
  );
  const profileActivation = runSmoke(["--profile-metadata", metadataPath]);
  if (profileActivation.status === 0) {
    throw new Error(
      "profile activation smoke must still require activated-file verification"
    );
  }
  assert.match(profileActivation.stderr, /activated-file verification reached/);
  assert.equal(fs.existsSync(ananCalls), false);
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Smoke release Erhua profile gate tests passed.");
