#!/usr/bin/env node

import process from "node:process";

const defaultArtifactName = "qintopia-message-sidecar-linux-x86_64-gnu";
const defaultKeepCount = 10;
const apiVersion = "2022-11-28";

const args = process.argv.slice(2);
const options = {
  artifactName: process.env.ARTIFACT_NAME || defaultArtifactName,
  keepCount: Number.parseInt(
    process.env.QINTOPIA_ARTIFACT_KEEP_COUNT || `${defaultKeepCount}`,
    10
  ),
  currentRunId: process.env.GITHUB_RUN_ID || "",
  dryRun: false,
};

for (let index = 0; index < args.length; index += 1) {
  const arg = args[index];
  if (arg === "--") {
    continue;
  } else if (arg === "--artifact-name") {
    options.artifactName = args[index + 1] || "";
    index += 1;
  } else if (arg === "--keep") {
    options.keepCount = Number.parseInt(args[index + 1] || "", 10);
    index += 1;
  } else if (arg === "--current-run-id") {
    options.currentRunId = args[index + 1] || "";
    index += 1;
  } else if (arg === "--dry-run") {
    options.dryRun = true;
  } else if (arg === "-h" || arg === "--help") {
    console.log(`Usage:
  node tools/deploy/prune-github-artifacts.mjs [--artifact-name <name>] [--keep <count>] [--current-run-id <id>] [--dry-run]

Environment:
  GITHUB_TOKEN                 Required. Token with Actions artifact read/delete permission.
  GITHUB_REPOSITORY            Required. owner/repo.
  GITHUB_API_URL               Optional. Defaults to https://api.github.com.
  ARTIFACT_NAME                Optional. Defaults to ${defaultArtifactName}.
  QINTOPIA_ARTIFACT_KEEP_COUNT Optional. Defaults to ${defaultKeepCount}.
  GITHUB_RUN_ID                Optional. Used to wait until the current run artifact is listed.
`);
    process.exit(0);
  } else {
    throw new Error(`Unknown argument: ${arg}`);
  }
}

if (!options.artifactName) {
  throw new Error("artifact name is required");
}

if (!Number.isInteger(options.keepCount) || options.keepCount < 1) {
  throw new Error("--keep must be a positive integer");
}

const token = process.env.GITHUB_TOKEN;
if (!token) {
  throw new Error("GITHUB_TOKEN is required");
}

const repository = process.env.GITHUB_REPOSITORY;
if (!repository || !repository.includes("/")) {
  throw new Error("GITHUB_REPOSITORY must be set to owner/repo");
}

const apiBase = (process.env.GITHUB_API_URL || "https://api.github.com").replace(
  /\/$/,
  ""
);

const headers = {
  Accept: "application/vnd.github+json",
  Authorization: `Bearer ${token}`,
  "X-GitHub-Api-Version": apiVersion,
  "User-Agent": "qintopia-agent-os-artifact-pruner",
};

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const requestJson = async (url, init = {}) => {
  const response = await fetch(url, {
    ...init,
    headers: {
      ...headers,
      ...(init.headers || {}),
    },
  });
  if (!response.ok) {
    const body = await response.text();
    throw new Error(
      `${init.method || "GET"} ${url} failed: ${response.status} ${body}`
    );
  }
  if (response.status === 204) {
    return null;
  }
  return response.json();
};

const listArtifacts = async () => {
  const artifacts = [];
  for (let page = 1; ; page += 1) {
    const url = `${apiBase}/repos/${repository}/actions/artifacts?per_page=100&page=${page}`;
    const payload = await requestJson(url);
    artifacts.push(...(payload.artifacts || []));
    if (!payload.artifacts || payload.artifacts.length < 100) {
      break;
    }
  }
  return artifacts
    .filter((artifact) => artifact.name === options.artifactName)
    .sort((left, right) => {
      const byCreatedAt =
        Date.parse(right.created_at || "") - Date.parse(left.created_at || "");
      if (byCreatedAt !== 0) {
        return byCreatedAt;
      }
      return Number(right.id) - Number(left.id);
    });
};

const listArtifactsWithCurrentRun = async () => {
  let artifacts = [];
  for (let attempt = 1; attempt <= 6; attempt += 1) {
    artifacts = await listArtifacts();
    if (
      !options.currentRunId ||
      artifacts.some(
        (artifact) => `${artifact.workflow_run?.id || ""}` === options.currentRunId
      )
    ) {
      return artifacts;
    }
    console.log(
      `Current run artifact is not listed yet; retrying (${attempt}/6) after upload finalization.`
    );
    await sleep(5000);
  }
  return artifacts;
};

const artifacts = await listArtifactsWithCurrentRun();
const keep = artifacts.slice(0, options.keepCount);
const prune = artifacts.slice(options.keepCount);

console.log(
  `Found ${artifacts.length} artifacts named ${options.artifactName}; keeping ${keep.length}, pruning ${prune.length}.`
);

for (const artifact of keep) {
  console.log(
    `Keep artifact ${artifact.id} run=${artifact.workflow_run?.id || "unknown"} sha=${
      artifact.workflow_run?.head_sha || "unknown"
    } created_at=${artifact.created_at}`
  );
}

for (const artifact of prune) {
  console.log(
    `${options.dryRun ? "Would delete" : "Deleting"} artifact ${artifact.id} run=${
      artifact.workflow_run?.id || "unknown"
    } sha=${artifact.workflow_run?.head_sha || "unknown"} created_at=${
      artifact.created_at
    } size=${artifact.size_in_bytes || 0}`
  );
  if (!options.dryRun) {
    await requestJson(
      `${apiBase}/repos/${repository}/actions/artifacts/${artifact.id}`,
      {
        method: "DELETE",
      }
    );
  }
}
