#!/usr/bin/env node

import fs from "node:fs";
import process from "node:process";
import { validatePrBody } from "./pr-body.mjs";
import { commandExists, run } from "./run-command.mjs";

const errors = [];
const warnings = [];

if (!commandExists("git")) {
  errors.push("git is not installed or not on PATH");
}

if (!commandExists("gh")) {
  errors.push("GitHub CLI is missing; run pnpm pr:bootstrap");
}

let branch = "";
try {
  branch = run("git", ["rev-parse", "--abbrev-ref", "HEAD"]);
  if (branch === "master") {
    errors.push(
      "current branch is master; create a feature branch before opening a PR"
    );
  }
} catch {
  errors.push("not inside a git repository");
}

if (branch && branch !== "master") {
  try {
    run("git", ["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"]);
  } catch {
    warnings.push("current branch has no upstream; push with git push -u origin HEAD");
  }
}

try {
  const porcelain = run("git", ["status", "--porcelain"]);
  if (porcelain) {
    warnings.push("working tree has uncommitted changes");
  }
} catch {
  // Already reported as not in a git repository.
}

const bodyFile = process.argv[2] || process.env.PR_BODY_FILE || "";
if (bodyFile) {
  if (!fs.existsSync(bodyFile)) {
    errors.push(`PR body file does not exist: ${bodyFile}`);
  } else {
    const body = fs.readFileSync(bodyFile, "utf8");
    for (const error of validatePrBody(body)) {
      errors.push(`${bodyFile}: ${error}`);
    }
  }
} else {
  warnings.push("no PR body file provided; pass one to validate before gh pr create");
}

for (const warning of warnings) {
  console.warn(`warning: ${warning}`);
}

if (errors.length > 0) {
  console.error("PR doctor failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log("PR doctor passed.");
