#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpParent = fs.existsSync("/private/tmp") ? "/private/tmp" : "/tmp";
const tmpRoot = fs.mkdtempSync(
  path.join(tmpParent, "qintopia-xiaoman-plan-confirmation-hermes-cron-")
);
const sourceApply = path.join(
  repoRoot,
  "deploy/sidecar/scripts/apply-xiaoman-weekly-plan-confirmation-hermes-cron.sh"
);
const fixedReleaseDir = "/home/ubuntu/qintopia-agent-os-releases/current";
const fixedWrapperDest =
  "/home/ubuntu/.hermes/scripts/qintopia_xiaoman_weekly_plan_confirmation.sh";
const fixedCronFile = "/home/ubuntu/.hermes/profiles/xiaoman/cron/jobs.json";
const fixedProfileEnv = "/home/ubuntu/.hermes/profiles/xiaoman/.env";
const fixedSyncScript =
  "/home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/sync-hermes-cron-snapshot.sh";
const approval = "approved-production-xiaoman-weekly-plan-confirmation-hermes-cron";

const writeExecutable = (filePath, content) => {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content, "utf8");
  fs.chmodSync(filePath, 0o755);
};
const sha256 = (bytes) => crypto.createHash("sha256").update(bytes).digest("hex");
const modeOf = (filePath) => fs.statSync(filePath).mode & 0o777;
const listBackups = (cronDir) =>
  fs
    .readdirSync(cronDir)
    .filter((name) => name.startsWith("jobs.json.pre-"))
    .sort();

try {
  const python = spawnSync("which", ["python3"], { encoding: "utf8" }).stdout.trim();
  if (!python) {
    throw new Error("python3 is required for the weekly plan confirmation fixture");
  }

  const scriptsDir = path.join(tmpRoot, "scripts");
  const applyScript = path.join(
    scriptsDir,
    "apply-xiaoman-weekly-plan-confirmation-hermes-cron.sh"
  );
  const releaseRoot = path.join(tmpRoot, "releases");
  const releaseSha = "0123456789abcdef0123456789abcdef01234567";
  const releaseDir = path.join(releaseRoot, releaseSha);
  const releaseCurrent = path.join(releaseRoot, "current");
  const wrapperSource = path.join(
    releaseDir,
    "runtime",
    "hermes",
    "scripts",
    "qintopia_xiaoman_weekly_plan_confirmation.sh"
  );
  const fakeSync = path.join(
    releaseDir,
    "deploy",
    "sidecar",
    "scripts",
    "sync-hermes-cron-snapshot.sh"
  );
  const syncLog = path.join(tmpRoot, "sync.log");
  const profileDir = path.join(
    tmpRoot,
    "home",
    "ubuntu",
    ".hermes",
    "profiles",
    "xiaoman"
  );
  const cronDir = path.join(profileDir, "cron");
  const cronFile = path.join(cronDir, "jobs.json");
  const profileEnv = path.join(profileDir, ".env");
  const wrapperDest = path.join(
    tmpRoot,
    "home",
    "ubuntu",
    ".hermes",
    "scripts",
    "qintopia_xiaoman_weekly_plan_confirmation.sh"
  );

  fs.mkdirSync(cronDir, { recursive: true });
  writeExecutable(wrapperSource, "#!/usr/bin/env bash\nexit 0\n");
  writeExecutable(
    fakeSync,
    `#!/usr/bin/env bash\nprintf run >>${JSON.stringify(syncLog)}\n`
  );
  fs.symlinkSync(releaseDir, releaseCurrent, "dir");
  fs.writeFileSync(
    profileEnv,
    "WECOM_HOME_CHANNEL=FixtureChat\nFEISHU_APP_SECRET=supersecret\n",
    "utf8"
  );
  fs.chmodSync(profileEnv, 0o600);

  const otherJob = {
    id: "other00000001",
    name: "其他任务",
    schedule: { kind: "cron", expr: "0 9 * * *", display: "0 9 * * *" },
    no_agent: true,
    script: "other.sh",
    enabled: true,
    last_run_at: "2026-08-09T09:00:00+08:00",
    next_run_at: "2026-08-10T09:00:00+08:00",
    state: "scheduled",
    last_status: "ok",
    repeat: { times: null, completed: 1 },
  };
  fs.writeFileSync(
    cronFile,
    JSON.stringify({ schema_version: 1, jobs: [otherJob] }, null, 2),
    "utf8"
  );
  fs.chmodSync(cronFile, 0o600);

  const applySource = fs
    .readFileSync(sourceApply, "utf8")
    .replaceAll("/usr/bin/python3", python)
    .replaceAll(fixedReleaseDir, releaseCurrent)
    .replaceAll(fixedWrapperDest, wrapperDest)
    .replaceAll(fixedCronFile, cronFile)
    .replaceAll(fixedProfileEnv, profileEnv)
    .replaceAll(fixedSyncScript, fakeSync);
  if (
    applySource.includes(
      "QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_HERMES_CRON_FILE"
    ) ||
    applySource.includes(
      "QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_HERMES_CRON_PROFILE_DIR"
    )
  ) {
    throw new Error(
      "weekly plan confirmation apply script must not accept path overrides"
    );
  }
  writeExecutable(applyScript, applySource);

  const run = (mode, extraEnv = {}) =>
    spawnSync("bash", [applyScript, mode], {
      cwd: repoRoot,
      env: {
        ...process.env,
        ...extraEnv,
      },
      encoding: "utf8",
    });

  let result = run("--install");
  if (result.status === 0) {
    throw new Error("weekly plan confirmation apply accepted missing owner approval");
  }
  if (
    fs.existsSync(wrapperDest) ||
    fs.readdirSync(cronDir).some((n) => n.startsWith("jobs.json.pre-"))
  ) {
    throw new Error(
      "weekly plan confirmation apply mutated state without owner approval"
    );
  }

  result = run("--install", {
    QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_HERMES_CRON: approval,
  });
  if (result.status !== 0) {
    throw new Error(
      `weekly plan confirmation install failed\n${result.stdout}\n${result.stderr}`
    );
  }
  const combined = `${result.stdout}\n${result.stderr}`;
  for (const forbidden of ["FixtureChat", "supersecret"]) {
    if (combined.includes(forbidden)) {
      throw new Error(
        `weekly plan confirmation apply leaked sensitive value: ${forbidden}`
      );
    }
  }
  if (
    !combined.includes('"status":"weekly_plan_confirmation_hermes_cron_applied"') ||
    !combined.includes('"enabled":false') ||
    !combined.includes('"backup_created":true') ||
    !combined.includes('"live_profile_modified":true') ||
    !combined.includes('"external_calls_executed":false') ||
    !combined.includes("weekly plan confirmation Hermes cron apply passed")
  ) {
    throw new Error(
      `weekly plan confirmation install evidence incomplete\n${combined}`
    );
  }
  if (!fs.readFileSync(syncLog, "utf8").includes("run")) {
    throw new Error("weekly plan confirmation apply did not run the snapshot sync");
  }
  if (modeOf(wrapperDest) !== 0o700) {
    throw new Error("weekly plan confirmation wrapper mode is not 0700");
  }

  let cron = JSON.parse(fs.readFileSync(cronFile, "utf8"));
  const installedJob = cron.jobs.find((job) => job.name === "小满·周日活动计划确认");
  if (!installedJob || installedJob.enabled !== false) {
    throw new Error("weekly plan confirmation install did not insert a disabled job");
  }
  if (!/^[0-9a-f]{12}$/.test(installedJob.id || "")) {
    throw new Error("weekly plan confirmation install did not generate a 12-hex id");
  }
  const preserved = cron.jobs.find((job) => job.id === otherJob.id);
  for (const key of ["last_run_at", "next_run_at", "state", "last_status", "repeat"]) {
    assert.deepEqual(
      preserved?.[key],
      otherJob[key],
      `weekly plan confirmation apply lost other job runtime field: ${key}`
    );
  }
  if (listBackups(cronDir).length !== 1) {
    throw new Error("weekly plan confirmation install did not create one backup");
  }

  const firstSha = sha256(fs.readFileSync(cronFile));
  result = run("--install", {
    QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_HERMES_CRON: approval,
  });
  if (result.status !== 0 || !result.stdout.includes('"backup_created":false')) {
    throw new Error("weekly plan confirmation install is not idempotent");
  }
  if (
    sha256(fs.readFileSync(cronFile)) !== firstSha ||
    listBackups(cronDir).length !== 1
  ) {
    throw new Error("idempotent install mutated cron state");
  }

  result = run("--enable", {
    QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_HERMES_CRON: approval,
  });
  if (result.status !== 0 || !result.stdout.includes('"enabled":true')) {
    throw new Error(
      `weekly plan confirmation enable failed\n${result.stdout}\n${result.stderr}`
    );
  }
  cron = JSON.parse(fs.readFileSync(cronFile, "utf8"));
  const enabledJob = cron.jobs.find((job) => job.name === "小满·周日活动计划确认");
  if (enabledJob.enabled !== true) {
    throw new Error("weekly plan confirmation enable did not flip the job");
  }
  if (cron.jobs.find((job) => job.id === otherJob.id).enabled !== true) {
    throw new Error("weekly plan confirmation enable changed another job");
  }
  if (listBackups(cronDir).length !== 2) {
    throw new Error("weekly plan confirmation enable did not create a backup");
  }

  const enabledSha = sha256(fs.readFileSync(cronFile));
  result = run("--enable", {
    QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_HERMES_CRON: approval,
  });
  if (result.status !== 0 || !result.stdout.includes('"backup_created":false')) {
    throw new Error("weekly plan confirmation enable is not idempotent");
  }
  if (
    sha256(fs.readFileSync(cronFile)) !== enabledSha ||
    listBackups(cronDir).length !== 2
  ) {
    throw new Error("idempotent enable mutated cron state");
  }

  const driftJob = cron.jobs.find((job) => job.name === "小满·周日活动计划确认");
  driftJob.schedule = { kind: "cron", expr: "0 21 * * 0", display: "0 21 * * 0" };
  fs.writeFileSync(cronFile, JSON.stringify(cron, null, 2), "utf8");
  result = run("--install", {
    QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_HERMES_CRON: approval,
  });
  if (result.status === 0 || !result.stderr.includes("definition drifts")) {
    throw new Error("weekly plan confirmation apply accepted a drifted declaration");
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Xiaoman weekly plan confirmation Hermes cron apply test passed.");
