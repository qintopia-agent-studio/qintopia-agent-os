#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "qintopia-hermes-cron-live-parity-")
);
const sourceScript = path.join(
  repoRoot,
  "deploy/sidecar/scripts/hermes-cron-live-parity-observation-smoke.sh"
);

const commandPath = (name) => {
  const result = spawnSync("which", [name], { encoding: "utf8" });
  const resolved = result.stdout.trim();
  if (!resolved) {
    throw new Error(`${name} is required for the Hermes cron live parity fixture`);
  }
  return resolved;
};

const writeJson = (filePath, value) => {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`, "utf8");
  fs.chmodSync(filePath, 0o600);
};

const reviewedJobs = [
  [
    "xiaoman",
    "小满·周一活动预告（文字稿+海报简报）",
    "30 9 * * 1",
    "qintopia_xiaoman_weekly_preview.sh",
  ],
  [
    "xiaoman",
    "小满·周六活动招募",
    "0 10 * * 6",
    "qintopia_xiaoman_weekly_recruitment.sh",
  ],
  [
    "xiaoman",
    "小满·周日活动计划确认",
    "0 20 * * 0",
    "qintopia_xiaoman_weekly_plan_confirmation.sh",
  ],
  [
    "xiaoman",
    "小满·每日案例日报",
    "0 8 * * *",
    "qintopia_xiaoman_daily_case_report.sh",
  ],
  ["erhua", "二花·每日早报", "10 8 * * *", "qintopia_erhua_morning_brief.sh"],
];

const liveJob = ([profile, name, expr, script], chatId) => ({
  id: `${profile}-${script}`,
  name,
  schedule: { kind: "cron", expr, display: expr },
  no_agent: true,
  script,
  deliver: "origin",
  origin: {
    platform: "wecom",
    chat_id: chatId,
    chat_name: null,
    thread_id: null,
  },
  enabled: false,
  skills: [],
});

try {
  const releaseCurrent = path.join(tmpRoot, "release", "current");
  const registryFile = path.join(
    releaseCurrent,
    "runtime",
    "hermes",
    "cron",
    "reviewed-cron-jobs.json"
  );
  const xiaomanCronFile = path.join(
    tmpRoot,
    "hermes",
    "profiles",
    "xiaoman",
    "cron",
    "jobs.json"
  );
  const erhuaCronFile = path.join(
    tmpRoot,
    "hermes",
    "profiles",
    "erhua",
    "cron",
    "jobs.json"
  );
  const xiaomanEnvFile = path.join(tmpRoot, "hermes", "profiles", "xiaoman", ".env");
  const erhuaEnvFile = path.join(tmpRoot, "hermes", "profiles", "erhua", ".env");
  const scriptPath = path.join(tmpRoot, "hermes-cron-live-parity-observation-smoke.sh");

  writeJson(registryFile, {
    schema_version: 1,
    reviewed_jobs: reviewedJobs.map(([profile, name, scheduleExpr, script]) => ({
      profile,
      name,
      schedule_expr: scheduleExpr,
      script,
      no_agent: true,
      deliver: "origin",
      origin_platform: "wecom",
    })),
  });
  fs.mkdirSync(path.dirname(xiaomanEnvFile), { recursive: true });
  fs.mkdirSync(path.dirname(erhuaEnvFile), { recursive: true });
  fs.writeFileSync(xiaomanEnvFile, "WECOM_HOME_CHANNEL=xiaoman-secret-chat\n", "utf8");
  fs.writeFileSync(erhuaEnvFile, "WECOM_HOME_CHANNEL=erhua-secret-chat\n", "utf8");
  fs.chmodSync(xiaomanEnvFile, 0o600);
  fs.chmodSync(erhuaEnvFile, 0o600);

  const xiaomanJobs = reviewedJobs
    .filter(([profile]) => profile === "xiaoman")
    .map((entry) => liveJob(entry, "xiaoman-secret-chat"));
  const erhuaJobs = reviewedJobs
    .filter(([profile]) => profile === "erhua")
    .map((entry) => liveJob(entry, "erhua-secret-chat"));
  writeJson(xiaomanCronFile, {
    schema_version: 1,
    jobs: xiaomanJobs,
    padding: "x".repeat(70_000),
  });
  writeJson(erhuaCronFile, { schema_version: 1, jobs: erhuaJobs });

  const source = fs
    .readFileSync(sourceScript, "utf8")
    .replaceAll(
      "/home/ubuntu/qintopia-agent-os-releases/current/runtime/hermes/cron/reviewed-cron-jobs.json",
      registryFile
    )
    .replaceAll("/home/ubuntu/.hermes/profiles/xiaoman/cron/jobs.json", xiaomanCronFile)
    .replaceAll("/home/ubuntu/.hermes/profiles/xiaoman/.env", xiaomanEnvFile)
    .replaceAll("/home/ubuntu/.hermes/profiles/erhua/cron/jobs.json", erhuaCronFile)
    .replaceAll("/home/ubuntu/.hermes/profiles/erhua/.env", erhuaEnvFile)
    .replaceAll("/usr/bin/python3", commandPath("python3"));
  fs.writeFileSync(scriptPath, source, "utf8");
  fs.chmodSync(scriptPath, 0o755);

  let result = spawnSync("bash", [scriptPath], {
    cwd: repoRoot,
    env: {
      ...process.env,
      QINTOPIA_HERMES_CRON_LIVE_PARITY_OBSERVATION_ENABLE: "1",
    },
    encoding: "utf8",
  });
  if (result.status !== 0) {
    throw new Error(
      `expected live parity to accept >64KiB cron envelope\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`
    );
  }
  if (
    !result.stdout.includes("hermes_cron_live_parity_result=success") ||
    !result.stdout.includes("hermes_cron_live_parity_live_count=5") ||
    !result.stdout.includes("hermes_cron_live_parity_enabled_count=0")
  ) {
    throw new Error(`unexpected live parity success output\n${result.stdout}`);
  }

  writeJson(xiaomanCronFile, {
    schema_version: 1,
    jobs: xiaomanJobs,
    padding: "secret-padding".repeat(90_000),
  });
  result = spawnSync("bash", [scriptPath], {
    cwd: repoRoot,
    env: {
      ...process.env,
      QINTOPIA_HERMES_CRON_LIVE_PARITY_OBSERVATION_ENABLE: "1",
    },
    encoding: "utf8",
  });
  if (result.status === 0) {
    throw new Error("expected oversized live cron to fail");
  }
  if (
    !result.stdout.includes("hermes_cron_live_parity_observation_error=cron_too_large")
  ) {
    throw new Error(`unexpected oversized failure output\n${result.stdout}`);
  }
  if (`${result.stdout}\n${result.stderr}`.includes("secret-padding")) {
    throw new Error("oversized cron observation leaked file content");
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}
