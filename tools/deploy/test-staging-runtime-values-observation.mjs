#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const script = path.join(
  repoRoot,
  "deploy/sidecar/scripts/staging-runtime-values-observation-smoke.sh"
);
const tmpRoot = fs.mkdtempSync(path.join(repoRoot, ".tmp-staging-runtime-values-"));
const valuesFile = path.join(tmpRoot, "message-sidecar-staging-values.json");
const envFile = path.join(tmpRoot, "message-sidecar-staging.env");
const renderer = path.join(tmpRoot, "render-staging-runtime-env.py");
const secretValue = "staging-values-secret-must-not-appear";

const runObservation = (extraEnv = {}) =>
  spawnSync("bash", [script], {
    cwd: repoRoot,
    env: {
      ...process.env,
      QINTOPIA_STAGING_RUNTIME_VALUES_OBSERVATION_ENABLE: "1",
      QINTOPIA_STAGING_RUNTIME_VALUES_OBSERVATION_TEST_MODE: "1",
      QINTOPIA_STAGING_RUNTIME_VALUES_OBSERVATION_VALUES_FILE: valuesFile,
      QINTOPIA_STAGING_RUNTIME_VALUES_OBSERVATION_ENV_FILE: envFile,
      QINTOPIA_STAGING_RUNTIME_VALUES_OBSERVATION_RENDERER: renderer,
      ...extraEnv,
    },
    encoding: "utf8",
  });

const parseReport = (result) => {
  const line = result.stdout
    .split(/\r?\n/)
    .find((entry) => entry.startsWith("staging_runtime_values_observation="));
  if (!line) {
    throw new Error(
      `missing observation report\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`
    );
  }
  return JSON.parse(line.slice("staging_runtime_values_observation=".length));
};

try {
  let result = runObservation();
  if (result.status !== 0) {
    throw new Error(`missing observation should not fail\nstderr:\n${result.stderr}`);
  }
  let report = parseReport(result);
  if (
    report.success !== true ||
    report.ready_for_render_validation !== false ||
    report.action_status !== "not_ready" ||
    report.values_file_present !== false ||
    report.renderer_present !== false ||
    !report.limitations.includes("values_file_path_missing") ||
    !report.limitations.includes("renderer_path_missing")
  ) {
    throw new Error(`missing report is invalid: ${JSON.stringify(report)}`);
  }

  result = runObservation({
    QINTOPIA_STAGING_RUNTIME_VALUES_OBSERVATION_VALUES_FILE: path.join(
      tmpRoot,
      "missing-staging-parent",
      "message-sidecar-staging-values.json"
    ),
    QINTOPIA_STAGING_RUNTIME_VALUES_OBSERVATION_ENV_FILE: path.join(
      tmpRoot,
      "missing-staging-parent",
      "message-sidecar-staging.env"
    ),
    QINTOPIA_STAGING_RUNTIME_VALUES_OBSERVATION_RENDERER: path.join(
      tmpRoot,
      "missing-staging-parent",
      "render-staging-runtime-env.py"
    ),
  });
  report = parseReport(result);
  if (
    result.status !== 0 ||
    report.values_file_present !== false ||
    report.env_file_present !== false ||
    report.renderer_present !== false ||
    !report.limitations.includes("values_file_path_parent_missing") ||
    !report.limitations.includes("env_file_path_parent_missing") ||
    !report.limitations.includes("renderer_path_parent_missing")
  ) {
    throw new Error(`missing parent report is invalid: ${JSON.stringify(report)}`);
  }

  fs.writeFileSync(valuesFile, `{"secret":"${secretValue}"}`, {
    encoding: "utf8",
    mode: 0o600,
  });
  fs.writeFileSync(renderer, "#!/usr/bin/env python3\nraise SystemExit(99)\n", {
    encoding: "utf8",
    mode: 0o500,
  });

  result = runObservation();
  if (result.status !== 0) {
    throw new Error(`ready observation failed\nstderr:\n${result.stderr}`);
  }
  report = parseReport(result);
  if (
    report.ready_for_render_validation !== true ||
    report.action_status !== "ready_for_render_validation" ||
    report.values_file_secure !== true ||
    report.renderer_executable !== true ||
    report.env_file_present !== false ||
    `${result.stdout}\n${result.stderr}`.includes(secretValue)
  ) {
    throw new Error(`ready report is invalid: ${JSON.stringify(report)}`);
  }

  fs.writeFileSync(envFile, `QINTOPIA_SIDECAR_DATABASE_URL=${secretValue}\n`, {
    encoding: "utf8",
    mode: 0o600,
  });
  result = runObservation();
  report = parseReport(result);
  if (
    result.status !== 0 ||
    report.action_status !== "rendered_env_already_present" ||
    report.ready_for_render_validation !== false ||
    report.env_file_present !== true ||
    !report.limitations.includes("env_file_already_present") ||
    `${result.stdout}\n${result.stderr}`.includes(secretValue)
  ) {
    throw new Error(`existing env report is invalid: ${JSON.stringify(report)}`);
  }

  fs.rmSync(envFile);
  fs.chmodSync(valuesFile, 0o644);
  result = runObservation();
  report = parseReport(result);
  if (
    result.status !== 0 ||
    report.values_file_secure !== false ||
    !report.limitations.includes("values_file_path_group_or_world_readable")
  ) {
    throw new Error(`readable values report is invalid: ${JSON.stringify(report)}`);
  }

  fs.chmodSync(valuesFile, 0o666);
  result = runObservation();
  report = parseReport(result);
  if (
    result.status !== 0 ||
    report.values_file_secure !== false ||
    !report.limitations.includes("values_file_path_group_or_world_writable")
  ) {
    throw new Error(`writable values report is invalid: ${JSON.stringify(report)}`);
  }
  fs.chmodSync(valuesFile, 0o600);

  fs.writeFileSync(envFile, `QINTOPIA_SIDECAR_DATABASE_URL=${secretValue}\n`, {
    encoding: "utf8",
    mode: 0o644,
  });
  result = runObservation();
  report = parseReport(result);
  if (
    result.status !== 0 ||
    report.env_file_secure !== false ||
    !report.limitations.includes("env_file_path_group_or_world_readable")
  ) {
    throw new Error(`readable env report is invalid: ${JSON.stringify(report)}`);
  }
  fs.rmSync(envFile);

  const linkedParent = path.join(tmpRoot, "linked-staging-parent");
  fs.symlinkSync(tmpRoot, linkedParent, "dir");
  result = runObservation({
    QINTOPIA_STAGING_RUNTIME_VALUES_OBSERVATION_VALUES_FILE: path.join(
      linkedParent,
      "message-sidecar-staging-values.json"
    ),
  });
  report = parseReport(result);
  if (
    result.status !== 0 ||
    report.values_file_secure !== false ||
    !report.limitations.includes("values_file_path_parent_is_symlink")
  ) {
    throw new Error(`symlink parent report is invalid: ${JSON.stringify(report)}`);
  }

  console.log("Staging runtime values observation smoke test passed.");
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}
