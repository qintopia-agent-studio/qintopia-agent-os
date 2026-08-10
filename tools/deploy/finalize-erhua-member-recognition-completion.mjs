#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const args = parseArgs(process.argv.slice(2));
if (
  !args.roomSync ||
  !args.profile ||
  !args.coverage ||
  !args.canary ||
  !args.summaryOutput
) {
  fail(
    [
      "usage: node tools/deploy/finalize-erhua-member-recognition-completion.mjs",
      "--room-sync <identity-backfill-room-member-sync-apply-output.json>",
      "--profile <member-profile-quiet-apply-output.json>",
      "--coverage <identity-bootstrap-dry-run-output.json>",
      "--canary <answer-context-canary-output.jsonl>",
      "--summary-output <sanitized-completion-summary.json>",
      "[--require-active-profiles]",
    ].join(" ")
  );
}

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const summaryOutput = path.resolve(args.summaryOutput);
const summaryOutputDir = path.dirname(summaryOutput);
if (!fs.existsSync(summaryOutputDir)) {
  fail(`summary output directory does not exist: ${summaryOutputDir}`);
}

const completionCheckerArgs = [
  path.join(scriptDir, "check-erhua-member-recognition-completion.mjs"),
  "--room-sync",
  args.roomSync,
  "--profile",
  args.profile,
  "--coverage",
  args.coverage,
  "--canary",
  args.canary,
  "--summary-output",
  summaryOutput,
];
if (args.requireActiveProfiles) {
  completionCheckerArgs.push("--require-active-profiles");
}
runNode("completion checker", completionCheckerArgs);

if (!fs.existsSync(summaryOutput)) {
  fail(`completion checker did not write summary output: ${summaryOutput}`);
}

const summaryCheckerArgs = [
  path.join(scriptDir, "check-erhua-member-recognition-completion-summary.mjs"),
  summaryOutput,
];
if (args.requireActiveProfiles) {
  summaryCheckerArgs.push("--require-active-profiles");
}
runNode("completion summary checker", summaryCheckerArgs);

console.log(
  `Erhua member recognition completion finalized: sanitized summary written and verified at ${summaryOutput}.`
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
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

function parseArgs(argv) {
  const parsed = {};
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--room-sync") {
      parsed.roomSync = argv[++index];
    } else if (arg === "--profile") {
      parsed.profile = argv[++index];
    } else if (arg === "--coverage") {
      parsed.coverage = argv[++index];
    } else if (arg === "--canary") {
      parsed.canary = argv[++index];
    } else if (arg === "--summary-output") {
      parsed.summaryOutput = argv[++index];
    } else if (arg === "--require-active-profiles") {
      parsed.requireActiveProfiles = true;
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
