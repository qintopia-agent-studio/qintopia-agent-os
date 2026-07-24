#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { execFileSync } from "node:child_process";

const repoRoot = process.cwd();

const run = (command, args) =>
  execFileSync(command, args, {
    cwd: repoRoot,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  }).trim();

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

const workflowRuns = (runsJson) =>
  Array.isArray(runsJson) ? runsJson : (runsJson?.workflow_runs ?? []);

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

const runReleaseTagCandidates = (runRecord) =>
  [
    runRecord?.head_branch,
    runRecord?.headBranch,
    runRecord?.display_title,
    runRecord?.displayTitle,
  ]
    .map((value) => String(value || "").trim())
    .filter(Boolean)
    .filter((value, index, values) => values.indexOf(value) === index);

const runHeadSha = (runRecord) =>
  String(runRecord?.head_sha || runRecord?.headSha || "");

const isSuccessfulReleaseDeployRun = (runRecord) =>
  runRecord &&
  runRecord.conclusion === "success" &&
  (!runRecord.event || runRecord.event === "release");

const commitForTag = (tag) => run("git", ["rev-list", "-n", "1", `${tag}^{commit}`]);

const isAncestor = (candidateCommit, headCommit) => {
  try {
    run("git", ["merge-base", "--is-ancestor", candidateCommit, headCommit]);
    return true;
  } catch {
    return false;
  }
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

const latestSuccessfulDeployedReleaseTag = ({
  currentTag,
  releases,
  runs,
  results,
  currentCommit,
}) => {
  const publishedTags = new Set(
    releases.filter(isPublishedProductionRelease).map(releaseTag)
  );
  const succeededReleaseShas = new Set(
    results
      .filter(
        (result) => result?.status === "succeeded" && hasTrustedDeployIdentity(result)
      )
      .map((result) => String(result.release_sha || ""))
      .filter(Boolean)
  );

  for (const runRecord of runs) {
    if (!isSuccessfulReleaseDeployRun(runRecord)) {
      continue;
    }
    for (const tag of runReleaseTagCandidates(runRecord)) {
      if (tag === currentTag || !publishedTags.has(tag)) {
        continue;
      }

      let tagCommit = "";
      try {
        tagCommit = commitForTag(tag);
      } catch {
        continue;
      }
      if (!succeededReleaseShas.has(tagCommit)) {
        continue;
      }
      if (runHeadSha(runRecord) && runHeadSha(runRecord) !== tagCommit) {
        continue;
      }
      if (isAncestor(tagCommit, currentCommit)) {
        return tag;
      }
    }
  }
  return "";
};

const writeSummary = ({
  outputPath,
  currentTag,
  selectedBaseTag,
  previousPublishedReleaseTag,
  successfulDeployedReleaseTag,
}) => {
  if (!outputPath) {
    return;
  }

  const lines = [
    "## Release Deploy Base",
    "",
    `Current Release tag: ${currentTag}`,
    `Previous published Release tag: ${previousPublishedReleaseTag || "none"}`,
    `Latest successful deployed Release tag: ${successfulDeployedReleaseTag || "none"}`,
    `Selected restart diff base: ${selectedBaseTag || "none"}`,
  ];
  if (
    selectedBaseTag &&
    previousPublishedReleaseTag &&
    selectedBaseTag !== previousPublishedReleaseTag
  ) {
    lines.push(
      "",
      "Selected the latest successful deployed Release because a newer published Release was not the live deployed baseline."
    );
  }

  fs.mkdirSync(path.dirname(path.resolve(repoRoot, outputPath)), {
    recursive: true,
  });
  fs.writeFileSync(path.resolve(repoRoot, outputPath), `${lines.join("\n")}\n`);
};

const main = () => {
  const currentTag = argValue("--current-tag");
  const releasesFile = argValue("--releases-file");
  const workflowRunsFile = argValue("--workflow-runs-file");
  const deployResultsFile = argValue("--deploy-results-file");
  const summaryOutput = argValue("--summary-output");

  if (!currentTag) {
    throw new Error("--current-tag is required");
  }
  if (!releasesFile) {
    throw new Error("--releases-file is required");
  }
  if (!workflowRunsFile) {
    throw new Error("--workflow-runs-file is required");
  }
  if (!deployResultsFile) {
    throw new Error("--deploy-results-file is required");
  }

  const releases = readJsonFile(releasesFile, []);
  const runs = workflowRuns(readJsonFile(workflowRunsFile, []));
  const results = deployResults(readJsonFile(deployResultsFile, []));
  const currentCommit = commitForTag(currentTag);
  const previousPublishedReleaseTag = latestPreviousPublishedReleaseTag({
    currentTag,
    releases,
    currentCommit,
  });
  const successfulDeployedReleaseTag = latestSuccessfulDeployedReleaseTag({
    currentTag,
    releases,
    runs,
    results,
    currentCommit,
  });
  const selectedBaseTag = successfulDeployedReleaseTag || previousPublishedReleaseTag;

  if (!selectedBaseTag) {
    throw new Error(`Could not resolve deploy base Release tag for ${currentTag}`);
  }

  writeSummary({
    outputPath: summaryOutput,
    currentTag,
    selectedBaseTag,
    previousPublishedReleaseTag,
    successfulDeployedReleaseTag,
  });
  process.stdout.write(`${selectedBaseTag}\n`);
};

try {
  main();
} catch (error) {
  console.error(error.message);
  process.exit(1);
}
