#!/usr/bin/env node

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { pathToFileURL } from "node:url";

const repoRoot = process.cwd();
const scriptPath = path.join(
  repoRoot,
  "tools",
  "deploy",
  "finalize-xiaoman-production-completion-evidence.mjs"
);
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "finalize-xiaoman-production-completion-evidence-")
);

try {
  const fixture = createFixtureRepo();
  const result = runFinalizer(fixture.repoRoot, fixture.binRoot, fixture.files);
  assert.equal(result.status, 0, result.stderr);
  assert.match(
    result.stdout,
    /Xiaoman production completion evidence finalized: .*completed-xiaoman-production-completion-evidence\.json/
  );

  const logLines = fs
    .readFileSync(fixture.logFile, "utf8")
    .trim()
    .split("\n")
    .filter(Boolean);
  assert.deepEqual(logLines, ["builder", "checker"]);

  const manifest = JSON.parse(fs.readFileSync(fixture.outputFile, "utf8"));
  assert.equal(manifest.schema, "xiaoman-production-completion-evidence-v1");

  fs.rmSync(fixture.files.qiweStaging);
  const missingEvidence = runFinalizer(
    fixture.repoRoot,
    fixture.binRoot,
    fixture.files
  );
  assert.notEqual(missingEvidence.status, 0);
  assert.match(missingEvidence.stderr, /qiwe-staging file does not exist/);

  fixture.files.qiweStaging = writeFile(
    fixture.repoRoot,
    "qiwe-staging-output.txt",
    "qiwe_image_send_staging_evidence={}\n"
  );

  const missingOutputDir = runFinalizer(fixture.repoRoot, fixture.binRoot, {
    ...fixture.files,
    output: path.join(
      fixture.repoRoot,
      "missing-dir",
      "completed-xiaoman-production-completion-evidence.json"
    ),
  });
  assert.notEqual(missingOutputDir.status, 0);
  assert.match(missingOutputDir.stderr, /output directory does not exist/);
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Xiaoman production completion evidence finalizer test passed.");

function runFinalizer(repoFixtureRoot, binRoot, files) {
  const wrapperPath = path.join(repoFixtureRoot, "run-finalizer.mjs");
  fs.writeFileSync(
    wrapperPath,
    `await import(${JSON.stringify(pathToFileURL(scriptPath).href)});\n`,
    "utf8"
  );

  return spawnSync(
    process.execPath,
    [
      wrapperPath,
      "--release-please-pr-number",
      "258",
      "--release-please-head-sha",
      "0123456789abcdef0123456789abcdef01234567",
      "--release-tag",
      "v0.2.25",
      "--released-commit-sha",
      "89abcdef0123456789abcdef0123456789abcdef",
      "--qiwe-production-enablement-pr-number",
      "233",
      "--qiwe-production-enablement-head-sha",
      "fedcba9876543210fedcba9876543210fedcba98",
      "--staging-runtime-readiness",
      files.stagingRuntimeReadiness,
      "--huabaosi-staging",
      files.huabaosiStaging,
      "--qiwe-staging",
      files.qiweStaging,
      "--huabaosi-production-canary",
      files.huabaosiProductionCanary,
      "--production-real-activity",
      files.productionRealActivity,
      "--qiwe-group-arrival-confirmation",
      files.qiweGroupArrivalConfirmation,
      "--output",
      files.output,
    ],
    {
      cwd: repoFixtureRoot,
      encoding: "utf8",
      env: {
        ...process.env,
        PATH: `${binRoot}${path.delimiter}${process.env.PATH ?? ""}`,
        FAKE_LOG_FILE: files.logFile,
      },
    }
  );
}

function createFixtureRepo() {
  const repoFixtureRoot = path.join(tmpRoot, "repo");
  const binRoot = path.join(tmpRoot, "bin");
  fs.mkdirSync(path.join(repoFixtureRoot, "dist"), { recursive: true });
  fs.mkdirSync(binRoot, { recursive: true });

  const files = {
    stagingRuntimeReadiness: writeFile(
      repoFixtureRoot,
      "staging-runtime-readiness-output.txt",
      "staging_runtime_readiness_evidence={}\n"
    ),
    huabaosiStaging: writeFile(
      repoFixtureRoot,
      "huabaosi-staging-output.txt",
      "huabaosi_image_generation_staging_evidence={}\n"
    ),
    qiweStaging: writeFile(
      repoFixtureRoot,
      "qiwe-staging-output.txt",
      "qiwe_image_send_staging_evidence={}\n"
    ),
    huabaosiProductionCanary: writeFile(
      repoFixtureRoot,
      "huabaosi-production-canary-output.txt",
      "huabaosi_image_generation_production_canary_evidence={}\n"
    ),
    productionRealActivity: writeFile(
      repoFixtureRoot,
      "production-evidence-output.txt",
      "xiaoman_real_activity_production_evidence={}\n"
    ),
    qiweGroupArrivalConfirmation: writeFile(
      repoFixtureRoot,
      "qiwe-group-arrival-confirmation-output.txt",
      "xiaoman_qiwe_group_arrival_confirmation_evidence={}\n"
    ),
    output: path.join(
      repoFixtureRoot,
      "dist",
      "completed-xiaoman-production-completion-evidence.json"
    ),
    logFile: path.join(repoFixtureRoot, "fake-node.log"),
  };

  writeExecutable(
    path.join(binRoot, "node"),
    [
      "#!/usr/bin/env bash",
      "set -euo pipefail",
      'log_file="${FAKE_LOG_FILE:?}"',
      'script="${1:-}"',
      "shift || true",
      'if [[ "$script" == *"build-xiaoman-production-completion-manifest.mjs" ]]; then',
      '  output_path=""',
      "  while [[ $# -gt 0 ]]; do",
      '    if [[ "$1" == "--output" ]]; then',
      '      output_path="${2:-}"',
      "      break",
      "    fi",
      "    shift",
      "  done",
      '  if [[ -z "$output_path" ]]; then',
      "    exit 11",
      "  fi",
      "  printf 'builder\\n' >> \"$log_file\"",
      '  mkdir -p "$(dirname "$output_path")"',
      '  printf \'%s\\n\' \'{"schema":"xiaoman-production-completion-evidence-v1"}\' > "$output_path"',
      "  exit 0",
      "fi",
      'if [[ "$script" == *"check-xiaoman-production-completion-evidence.mjs" ]]; then',
      '  manifest_path=""',
      "  while [[ $# -gt 0 ]]; do",
      '    if [[ "$1" == "--manifest" ]]; then',
      '      manifest_path="${2:-}"',
      "      break",
      "    fi",
      "    shift",
      "  done",
      '  if [[ -z "$manifest_path" || ! -f "$manifest_path" ]]; then',
      "    exit 12",
      "  fi",
      "  printf 'checker\\n' >> \"$log_file\"",
      "  exit 0",
      "fi",
      "printf 'unexpected node invocation: %s\\n' \"$script\" >&2",
      "exit 13",
      "",
    ].join("\n")
  );

  return {
    repoRoot: repoFixtureRoot,
    binRoot,
    files,
    logFile: files.logFile,
    outputFile: files.output,
  };
}

function writeFile(root, name, contents) {
  const filePath = path.join(root, name);
  fs.writeFileSync(filePath, contents, "utf8");
  return filePath;
}

function writeExecutable(filePath, contents) {
  fs.writeFileSync(filePath, contents, "utf8");
  fs.chmodSync(filePath, 0o755);
}
