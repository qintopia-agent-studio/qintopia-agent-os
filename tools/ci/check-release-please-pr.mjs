#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const repoRoot = process.cwd();
const errors = [];

const readText = (relativePath) =>
  fs.readFileSync(path.join(repoRoot, relativePath), "utf8");

const exists = (relativePath) => fs.existsSync(path.join(repoRoot, relativePath));

const addError = (message) => {
  errors.push(message);
};

const requireReleasePleasePullRequest = () => {
  const eventPath = process.env.GITHUB_EVENT_PATH ?? "";
  if (process.env.GITHUB_EVENT_NAME !== "pull_request") {
    addError("release-please:check must run from a pull_request event.");
    return null;
  }
  if (!eventPath || !fs.existsSync(eventPath)) {
    addError("GITHUB_EVENT_PATH is missing; cannot inspect pull_request metadata.");
    return null;
  }

  const event = JSON.parse(fs.readFileSync(eventPath, "utf8"));
  const pullRequest = event.pull_request ?? {};
  const headRef = pullRequest.head?.ref ?? "";
  const author = pullRequest.user?.login ?? "";
  const body = pullRequest.body ?? "";
  const releasePleaseAuthors = new Set(["github-actions[bot]", "app/github-actions"]);
  if (
    !headRef.startsWith("release-please--branches--") ||
    !releasePleaseAuthors.has(author) ||
    !body.includes("This PR was generated with [Release Please]")
  ) {
    addError("pull_request is not a Release Please generated release PR.");
  }

  return pullRequest;
};

const validateManifest = () => {
  const manifestPath = ".release-please-manifest.json";
  if (!exists(manifestPath)) {
    addError(`${manifestPath}: missing Release Please manifest`);
    return;
  }

  let manifest;
  try {
    manifest = JSON.parse(readText(manifestPath));
  } catch (error) {
    addError(`${manifestPath}: must be valid JSON: ${error.message}`);
    return;
  }

  const version = manifest["."];
  if (
    typeof version !== "string" ||
    !/^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(version)
  ) {
    addError(`${manifestPath}: root package version must be a SemVer string`);
  }
};

const validateChangelog = () => {
  const changelogPath = "CHANGELOG.md";
  if (!exists(changelogPath)) {
    addError(`${changelogPath}: missing root changelog`);
    return;
  }

  const changelog = readText(changelogPath);
  if (!changelog.startsWith("# Changelog\n")) {
    addError(`${changelogPath}: must start with '# Changelog'`);
  }
  if (!changelog.includes("## [Unreleased]")) {
    addError(`${changelogPath}: must keep an Unreleased section`);
  }
  if (!/^## \[\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?\]/m.test(changelog)) {
    addError(`${changelogPath}: must include a released version section`);
  }
};

requireReleasePleasePullRequest();
validateManifest();
validateChangelog();

if (errors.length > 0) {
  console.error("Release Please PR check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log("Release Please PR check passed.");
