#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { execFileSync } from "node:child_process";
import { validatePrBody } from "./pr-body.mjs";
import { commandExists, run } from "./run-command.mjs";

const argValue = (name) => {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] : "";
};

const branch = run("git", ["rev-parse", "--abbrev-ref", "HEAD"]);
if (branch === "master") {
  console.error("Refusing to create a PR from master.");
  process.exit(1);
}

if (!commandExists("gh")) {
  console.error("GitHub CLI is missing. Run pnpm pr:bootstrap.");
  process.exit(1);
}

const commandErrorOutput = (error) =>
  [error?.stdout, error?.stderr, error?.message, error?.code, error?.signal]
    .filter(Boolean)
    .map((value) => value.toString())
    .join("\n");

const isTransientGitHubApiError = (error) =>
  /error connecting to api\.github\.com|connection reset|connection refused|i\/o timeout|TLS handshake timeout|lookup api\.github\.com|stream error|ETIMEDOUT|SIGTERM/i.test(
    commandErrorOutput(error)
  );

const runGh = (args, options = {}) => {
  const attempts = options.attempts ?? 1;
  for (let attempt = 1; attempt <= attempts; attempt += 1) {
    try {
      return execFileSync("gh", args, {
        encoding: "utf8",
        stdio: ["ignore", "pipe", "pipe"],
        timeout: 15_000,
      }).trim();
    } catch (error) {
      if (attempt === attempts || !isTransientGitHubApiError(error)) {
        throw error;
      }
      console.error(
        `GitHub API connection failed; retrying gh ${args.join(" ")} (${attempt}/${attempts})...`
      );
    }
  }
  return "";
};

const recordPrUrl = (url) => {
  fs.writeFileSync(path.join(os.tmpdir(), "qintopia-last-pr-url.txt"), `${url}\n`);
  console.log(url);
};

try {
  const existingUrl = runGh(["pr", "view", "--json", "url", "--jq", ".url"], {
    attempts: 3,
  });
  if (existingUrl) {
    recordPrUrl(existingUrl);
    process.exit(0);
  }
} catch (error) {
  if (isTransientGitHubApiError(error)) {
    console.error(
      "Could not check for an existing PR because gh could not reach api.github.com."
    );
    console.error(
      "If top-level gh pr commands work, rerun this repository script with network approval instead of re-authenticating gh."
    );
    process.exit(1);
  }
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
  run("git", ["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"]);
} catch {
  run("git", ["push", "-u", "origin", "HEAD"], { stdio: "inherit" });
}

const bodyPath = path.resolve(bodyFile);
const url = runGh(
  [
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
  ],
  { attempts: 3 }
);

recordPrUrl(url);
