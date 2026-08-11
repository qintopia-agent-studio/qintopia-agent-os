#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpParent = fs.existsSync("/private/tmp") ? "/private/tmp" : "/tmp";
const tmpRoot = fs.mkdtempSync(
  path.join(tmpParent, "qintopia-xiaoman-daily-case-report-hermes-cron-")
);
const sourceApply = path.join(
  repoRoot,
  "deploy/sidecar/scripts/apply-xiaoman-daily-case-report-hermes-cron.sh"
);
const sourceWrapper = path.join(
  repoRoot,
  "runtime/hermes/scripts/qintopia_xiaoman_daily_case_report.sh"
);
const sourceRegistry = path.join(
  repoRoot,
  "runtime/hermes/cron/reviewed-cron-jobs.json"
);
const sourceDeclaration = path.join(
  repoRoot,
  "runtime/hermes/cron/xiaoman/daily-case-report.job.json"
);

const fixedCronFile = "/home/ubuntu/.hermes/profiles/xiaoman/cron/jobs.json";
const fixedProfileEnv = "/home/ubuntu/.hermes/profiles/xiaoman/.env";
const fixedScriptsDir = "/home/ubuntu/.hermes/scripts";
const fixedReleaseCurrent = "/home/ubuntu/qintopia-agent-os-releases/current";
const approval = "approved-production-xiaoman-daily-case-report-hermes-cron";

const jobName = "小满·每日案例日报";
const jobExpr = "0 8 * * *";
const jobScript = "qintopia_xiaoman_daily_case_report.sh";
const chatIdFixture = "chatIdFixtureValueTestOnly000";

const sha256 = (bytes) => crypto.createHash("sha256").update(bytes).digest("hex");
const modeOf = (filePath) => fs.statSync(filePath).mode & 0o777;
const readJson = (filePath) => JSON.parse(fs.readFileSync(filePath, "utf8"));
// The apply script rewrites the cron file with sorted keys, so structural comparisons
// must ignore key order and compare canonical forms instead.
const canonical = (value) => {
  if (Array.isArray(value)) {
    return value.map(canonical);
  }
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.keys(value)
        .sort()
        .map((key) => [key, canonical(value[key])])
    );
  }
  return value;
};
const canonicalJson = (value) => JSON.stringify(canonical(value));
const listBackups = (cronDir) =>
  fs
    .readdirSync(cronDir)
    .filter((name) => name.startsWith("jobs.json.daily-case-report-"))
    .sort();
const check = (condition, message) => {
  if (!condition) {
    throw new Error(message);
  }
};

try {
  const python = spawnSync("which", ["python3"], { encoding: "utf8" }).stdout.trim();
  check(Boolean(python), "python3 is required for the Hermes cron apply fixture");

  // The daily case-report worker uploads through the Huabaosi Feishu boundary and
  // renders through system Pillow/psql, so the wrapper must pin both release SHAs at
  // the exec boundary and keep PATH on the fixed system directories.
  const wrapperBody = fs.readFileSync(sourceWrapper, "utf8");
  for (const fragment of [
    'export PATH="/usr/bin:/bin"',
    'export QINTOPIA_DEPLOYED_COMMIT_SHA="$release_sha"',
    'export QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA="$release_sha"',
    "export QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED=1",
    "deploy/sidecar/scripts/xiaoman-daily-case-report-auto-publish-worker.sh",
  ]) {
    check(
      wrapperBody.includes(fragment),
      `wrapper is missing the reviewed binding: ${fragment}`
    );
  }

  const profileDir = path.join(tmpRoot, "home/ubuntu/.hermes/profiles/xiaoman");
  const cronDir = path.join(profileDir, "cron");
  const cronFile = path.join(cronDir, "jobs.json");
  const profileEnv = path.join(profileDir, ".env");
  const scriptsDir = path.join(tmpRoot, "home/ubuntu/.hermes/scripts");
  const releaseCurrent = path.join(tmpRoot, "release/current");
  const releaseWrapper = path.join(
    releaseCurrent,
    "runtime/hermes/scripts/qintopia_xiaoman_daily_case_report.sh"
  );
  const installedWrapper = path.join(scriptsDir, jobScript);

  fs.mkdirSync(cronDir, { recursive: true });
  fs.mkdirSync(scriptsDir, { recursive: true });
  fs.mkdirSync(path.dirname(releaseWrapper), { recursive: true });
  fs.copyFileSync(sourceWrapper, releaseWrapper);
  fs.chmodSync(releaseWrapper, 0o755);

  fs.writeFileSync(
    profileEnv,
    [
      "# Xiaoman profile env fixture",
      "WECOM_SOME_OTHER_TARGET=ignored",
      `WECOM_HOME_CHANNEL=${chatIdFixture}`,
      "",
    ].join("\n"),
    "utf8"
  );
  fs.chmodSync(profileEnv, 0o600);

  const applyScript = path.join(
    tmpRoot,
    "apply-xiaoman-daily-case-report-hermes-cron.sh"
  );
  const applySource = fs
    .readFileSync(sourceApply, "utf8")
    .replaceAll("/usr/bin/python3", python)
    .replaceAll(fixedReleaseCurrent, releaseCurrent)
    .replaceAll(fixedCronFile, cronFile)
    .replaceAll(fixedProfileEnv, profileEnv)
    .replaceAll(fixedScriptsDir, scriptsDir);
  check(
    !applySource.includes("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_HERMES_CRON_FILE") &&
      !applySource.includes("QINTOPIA_XIAOMAN_PROFILE_DIR"),
    "apply script must not accept cron or profile path overrides"
  );
  fs.writeFileSync(applyScript, applySource, "utf8");
  fs.chmodSync(applyScript, 0o755);

  const writeCron = (document) => {
    fs.writeFileSync(cronFile, `${JSON.stringify(document, null, 2)}\n`, "utf8");
    fs.chmodSync(cronFile, 0o600);
  };
  const runningJob = () => ({
    id: "0123456789ab",
    name: "小满·周日活动计划确认",
    schedule: { kind: "cron", expr: "0 20 * * 0", display: "0 20 * * 0" },
    no_agent: true,
    script: "qintopia_xiaoman_weekly_plan_confirmation.sh",
    deliver: "origin",
    origin: {
      platform: "wecom",
      chat_id: "wrOtherChannel",
      chat_name: null,
      thread_id: null,
    },
    enabled: true,
    skills: [],
    last_run_at: "2026-08-10T12:00:00Z",
    next_run_at: "2026-08-16T12:00:00Z",
    state: "idle",
    last_status: "ok",
    last_error: null,
    repeat: { completed: 3 },
  });
  const emptyEnvelope = () => ({
    schema_version: 1,
    retired_by: "retire-xiaoman-legacy-cron-production.sh",
    retired_at: "2026-08-10T00:00:00Z",
    jobs: [],
  });

  const run = (args, extraEnv = {}) =>
    spawnSync("bash", [applyScript, ...args], {
      cwd: repoRoot,
      env: { ...process.env, ...extraEnv },
      encoding: "utf8",
    });
  const runApproved = (args) =>
    run(args, { QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_HERMES_CRON: approval });
  const forbidLeak = (result, label) => {
    const combined = `${result.stdout}\n${result.stderr}`;
    check(
      !combined.includes(chatIdFixture),
      `${label} leaked the real origin chat id into its evidence`
    );
  };

  // The shipped declaration template and registry entry must stay in sync with the
  // apply script, because the observation allowlist matches on these exact fields.
  const declaration = readJson(sourceDeclaration);
  check(
    declaration.name === jobName &&
      declaration.schedule.expr === jobExpr &&
      declaration.script === jobScript &&
      declaration.no_agent === true &&
      declaration.deliver === "origin" &&
      declaration.enabled === false &&
      declaration.origin.chat_id === "{{QINTOPIA_XIAOMAN_TECHNICAL_HOME_CHANNEL}}",
    "declaration template drifted from the reviewed daily case report contract"
  );
  const registry = readJson(sourceRegistry);
  const registryEntry = registry.reviewed_jobs.find((entry) => entry.name === jobName);
  check(
    registryEntry &&
      registryEntry.profile === "xiaoman" &&
      registryEntry.schedule_expr === jobExpr &&
      registryEntry.script === jobScript &&
      registryEntry.no_agent === true &&
      registryEntry.deliver === "origin" &&
      registryEntry.origin_platform === "wecom",
    "reviewed registry entry drifted from the daily case report declaration"
  );

  // Missing owner approval must not touch any live state.
  writeCron(emptyEnvelope());
  let baselineSha = sha256(fs.readFileSync(cronFile));
  let result = run(["--install"]);
  check(result.status !== 0, "apply accepted a missing owner approval");
  check(
    sha256(fs.readFileSync(cronFile)) === baselineSha &&
      !fs.existsSync(installedWrapper) &&
      listBackups(cronDir).length === 0,
    "apply mutated state without owner approval"
  );

  // Install into the retired empty envelope.
  result = runApproved(["--install"]);
  check(result.status === 0, `install failed\n${result.stdout}\n${result.stderr}`);
  forbidLeak(result, "install");
  check(
    result.stdout.includes('"status":"daily_case_report_hermes_cron_installed"') &&
      result.stdout.includes('"job_enabled":false') &&
      result.stdout.includes('"wrapper_installed":true') &&
      result.stdout.includes('"backup_created":true') &&
      result.stdout.includes(
        "xiaoman_daily_case_report_hermes_cron_snapshot_sync_ok=false"
      ),
    `install did not emit sanitized evidence\n${result.stdout}`
  );
  let cron = readJson(cronFile);
  check(
    cron.schema_version === 1 &&
      cron.retired_by === "retire-xiaoman-legacy-cron-production.sh" &&
      cron.jobs.length === 1,
    "install did not preserve the cron envelope"
  );
  let inserted = cron.jobs[0];
  check(
    /^[0-9a-f]{12}$/.test(inserted.id) &&
      inserted.name === jobName &&
      inserted.schedule.kind === "cron" &&
      inserted.schedule.expr === jobExpr &&
      inserted.no_agent === true &&
      inserted.script === jobScript &&
      inserted.deliver === "origin" &&
      inserted.origin.platform === "wecom" &&
      inserted.origin.chat_id === chatIdFixture &&
      inserted.origin.chat_name === null &&
      inserted.origin.thread_id === null &&
      inserted.enabled === false,
    `install wrote an unexpected job declaration\n${JSON.stringify(inserted)}`
  );
  check(modeOf(cronFile) === 0o600, "install did not normalize the cron file mode");
  check(
    modeOf(installedWrapper) === 0o700 &&
      fs.readFileSync(installedWrapper, "utf8") ===
        fs.readFileSync(releaseWrapper, "utf8"),
    "install did not deploy the release wrapper at mode 0700"
  );
  let backups = listBackups(cronDir);
  check(backups.length === 1, "install did not create exactly one backup");
  check(
    modeOf(path.join(cronDir, backups[0])) === 0o600 &&
      sha256(fs.readFileSync(path.join(cronDir, backups[0]))) === baselineSha,
    "install backup did not preserve the previous cron bytes"
  );

  // A second install must refuse the duplicate name instead of appending twice.
  baselineSha = sha256(fs.readFileSync(cronFile));
  result = runApproved(["--install"]);
  check(result.status !== 0, "install accepted a duplicate daily case report job");
  check(
    result.stderr.includes("already declares the daily case report job") &&
      sha256(fs.readFileSync(cronFile)) === baselineSha &&
      listBackups(cronDir).length === 1,
    "duplicate install mutated the cron file"
  );

  // --enable flips only the reviewed job and preserves daemon runtime fields.
  const other = runningJob();
  cron = readJson(cronFile);
  cron.jobs = [other, cron.jobs[0]];
  writeCron(cron);
  baselineSha = sha256(fs.readFileSync(cronFile));
  result = runApproved(["--enable"]);
  check(result.status === 0, `enable failed\n${result.stdout}\n${result.stderr}`);
  forbidLeak(result, "enable");
  check(
    result.stdout.includes('"status":"daily_case_report_hermes_cron_enabled"') &&
      result.stdout.includes('"job_enabled":true') &&
      result.stdout.includes('"live_profile_modified":true'),
    `enable did not emit sanitized evidence\n${result.stdout}`
  );
  cron = readJson(cronFile);
  check(cron.jobs.length === 2, "enable changed the job count");
  const untouched = cron.jobs.find((job) => job.name === other.name);
  check(
    canonicalJson(untouched) === canonicalJson(other),
    "enable modified an unrelated job or dropped its daemon runtime fields"
  );
  const reviewed = cron.jobs.find((job) => job.name === jobName);
  check(
    reviewed.enabled === true && reviewed.origin.chat_id === chatIdFixture,
    "enable did not flip the reviewed job"
  );
  check(listBackups(cronDir).length === 2, "enable did not create a backup");

  // --enable is idempotent and must not rewrite an already enabled job.
  baselineSha = sha256(fs.readFileSync(cronFile));
  result = runApproved(["--enable"]);
  check(result.status === 0, `repeat enable failed\n${result.stderr}`);
  check(
    result.stdout.includes(
      '"status":"daily_case_report_hermes_cron_already_enabled"'
    ) &&
      result.stdout.includes('"live_profile_modified":false') &&
      sha256(fs.readFileSync(cronFile)) === baselineSha &&
      listBackups(cronDir).length === 2,
    "repeat enable was not idempotent"
  );

  // --enable must refuse a drifted installed wrapper.
  fs.writeFileSync(installedWrapper, "#!/usr/bin/env bash\necho drift\n", "utf8");
  fs.chmodSync(installedWrapper, 0o700);
  result = runApproved(["--enable"]);
  check(
    result.status !== 0 &&
      result.stderr.includes("does not match the release-local wrapper source"),
    "enable accepted a drifted installed wrapper"
  );
  fs.copyFileSync(releaseWrapper, installedWrapper);
  fs.chmodSync(installedWrapper, 0o700);

  // --enable must refuse a cron file without the reviewed declaration.
  writeCron(emptyEnvelope());
  result = runApproved(["--enable"]);
  check(
    result.status !== 0 &&
      result.stderr.includes("exactly one reviewed daily case report job"),
    "enable accepted a cron file without the reviewed job"
  );

  // --enable must refuse a declaration whose schedule drifted from the registry.
  const drifted = emptyEnvelope();
  drifted.jobs = [
    {
      id: "abcdef012345",
      name: jobName,
      schedule: { kind: "cron", expr: "30 9 * * 2", display: "30 9 * * 2" },
      no_agent: true,
      script: jobScript,
      deliver: "origin",
      origin: {
        platform: "wecom",
        chat_id: chatIdFixture,
        chat_name: null,
        thread_id: null,
      },
      enabled: false,
      skills: [],
    },
  ];
  writeCron(drifted);
  result = runApproved(["--enable"]);
  check(
    result.status !== 0 && result.stderr.includes("schedule does not match"),
    "enable accepted a drifted schedule expression"
  );

  // --enable must refuse delivery-boundary drift: a job with the reviewed name,
  // schedule, and script but a changed delivery target or mode must not activate.
  const reviewedJob = () => ({
    id: "abcdef012345",
    name: jobName,
    schedule: { kind: "cron", expr: jobExpr, display: jobExpr },
    no_agent: true,
    script: jobScript,
    deliver: "origin",
    origin: {
      platform: "wecom",
      chat_id: chatIdFixture,
      chat_name: null,
      thread_id: null,
    },
    enabled: false,
    skills: [],
  });
  const enableWithJob = (job) => {
    const envelope = emptyEnvelope();
    envelope.jobs = [job];
    writeCron(envelope);
    return runApproved(["--enable"]);
  };

  result = enableWithJob({ ...reviewedJob(), deliver: "none" });
  check(
    result.status !== 0 && result.stderr.includes("deliver mode does not match"),
    "enable accepted a drifted deliver mode"
  );

  const platformDrift = reviewedJob();
  platformDrift.origin = { ...platformDrift.origin, platform: "feishu" };
  result = enableWithJob(platformDrift);
  check(
    result.status !== 0 && result.stderr.includes("origin platform does not match"),
    "enable accepted a drifted origin platform"
  );

  const threadDrift = reviewedJob();
  threadDrift.origin = { ...threadDrift.origin, thread_id: "thread-1" };
  result = enableWithJob(threadDrift);
  check(
    result.status !== 0 && result.stderr.includes("origin routing fields do not match"),
    "enable accepted drifted origin routing fields"
  );

  const chatDrift = reviewedJob();
  chatDrift.origin = { ...chatDrift.origin, chat_id: "unreviewed-chat-id" };
  result = enableWithJob(chatDrift);
  check(
    result.status !== 0 &&
      result.stderr.includes("origin chat id drifted from the Xiaoman profile env"),
    "enable accepted a drifted origin chat id"
  );

  // The wrapper must stay silent on success: Hermes delivers any stdout straight to the
  // origin chat, and the worker prints an operations-review summary that must not leak.
  const wrapperStateRoot = path.join(tmpRoot, "state");
  const wrapperEnvFile = path.join(tmpRoot, "message-sidecar.env");
  const wrapperReleaseRoot = path.join(tmpRoot, "wrapper-releases");
  const wrapperReleaseSha = "a".repeat(40);
  const wrapperReleaseDir = path.join(wrapperReleaseRoot, wrapperReleaseSha);
  const fakeWorker = path.join(
    wrapperReleaseDir,
    "deploy/sidecar/scripts/xiaoman-daily-case-report-auto-publish-worker.sh"
  );
  const draftBody = "operator review draft body that must never reach the group";
  fs.mkdirSync(path.dirname(fakeWorker), { recursive: true });
  fs.symlinkSync(wrapperReleaseDir, path.join(wrapperReleaseRoot, "current"));
  fs.writeFileSync(
    wrapperEnvFile,
    [
      "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED=0",
      "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_PRODUCTION_APPROVAL=approved-production-xiaoman-daily-case-report",
      "QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE=1",
      "PATH=/tmp/unreviewed",
      "",
    ].join("\n"),
    "utf8"
  );
  const writeFakeWorker = (exitCode) => {
    fs.writeFileSync(
      fakeWorker,
      [
        "#!/usr/bin/env bash",
        'echo "ENABLED=${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED:-unset}"',
        'echo "SHA=${QINTOPIA_DEPLOYED_COMMIT_SHA:-unset}"',
        'echo "HUABAOSI_SHA=${QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA:-unset}"',
        'echo "PATH_VALUE=${PATH}"',
        'echo "APPROVAL=${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_PRODUCTION_APPROVAL:-unset}"',
        'echo "RELEASE_DIR_DEFINED=${QINTOPIA_RELEASE_DIR+yes}"',
        'echo "WRAPPER_PATH_DEFINED=${QINTOPIA_XIAOMAN_WRAPPER_PATH+yes}"',
        'echo "OUTPUT_DIR_DEFINED=${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_OUTPUT_DIR+yes}"',
        `echo "${draftBody}"`,
        `exit ${exitCode}`,
        "",
      ].join("\n"),
      "utf8"
    );
    fs.chmodSync(fakeWorker, 0o755);
  };

  const wrapperScript = path.join(tmpRoot, "qintopia_xiaoman_daily_case_report.sh");
  fs.writeFileSync(
    wrapperScript,
    fs
      .readFileSync(sourceWrapper, "utf8")
      .replaceAll(fixedReleaseCurrent, path.join(wrapperReleaseRoot, "current"))
      .replaceAll("/etc/qintopia/message-sidecar.env", wrapperEnvFile)
      .replaceAll("/home/ubuntu/.local/state/qintopia-agentos", wrapperStateRoot),
    "utf8"
  );
  check(
    fs
      .readFileSync(sourceWrapper, "utf8")
      .includes(
        'WORKER="${release_dir}/deploy/sidecar/scripts/xiaoman-daily-case-report-auto-publish-worker.sh"'
      ),
    "daily case report wrapper must execute the worker from the resolved release dir"
  );
  fs.chmodSync(wrapperScript, 0o755);
  const wrapperLog = path.join(
    wrapperStateRoot,
    "xiaoman-daily-case-report/hermes-cron.log"
  );
  const runWrapper = () =>
    spawnSync("bash", [wrapperScript], {
      cwd: tmpRoot,
      // Hermes passes the gateway process env through; the worker refuses these keys.
      env: {
        ...process.env,
        QINTOPIA_RELEASE_DIR: "/evil/release",
        QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_OUTPUT_DIR: "/evil/output",
      },
      encoding: "utf8",
    });

  writeFakeWorker(0);
  result = runWrapper();
  check(result.status === 0, `wrapper success path failed\n${result.stderr}`);
  check(
    result.stdout === "" && result.stderr === "",
    `wrapper success path was not silent\n${result.stdout}\n${result.stderr}`
  );
  let logText = fs.readFileSync(wrapperLog, "utf8");
  check(
    logText.includes(draftBody) &&
      logText.includes("run=ok") &&
      logText.includes("ENABLED=1") &&
      logText.includes(`SHA=${wrapperReleaseSha}`) &&
      logText.includes(`HUABAOSI_SHA=${wrapperReleaseSha}`) &&
      logText.includes("PATH_VALUE=/usr/bin:/bin\n") &&
      logText.includes("APPROVAL=approved-production-xiaoman-daily-case-report") &&
      logText.includes("RELEASE_DIR_DEFINED=\n") &&
      logText.includes("WRAPPER_PATH_DEFINED=\n") &&
      logText.includes("OUTPUT_DIR_DEFINED=\n"),
    `wrapper did not normalize the worker environment\n${logText}`
  );

  writeFakeWorker(3);
  result = runWrapper();
  check(result.status === 3, `wrapper did not propagate the worker exit code`);
  check(
    result.stdout.trim() ===
      "xiaoman-daily-case-report worker failed (exit=3); evidence in server-local log",
    `wrapper failure path printed unexpected output\n${result.stdout}`
  );
  check(
    !result.stdout.includes(draftBody) && !result.stderr.includes(draftBody),
    "wrapper failure alert leaked worker output"
  );
  logText = fs.readFileSync(wrapperLog, "utf8");
  check(
    logText.includes("run=failed exit=3"),
    "wrapper did not record the failed run in the server-local log"
  );

  // A profile env without exactly one reviewed channel key must fail closed.
  writeCron(emptyEnvelope());
  fs.rmSync(installedWrapper, { force: true });
  fs.writeFileSync(profileEnv, "WECOM_HOME_CHANNEL=\n", "utf8");
  fs.chmodSync(profileEnv, 0o600);
  result = runApproved(["--install"]);
  check(
    result.status !== 0 && result.stderr.includes("WECOM_HOME_CHANNEL"),
    "install accepted an empty origin chat id"
  );
  check(
    readJson(cronFile).jobs.length === 0 && !fs.existsSync(installedWrapper),
    "failed install mutated live state"
  );
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Xiaoman daily case report Hermes cron apply test passed.");
