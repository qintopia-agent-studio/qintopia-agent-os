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
  path.join(tmpParent, "qintopia-erhua-morning-brief-hermes-cron-")
);
const sourceApply = path.join(
  repoRoot,
  "deploy/sidecar/scripts/apply-erhua-morning-brief-hermes-cron.sh"
);
const fixedReleaseDir = "/home/ubuntu/qintopia-agent-os-releases/current";
const fixedWrapperDest =
  "/home/ubuntu/.hermes/profiles/erhua/scripts/qintopia_erhua_morning_brief.sh";
const fixedCronFile = "/home/ubuntu/.hermes/profiles/erhua/cron/jobs.json";
const fixedProfileEnv = "/home/ubuntu/.hermes/profiles/erhua/.env";
const fixedSyncScript =
  "/home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/sync-hermes-cron-snapshot.sh";
const approval = "approved-production-erhua-morning-brief-hermes-cron";

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
    throw new Error("python3 is required for the morning brief fixture");
  }

  const scriptsDir = path.join(tmpRoot, "scripts");
  const applyScript = path.join(scriptsDir, "apply-erhua-morning-brief-hermes-cron.sh");
  const releaseRoot = path.join(tmpRoot, "releases");
  const releaseSha = "0123456789abcdef0123456789abcdef01234567";
  const releaseDir = path.join(releaseRoot, releaseSha);
  const releaseCurrent = path.join(releaseRoot, "current");
  const wrapperSource = path.join(
    releaseDir,
    "runtime",
    "hermes",
    "scripts",
    "qintopia_erhua_morning_brief.sh"
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
    "erhua"
  );
  const cronDir = path.join(profileDir, "cron");
  const cronFile = path.join(cronDir, "jobs.json");
  const profileEnv = path.join(profileDir, ".env");
  const wrapperDest = path.join(
    profileDir,
    "scripts",
    "qintopia_erhua_morning_brief.sh"
  );
  const updatedAt = "2026-08-10T01:00:00Z";

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
    JSON.stringify({ updated_at: updatedAt, jobs: [otherJob] }, null, 2),
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
    applySource.includes("QINTOPIA_ERHUA_MORNING_BRIEF_HERMES_CRON_FILE") ||
    applySource.includes("QINTOPIA_ERHUA_MORNING_BRIEF_HERMES_CRON_PROFILE_DIR")
  ) {
    throw new Error("Erhua morning brief apply script must not accept path overrides");
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
    throw new Error("Erhua morning brief apply accepted missing owner approval");
  }
  if (
    fs.existsSync(wrapperDest) ||
    fs.readdirSync(cronDir).some((n) => n.startsWith("jobs.json.pre-"))
  ) {
    throw new Error("Erhua morning brief apply mutated state without owner approval");
  }

  result = run("--install", {
    QINTOPIA_ERHUA_MORNING_BRIEF_HERMES_CRON: approval,
  });
  if (result.status !== 0) {
    throw new Error(
      `Erhua morning brief install failed\n${result.stdout}\n${result.stderr}`
    );
  }
  const combined = `${result.stdout}\n${result.stderr}`;
  for (const forbidden of ["FixtureChat", "supersecret"]) {
    if (combined.includes(forbidden)) {
      throw new Error(`Erhua morning brief apply leaked sensitive value: ${forbidden}`);
    }
  }
  if (
    !combined.includes('"status":"erhua_morning_brief_hermes_cron_applied"') ||
    !combined.includes('"enabled":false') ||
    !combined.includes('"backup_created":true') ||
    !combined.includes('"live_profile_modified":true') ||
    !combined.includes('"external_calls_executed":false') ||
    !combined.includes('"updated_at_preserved":true') ||
    !combined.includes("Erhua morning brief Hermes cron apply passed")
  ) {
    throw new Error(`Erhua morning brief install evidence incomplete\n${combined}`);
  }
  if (!fs.readFileSync(syncLog, "utf8").includes("run")) {
    throw new Error("Erhua morning brief apply did not run the snapshot sync");
  }
  if (modeOf(wrapperDest) !== 0o700) {
    throw new Error("Erhua morning brief wrapper mode is not 0700");
  }

  let cron = JSON.parse(fs.readFileSync(cronFile, "utf8"));
  if (cron.schema_version !== 1) {
    throw new Error("Erhua morning brief apply did not normalize schema_version");
  }
  if (cron.updated_at !== updatedAt) {
    throw new Error("Erhua morning brief apply did not preserve updated_at");
  }
  const installedJob = cron.jobs.find((job) => job.name === "二花·每日早报");
  if (!installedJob || installedJob.enabled !== false) {
    throw new Error("Erhua morning brief install did not insert a disabled job");
  }
  if (!/^[0-9a-f]{12}$/.test(installedJob.id || "")) {
    throw new Error("Erhua morning brief install did not generate a 12-hex id");
  }
  const preserved = cron.jobs.find((job) => job.id === otherJob.id);
  for (const key of ["last_run_at", "next_run_at", "state", "last_status", "repeat"]) {
    assert.deepEqual(
      preserved?.[key],
      otherJob[key],
      `Erhua morning brief apply lost other job runtime field: ${key}`
    );
  }
  if (listBackups(cronDir).length !== 1) {
    throw new Error("Erhua morning brief install did not create one backup");
  }

  const firstSha = sha256(fs.readFileSync(cronFile));
  result = run("--install", {
    QINTOPIA_ERHUA_MORNING_BRIEF_HERMES_CRON: approval,
  });
  if (result.status !== 0 || !result.stdout.includes('"backup_created":false')) {
    throw new Error("Erhua morning brief install is not idempotent");
  }
  if (
    sha256(fs.readFileSync(cronFile)) !== firstSha ||
    listBackups(cronDir).length !== 1
  ) {
    throw new Error("idempotent install mutated cron state");
  }

  result = run("--enable", {
    QINTOPIA_ERHUA_MORNING_BRIEF_HERMES_CRON: approval,
  });
  if (result.status !== 0 || !result.stdout.includes('"enabled":true')) {
    throw new Error(
      `Erhua morning brief enable failed\n${result.stdout}\n${result.stderr}`
    );
  }
  cron = JSON.parse(fs.readFileSync(cronFile, "utf8"));
  if (cron.updated_at !== updatedAt) {
    throw new Error("Erhua morning brief enable did not preserve updated_at");
  }
  const enabledJob = cron.jobs.find((job) => job.name === "二花·每日早报");
  if (enabledJob.enabled !== true) {
    throw new Error("Erhua morning brief enable did not flip the job");
  }
  if (cron.jobs.find((job) => job.id === otherJob.id).enabled !== true) {
    throw new Error("Erhua morning brief enable changed another job");
  }
  if (listBackups(cronDir).length !== 2) {
    throw new Error("Erhua morning brief enable did not create a backup");
  }

  const enabledSha = sha256(fs.readFileSync(cronFile));
  result = run("--enable", {
    QINTOPIA_ERHUA_MORNING_BRIEF_HERMES_CRON: approval,
  });
  if (result.status !== 0 || !result.stdout.includes('"backup_created":false')) {
    throw new Error("Erhua morning brief enable is not idempotent");
  }
  if (
    sha256(fs.readFileSync(cronFile)) !== enabledSha ||
    listBackups(cronDir).length !== 2
  ) {
    throw new Error("idempotent enable mutated cron state");
  }

  const driftJob = cron.jobs.find((job) => job.name === "二花·每日早报");
  driftJob.schedule = { kind: "cron", expr: "10 9 * * *", display: "10 9 * * *" };
  fs.writeFileSync(cronFile, JSON.stringify(cron, null, 2), "utf8");
  result = run("--install", {
    QINTOPIA_ERHUA_MORNING_BRIEF_HERMES_CRON: approval,
  });
  if (result.status === 0 || !result.stderr.includes("definition drifts")) {
    throw new Error("Erhua morning brief apply accepted a drifted declaration");
  }

  driftJob.schedule = { kind: "cron", expr: "10 8 * * *", display: "10 8 * * *" };
  driftJob.origin.chat_name = "unreviewed-chat-name";
  fs.writeFileSync(cronFile, JSON.stringify(cron, null, 2), "utf8");
  result = run("--enable", {
    QINTOPIA_ERHUA_MORNING_BRIEF_HERMES_CRON: approval,
  });
  if (result.status === 0 || !result.stderr.includes("definition drifts")) {
    throw new Error("Erhua morning brief apply accepted drifted origin routing fields");
  }

  driftJob.origin.chat_name = null;
  driftJob.deliver = "none";
  fs.writeFileSync(cronFile, JSON.stringify(cron, null, 2), "utf8");
  result = run("--enable", {
    QINTOPIA_ERHUA_MORNING_BRIEF_HERMES_CRON: approval,
  });
  if (result.status === 0 || !result.stderr.includes("definition drifts")) {
    throw new Error("Erhua morning brief apply accepted drifted deliver mode");
  }

  driftJob.deliver = "origin";
  driftJob.origin.platform = "feishu";
  fs.writeFileSync(cronFile, JSON.stringify(cron, null, 2), "utf8");
  result = run("--enable", {
    QINTOPIA_ERHUA_MORNING_BRIEF_HERMES_CRON: approval,
  });
  if (result.status === 0 || !result.stderr.includes("definition drifts")) {
    throw new Error("Erhua morning brief apply accepted drifted origin platform");
  }

  driftJob.origin.platform = "wecom";
  driftJob.origin.chat_id = "unreviewed-chat-id";
  fs.writeFileSync(cronFile, JSON.stringify(cron, null, 2), "utf8");
  result = run("--enable", {
    QINTOPIA_ERHUA_MORNING_BRIEF_HERMES_CRON: approval,
  });
  if (result.status === 0 || !result.stderr.includes("definition drifts")) {
    throw new Error("Erhua morning brief apply accepted drifted origin chat id");
  }

  const staleSha = "9".repeat(40);
  const sidecarEnvFile = path.join(tmpRoot, "message-sidecar.env");
  fs.writeFileSync(
    sidecarEnvFile,
    `QINTOPIA_DEPLOYED_COMMIT_SHA=${staleSha}\nPATH=/tmp/unreviewed\n`,
    "utf8"
  );
  fs.chmodSync(sidecarEnvFile, 0o600);
  const stateRoot = path.join(tmpRoot, "state");
  const fakeWorker = path.join(
    releaseDir,
    "deploy",
    "sidecar",
    "scripts",
    "erhua-morning-brief-worker.sh"
  );
  writeExecutable(
    fakeWorker,
    '#!/usr/bin/env bash\nprintf "worker_sha=%s\\n" "${QINTOPIA_DEPLOYED_COMMIT_SHA:-unset}"\nprintf "worker_path=%s\\n" "${PATH}"\n'
  );
  const wrapperFixture = fs
    .readFileSync(
      path.join(
        repoRoot,
        "runtime",
        "hermes",
        "scripts",
        "qintopia_erhua_morning_brief.sh"
      ),
      "utf8"
    )
    .replaceAll(fixedReleaseDir, releaseCurrent)
    .replaceAll("/etc/qintopia/message-sidecar.env", sidecarEnvFile)
    .replaceAll("/home/ubuntu/.local/state/qintopia-agentos/", `${stateRoot}/`);
  if (
    !fs
      .readFileSync(
        path.join(
          repoRoot,
          "runtime",
          "hermes",
          "scripts",
          "qintopia_erhua_morning_brief.sh"
        ),
        "utf8"
      )
      .includes(
        'WORKER="${release_dir}/deploy/sidecar/scripts/erhua-morning-brief-worker.sh"'
      )
  ) {
    throw new Error(
      "morning brief wrapper must execute the worker from the resolved release dir"
    );
  }
  const wrapperPath = path.join(scriptsDir, "qintopia_erhua_morning_brief.sh");
  writeExecutable(wrapperPath, wrapperFixture);
  const wrapperRun = spawnSync("bash", [wrapperPath], {
    cwd: repoRoot,
    env: { PATH: "/usr/bin:/bin:/usr/sbin:/sbin" },
    encoding: "utf8",
  });
  if (wrapperRun.status !== 0) {
    throw new Error(
      `morning brief wrapper failed\n${wrapperRun.stdout}\n${wrapperRun.stderr}`
    );
  }
  const wrapperLog = fs.readFileSync(
    path.join(stateRoot, "erhua-morning-brief", "hermes-cron.log"),
    "utf8"
  );
  if (!wrapperLog.includes(`worker_sha=${releaseSha}`)) {
    throw new Error(
      "morning brief wrapper did not bind the release SHA for the worker"
    );
  }
  if (wrapperLog.includes(`worker_sha=${staleSha}`)) {
    throw new Error(
      "morning brief wrapper let a stale persistent env value override the release SHA"
    );
  }
  if (!wrapperLog.includes("worker_path=/usr/bin:/bin:/usr/sbin:/sbin")) {
    throw new Error(
      "morning brief wrapper let persistent PATH override the fixed PATH"
    );
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Erhua morning brief Hermes cron apply test passed.");
