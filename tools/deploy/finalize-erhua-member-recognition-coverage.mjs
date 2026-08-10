#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const args = parseArgs(process.argv.slice(2));
if (!args.coverage || !args.summaryOutput) {
  fail(
    [
      "usage: node tools/deploy/finalize-erhua-member-recognition-coverage.mjs",
      "--coverage <identity-bootstrap-dry-run-output.json>",
      "--summary-output <sanitized-coverage-summary.json>",
      "[--require-active-profiles]",
      "[--expect-pass]",
    ].join(" ")
  );
}

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const summaryOutput = path.resolve(args.summaryOutput);
const summaryOutputDir = path.dirname(summaryOutput);
if (!fs.existsSync(summaryOutputDir)) {
  fail(`summary output directory does not exist: ${summaryOutputDir}`);
}

const coverageCheckerArgs = [
  path.join(scriptDir, "check-erhua-member-recognition-coverage.mjs"),
  args.coverage,
  "--summary-output",
  summaryOutput,
];
if (args.requireActiveProfiles) {
  coverageCheckerArgs.push("--require-active-profiles");
}
const coverageResult = runNode("coverage checker", coverageCheckerArgs);

if (!fs.existsSync(summaryOutput)) {
  fail(`coverage checker did not write summary output: ${summaryOutput}`);
}

const summaryCheckerArgs = [
  path.join(scriptDir, "check-erhua-member-recognition-coverage-summary.mjs"),
  summaryOutput,
];
if (args.expectPass) {
  summaryCheckerArgs.push("--expect-pass");
}
if (args.requireActiveProfiles) {
  summaryCheckerArgs.push("--require-active-profiles");
}
const summaryResult = runNode("coverage summary checker", summaryCheckerArgs);
if (summaryResult.status !== 0) {
  process.exit(summaryResult.status ?? 1);
}
if (coverageResult.status !== 0) {
  process.exit(coverageResult.status ?? 1);
}

console.log(
  `Erhua member recognition coverage finalized: sanitized summary written and verified at ${summaryOutput}.`
);

function runNode(label, commandArgs) {
  const result = spawnSync(process.execPath, commandArgs, {
    cwd: process.cwd(),
    encoding: "utf8",
  });
  if (result.stdout) {
    process.stdout.write(result.stdout);
  }
  if (result.stderr) {
    process.stderr.write(result.stderr);
  }
  if (result.error) {
    fail(`${label} failed to start: ${result.error.message}`);
  }
  return result;
}

function parseArgs(argv) {
  const parsed = {};
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--coverage") {
      parsed.coverage = argv[++index];
    } else if (arg === "--summary-output") {
      parsed.summaryOutput = argv[++index];
    } else if (arg === "--require-active-profiles") {
      parsed.requireActiveProfiles = true;
    } else if (arg === "--expect-pass") {
      parsed.expectPass = true;
    } else {
      fail(`unknown argument: ${arg}`);
    }
  }
  return parsed;
}

function fail(message) {
  console.error(message);
  process.exit(1);
}
