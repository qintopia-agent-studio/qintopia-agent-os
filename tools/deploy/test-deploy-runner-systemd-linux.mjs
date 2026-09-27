#!/usr/bin/env node

import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawn, spawnSync } from "node:child_process";

if (process.argv[2] === "--anan-helper-only") {
  const helper = process.argv[3];
  assert.ok(helper && fs.existsSync(helper), "official Anan helper path is required");
  const exercise = `import importlib.util, json, sys
spec = importlib.util.spec_from_file_location("restart_anan", sys.argv[1])
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
class Control:
    def __init__(self, existing=None): self.marker = existing; self.clears = 0
    def read_drain_request(self, **kwargs): return self.marker
    def write_drain_request(self, **kwargs):
        self.marker = {"principal": kwargs["principal"]}; return self.marker
    def clear_drain_request(self, **kwargs):
        self.marker = None; self.clears += 1; return True
def scenario(records, timeout=10, existing=None):
    control = Control(existing); now = [0]; restarts = []; index = [0]
    def snapshot():
        value = records[min(index[0], len(records)-1)]; index[0] += 1; return value
    def sleep(seconds): now[0] += seconds
    outcome = "ready"
    try:
        module.drain_and_restart(control, snapshot, lambda: restarts.append(now[0]),
            home="simulated-profile", pid=42, clock=lambda: now[0], sleep=sleep, timeout=timeout)
    except module.Deferred as exc: outcome = str(exc)
    return outcome, restarts, control.clears, control.marker
def status(count, updated):
    return {"pid": 42, "active_agents": count, "gateway_state": "draining", "updated_at": updated}
success = scenario([status(1,"t0"), status(0,"t1"), status(0,"t2"), status(0,"t3")])
timeout = scenario([status(1,"t0")], timeout=3)
existing = scenario([], existing={"principal":"operator"})
assert success == ("ready", [3], 1, None), success
assert timeout == ("active_work_timeout", [], 1, None), timeout
assert existing == ("existing_drain", [], 0, {"principal":"operator"}), existing
print(json.dumps({"official_helper_zero_work": success[0],
                  "official_helper_timeout": timeout[0], "existing_drain": existing[0]}))
`;
  const result = spawnSync("python3", ["-c", exercise, helper], { encoding: "utf8" });
  if (result.status !== 0)
    throw new Error(`official Anan helper exercise failed: ${result.stderr}`);
  console.log(result.stdout.trim());
  process.exit(0);
}

if (
  process.getuid?.() !== 0 ||
  fs.readFileSync("/proc/1/comm", "utf8").trim() !== "systemd"
) {
  throw new Error("run as root inside the disposable Ubuntu 24.04 systemd PID1 VM");
}
if (process.argv[2] === "--anan-entry") {
  const [smokeSource, helperSource] = process.argv.slice(3);
  for (const source of [smokeSource, helperSource]) {
    assert.ok(source && fs.existsSync(source), "official Anan entry source is missing");
  }
  const core = "/home/ubuntu/.local/share/hermes-releases/v2026.9.21";
  const profile = "/home/ubuntu/.hermes/profiles/anan";
  const releaseRoot = "/home/ubuntu/qintopia-agent-os-releases";
  const release = path.join(releaseRoot, "a".repeat(40));
  const userUnit = "/home/ubuntu/.config/systemd/user/hermes-gateway-anan.service";
  const stateDir = fs.mkdtempSync("/tmp/anan-entry-state-");
  const requestId = "deploy-20260927T010203Z-abcdef0";
  const uid = 1000;
  const bus = `unix:path=/run/user/${uid}/bus`;
  const userEnv = {
    ...process.env,
    XDG_RUNTIME_DIR: `/run/user/${uid}`,
    DBUS_SESSION_BUS_ADDRESS: bus,
  };
  const run = (command, args, options = {}) =>
    spawnSync(command, args, {
      encoding: "utf8",
      ...options,
    });
  const asUser = (...args) =>
    run("runuser", [
      "-u",
      "ubuntu",
      "--",
      "env",
      `XDG_RUNTIME_DIR=/run/user/${uid}`,
      `DBUS_SESSION_BUS_ADDRESS=${bus}`,
      "systemctl",
      "--user",
      ...args,
    ]);
  const check = (result, label) => {
    assert.equal(result.status, 0, `${label}: ${result.stderr || result.stdout}`);
    return result;
  };
  const write = (target, content, mode = 0o644) => {
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, content, { mode });
    fs.chmodSync(target, mode);
  };
  for (const target of [core, releaseRoot, userUnit]) {
    assert.equal(
      fs.existsSync(target),
      false,
      `Anan entry fixture path exists: ${target}`
    );
  }
  const preexistingProfile = fs.existsSync(profile);
  if (preexistingProfile) {
    assert.deepEqual(
      fs.readdirSync(profile),
      [],
      "simulated Anan profile is not empty"
    );
  }
  let unitStarted = false;
  try {
    const python = path.join(core, "venv/bin/python");
    fs.mkdirSync(path.dirname(python), { recursive: true });
    fs.symlinkSync("/usr/bin/python3", python);
    write(path.join(core, "gateway/__init__.py"), "");
    write(
      path.join(core, "gateway/drain_control.py"),
      `import json
from pathlib import Path
def drain_request_path(home): return Path(home) / "simulated-drain.json"
def read_drain_request(home):
    p = drain_request_path(home)
    return json.loads(p.read_text()) if p.exists() else None
def write_drain_request(home, principal, suppress_notification):
    record = {"principal": principal}
    drain_request_path(home).write_text(json.dumps(record))
    return record
def clear_drain_request(home):
    drain_request_path(home).unlink()
    return True
`
    );
    write(
      path.join(core, "gateway/status.py"),
      `import subprocess, time
def read_runtime_status(path):
    pid = int(subprocess.check_output(["systemctl", "--user", "show",
        "hermes-gateway-anan.service", "--property=MainPID", "--value"]))
    return {"pid": pid, "active_agents": 0, "gateway_state": "draining",
            "updated_at": str(time.monotonic())}
def runtime_status_is_stale(record, ttl_s): return False
def runtime_status_pid_is_live(record): return record["pid"] > 0
`
    );
    fs.mkdirSync(profile, { recursive: true });
    write(
      userUnit,
      `[Unit]\nDescription=Simulated Anan gateway\n[Service]\nType=simple\nWorkingDirectory=${profile}\nExecStart=${python} -c 'import time; time.sleep(3600)'\n`
    );
    const smoke = path.join(release, "deploy/runner/smoke-release.sh");
    const helper = path.join(release, "runtime/hermes/restart_anan.py");
    fs.mkdirSync(path.join(stateDir, "results"));
    write(smoke, fs.readFileSync(smokeSource), 0o755);
    write(helper, fs.readFileSync(helperSource));
    check(
      run("chown", [
        "-R",
        "ubuntu:ubuntu",
        core,
        profile,
        "/home/ubuntu/.config/systemd/user",
      ]),
      "fixture ownership"
    );
    check(asUser("daemon-reload"), "user daemon reload");
    check(asUser("start", "hermes-gateway-anan.service"), "start simulated gateway");
    unitStarted = true;
    const before = check(
      asUser("show", "hermes-gateway-anan.service", "--property=MainPID", "--value"),
      "initial gateway PID"
    ).stdout.trim();
    assert.match(before, /^[1-9][0-9]*$/);
    const result = run(
      "bash",
      [smoke, "--restart-targets", "hermes-anan", "--release-root", releaseRoot],
      {
        env: {
          ...process.env,
          QINTOPIA_DEPLOY_REQUEST_ID: requestId,
          QINTOPIA_DEPLOY_RUNNER_STATE_DIR: stateDir,
        },
      }
    );
    assert.equal(
      result.status,
      0,
      `official Anan smoke failed: ${result.stderr || result.stdout}`
    );
    const receipt = JSON.parse(
      fs.readFileSync(
        path.join(stateDir, "results", `${requestId}.anan-drain.json`),
        "utf8"
      )
    );
    assert.equal(receipt.result, "success");
    assert.equal(receipt.exec_main_status, 0);
    const after = check(
      asUser("show", "hermes-gateway-anan.service", "--property=MainPID", "--value"),
      "restarted gateway PID"
    ).stdout.trim();
    assert.notEqual(after, before, "Anan gateway did not restart");
    assert.equal(
      fs.existsSync(path.join(profile, "simulated-drain.json")),
      false,
      "official helper left its drain marker"
    );
    const invocation = receipt.invocation_id;
    assert.match(invocation, /^[0-9a-f]{32}$/);
    assert.match(
      result.stdout,
      new RegExp(`invocation=${invocation} result=success exit=0`)
    );
    console.log(
      JSON.stringify({
        officialEntry: true,
        smokeEntry: true,
        userSystemd: true,
        gatewayRestarted: true,
        invocation,
      })
    );
  } finally {
    if (unitStarted) asUser("stop", "hermes-gateway-anan.service");
    fs.rmSync(userUnit, { force: true });
    asUser("daemon-reload");
    fs.rmSync(core, { recursive: true, force: true });
    if (!preexistingProfile) fs.rmSync(profile, { recursive: true, force: true });
    fs.rmSync(releaseRoot, { recursive: true, force: true });
    fs.rmSync(stateDir, { recursive: true, force: true });
  }
  process.exit(0);
}
if (process.argv[2] === "--takeover-finalize") {
  const [launcher, waiter, poller, recoveryHelper, holdSource] = process.argv.slice(3);
  for (const source of [launcher, waiter, poller, recoveryHelper, holdSource]) {
    assert.ok(
      source && fs.existsSync(source),
      "takeover finalization source is missing"
    );
  }
  const state = "/var/lib/qintopia-agent-os-deploy";
  const recovery = path.join(state, "recovery");
  const staged = path.join(recovery, "staged");
  const hold = path.join(recovery, "hold");
  const dropin =
    "/etc/systemd/system/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf";
  const releaseRoot = "/home/ubuntu/qintopia-agent-os-releases";
  const service = "qintopia-agent-os-deploy-runner.service";
  const timer = "qintopia-agent-os-deploy-runner.timer";
  const servicePath = `/run/systemd/system/${service}`;
  const timerPath = `/run/systemd/system/${timer}`;
  const oSha = "16e8d56b98001579c6288ba13199b80d6d3dfc74";
  const pSha = "83d694f2c3bc21fd78a73d25da3197379e2a14d5";
  const tSha = "70e7984fab92ddab956009585212d0e9729767b5";
  const rSha = "4".repeat(40);
  const requestId = "deploy-20260927T010203Z-abcdef0";
  const key = "simulated-finalize-key";
  const fixture = fs.mkdtempSync("/tmp/qintopia-takeover-finalize-");
  const replayed = path.join(fixture, "poller-replayed");
  const written = [
    staged,
    hold,
    dropin,
    releaseRoot,
    servicePath,
    timerPath,
    path.join(recovery, "takeover.json"),
    path.join(recovery, "takeover-consumed"),
    path.join(recovery, `${requestId}.json`),
    path.join(state, "requests/processed", `${requestId}.json`),
    path.join(state, "results", `${requestId}.json`),
  ];
  for (const target of written) {
    assert.equal(
      fs.existsSync(target),
      false,
      `takeover fixture path exists: ${target}`
    );
  }
  const run = (command, args, options = {}) =>
    spawnSync(command, args, { encoding: "utf8", ...options });
  const check = (result, label) => {
    assert.equal(result.status, 0, `${label}: ${result.stderr || result.stdout}`);
    return result;
  };
  const write = (target, content, mode = 0o644) => {
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, content, { mode });
    fs.chmodSync(target, mode);
  };
  const sha256 = (value) => crypto.createHash("sha256").update(value).digest("hex");
  const canonical = (value) =>
    Array.isArray(value)
      ? `[${value.map(canonical).join(",")}]`
      : value && typeof value === "object"
        ? `{${Object.keys(value)
            .sort()
            .map((name) => `${JSON.stringify(name)}:${canonical(value[name])}`)
            .join(",")}}`
        : JSON.stringify(value);
  const sign = (unsigned, issuer, signedAt, field) => {
    const metadata = {
      algorithm: "hmac-sha256",
      issuer,
      key_id: "simulated",
      signed_at: signedAt,
    };
    return {
      ...unsigned,
      signature: {
        ...metadata,
        value: crypto
          .createHmac("sha256", key)
          .update(canonical({ [field]: unsigned, signature: metadata }))
          .digest("hex"),
      },
    };
  };
  const wrapperDir = path.join(fixture, "bin");
  write(
    path.join(wrapperDir, "systemctl"),
    `#!/bin/bash
set -euo pipefail
if [[ "$QINTOPIA_FAULT" == reload-fail && "$1" == daemon-reload ]]; then exit 41; fi
if [[ "$QINTOPIA_FAULT" == enable-fail && "$1" == enable ]]; then exit 42; fi
if [[ "$QINTOPIA_FAULT" == kill-after-enable && "$1" == enable ]]; then
  /usr/bin/systemctl "$@"
  kill -9 "$PPID"
  exit 99
fi
exec /usr/bin/systemctl "$@"
`,
    0o755
  );
  write(
    path.join(wrapperDir, "rm"),
    `#!/bin/bash
set -euo pipefail
if [[ "$QINTOPIA_FAULT" == kill-after-unlink && "$1" == "$QINTOPIA_HOLD_FILE" ]]; then
  /usr/bin/rm "$@"
  kill -9 "$PPID"
  exit 99
fi
exec /usr/bin/rm "$@"
`,
    0o755
  );
  const launch = (mode, fault = "") =>
    run("bash", [launcher, mode, ...(mode === "prepare" ? [] : [requestId])], {
      env: {
        ...process.env,
        PATH: `${wrapperDir}:${process.env.PATH}`,
        QINTOPIA_FAULT: fault,
        QINTOPIA_HOLD_FILE: hold,
        DEPLOY_REQUEST_SIGNING_KEY: key,
        DEPLOY_REQUEST_SIGNING_KEY_ID: "simulated",
      },
    });
  try {
    write(
      servicePath,
      `[Unit]\nDescription=Simulated takeover runner\n[Service]\nType=oneshot\nExecStart=/usr/bin/touch ${replayed}\n`
    );
    write(
      timerPath,
      `[Unit]\nDescription=Simulated takeover timer\n[Timer]\nOnCalendar=daily\n[Install]\nWantedBy=timers.target\n`
    );
    check(run("systemctl", ["daemon-reload"]), "fixture daemon reload");
    check(run("systemctl", ["enable", "--now", timer]), "initial timer enable");
    const stagedRunner = path.join(staged, "payload/deploy/runner");
    fs.mkdirSync(stagedRunner, { recursive: true });
    const stagedFiles = [
      [poller, "payload/deploy/runner/poll-deploy-requests.sh"],
      [recoveryHelper, "payload/deploy/runner/recover-release-lineage.sh"],
      [
        holdSource,
        "payload/deploy/runner/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf",
      ],
      [waiter, "payload/deploy/runner/wait-deploy-result.sh"],
    ];
    const manifestFiles = [];
    for (const [source, relative] of stagedFiles) {
      const target = path.join(staged, relative);
      write(target, fs.readFileSync(source), relative.endsWith(".sh") ? 0o755 : 0o644);
      manifestFiles.push({
        path: relative,
        sha256: sha256(fs.readFileSync(target)),
        size_bytes: fs.statSync(target).size,
      });
    }
    write(
      path.join(staged, "artifact-manifest.json"),
      JSON.stringify({
        schema_version: 1,
        target: "server-operator-files",
        files: manifestFiles,
      }) + "\n"
    );
    write(
      path.join(staged, "SHA256SUMS"),
      manifestFiles.map((item) => `${item.sha256}  ${item.path}\n`).join("")
    );
    const prepareFailed = launch("prepare", "reload-fail");
    assert.equal(
      prepareFailed.status,
      41,
      `daemon-reload injection: ${prepareFailed.stderr}`
    );
    assert.equal(fs.existsSync(hold), true, "daemon-reload failure cleared hold");
    assert.equal(
      check(
        run("systemctl", ["show", timer, "--property=UnitFileState", "--value"]),
        "timer state after reload failure"
      ).stdout.trim(),
      "disabled"
    );
    check(run("systemctl", ["daemon-reload"]), "load persistent guard after failure");

    const now = new Date().toISOString();
    const request = sign(
      {
        schema_version: 1,
        request_id: requestId,
        environment: "production",
        repository: "qintopia-agent-studio/qintopia-agent-os",
        requested_by: "fixture",
        created_at: now,
        expires_at: new Date(Date.parse(now) + 3600000).toISOString(),
        commit_sha: oSha,
        runtime_sha: pSha,
        deploy_bundle_sha: rSha,
        release_sha: tSha,
        runtime_artifact_profile: "huabaosi-production",
        release_scope: ["deploy-bundle"],
        restart_targets: ["qintopia-system-services"],
        rollback_on_smoke_failure: true,
        dry_run: false,
        cos: {
          bucket: "simulated",
          region: "simulated",
          prefix: "qintopia-agent-os",
          request_key: `qintopia-agent-os/deploy-requests/production/requests/${requestId}.json`,
          result_key: `qintopia-agent-os/deploy-results/production/${requestId}.json`,
        },
      },
      "github-actions",
      now,
      "request"
    );
    const result = sign(
      {
        schema_version: 1,
        request_id: requestId,
        environment: "production",
        status: "succeeded",
        started_at: now,
        finished_at: now,
        release_sha: tSha,
        commit_sha: oSha,
        runtime_sha: pSha,
        deploy_bundle_sha: rSha,
        runtime_artifact_profile: "huabaosi-production",
        release_scope: ["deploy-bundle"],
        restart_targets: ["qintopia-system-services"],
        previous_sha: oSha,
        current_target: path.join(releaseRoot, tSha),
        checks: [{ name: "deploy-runner", status: "passed" }],
        rollback: { attempted: false, status: "not_needed" },
      },
      "qintopia-deploy-runner",
      now,
      "result"
    );
    for (const sha of [oSha, tSha])
      fs.mkdirSync(path.join(releaseRoot, sha), { recursive: true });
    const oldManifest = { release_sha: oSha, previous_sha: pSha };
    const tManifest = {
      release_sha: tSha,
      previous_sha: oSha,
      request_id: requestId,
      release_scope: ["deploy-bundle"],
      restart_targets: ["qintopia-system-services"],
      commit_sha: oSha,
      runtime_sha: pSha,
      deploy_bundle_sha: rSha,
      runtime_artifact_profile: "huabaosi-production",
    };
    const oldManifestPath = path.join(releaseRoot, oSha, "manifest.json");
    const tManifestPath = path.join(releaseRoot, tSha, "manifest.json");
    write(oldManifestPath, JSON.stringify(oldManifest) + "\n");
    write(tManifestPath, JSON.stringify(tManifest) + "\n");
    fs.symlinkSync(path.join(releaseRoot, tSha), path.join(releaseRoot, "current"));
    fs.symlinkSync(path.join(releaseRoot, oSha), path.join(releaseRoot, "previous"));
    const requestPath = path.join(state, "requests/processed", `${requestId}.json`);
    const resultPath = path.join(state, "results", `${requestId}.json`);
    write(requestPath, JSON.stringify(request) + "\n", 0o600);
    write(resultPath, JSON.stringify(result) + "\n", 0o600);
    write(path.join(recovery, "takeover-consumed"), requestId + "\n", 0o600);
    write(
      path.join(recovery, `${requestId}.json`),
      JSON.stringify({
        direction: "O→T",
        phase: "intent",
        request_id: requestId,
        request_sha256: sha256(fs.readFileSync(requestPath)),
        original_current_sha: oSha,
        original_previous_sha: pSha,
        manifest_sha256: { current: sha256(fs.readFileSync(oldManifestPath)) },
      }) + "\n",
      0o600
    );
    const failEnable = launch("finalize", "enable-fail");
    assert.equal(failEnable.status, 75, `timer enable failure: ${failEnable.stderr}`);
    assert.equal(fs.existsSync(hold), true, "timer enable failure cleared hold");
    const killedBeforeUnlink = launch("finalize", "kill-after-enable");
    assert.notEqual(killedBeforeUnlink.status, 0, "caller survived injected SIGKILL");
    assert.equal(fs.existsSync(hold), true, "caller death cleared hold before unlink");
    check(run("systemctl", ["start", service]), "held service start");
    assert.equal(
      fs.existsSync(replayed),
      false,
      "active timer bypassed recovery guard"
    );
    check(launch("finalize"), "resume finalization without replay");
    assert.equal(fs.existsSync(hold), false, "successful finalization retained hold");
    assert.equal(fs.existsSync(replayed), false, "finalize replayed poller");
    assert.equal(
      check(run("systemctl", ["is-active", timer]), "restored timer").stdout.trim(),
      "active"
    );
    write(hold, "", 0o600);
    const killedAfterUnlink = launch("finalize", "kill-after-unlink");
    assert.notEqual(
      killedAfterUnlink.status,
      0,
      "post-unlink SIGKILL was not injected"
    );
    assert.equal(fs.existsSync(hold), false, "post-unlink death restored stale hold");
    check(launch("finalize"), "idempotent finalization after caller death");
    assert.equal(fs.existsSync(replayed), false, "retry replayed poller");
    console.log("Fixed takeover finalization fault matrix passed.");
  } finally {
    run("systemctl", ["disable", "--now", timer]);
    for (const target of [
      servicePath,
      timerPath,
      dropin,
      path.join(recovery, "hold"),
      path.join(recovery, "takeover.json"),
      path.join(recovery, "takeover-consumed"),
      path.join(recovery, `${requestId}.json`),
      path.join(state, "requests/processed", `${requestId}.json`),
      path.join(state, "results", `${requestId}.json`),
    ])
      fs.rmSync(target, { force: true });
    fs.rmSync(staged, { recursive: true, force: true });
    fs.rmSync(releaseRoot, { recursive: true, force: true });
    run("systemctl", ["daemon-reload"]);
    fs.rmSync(fixture, { recursive: true, force: true });
  }
  process.exit(0);
}
if (process.argv[2] === "--recovery-negative") {
  const [recoverySource, waiterSource, holdSource] = process.argv.slice(3);
  for (const source of [recoverySource, waiterSource, holdSource]) {
    assert.ok(source && fs.existsSync(source), "recovery fixture source is missing");
  }
  const releaseRoot = "/home/ubuntu/qintopia-agent-os-releases";
  const stateRoot = "/var/lib/qintopia-agent-os-deploy";
  const unitDir = "/run/systemd/system";
  const dropin =
    "/etc/systemd/system/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf";
  const envFile = "/etc/qintopia/cos-artifacts.env";
  const service = "qintopia-agent-os-deploy-runner.service";
  const timer = "qintopia-agent-os-deploy-runner.timer";
  const requestId = "deploy-20260927T010203Z-abcdef0";
  const oSha = "16e8d56b98001579c6288ba13199b80d6d3dfc74";
  const pSha = "83d694f2c3bc21fd78a73d25da3197379e2a14d5";
  const tSha = "70e7984fab92ddab956009585212d0e9729767b5";
  const rSha = "4".repeat(40);
  const key = "simulated-recovery-key";
  const fixtureRoot = fs.mkdtempSync("/tmp/qintopia-recovery-negative-");
  const written = [
    releaseRoot,
    path.join(stateRoot, "recovery", "hold"),
    path.join(stateRoot, "recovery", `${requestId}.json`),
    path.join(stateRoot, "requests", "pending", `${requestId}.json`),
    path.join(stateRoot, "results", `${requestId}.json`),
    path.join(unitDir, service),
    path.join(unitDir, timer),
    dropin,
    envFile,
  ];
  for (const target of written) {
    assert.equal(
      fs.existsSync(target),
      false,
      `recovery fixture path already exists: ${target}`
    );
  }
  const write = (target, value, mode = 0o644) => {
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, value, { mode });
    fs.chmodSync(target, mode);
  };
  const canonical = (value) =>
    Array.isArray(value)
      ? `[${value.map(canonical).join(",")}]`
      : value && typeof value === "object"
        ? `{${Object.keys(value)
            .sort()
            .map((name) => `${JSON.stringify(name)}:${canonical(value[name])}`)
            .join(",")}}`
        : JSON.stringify(value);
  const sign = (unsigned, issuer, signedAt, field) => {
    const metadata = {
      algorithm: "hmac-sha256",
      issuer,
      key_id: "simulated",
      signed_at: signedAt,
    };
    return {
      ...unsigned,
      signature: {
        ...metadata,
        value: crypto
          .createHmac("sha256", key)
          .update(canonical({ [field]: unsigned, signature: metadata }))
          .digest("hex"),
      },
    };
  };
  const requestPath = path.join(stateRoot, "requests", "pending", `${requestId}.json`);
  const resultPath = path.join(stateRoot, "results", `${requestId}.json`);
  const journalPath = path.join(stateRoot, "recovery", `${requestId}.json`);
  const now = new Date().toISOString();
  const cos = {
    bucket: "simulated",
    region: "simulated",
    prefix: "qintopia-agent-os",
    request_key: `qintopia-agent-os/deploy-requests/production/requests/${requestId}.json`,
    result_key: `qintopia-agent-os/deploy-results/production/${requestId}.json`,
  };
  const request = sign(
    {
      schema_version: 1,
      request_id: requestId,
      environment: "production",
      repository: "qintopia-agent-studio/qintopia-agent-os",
      requested_by: "fixture",
      created_at: now,
      expires_at: new Date(Date.parse(now) + 3600000).toISOString(),
      commit_sha: oSha,
      runtime_sha: pSha,
      deploy_bundle_sha: rSha,
      release_sha: tSha,
      runtime_artifact_profile: "huabaosi-production",
      release_scope: ["deploy-bundle"],
      restart_targets: ["qintopia-system-services"],
      rollback_on_smoke_failure: true,
      dry_run: false,
      cos,
    },
    "github-actions",
    now,
    "request"
  );
  const failed = sign(
    {
      schema_version: 1,
      request_id: requestId,
      environment: "production",
      status: "failed",
      started_at: now,
      finished_at: now,
      release_sha: tSha,
      commit_sha: oSha,
      runtime_sha: pSha,
      deploy_bundle_sha: rSha,
      runtime_artifact_profile: "huabaosi-production",
      release_scope: ["deploy-bundle"],
      restart_targets: ["qintopia-system-services"],
      previous_sha: oSha,
      current_target: path.join(releaseRoot, tSha),
      checks: [{ name: "deploy-runner", status: "failed" }],
      rollback: { attempted: false, status: "not_needed" },
    },
    "qintopia-deploy-runner",
    now,
    "result"
  );
  const { signature: _failedSignature, ...unsignedFailed } = failed;
  const succeeded = sign(
    {
      ...unsignedFailed,
      status: "succeeded",
      checks: [{ name: "deploy-runner", status: "passed" }],
    },
    "qintopia-deploy-runner",
    now,
    "result"
  );
  const remoteRequest = path.join(fixtureRoot, "request.json");
  const remoteResult = path.join(fixtureRoot, "result.json");
  const remoteSuccess = path.join(fixtureRoot, "success.json");
  const coscli = path.join(fixtureRoot, "coscli");
  const touched = path.join(fixtureRoot, "pointer-action");
  try {
    for (const sha of [oSha, pSha, tSha]) {
      fs.mkdirSync(path.join(releaseRoot, sha, "deploy", "runner"), {
        recursive: true,
      });
      write(
        path.join(releaseRoot, sha, "manifest.json"),
        `${JSON.stringify({ release_sha: sha, previous_sha: sha === oSha ? pSha : oSha })}\n`
      );
    }
    fs.symlinkSync(path.join(releaseRoot, tSha), path.join(releaseRoot, "current"));
    fs.symlinkSync(path.join(releaseRoot, oSha), path.join(releaseRoot, "previous"));
    fs.copyFileSync(
      recoverySource,
      path.join(releaseRoot, tSha, "deploy/runner/recover-release-lineage.sh")
    );
    fs.copyFileSync(
      waiterSource,
      path.join(releaseRoot, tSha, "deploy/runner/wait-deploy-result.sh")
    );
    fs.chmodSync(
      path.join(releaseRoot, tSha, "deploy/runner/wait-deploy-result.sh"),
      0o755
    );
    fs.mkdirSync(
      path.join(
        releaseRoot,
        tSha,
        "deploy/runner/qintopia-agent-os-deploy-runner.service.d"
      ),
      { recursive: true }
    );
    fs.copyFileSync(
      holdSource,
      path.join(
        releaseRoot,
        tSha,
        "deploy/runner/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf"
      )
    );
    write(
      path.join(releaseRoot, tSha, "deploy/runner/rollback-release.sh"),
      `#!/bin/sh\ntouch ${touched}\nexit 99\n`,
      0o755
    );
    write(requestPath, `${JSON.stringify(request)}\n`, 0o600);
    write(resultPath, `${JSON.stringify(failed)}\n`, 0o600);
    fs.copyFileSync(requestPath, remoteRequest);
    fs.copyFileSync(resultPath, remoteResult);
    write(remoteSuccess, `${JSON.stringify(succeeded)}\n`, 0o600);
    const manifestDigest = (sha) =>
      crypto
        .createHash("sha256")
        .update(fs.readFileSync(path.join(releaseRoot, sha, "manifest.json")))
        .digest("hex");
    write(
      journalPath,
      `${JSON.stringify({
        schema_version: 1,
        request_id: requestId,
        request_sha256: crypto
          .createHash("sha256")
          .update(fs.readFileSync(requestPath))
          .digest("hex"),
        direction: "O→T",
        original_current_sha: oSha,
        original_previous_sha: pSha,
        manifest_sha256: {
          current: manifestDigest(oSha),
          previous: manifestDigest(pSha),
        },
        phase: "intent",
      })}\n`,
      0o600
    );
    write(path.join(stateRoot, "recovery", "hold"), "", 0o600);
    fs.mkdirSync(path.dirname(dropin), { recursive: true });
    fs.copyFileSync(holdSource, dropin);
    write(
      path.join(unitDir, service),
      "[Unit]\nDescription=Simulated runner\n[Service]\nType=oneshot\nExecStart=/usr/bin/true\n"
    );
    write(
      path.join(unitDir, timer),
      "[Unit]\nDescription=Simulated runner timer\n[Timer]\nOnCalendar=daily\n[Install]\nWantedBy=timers.target\n"
    );
    write(
      envFile,
      `export TENCENT_COS_BUCKET=simulated TENCENT_COS_REGION=simulated TENCENT_COS_SECRET_ID=simulated TENCENT_COS_SECRET_KEY=simulated DEPLOY_REQUEST_SIGNING_KEY=${key} DEPLOY_REQUEST_SIGNING_KEY_ID=simulated COSCLI_PATH=${coscli}\n`,
      0o600
    );
    write(
      coscli,
      `#!/bin/bash\nset -euo pipefail\n[[ "$1" != cp ]] && exit 0\ncase "$2" in\n  *deploy-requests*) cp "$QINTOPIA_REMOTE_REQUEST_FILE" "$3" ;;\n  *deploy-results*)\n    [[ "$QINTOPIA_FAKE_COS_MODE" == missing ]] && exit 1\n    [[ "$QINTOPIA_FAKE_COS_MODE" == unreadable ]] && exit 2\n    if [[ "$QINTOPIA_FAKE_COS_MODE" == success ]]; then cp "$QINTOPIA_REMOTE_SUCCESS_FILE" "$3"; else cp "$QINTOPIA_REMOTE_RESULT_FILE" "$3"; fi ;;\n  *) exit 3 ;;\nesac\n`,
      0o755
    );
    const reload = spawnSync("systemctl", ["daemon-reload"], { encoding: "utf8" });
    assert.equal(reload.status, 0, reload.stderr);
    for (const mode of [
      "missing",
      "unreadable",
      "success",
      "conflict-request",
      "bad-signature",
      "local-missing",
      "local-success",
      "match",
    ]) {
      write(resultPath, `${JSON.stringify(failed)}\n`, 0o600);
      fs.copyFileSync(requestPath, remoteRequest);
      fs.copyFileSync(resultPath, remoteResult);
      if (mode === "conflict-request")
        write(
          remoteRequest,
          `${JSON.stringify({ ...request, requested_by: "tampered" })}\n`
        );
      if (mode === "bad-signature")
        write(remoteResult, `${JSON.stringify({ ...failed, status: "succeeded" })}\n`);
      if (mode === "local-missing") fs.rmSync(resultPath);
      if (mode === "local-success")
        write(resultPath, `${JSON.stringify(succeeded)}\n`, 0o600);
      const attempt = spawnSync(
        "bash",
        [
          path.join(releaseRoot, tSha, "deploy/runner/recover-release-lineage.sh"),
          "--request-id",
          requestId,
        ],
        {
          encoding: "utf8",
          env: {
            ...process.env,
            QINTOPIA_FAKE_COS_MODE: mode,
            QINTOPIA_REMOTE_REQUEST_FILE: remoteRequest,
            QINTOPIA_REMOTE_RESULT_FILE: remoteResult,
            QINTOPIA_REMOTE_SUCCESS_FILE: remoteSuccess,
          },
        }
      );
      if (mode === "match") {
        assert.equal(
          attempt.status,
          99,
          `matching evidence did not reach pointer action: ${attempt.stderr}`
        );
        assert.equal(fs.existsSync(touched), true);
        fs.rmSync(touched);
      } else {
        assert.equal(attempt.status, 75, `${mode}: ${attempt.stderr}`);
        assert.match(
          attempt.stderr,
          mode === "local-missing"
            ? /local result missing/
            : mode === "local-success"
              ? /signed success result exists/
              : /COS request or result is absent, unreadable, or conflicts/,
          `${mode}: recovery did not reach the expected evidence gate`
        );
      }
      assert.equal(
        fs.realpathSync(path.join(releaseRoot, "current")),
        path.join(releaseRoot, tSha)
      );
      assert.equal(
        fs.realpathSync(path.join(releaseRoot, "previous")),
        path.join(releaseRoot, oSha)
      );
      assert.equal(
        fs.existsSync(touched),
        false,
        `${mode}: unexpected pointer action marker`
      );
    }
    console.log(
      "Recovery COS missing/unreadable/conflict/success/unknown negatives passed."
    );
  } finally {
    for (const target of [
      path.join(unitDir, service),
      path.join(unitDir, timer),
      dropin,
      envFile,
    ]) {
      fs.rmSync(target, { force: true });
    }
    for (const target of [
      requestPath,
      resultPath,
      journalPath,
      path.join(stateRoot, "recovery", "hold"),
    ]) {
      fs.rmSync(target, { force: true });
    }
    for (const target of [releaseRoot, fixtureRoot]) {
      fs.rmSync(target, { recursive: true, force: true });
    }
    spawnSync("systemctl", ["daemon-reload"]);
  }
  process.exit(0);
}
if (process.argv[2] === "--fixed-takeover-lock") {
  const [oldRunnerSource, pollerSource, waiterSource] = process.argv.slice(3);
  for (const source of [oldRunnerSource, pollerSource, waiterSource]) {
    assert.ok(source && fs.existsSync(source), "fixed takeover source is missing");
  }
  const releaseRoot = "/home/ubuntu/qintopia-agent-os-releases";
  const oSha = "16e8d56b98001579c6288ba13199b80d6d3dfc74";
  const pSha = "83d694f2c3bc21fd78a73d25da3197379e2a14d5";
  const tSha = "70e7984fab92ddab956009585212d0e9729767b5";
  const rSha = "4".repeat(40);
  const key = "simulated-fixed-takeover-key";
  assert.equal(
    fs.existsSync(releaseRoot),
    false,
    "simulated release root already exists"
  );
  const fixture = fs.mkdtempSync("/tmp/qintopia-fixed-takeover-");
  const run = (command, args, options = {}) =>
    spawnSync(command, args, { encoding: "utf8", ...options });
  const waitFor = (target, timeout = 5000) => {
    const deadline = Date.now() + timeout;
    while (!fs.existsSync(target) && Date.now() < deadline) {
      Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 50);
    }
    assert.ok(fs.existsSync(target), `stage not reached: ${target}`);
  };
  const canonical = (value) =>
    Array.isArray(value)
      ? `[${value.map(canonical).join(",")}]`
      : value && typeof value === "object"
        ? `{${Object.keys(value)
            .sort()
            .map((name) => `${JSON.stringify(name)}:${canonical(value[name])}`)
            .join(",")}}`
        : JSON.stringify(value);
  const write = (target, value, mode = 0o644) => {
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, value, { mode });
    fs.chmodSync(target, mode);
  };
  const oldRunner = path.join(
    releaseRoot,
    oSha,
    "deploy/runner/qintopia-agent-os-deploy-runner"
  );
  const coscli = path.join(fixture, "coscli");
  const unrelatedLock = path.join(fixture, "unrelated.lock");
  let unrelatedHolder;
  let transientUnit = false;
  try {
    for (const sha of [oSha, pSha]) {
      write(
        path.join(releaseRoot, sha, "manifest.json"),
        `${JSON.stringify({ release_sha: sha, previous_sha: sha === oSha ? pSha : "0".repeat(40) })}\n`
      );
    }
    fs.symlinkSync(path.join(releaseRoot, oSha), path.join(releaseRoot, "current"));
    fs.symlinkSync(path.join(releaseRoot, pSha), path.join(releaseRoot, "previous"));
    fs.mkdirSync(path.dirname(oldRunner), { recursive: true });
    fs.copyFileSync(oldRunnerSource, oldRunner);
    fs.chmodSync(oldRunner, 0o755);
    const runnerDir = path.dirname(oldRunner);
    fs.copyFileSync(waiterSource, path.join(runnerDir, "wait-deploy-result.sh"));
    fs.chmodSync(path.join(runnerDir, "wait-deploy-result.sh"), 0o755);
    const quiesceStarted = path.join(fixture, "quiesce-started");
    write(
      path.join(runnerDir, "quiesce-space-automation-runtime.sh"),
      `#!/bin/bash\necho started >"${quiesceStarted}"\nsleep 2\n`,
      0o755
    );
    write(
      path.join(runnerDir, "promote-release.sh"),
      `#!/bin/bash\nset -euo pipefail\nexec 9>"$QINTOPIA_UNRELATED_LOCK"\nif flock -n 9; then echo redirected >"$QINTOPIA_UNRELATED_RESULT"; else echo blocked >"$QINTOPIA_UNRELATED_RESULT"; fi\necho "$PPID" >"$QINTOPIA_RUNNER_PID_FILE"\necho started >"$QINTOPIA_PROMOTER_STARTED"\nsleep "$QINTOPIA_PROMOTE_SLEEP"\nexit 42\n`,
      0o755
    );
    write(
      coscli,
      `#!/bin/bash\nset -euo pipefail\n[[ "$1" != cp ]] && exit 0\nif [[ "$2" == cos://* ]]; then\n  case "$2" in\n    *deploy-requests/production/current.json) cp "$QINTOPIA_POINTER_FILE" "$3" ;;\n    *deploy-requests/production/requests/*) cp "$QINTOPIA_REQUEST_FILE" "$3" ;;\n    *deploy-results/production/*) if [[ -f "$QINTOPIA_REMOTE_RESULT" ]]; then cp "$QINTOPIA_REMOTE_RESULT" "$3"; else echo NoSuchKey >&2; exit 1; fi ;;\n    *) exit 64 ;;\n  esac\nelse\n  [[ "$3" == cos://*deploy-results/production/* ]] || exit 64\n  echo uploading >"$QINTOPIA_UPLOAD_STARTED"\n  sleep "$QINTOPIA_UPLOAD_SLEEP"\n  cp "$2" "$QINTOPIA_REMOTE_RESULT"\nfi\n`,
      0o755
    );
    unrelatedHolder = spawn("flock", ["-n", unrelatedLock, "sleep", "90"], {
      stdio: "ignore",
    });
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 150);
    assert.notEqual(run("flock", ["-n", unrelatedLock, "true"]).status, 0);
    const phases = [
      "drift",
      "journal-start",
      "parent-death",
      "runner-kill",
      "upload-gap",
      "cgroup-stop",
    ];
    for (const phase of phases) {
      fs.rmSync(quiesceStarted, { force: true });
      const state = path.join(fixture, phase, "state");
      const requestId = `deploy-20260927T02030${4 + phases.indexOf(phase)}Z-abcdef${phases.indexOf(phase)}`;
      const started = path.join(fixture, phase, "promoter-started");
      const upload = path.join(fixture, phase, "upload-started");
      const unrelated = path.join(fixture, phase, "unrelated-result");
      const runnerPid = path.join(fixture, phase, "runner-pid");
      const pointer = path.join(fixture, phase, "pointer.json");
      const requestFile = path.join(fixture, phase, "request.json");
      const remoteResult = path.join(fixture, phase, "remote-result.json");
      const now = new Date().toISOString();
      const cos = {
        bucket: "simulated",
        region: "simulated",
        prefix: "qintopia-agent-os",
        request_key: `qintopia-agent-os/deploy-requests/production/requests/${requestId}.json`,
        result_key: `qintopia-agent-os/deploy-results/production/${requestId}.json`,
      };
      const unsigned = {
        schema_version: 1,
        request_id: requestId,
        environment: "production",
        repository: "qintopia-agent-studio/qintopia-agent-os",
        requested_by: "fixture",
        created_at: now,
        expires_at: new Date(Date.parse(now) + 3600000).toISOString(),
        commit_sha: oSha,
        runtime_sha: pSha,
        deploy_bundle_sha: rSha,
        release_sha: tSha,
        runtime_artifact_profile: "huabaosi-production",
        release_scope: ["deploy-bundle"],
        restart_targets: ["qintopia-system-services"],
        rollback_on_smoke_failure: true,
        dry_run: false,
        cos,
      };
      const metadata = {
        algorithm: "hmac-sha256",
        issuer: "github-actions",
        key_id: "simulated",
        signed_at: now,
      };
      const request = {
        ...unsigned,
        signature: {
          ...metadata,
          value: crypto
            .createHmac("sha256", key)
            .update(canonical({ request: unsigned, signature: metadata }))
            .digest("hex"),
        },
      };
      write(requestFile, `${JSON.stringify(request)}\n`);
      write(
        pointer,
        `${JSON.stringify({
          schema_version: 1,
          environment: "production",
          repository: unsigned.repository,
          request_id: requestId,
          request_key: cos.request_key,
          result_key: cos.result_key,
        })}\n`
      );
      write(path.join(state, "recovery", "hold"), "", 0o600);
      const env = {
        ...process.env,
        QINTOPIA_COS_ENV_FILE: path.join(fixture, "missing.env"),
        QINTOPIA_DEPLOY_RUNNER_STATE_DIR: state,
        QINTOPIA_DEPLOY_RUNNER_BIN: oldRunner,
        QINTOPIA_EXPECTED_DEPLOY_REQUEST_ID: requestId,
        COSCLI_PATH: coscli,
        TENCENT_COS_BUCKET: "simulated",
        TENCENT_COS_REGION: "simulated",
        TENCENT_COS_SECRET_ID: "simulated",
        TENCENT_COS_SECRET_KEY: "simulated",
        DEPLOY_REQUEST_SIGNING_KEY: key,
        DEPLOY_REQUEST_SIGNING_KEY_ID: "simulated",
        QINTOPIA_POINTER_FILE: pointer,
        QINTOPIA_REQUEST_FILE: requestFile,
        QINTOPIA_REMOTE_RESULT: remoteResult,
        QINTOPIA_UPLOAD_STARTED: upload,
        QINTOPIA_UPLOAD_SLEEP: phase === "upload-gap" ? "4" : "0",
        QINTOPIA_PROMOTER_STARTED: started,
        QINTOPIA_PROMOTE_SLEEP: ["parent-death", "runner-kill", "cgroup-stop"].includes(
          phase
        )
          ? "6"
          : "0",
        QINTOPIA_RUNNER_PID_FILE: runnerPid,
        QINTOPIA_UNRELATED_LOCK: unrelatedLock,
        QINTOPIA_UNRELATED_RESULT: unrelated,
        QINTOPIA_RELEASE_ROOT: releaseRoot,
      };
      if (phase === "drift") {
        fs.appendFileSync(oldRunner, "\n# simulated drift\n");
        const rejected = run("bash", [pollerSource], { env });
        assert.equal(rejected.status, 75, rejected.stderr);
        assert.match(rejected.stderr, /old runner identity mismatch/);
        assert.equal(fs.existsSync(started), false);
        assert.equal(
          fs.existsSync(path.join(state, "recovery", `${requestId}.json`)),
          false
        );
        fs.copyFileSync(oldRunnerSource, oldRunner);
        fs.chmodSync(oldRunner, 0o755);
        continue;
      }
      if (phase === "cgroup-stop") {
        const environment = path.join(fixture, phase, "environment");
        write(
          environment,
          Object.entries(env)
            .filter(
              ([name]) =>
                name.startsWith("QINTOPIA_") ||
                name.startsWith("TENCENT_") ||
                name.startsWith("DEPLOY_") ||
                name === "COSCLI_PATH"
            )
            .map(([name, value]) => `${name}=${value}`)
            .join("\n") + "\n",
          0o600
        );
        const unitName = "qintopia-fixed-takeover-simulated.service";
        run("systemctl", ["reset-failed", unitName]);
        const launched = run("systemd-run", [
          "--unit",
          unitName,
          "--service-type=oneshot",
          "--no-block",
          "--uid=root",
          "--gid=root",
          `--property=EnvironmentFile=${environment}`,
          "/bin/bash",
          pollerSource,
        ]);
        assert.equal(launched.status, 0, launched.stderr);
        transientUnit = true;
        waitFor(started);
        const deployLock = path.join(state, "deploy.lock");
        assert.notEqual(run("flock", ["-n", deployLock, "true"]).status, 0);
        const stopped = run("systemctl", ["stop", unitName]);
        assert.equal(stopped.status, 0, stopped.stderr);
        assert.equal(run("flock", ["-n", deployLock, "true"]).status, 0);
        assert.equal(
          fs.existsSync(path.join(state, "requests", "claimed", `${requestId}.json`)),
          true
        );
        assert.equal(fs.existsSync(path.join(state, "recovery", "hold")), true);
        const retry = run("bash", [pollerSource], { env });
        assert.equal(retry.status, 75, retry.stderr);
        assert.match(retry.stderr, /unfinished deploy request claim/);
        run("systemctl", ["reset-failed", unitName]);
        transientUnit = false;
        continue;
      }
      const child = spawn("bash", [pollerSource], {
        env,
        stdio: ["ignore", "pipe", "pipe"],
      });
      let stdout = "",
        stderr = "";
      child.stdout.on("data", (chunk) => {
        stdout += chunk;
      });
      child.stderr.on("data", (chunk) => {
        stderr += chunk;
      });
      if (phase === "journal-start") {
        waitFor(quiesceStarted);
        assert.equal(
          fs.existsSync(path.join(state, "recovery", `${requestId}.json`)),
          true
        );
        assert.equal(fs.existsSync(started), false);
        assert.notEqual(
          run("flock", ["-n", path.join(state, "deploy.lock"), "true"]).status,
          0,
          "deploy lock was free after journal before promotion"
        );
      }
      waitFor(started);
      assert.equal(
        fs.readFileSync(unrelated, "utf8").trim(),
        "blocked",
        "unrelated child flock was redirected to deploy.lock"
      );
      const deployLock = path.join(state, "deploy.lock");
      assert.notEqual(run("flock", ["-n", deployLock, "true"]).status, 0);
      if (phase === "parent-death") {
        child.kill("SIGKILL");
        assert.notEqual(
          run("flock", ["-n", deployLock, "true"]).status,
          0,
          "runner lost inherited lock when poller parent died"
        );
        Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 6600);
        assert.equal(
          run("flock", ["-n", deployLock, "true"]).status,
          0,
          "inherited lock remained after runner exited"
        );
        assert.equal(
          fs.existsSync(path.join(state, "requests", "claimed", `${requestId}.json`)),
          true
        );
        assert.equal(fs.existsSync(path.join(state, "recovery", "hold")), true);
        const retry = run("bash", [pollerSource], { env });
        assert.equal(retry.status, 75, retry.stderr);
        assert.match(retry.stderr, /unfinished deploy request claim/);
      } else if (phase === "runner-kill") {
        process.kill(Number(fs.readFileSync(runnerPid, "utf8").trim()), "SIGKILL");
        assert.notEqual(
          run("flock", ["-n", deployLock, "true"]).status,
          0,
          "promoter lost inherited lock when old runner died"
        );
        const completed = await new Promise((resolve) => child.on("close", resolve));
        assert.notEqual(completed, 0, `${stdout}\n${stderr}`);
        assert.equal(
          fs.existsSync(path.join(state, "requests", "claimed", `${requestId}.json`)),
          true
        );
        assert.equal(fs.existsSync(path.join(state, "recovery", "hold")), true);
        Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 6600);
        assert.equal(run("flock", ["-n", deployLock, "true"]).status, 0);
        const retry = run("bash", [pollerSource], { env });
        assert.equal(retry.status, 75, retry.stderr);
        assert.match(retry.stderr, /unfinished deploy request claim/);
      } else if (phase === "journal-start") {
        const completed = await new Promise((resolve) => child.on("close", resolve));
        assert.equal(completed, 42, `${stdout}\n${stderr}`);
        assert.equal(run("flock", ["-n", deployLock, "true"]).status, 0);
      } else {
        waitFor(upload, 6000);
        assert.notEqual(
          run("flock", ["-n", deployLock, "true"]).status,
          0,
          "deploy lock released before upload/archive completed"
        );
        const completed = await new Promise((resolve) => child.on("close", resolve));
        assert.equal(completed, 42, `${stdout}\n${stderr}`);
        assert.equal(run("flock", ["-n", deployLock, "true"]).status, 0);
        assert.equal(
          fs.existsSync(path.join(state, "requests", "claimed", `${requestId}.json`)),
          false
        );
        assert.equal(
          fs.existsSync(path.join(state, "requests", "failed", `${requestId}.json`)),
          true
        );
      }
    }
    const adapter = fs
      .readFileSync(pollerSource, "utf8")
      .match(/^    flock\(\) \{[\s\S]*?^    \}/m)?.[0];
    assert.ok(adapter, "fixed poller lock adapter was not found");
    const adapterState = path.join(fixture, "adapter-state");
    fs.mkdirSync(adapterState, { recursive: true });
    const adapterLock = path.join(adapterState, "deploy.lock");
    const adapterEnv = {
      ...process.env,
      QINTOPIA_FIXED_TAKEOVER_LOCK: "1",
      QINTOPIA_DEPLOY_RUNNER_BIN: oldRunner,
      QINTOPIA_DEPLOY_RUNNER_STATE_DIR: adapterState,
      fixed_runner: oldRunner,
      fixed_runner_sha256: crypto
        .createHash("sha256")
        .update(fs.readFileSync(oldRunner))
        .digest("hex"),
    };
    const adapterCall = (setup, args = "-n 9", runnerName = oldRunner) =>
      run("bash", ["-c", `${setup}\n${adapter}\nflock ${args}`, runnerName], {
        env: adapterEnv,
      });
    const missingFd = adapterCall("");
    assert.equal(missingFd.status, 75, `missing FD7 was accepted: ${missingFd.stderr}`);
    assert.equal(
      adapterCall(`exec 7>"${unrelatedLock}"`).status,
      75,
      "wrong FD7 inode was accepted"
    );
    assert.equal(
      adapterCall(`exec 7>"${adapterLock}"; command flock -n 7`).status,
      0,
      "correct inherited FD7 was rejected"
    );
    assert.notEqual(
      adapterCall(
        `exec 7>"${adapterLock}"; command flock -n 7; exec 8>"${unrelatedLock}"`,
        "-n 8"
      ).status,
      0,
      "other flock arguments were redirected"
    );
    assert.notEqual(
      adapterCall(
        `exec 7>"${adapterLock}"; command flock -n 7; exec 9>"${unrelatedLock}"`,
        "-n 9",
        "/tmp/unrelated-script"
      ).status,
      0,
      "unrelated script flock was redirected"
    );
    fs.appendFileSync(oldRunner, "\n# simulated post-check drift\n");
    assert.equal(
      adapterCall(`exec 7>"${adapterLock}"; command flock -n 7`).status,
      75,
      "post-check runner drift was accepted"
    );
    fs.copyFileSync(oldRunnerSource, oldRunner);
    console.log("Exact old-runner fixed takeover lock handoff passed.");
  } finally {
    if (transientUnit) {
      run("systemctl", ["stop", "qintopia-fixed-takeover-simulated.service"]);
      run("systemctl", ["reset-failed", "qintopia-fixed-takeover-simulated.service"]);
    }
    unrelatedHolder?.kill("SIGKILL");
    fs.rmSync(releaseRoot, { recursive: true, force: true });
    fs.rmSync(fixture, { recursive: true, force: true });
  }
  process.exit(0);
}
const osRelease = fs.readFileSync("/etc/os-release", "utf8");
assert.match(osRelease, /VERSION_ID="24\.04"/);
fs.mkdirSync("/var/lib/qintopia-agent-os-deploy", { recursive: true });
const root = fs.mkdtempSync("/var/lib/qintopia-agent-os-deploy/systemd-linux-");
fs.chmodSync(root, 0o755);
const profile = "/home/ubuntu/.hermes/profiles/anan";
const otherProfile = "/home/ubuntu/.hermes/profiles/erhua";
const unit = "qintopia-agent-os-anan-drain-restart.service";
const deployUnit = "qintopia-agent-os-deploy-runner.service";
const state = path.join(root, "state");
const marker = path.join(profile, ".drain_request.json");
const completion = path.join(profile, `.qintopia-${path.basename(root)}-finished`);
let serviceCreated = false;
let holdCreated = false;
let releaseFixtureCreated = false;
let installedHoldDropin = false;
let timerCreated = false;
let recoveryArtifactsCreated = false;
let recoveryArtifactPaths = [];
let cosEnvCreated = false;
const run = (command, args, options = {}) => {
  const result = spawnSync(command, args, { encoding: "utf8", ...options });
  if (result.error) throw result.error;
  return result;
};
const requireSuccess = (command, args, options) => {
  const result = run(command, args, options);
  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed: ${result.stderr}`);
  }
  return result.stdout;
};
const sleep = (milliseconds) =>
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, milliseconds);
const show = (name, property) =>
  requireSuccess("systemctl", [
    "show",
    name,
    `--property=${property}`,
    "--value",
  ]).trim();
const write = (file, content, mode = 0o644) => {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, content, { mode });
  fs.chmodSync(file, mode);
};
const baseProperties = [
  "--unit=qintopia-agent-os-anan-drain-restart.service",
  "--service-type=oneshot",
  "--wait",
  "--uid=ubuntu",
  "--gid=ubuntu",
  "--property=TimeoutStartSec=infinity",
  "--property=NoNewPrivileges=yes",
  "--property=PrivateTmp=yes",
  "--property=ProtectSystem=strict",
  "--property=ProtectHome=read-only",
  `--property=ReadWritePaths=${profile}`,
  `--property=Environment=XDG_RUNTIME_DIR=/run/user/${run("id", ["-u", "ubuntu"]).stdout.trim()}`,
];

try {
  assert.equal(
    run("id", ["-u", "ubuntu"]).status,
    0,
    "VM needs a simulated ubuntu user"
  );
  if (fs.existsSync(marker))
    throw new Error("preexisting Anan marker must not be removed");
  requireSuccess("install", [
    "-d",
    "-o",
    "ubuntu",
    "-g",
    "ubuntu",
    "-m",
    "0700",
    profile,
  ]);
  requireSuccess("install", [
    "-d",
    "-o",
    "ubuntu",
    "-g",
    "ubuntu",
    "-m",
    "0700",
    otherProfile,
  ]);
  const markerHelper = path.join(root, "marker-helper.py");
  write(
    markerHelper,
    `import errno, json, os, tempfile\nfrom pathlib import Path\nprofile = Path(${JSON.stringify(profile)})\nfd, temporary = tempfile.mkstemp(prefix=".drain-test-", dir=profile)\nwith os.fdopen(fd, "w") as file:\n    json.dump({"simulated": True}, file)\n    file.flush(); os.fsync(file.fileno())\nos.replace(temporary, profile / ".drain_request.json")\ntry:\n    (Path(${JSON.stringify(otherProfile)}) / "forbidden").write_text("fail")\nexcept OSError as error:\n    if error.errno not in (errno.EROFS, errno.EACCES): raise\nelse:\n    raise SystemExit("sandbox allowed writing another Profile")\n(profile / ".drain_request.json").unlink()\n`,
    0o644
  );
  const first = run("systemd-run", [
    ...baseProperties,
    "/usr/bin/python3",
    markerHelper,
  ]);
  if (first.status !== 0)
    throw new Error(`simulated marker helper failed: ${first.stderr}`);
  const firstOutput = first.stdout + first.stderr;
  const invocation = firstOutput.match(/invocation ID: ([0-9a-f]{32})/)?.[1];
  assert.ok(invocation, "--wait must return InvocationID");
  assert.match(firstOutput, /Finished with result: success/);
  assert.match(firstOutput, /Main processes terminated with: code=exited\/status=0/);
  assert.equal(
    fs.existsSync(marker),
    false,
    "helper must clear only its simulated marker"
  );
  assert.equal(fs.existsSync(path.join(otherProfile, "forbidden")), false);

  const longHelper = path.join(root, "long-helper.py");
  write(
    longHelper,
    `import os, time\nfrom pathlib import Path\ntime.sleep(4)\npath = Path(${JSON.stringify(completion)})\nwith path.open("w") as file:\n    file.write(os.environ["INVOCATION_ID"] + "\\n")\n    file.flush(); os.fsync(file.fileno())\nprint("SIMULATED_HELPER_FINISHED", flush=True)\n`,
    0o644
  );
  const client = spawn(
    "systemd-run",
    [...baseProperties, "/usr/bin/python3", longHelper],
    {
      stdio: ["ignore", "pipe", "pipe"],
    }
  );
  let active = false;
  for (let attempt = 0; attempt < 40; attempt++) {
    if (show(unit, "ActiveState") === "activating") {
      active = true;
      break;
    }
    sleep(100);
  }
  assert.ok(active, "long helper never became active");
  assert.equal(show(unit, "User"), "ubuntu");
  assert.equal(show(unit, "Group"), "ubuntu");
  assert.equal(show(unit, "Type"), "oneshot");
  assert.equal(show(unit, "TimeoutStartUSec"), "infinity");
  assert.equal(show(unit, "NoNewPrivileges"), "yes");
  assert.equal(show(unit, "PrivateTmp"), "yes");
  assert.equal(show(unit, "ProtectSystem"), "strict");
  assert.equal(show(unit, "ProtectHome"), "read-only");
  assert.equal(show(unit, "ReadWritePaths"), profile);
  const activeInvocation = show(unit, "InvocationID");
  assert.match(activeInvocation, /^[0-9a-f]{32}$/);
  const busy = run("systemd-run", [...baseProperties, "/usr/bin/true"]);
  assert.notEqual(busy.status, 0, "StartTransientUnit=fail must reject the busy name");
  client.kill("SIGKILL");
  assert.equal(
    show(unit, "InvocationID"),
    activeInvocation,
    "caller death restarted helper"
  );
  sleep(4300);
  assert.notEqual(
    show(unit, "ActiveState"),
    "activating",
    "helper did not finish naturally"
  );
  assert.notEqual(run("systemctl", ["is-active", "--quiet", unit]).status, 0);
  assert.equal(
    fs.readFileSync(completion, "utf8").trim(),
    activeInvocation,
    "helper did not persist completion for the original invocation"
  );
  const invocationJournal = requireSuccess("journalctl", [
    "-u",
    unit,
    "--no-pager",
    "-o",
    "json",
  ])
    .trim()
    .split("\n")
    .filter(Boolean)
    .map((line) => JSON.parse(line))
    .filter(
      (entry) =>
        entry.INVOCATION_ID === activeInvocation ||
        entry._SYSTEMD_INVOCATION_ID === activeInvocation
    );
  assert.ok(
    invocationJournal.some((entry) => entry.MESSAGE === "SIMULATED_HELPER_FINISHED"),
    "original helper completion was not journaled"
  );
  assert.ok(
    invocationJournal.some((entry) =>
      entry.MESSAGE?.includes("Deactivated successfully")
    ),
    "systemd did not record natural success for the same invocation"
  );
  assert.ok(
    invocationJournal.some((entry) => entry.JOB_RESULT === "done"),
    "systemd did not finish the same invocation's start job"
  );

  const servicePath = `/run/systemd/system/${deployUnit}`;
  const dropinDir = `/run/systemd/system/${deployUnit}.d`;
  const hold = "/var/lib/qintopia-agent-os-deploy/recovery/hold";
  if (fs.existsSync(servicePath) || fs.existsSync(hold)) {
    throw new Error("VM contains an existing runner unit or recovery hold");
  }
  const started = path.join(root, "service-started");
  write(
    servicePath,
    `[Unit]\nDescription=Simulated deploy runner\n[Service]\nType=oneshot\nExecStart=/usr/bin/touch ${started}\n`
  );
  serviceCreated = true;
  fs.mkdirSync(dropinDir, { recursive: true });
  fs.copyFileSync(
    path.join(
      process.cwd(),
      "deploy/runner/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf"
    ),
    path.join(dropinDir, "10-recovery-hold.conf")
  );
  fs.mkdirSync(path.dirname(hold), { recursive: true });
  write(hold, "hold\n", 0o600);
  holdCreated = true;
  requireSuccess("systemctl", ["daemon-reload"]);
  requireSuccess("systemctl", ["start", deployUnit]);
  assert.equal(
    fs.existsSync(started),
    false,
    "hold condition did not block old service"
  );
  fs.rmSync(hold);
  holdCreated = false;
  requireSuccess("systemctl", ["start", deployUnit]);
  assert.equal(
    fs.existsSync(started),
    true,
    "old service did not run after hold removal"
  );

  fs.mkdirSync(path.join(state, "requests", "claimed"), { recursive: true });
  const lock = path.join(state, "poller.lock");
  const lockHolder = spawn("bash", ["-c", `exec 9>"${lock}"; flock -n 9; sleep 3`], {
    stdio: "ignore",
  });
  sleep(300);
  const contender = run("flock", ["-n", lock, "true"]);
  assert.notEqual(contender.status, 0, "real flock accepted concurrent consumer");
  lockHolder.kill("SIGKILL");
  sleep(200);
  assert.equal(run("flock", ["-n", lock, "true"]).status, 0);
  const claim = path.join(
    state,
    "requests",
    "claimed",
    "deploy-20260927T000000Z-abcdef0.json"
  );
  write(claim, '{"phase":"possibly_executing"}\n', 0o600);
  const poller = run(
    "bash",
    [path.join(process.cwd(), "deploy/runner/poll-deploy-requests.sh")],
    {
      env: {
        ...process.env,
        QINTOPIA_COS_ENV_FILE: path.join(root, "no-env"),
        QINTOPIA_DEPLOY_RUNNER_STATE_DIR: state,
        TENCENT_COS_BUCKET: "simulated",
        TENCENT_COS_REGION: "simulated",
        TENCENT_COS_SECRET_ID: "simulated",
        TENCENT_COS_SECRET_KEY: "simulated",
        DEPLOY_REQUEST_SIGNING_KEY: "simulated",
        DEPLOY_REQUEST_SIGNING_KEY_ID: "simulated",
      },
    }
  );
  assert.equal(poller.status, 75, "unfinished claim did not block poller");
  assert.match(poller.stderr, /unfinished deploy request claim/);

  requireSuccess("systemctl", ["stop", deployUnit]);
  fs.rmSync(`/run/systemd/system/${deployUnit}`, { force: true });
  fs.rmSync(`/run/systemd/system/${deployUnit}.d`, { recursive: true, force: true });
  serviceCreated = false;
  requireSuccess("systemctl", ["daemon-reload"]);

  const releaseRoot = "/home/ubuntu/qintopia-agent-os-releases";
  if (fs.existsSync(releaseRoot))
    throw new Error("VM release fixture root already exists");
  releaseFixtureCreated = true;
  const tSha = "1".repeat(40);
  const oSha = "2".repeat(40);
  const pSha = "3".repeat(40);
  for (const sha of [tSha, oSha, pSha]) {
    fs.mkdirSync(path.join(releaseRoot, sha, "deploy", "runner"), { recursive: true });
  }
  write(
    path.join(releaseRoot, tSha, "manifest.json"),
    JSON.stringify({ release_sha: tSha, previous_sha: oSha })
  );
  write(
    path.join(releaseRoot, oSha, "manifest.json"),
    JSON.stringify({ release_sha: oSha, previous_sha: pSha })
  );
  write(
    path.join(releaseRoot, pSha, "manifest.json"),
    JSON.stringify({ release_sha: pSha })
  );
  fs.symlinkSync(path.join(releaseRoot, tSha), path.join(releaseRoot, "current"));
  fs.symlinkSync(path.join(releaseRoot, oSha), path.join(releaseRoot, "previous"));
  const candidateOnly = "qintopia-agentos-simulated-candidate-only.service";
  write(
    `/run/systemd/system/${candidateOnly}`,
    "[Unit]\nDescription=Simulated candidate only\n[Service]\nType=oneshot\nExecStart=/usr/bin/true\n"
  );
  const oldStarted = path.join(root, "old-service-started");
  const oldUnitTemplate = path.join(root, "old-runner.service");
  write(
    oldUnitTemplate,
    `[Unit]\nDescription=Simulated old O runner\n[Service]\nType=oneshot\nExecStart=/usr/bin/touch ${oldStarted}\n`
  );
  const tInstaller = path.join(
    releaseRoot,
    tSha,
    "deploy/runner/install-release-systemd-units.sh"
  );
  const oInstaller = path.join(
    releaseRoot,
    oSha,
    "deploy/runner/install-release-systemd-units.sh"
  );
  write(
    tInstaller,
    `#!/usr/bin/env bash\nset -euo pipefail\nunit_files=(\n  qintopia-agentos-simulated-shared.service\n  ${candidateOnly}\n)\nrunner_unit_files=(\n  qintopia-agent-os-deploy-runner.service\n  qintopia-agent-os-deploy-runner.timer\n)\n`,
    0o755
  );
  write(
    oInstaller,
    `#!/usr/bin/env bash\nset -euo pipefail\nunit_files=(\n  qintopia-agentos-simulated-shared.service\n)\nrunner_unit_files=(\n  qintopia-agent-os-deploy-runner.service\n  qintopia-agent-os-deploy-runner.timer\n)\ncp "$O_UNIT_TEMPLATE" /run/systemd/system/qintopia-agent-os-deploy-runner.service\nsystemctl daemon-reload\n`,
    0o755
  );
  const fixedDropin =
    "/etc/systemd/system/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf";
  fs.mkdirSync(path.dirname(fixedDropin), { recursive: true });
  fs.copyFileSync(
    path.join(
      process.cwd(),
      "deploy/runner/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf"
    ),
    fixedDropin
  );
  installedHoldDropin = true;
  write(hold, "hold\n", 0o600);
  holdCreated = true;
  requireSuccess("systemctl", ["daemon-reload"]);
  const primitive = run(
    "bash",
    [
      path.join(process.cwd(), "deploy/runner/rollback-release.sh"),
      "--release-root",
      releaseRoot,
      "--expected-current-sha",
      tSha,
      "--expected-previous-sha",
      oSha,
      "--restore-previous-sha",
      pSha,
    ],
    {
      env: {
        ...process.env,
        O_UNIT_TEMPLATE: oldUnitTemplate,
        QINTOPIA_SYSTEMD_UNIT_DIR: "/run/systemd/system",
      },
    }
  );
  if (primitive.status !== 0)
    throw new Error(`real rollback primitive failed: ${primitive.stderr}`);
  assert.equal(
    fs.realpathSync(path.join(releaseRoot, "current")),
    path.join(releaseRoot, oSha)
  );
  assert.equal(
    fs.realpathSync(path.join(releaseRoot, "previous")),
    path.join(releaseRoot, pSha)
  );
  assert.equal(
    fs.existsSync(`/run/systemd/system/${candidateOnly}`),
    false,
    "real rollback did not remove candidate-only unit"
  );
  requireSuccess("systemctl", ["start", deployUnit]);
  assert.equal(
    fs.existsSync(oldStarted),
    false,
    "old O installer bypassed the persistent recovery hold"
  );

  const rSha = "4".repeat(40);
  fs.mkdirSync(path.join(releaseRoot, rSha, "deploy", "runner"), { recursive: true });
  write(
    path.join(releaseRoot, rSha, "manifest.json"),
    JSON.stringify({ release_sha: rSha, previous_sha: tSha })
  );
  write(
    path.join(releaseRoot, rSha, "deploy/runner/install-release-systemd-units.sh"),
    `#!/usr/bin/env bash\nset -euo pipefail\nunit_files=(\n  qintopia-agentos-simulated-shared.service\n)\nrunner_unit_files=(\n  qintopia-agent-os-deploy-runner.service\n  qintopia-agent-os-deploy-runner.timer\n)\n`,
    0o755
  );
  write(
    path.join(releaseRoot, tSha, "deploy/runner/smoke-release.sh"),
    "#!/usr/bin/env bash\nset -euo pipefail\nexit 0\n",
    0o755
  );
  for (const script of [
    "recover-release-lineage.sh",
    "wait-deploy-result.sh",
    "rollback-release.sh",
  ]) {
    const destination = path.join(releaseRoot, tSha, "deploy/runner", script);
    fs.copyFileSync(path.join(process.cwd(), "deploy/runner", script), destination);
    fs.chmodSync(destination, 0o755);
  }
  fs.mkdirSync(
    path.join(
      releaseRoot,
      tSha,
      "deploy/runner/qintopia-agent-os-deploy-runner.service.d"
    ),
    { recursive: true }
  );
  fs.copyFileSync(
    path.join(
      process.cwd(),
      "deploy/runner/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf"
    ),
    path.join(
      releaseRoot,
      tSha,
      "deploy/runner/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf"
    )
  );
  const timerPath = "/run/systemd/system/qintopia-agent-os-deploy-runner.timer";
  if (fs.existsSync(timerPath)) throw new Error("VM contains an existing deploy timer");
  write(
    timerPath,
    "[Unit]\nDescription=Simulated deploy timer\n[Timer]\nOnCalendar=hourly\n[Install]\nWantedBy=timers.target\n"
  );
  timerCreated = true;
  requireSuccess("systemctl", ["daemon-reload"]);
  const requestId = `deploy-20260927T000000Z-${crypto.randomBytes(7).toString("hex")}`;
  const signedAt = new Date().toISOString();
  const targets = [
    "qintopia-system-services",
    "hermes-erhua",
    "hermes-xiaoman",
    "hermes-silaoshi",
    "hermes-huabaosi",
    "hermes-anan",
  ];
  const canonical = (value) =>
    Array.isArray(value)
      ? `[${value.map(canonical).join(",")}]`
      : value !== null && typeof value === "object"
        ? `{${Object.keys(value)
            .sort()
            .map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`)
            .join(",")}}`
        : JSON.stringify(value);
  const signingKey = "simulated-key";
  const request = {
    schema_version: 1,
    request_id: requestId,
    environment: "production",
    repository: "qintopia-agent-studio/qintopia-agent-os",
    requested_by: "simulated",
    created_at: signedAt,
    expires_at: new Date(Date.parse(signedAt) + 3600000).toISOString(),
    release_sha: rSha,
    commit_sha: rSha,
    runtime_sha: rSha,
    runtime_artifact_profile: "huabaosi-production",
    deploy_bundle_sha: rSha,
    release_scope: ["sidecar-runtime", "deploy-bundle", "hermes-plugins"],
    restart_targets: targets,
    rollback_on_smoke_failure: true,
    dry_run: false,
    cos: {
      prefix: "qintopia-agent-os",
      bucket: "simulated",
      region: "simulated",
      request_key: `qintopia-agent-os/deploy-requests/production/requests/${requestId}.json`,
      result_key: `qintopia-agent-os/deploy-results/production/${requestId}.json`,
    },
  };
  const requestMetadata = {
    algorithm: "hmac-sha256",
    issuer: "github-actions",
    key_id: "simulated",
    signed_at: signedAt,
  };
  request.signature = {
    ...requestMetadata,
    value: crypto
      .createHmac("sha256", signingKey)
      .update(canonical({ request, signature: requestMetadata }))
      .digest("hex"),
  };
  const requestPath =
    "/var/lib/qintopia-agent-os-deploy/requests/pending/" + requestId + ".json";
  const resultPath = `/var/lib/qintopia-agent-os-deploy/results/${requestId}.json`;
  const journalPath = `/var/lib/qintopia-agent-os-deploy/recovery/${requestId}.json`;
  recoveryArtifactPaths = [requestPath, resultPath, journalPath];
  if ([requestPath, resultPath, journalPath].some((file) => fs.existsSync(file))) {
    throw new Error("VM contains existing simulated recovery evidence");
  }
  write(requestPath, JSON.stringify(request) + "\n", 0o600);
  recoveryArtifactsCreated = true;
  const result = {
    schema_version: 1,
    request_id: requestId,
    environment: "production",
    status: "failed",
    started_at: signedAt,
    finished_at: signedAt,
    release_sha: rSha,
    commit_sha: rSha,
    runtime_sha: rSha,
    runtime_artifact_profile: "huabaosi-production",
    deploy_bundle_sha: rSha,
    release_scope: request.release_scope,
    restart_targets: targets,
    previous_sha: oSha,
    current_target: "",
    checks: [],
    rollback: { attempted: false, status: "not_needed" },
    error: "simulated interruption",
  };
  const resultMetadata = {
    algorithm: "hmac-sha256",
    issuer: "qintopia-deploy-runner",
    key_id: "simulated",
    signed_at: signedAt,
  };
  result.signature = {
    ...resultMetadata,
    value: crypto
      .createHmac("sha256", signingKey)
      .update(canonical({ result, signature: resultMetadata }))
      .digest("hex"),
  };
  write(resultPath, JSON.stringify(result) + "\n", 0o600);
  const cosEnv = "/etc/qintopia/cos-artifacts.env";
  assert.equal(fs.existsSync(cosEnv), false, "VM contains an existing COS environment");
  const fakeCoscli = path.join(root, "simulated-coscli");
  write(
    fakeCoscli,
    `#!/bin/bash
set -euo pipefail
[[ "$1" == config ]] && exit 0
[[ "$1" == cp ]] || exit 64
case "$2" in
  *deploy-requests*) cp "${requestPath}" "$3" ;;
  *deploy-results*) cp "${resultPath}" "$3" ;;
  *) exit 64 ;;
esac
`,
    0o755
  );
  write(
    cosEnv,
    `export TENCENT_COS_BUCKET=simulated TENCENT_COS_REGION=simulated TENCENT_COS_SECRET_ID=simulated TENCENT_COS_SECRET_KEY=simulated DEPLOY_REQUEST_SIGNING_KEY=${signingKey} DEPLOY_REQUEST_SIGNING_KEY_ID=simulated COSCLI_PATH=${fakeCoscli}\n`,
    0o600
  );
  cosEnvCreated = true;
  const requestBytes = fs.readFileSync(requestPath);
  const sha256 = (bytes) => crypto.createHash("sha256").update(bytes).digest("hex");
  const journal = {
    schema_version: 1,
    request_id: requestId,
    request_sha256: sha256(requestBytes),
    direction: "T→R",
    original_current_sha: tSha,
    original_previous_sha: oSha,
    manifest_sha256: {
      current: sha256(fs.readFileSync(path.join(releaseRoot, tSha, "manifest.json"))),
      previous: sha256(fs.readFileSync(path.join(releaseRoot, oSha, "manifest.json"))),
    },
    phase: "intent",
  };
  write(journalPath, JSON.stringify(journal) + "\n", 0o600);
  const helper = path.join(
    releaseRoot,
    tSha,
    "deploy/runner/recover-release-lineage.sh"
  );
  const stateCases = [
    { name: "T/O", current: tSha, previous: oSha },
    { name: "T/T", current: tSha, previous: tSha },
    { name: "R/T", current: rSha, previous: tSha },
  ];
  for (const scenario of stateCases) {
    for (const pointer of ["current", "previous", "rollback-from"]) {
      fs.rmSync(path.join(releaseRoot, pointer), { force: true });
    }
    fs.symlinkSync(
      path.join(releaseRoot, scenario.current),
      path.join(releaseRoot, "current")
    );
    fs.symlinkSync(
      path.join(releaseRoot, scenario.previous),
      path.join(releaseRoot, "previous")
    );
    const recovered = run("bash", [helper, "--request-id", requestId], {
      env: {
        ...process.env,
        DEPLOY_REQUEST_SIGNING_KEY: signingKey,
        DEPLOY_REQUEST_SIGNING_KEY_ID: "simulated",
        O_UNIT_TEMPLATE: oldUnitTemplate,
        QINTOPIA_SYSTEMD_UNIT_DIR: "/run/systemd/system",
      },
    });
    if (recovered.status !== 0) {
      throw new Error(
        `${scenario.name} direction-bound recovery failed: ${recovered.stderr}`
      );
    }
    assert.equal(
      fs.realpathSync(path.join(releaseRoot, "current")),
      path.join(releaseRoot, tSha)
    );
    assert.equal(
      fs.realpathSync(path.join(releaseRoot, "previous")),
      path.join(releaseRoot, oSha)
    );
    assert.equal(
      fs.existsSync(hold),
      true,
      "recovery cleared hold before COS reconciliation"
    );
  }
  const { signature: _requestSignature, ...unsignedRequest } = request;
  const extraUnsigned = {
    ...unsignedRequest,
    restart_targets: [...targets, "hermes-wenyuange"],
  };
  const extraRequest = {
    ...extraUnsigned,
    signature: {
      ...requestMetadata,
      value: crypto
        .createHmac("sha256", signingKey)
        .update(canonical({ request: extraUnsigned, signature: requestMetadata }))
        .digest("hex"),
    },
  };
  write(requestPath, JSON.stringify(extraRequest) + "\n", 0o600);
  write(
    journalPath,
    JSON.stringify({
      ...journal,
      request_sha256: sha256(fs.readFileSync(requestPath)),
    }) + "\n",
    0o600
  );
  const extra = run("bash", [helper, "--request-id", requestId], {
    env: {
      ...process.env,
      DEPLOY_REQUEST_SIGNING_KEY: signingKey,
      DEPLOY_REQUEST_SIGNING_KEY_ID: "simulated",
      QINTOPIA_SYSTEMD_UNIT_DIR: "/run/systemd/system",
    },
  });
  assert.equal(extra.status, 75, `extra target was accepted: ${extra.stderr}`);
  assert.match(extra.stderr, /approved six-target live action/);
  assert.equal(
    fs.realpathSync(path.join(releaseRoot, "current")),
    path.join(releaseRoot, tSha)
  );
  assert.equal(
    fs.realpathSync(path.join(releaseRoot, "previous")),
    path.join(releaseRoot, oSha)
  );
  assert.equal(fs.existsSync(hold), true);
  console.log(
    JSON.stringify({
      os: "Ubuntu 24.04",
      arch: os.arch(),
      systemd: run("systemctl", ["--version"]).stdout.split("\n")[0],
      invocation,
      busyRejected: true,
      sandboxVerified: true,
      callerDeathRetained: true,
      holdVerified: true,
      realFlockVerified: true,
      claimVerified: true,
      oldUnitHoldVerified: true,
      candidateOnlyCleanupVerified: true,
      mixedForwardStatesVerified: stateCases.map(({ name }) => name),
      extraTargetRejected: true,
    })
  );
} finally {
  if (serviceCreated) {
    run("systemctl", ["stop", deployUnit]);
    fs.rmSync(`/run/systemd/system/${deployUnit}`, { force: true });
    fs.rmSync(`/run/systemd/system/${deployUnit}.d`, { recursive: true, force: true });
    run("systemctl", ["daemon-reload"]);
  }
  if (holdCreated)
    fs.rmSync("/var/lib/qintopia-agent-os-deploy/recovery/hold", { force: true });
  if (installedHoldDropin) {
    fs.rmSync(
      "/etc/systemd/system/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf",
      { force: true }
    );
  }
  if (timerCreated) {
    run("systemctl", ["disable", "--now", "qintopia-agent-os-deploy-runner.timer"]);
    fs.rmSync("/run/systemd/system/qintopia-agent-os-deploy-runner.timer", {
      force: true,
    });
  }
  if (recoveryArtifactsCreated) {
    for (const file of recoveryArtifactPaths) fs.rmSync(file, { force: true });
  }
  if (cosEnvCreated) fs.rmSync("/etc/qintopia/cos-artifacts.env", { force: true });
  if (releaseFixtureCreated) {
    fs.rmSync("/run/systemd/system/qintopia-agent-os-deploy-runner.service", {
      force: true,
    });
    fs.rmSync("/run/systemd/system/qintopia-agentos-simulated-candidate-only.service", {
      force: true,
    });
    fs.rmSync("/home/ubuntu/qintopia-agent-os-releases", {
      recursive: true,
      force: true,
    });
  }
  run("systemctl", ["daemon-reload"]);
  fs.rmSync(completion, { force: true });
  fs.rmSync(root, { recursive: true, force: true });
}
