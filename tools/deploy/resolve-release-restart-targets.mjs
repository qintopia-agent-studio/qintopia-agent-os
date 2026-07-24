#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { execFileSync } from "node:child_process";
import { pathToFileURL } from "node:url";
import {
  readRules,
  resolveTargets,
  writeSummary as writeRestartSummary,
} from "./resolve-restart-targets.mjs";

const repoRoot = process.cwd();

const run = (command, args) =>
  (
    execFileSync(command, args, {
      cwd: repoRoot,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    }) ?? ""
  ).trim();

const argValue = (name, fallback = "") => {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] || "" : fallback;
};

const readJsonFile = (filePath, fallback) => {
  if (!filePath) {
    return fallback;
  }
  return JSON.parse(fs.readFileSync(path.resolve(repoRoot, filePath), "utf8"));
};

const releaseTag = (release) => String(release?.tag_name || release?.tagName || "");

const isPublishedProductionRelease = (release) =>
  release &&
  release.draft !== true &&
  release.isDraft !== true &&
  release.prerelease !== true &&
  release.isPrerelease !== true &&
  /^v[0-9]/.test(releaseTag(release));

const deployResults = (resultsJson) =>
  Array.isArray(resultsJson) ? resultsJson : (resultsJson?.results ?? []);

const approvedRuntimeArtifactProfiles = new Set([
  "huabaosi-production",
  "qiwe-production",
]);

const hasTrustedDeployIdentity = (result) => {
  if (!result || typeof result !== "object") {
    return false;
  }
  const requiredShas = [
    result.release_sha,
    result.commit_sha,
    result.runtime_sha,
    result.deploy_bundle_sha,
  ];
  if (requiredShas.some((value) => !/^[0-9a-f]{40}$/.test(String(value || "")))) {
    return false;
  }
  if (
    !approvedRuntimeArtifactProfiles.has(String(result.runtime_artifact_profile || ""))
  ) {
    return false;
  }
  if (!Array.isArray(result.release_scope) || result.release_scope.length === 0) {
    return false;
  }
  if (!Array.isArray(result.restart_targets) || result.restart_targets.length === 0) {
    return false;
  }
  return true;
};

const commitForTag = (tag) => run("git", ["rev-list", "-n", "1", `${tag}^{commit}`]);

const isAncestor = (candidateCommit, headCommit) => {
  try {
    run("git", ["merge-base", "--is-ancestor", candidateCommit, headCommit]);
    return true;
  } catch {
    return false;
  }
};

const tagByCommit = (releases) => {
  const tags = new Map();
  for (const release of releases.filter(isPublishedProductionRelease)) {
    const tag = releaseTag(release);
    try {
      tags.set(commitForTag(tag), tag);
    } catch {
      // Ignore published Releases whose tag is not available in the checkout.
    }
  }
  return tags;
};

const latestPreviousPublishedReleaseTag = ({ currentTag, releases, currentCommit }) => {
  for (const release of releases) {
    const tag = releaseTag(release);
    if (tag === currentTag || !isPublishedProductionRelease(release)) {
      continue;
    }
    let tagCommit = "";
    try {
      tagCommit = commitForTag(tag);
    } catch {
      continue;
    }
    if (isAncestor(tagCommit, currentCommit)) {
      return tag;
    }
  }
  return "";
};

const changedFiles = ({ baseRef, headRef }) => {
  const output = run("git", ["diff", "--name-only", `${baseRef}..${headRef}`]);
  return output ? output.split("\n").filter(Boolean).sort() : [];
};

const initialTargetTags = ({ currentTag, releases, rules }) => {
  const currentCommit = commitForTag(currentTag);
  const previous = latestPreviousPublishedReleaseTag({
    currentTag,
    releases,
    currentCommit,
  });
  if (!previous) {
    throw new Error(
      `Could not resolve previous published Release tag for ${currentTag}`
    );
  }
  return new Map(rules.allowed_targets.map((target) => [target, previous]));
};

const applyDeployResult = ({ result, targetTags, commitToTag }) => {
  const status = String(result?.status || "");
  if (status !== "succeeded" || !hasTrustedDeployIdentity(result)) {
    return;
  }
  const releaseTagForResult = commitToTag.get(String(result?.release_sha || ""));
  if (!releaseTagForResult) {
    return;
  }
  for (const target of result?.restart_targets ?? []) {
    if (targetTags.has(target)) {
      targetTags.set(target, releaseTagForResult);
    }
  }
};

const resultOrderKey = (result) =>
  String(
    result?.finished_at ||
      result?.started_at ||
      result?.workflow_run?.run_started_at ||
      result?.workflow_run?.created_at ||
      result?.workflow_run?.updated_at ||
      ""
  );

const resultOrderId = (result) => Number(result?.workflow_run?.id || 0);

const sortedDeployResults = (results) =>
  [...results].sort((left, right) => {
    const byTime = resultOrderKey(left).localeCompare(resultOrderKey(right));
    if (byTime !== 0) {
      return byTime;
    }
    return resultOrderId(left) - resultOrderId(right);
  });

const resolveReleaseRestartTargets = ({ currentTag, releases, results, rules }) => {
  const commitToTag = tagByCommit(releases);
  const currentCommit = commitForTag(currentTag);
  const successfulResults = sortedDeployResults(results).filter((result) => {
    if (
      String(result?.status || "") !== "succeeded" ||
      !hasTrustedDeployIdentity(result)
    ) {
      return false;
    }
    const releaseSha = String(result?.release_sha || "");
    return releaseSha && isAncestor(releaseSha, currentCommit);
  });
  if (
    successfulResults.length > 1 &&
    successfulResults.some(
      (result) => !resultOrderKey(result) && !resultOrderId(result)
    )
  ) {
    throw new Error(
      "Multiple successful deploy results require deploy or workflow run timestamps"
    );
  }
  const firstPreviousTag = commitToTag.get(
    String(successfulResults[0]?.previous_sha || "")
  );
  const targetTags = firstPreviousTag
    ? new Map(rules.allowed_targets.map((target) => [target, firstPreviousTag]))
    : initialTargetTags({ currentTag, releases, rules });
  for (const result of successfulResults) {
    applyDeployResult({ result, targetTags, commitToTag });
  }

  const targets = new Set();
  const targetRanges = [];
  const unmatched = [];
  const matched = [];
  const ignored = [];
  for (const target of rules.allowed_targets) {
    const baseTag = targetTags.get(target);
    if (!baseTag || baseTag === currentTag) {
      continue;
    }
    const files = changedFiles({ baseRef: baseTag, headRef: currentTag });
    const resolution = resolveTargets({ changedFiles: files, rules });
    if (resolution.targets.includes(target)) {
      targets.add(target);
      targetRanges.push({ target, baseTag });
    }
    matched.push(...resolution.matched);
    ignored.push(...resolution.ignored);
    unmatched.push(...resolution.unmatched);
  }

  return {
    targets: rules.allowed_targets.filter((target) => targets.has(target)),
    targetRanges,
    matched,
    ignored,
    unmatched,
    override: false,
  };
};

const writeReleaseSummary = ({ outputPath, currentTag, resolution }) => {
  if (!outputPath) {
    return;
  }

  const lines = [
    "## Release Restart Targets",
    "",
    `Current Release tag: ${currentTag}`,
    `Resolved targets: ${resolution.targets.length ? resolution.targets.join(",") : "none"}`,
  ];
  if (resolution.targetRanges.length > 0) {
    lines.push(
      "",
      "Target baselines:",
      ...resolution.targetRanges.map(
        (item) => `- ${item.target}: ${item.baseTag}..${currentTag}`
      )
    );
  }
  const restartSummary = writeRestartSummary({
    mode: "production",
    baseRef: "per-target",
    headRef: currentTag,
    changedFiles: [],
    resolution,
  }).trimEnd();
  lines.push("", restartSummary);

  fs.mkdirSync(path.dirname(path.resolve(repoRoot, outputPath)), {
    recursive: true,
  });
  fs.writeFileSync(path.resolve(repoRoot, outputPath), `${lines.join("\n")}\n`);
};

const main = () => {
  const currentTag = argValue("--current-tag");
  const releasesFile = argValue("--releases-file");
  const deployResultsFile = argValue("--deploy-results-file");
  const rulesPath = argValue("--rules", "deploy/restart-target-rules.yaml");
  const summaryOutput = argValue("--summary-output");

  if (!currentTag) {
    throw new Error("--current-tag is required");
  }
  if (!releasesFile) {
    throw new Error("--releases-file is required");
  }
  if (!deployResultsFile) {
    throw new Error("--deploy-results-file is required");
  }

  const releases = readJsonFile(releasesFile, []);
  const results = deployResults(readJsonFile(deployResultsFile, []));
  const rules = readRules(rulesPath);
  const resolution = resolveReleaseRestartTargets({
    currentTag,
    releases,
    results,
    rules,
  });

  if (resolution.unmatched.length > 0) {
    writeReleaseSummary({ outputPath: summaryOutput, currentTag, resolution });
    console.error(
      "Release restart target resolution failed for unmatched production-adjacent files."
    );
    process.exit(1);
  }

  writeReleaseSummary({ outputPath: summaryOutput, currentTag, resolution });
  process.stdout.write(`${resolution.targets.join(",")}\n`);
};

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    main();
  } catch (error) {
    console.error(error.message);
    process.exit(1);
  }
}

export { resolveReleaseRestartTargets };
