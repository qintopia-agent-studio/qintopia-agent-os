#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { validatePrBody } from "./pr-body.mjs";
import { run } from "./run-command.mjs";

const argValue = (name) => {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] : "";
};

const branch = run("git", ["rev-parse", "--abbrev-ref", "HEAD"]);
if (branch === "master") {
  console.error("Refusing to create a PR from master.");
  process.exit(1);
}

const title =
  argValue("--title") ||
  process.env.PR_TITLE ||
  run("git", ["log", "-1", "--format=%s"]);
const bodyFile = argValue("--body-file") || process.env.PR_BODY_FILE;

if (!bodyFile) {
  console.error("Provide a completed PR body file with --body-file or PR_BODY_FILE.");
  console.error("Start from .github/PULL_REQUEST_TEMPLATE.md, fill it, then retry.");
  process.exit(1);
}

if (!fs.existsSync(bodyFile)) {
  console.error(`PR body file does not exist: ${bodyFile}`);
  process.exit(1);
}

const body = fs.readFileSync(bodyFile, "utf8");
const errors = validatePrBody(body);
if (errors.length > 0) {
  console.error("PR body is incomplete:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

try {
  run("gh", ["auth", "status"], { stdio: ["ignore", "pipe", "pipe"] });
} catch {
  console.error("GitHub CLI is missing or not authenticated. Run pnpm pr:doctor.");
  process.exit(1);
}

try {
  run("git", ["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"]);
} catch {
  run("git", ["push", "-u", "origin", "HEAD"], { stdio: "inherit" });
}

const bodyPath = path.resolve(bodyFile);
const url = run("gh", [
  "pr",
  "create",
  "--base",
  "master",
  "--head",
  branch,
  "--title",
  title,
  "--body-file",
  bodyPath,
]);

fs.writeFileSync(path.join(os.tmpdir(), "qintopia-last-pr-url.txt"), `${url}\n`);
console.log(url);
