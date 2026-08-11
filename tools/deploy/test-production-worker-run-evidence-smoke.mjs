#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "qintopia-worker-run-evidence-"));
const sourceScript = path.join(
  repoRoot,
  "deploy/sidecar/scripts/production-worker-run-evidence-smoke.sh"
);

const check = (condition, message) => {
  if (!condition) {
    throw new Error(message);
  }
};

const writeFile = (relativePath, content, mode = 0o600) => {
  const filePath = path.join(tmpRoot, relativePath);
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content, "utf8");
  fs.chmodSync(filePath, mode);
  return filePath;
};

const writeSummary = (task, worker, dateField = "date") =>
  writeFile(
    `state/${task}/latest-summary.json`,
    `${JSON.stringify(
      {
        schema_version: 1,
        worker,
        requires_human_confirmation: true,
        external_send_executed: false,
        safe_for_member_chat: false,
        [dateField]: "2026-08-10",
      },
      null,
      2
    )}\n`
  );

const run = (target) =>
  spawnSync("bash", [testScript, target], {
    cwd: repoRoot,
    env: {
      ...process.env,
      QINTOPIA_PRODUCTION_WORKER_RUN_EVIDENCE_ENABLE: "1",
    },
    encoding: "utf8",
  });

const expectStatus = (result, status, label) => {
  check(
    result.status === status,
    `${label} exited ${result.status}, expected ${status}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`
  );
};

const expectNoLeak = (result, label) => {
  const combined = `${result.stdout}\n${result.stderr}`;
  for (const forbidden of [
    "postgres://",
    "secret-token",
    "raw worker output",
    "group-id-fixture",
  ]) {
    check(!combined.includes(forbidden), `${label} leaked ${forbidden}`);
  }
};

const python = spawnSync("which", ["python3"], { encoding: "utf8" }).stdout.trim();
check(Boolean(python), "python3 is required for the worker-run evidence fixture");

const sourceBody = fs.readFileSync(sourceScript, "utf8");
for (const forbidden of [
  "systemctl",
  "timer_name",
  "service_name",
  "ExecMainStartTimestampUSec",
]) {
  check(!sourceBody.includes(forbidden), `script still depends on ${forbidden}`);
}

const testScript = path.join(tmpRoot, "production-worker-run-evidence-smoke.sh");
const testBody = sourceBody
  .replaceAll("/usr/bin/python3", python)
  .replaceAll(
    "/home/ubuntu/.local/state/qintopia-agentos",
    path.join(tmpRoot, "state")
  );
fs.writeFileSync(testScript, testBody, "utf8");
fs.chmodSync(testScript, 0o755);

let result = run("xiaoman-daily-case-report-worker-run");
expectStatus(result, 0, "missing Hermes log");
check(
  result.stdout.trim() === "xiaoman_daily_case_report_worker_run_result=not_started",
  `missing log emitted unexpected evidence\n${result.stdout}`
);

writeFile("state/xiaoman-daily-case-report/hermes-cron.log", "");
result = run("xiaoman-daily-case-report-worker-run");
expectStatus(result, 0, "empty Hermes log");
check(
  result.stdout.trim() === "xiaoman_daily_case_report_worker_run_result=not_started",
  `empty log emitted unexpected evidence\n${result.stdout}`
);

writeFile(
  "state/erhua-morning-brief/hermes-cron.log",
  [
    "raw worker output with postgres://secret@example.invalid/qintopia",
    "2026-08-10T01:30:00Z erhua-morning-brief run=ok",
    "QIWE_TOKEN=secret-token",
    "",
  ].join("\n")
);
result = run("erhua-morning-brief-worker-run");
expectStatus(result, 0, "Erhua success log");
expectNoLeak(result, "Erhua success log");
check(
  result.stdout.includes("erhua_morning_brief_worker_run_result=success") &&
    result.stdout.includes("erhua_morning_brief_worker_run_epoch=1786325400"),
  `Erhua success emitted unexpected evidence\n${result.stdout}`
);

writeFile(
  "state/xiaoman-weekly-preview/hermes-cron.log",
  [
    "2026-08-10T00:00:00Z xiaoman-weekly-preview run=failed exit=7",
    "raw worker output with group-id-fixture",
    "2026-08-10T01:30:00Z xiaoman-weekly-preview run=ok",
    "",
  ].join("\n")
);
writeSummary("xiaoman-weekly-preview", "xiaoman-weekly-preview-worker", "week_start");
result = run("xiaoman-weekly-preview-worker-run");
expectStatus(result, 0, "weekly preview success log");
expectNoLeak(result, "weekly preview success log");
check(
  result.stdout.includes("xiaoman_weekly_preview_worker_run_result=success") &&
    result.stdout.includes("xiaoman_weekly_preview_worker_summary_present=true") &&
    result.stdout.includes("xiaoman_weekly_preview_worker_summary_date=2026-08-10"),
  `weekly preview success emitted unexpected evidence\n${result.stdout}`
);

writeFile(
  "state/xiaoman-weekly-recruitment/hermes-cron.log",
  [
    "2026-08-10T01:30:00Z xiaoman-weekly-recruitment run=ok",
    "2026-08-10T01:40:00Z xiaoman-weekly-recruitment run=failed exit=2",
    "",
  ].join("\n")
);
writeSummary("xiaoman-weekly-recruitment", "xiaoman-weekly-recruitment-worker");
result = run("xiaoman-weekly-recruitment-worker-run");
expectStatus(result, 1, "latest failed sentinel");
expectNoLeak(result, "latest failed sentinel");
check(
  result.stdout.trim() === "xiaoman_weekly_recruitment_worker_run_error=worker_failed",
  `latest failed sentinel emitted unexpected evidence\n${result.stdout}`
);

writeFile(
  "state/xiaoman-weekly-plan-confirmation/hermes-cron.log",
  "2026-08-10T01:30:00Z xiaoman-weekly-plan-confirmation run=ok\n"
);
writeSummary("xiaoman-weekly-plan-confirmation", "unexpected-worker");
result = run("xiaoman-weekly-plan-confirmation-worker-run");
expectStatus(result, 1, "invalid weekly summary");
expectNoLeak(result, "invalid weekly summary");
check(
  result.stdout.trim() ===
    "xiaoman_weekly_plan_confirmation_worker_run_error=summary_invalid",
  `invalid summary emitted unexpected evidence\n${result.stdout}`
);

console.log("production worker-run evidence smoke fixture passed");
