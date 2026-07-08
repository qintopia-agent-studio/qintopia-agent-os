#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { execFileSync } from "node:child_process";

const repoRoot = process.cwd();

const argValue = (name, fallback = "") => {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] || "" : fallback;
};

const readJsonFile = (filePath) =>
  JSON.parse(fs.readFileSync(path.resolve(repoRoot, filePath), "utf8"));

export const workflowRuns = (runsJson) => {
  if (Array.isArray(runsJson)) {
    return runsJson.flatMap((page) =>
      Array.isArray(page?.workflow_runs) ? page.workflow_runs : [page]
    );
  }
  return runsJson?.workflow_runs ?? [];
};

const runId = (runRecord) => String(runRecord?.id || runRecord?.databaseId || "");

const runTimestamp = (runRecord) =>
  String(
    runRecord?.run_started_at ||
      runRecord?.runStartedAt ||
      runRecord?.created_at ||
      runRecord?.createdAt ||
      runRecord?.updated_at ||
      runRecord?.updatedAt ||
      ""
  );

const sortedWorkflowRuns = (runs) =>
  [...runs].sort((left, right) => {
    const byTime = runTimestamp(left).localeCompare(runTimestamp(right));
    if (byTime !== 0) {
      return byTime;
    }
    return Number(runId(left) || 0) - Number(runId(right) || 0);
  });

const deployResultMetadata = (runRecord) => ({
  id: runId(runRecord),
  created_at: String(runRecord?.created_at || runRecord?.createdAt || ""),
  run_started_at: String(runRecord?.run_started_at || runRecord?.runStartedAt || ""),
  updated_at: String(runRecord?.updated_at || runRecord?.updatedAt || ""),
});

export const attachWorkflowRunMetadata = (results, runRecord) =>
  results.map((result) => ({
    ...result,
    workflow_run: deployResultMetadata(runRecord),
  }));

const isReleaseDeployRun = (runRecord) =>
  runRecord &&
  (!runRecord.event || runRecord.event === "release") &&
  runRecord.status === "completed";

const run = (command, args) =>
  execFileSync(command, args, {
    cwd: repoRoot,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });

const normalizeLogLine = (line) => {
  const fields = line.split("\t");
  const content = fields.length >= 3 ? fields.slice(2).join("\t") : line;
  return content.replace(/^\d{4}-\d{2}-\d{2}T[0-9:.]+Z\s*/, "");
};

export const extractDeployResultsFromLog = (logText) => {
  const results = [];
  let collecting = false;
  let current = [];
  let depth = 0;

  for (const rawLine of String(logText || "").split(/\r?\n/)) {
    const line = normalizeLogLine(rawLine);
    if (!collecting) {
      if (
        line.includes("Deploy result succeeded:") ||
        line.includes("Deploy result failed:")
      ) {
        collecting = true;
        current = [];
        depth = 0;
      }
      continue;
    }

    const start = current.length === 0 ? line.indexOf("{") : 0;
    if (start < 0) {
      continue;
    }
    const jsonLine = line.slice(start);
    current.push(jsonLine);
    for (const char of jsonLine) {
      if (char === "{") {
        depth += 1;
      } else if (char === "}") {
        depth -= 1;
      }
    }
    if (depth === 0 && current.length > 0) {
      try {
        results.push(JSON.parse(current.join("\n")));
      } catch {
        // Ignore malformed snippets; the workflow result gate will still fail if
        // the server did not publish a valid deploy result for the current run.
      }
      collecting = false;
      current = [];
    }
  }

  return results;
};

const main = () => {
  const workflowRunsFile = argValue("--workflow-runs-file");
  const output = argValue("--output");
  const logDir = argValue("--log-dir");

  if (!workflowRunsFile) {
    throw new Error("--workflow-runs-file is required");
  }
  if (!output) {
    throw new Error("--output is required");
  }

  const runs = sortedWorkflowRuns(workflowRuns(readJsonFile(workflowRunsFile)));
  const results = [];
  for (const runRecord of runs) {
    if (!isReleaseDeployRun(runRecord)) {
      continue;
    }
    let logText = "";
    if (logDir) {
      const logPath = path.join(
        path.resolve(repoRoot, logDir),
        `${runId(runRecord)}.log`
      );
      if (!fs.existsSync(logPath)) {
        continue;
      }
      logText = fs.readFileSync(logPath, "utf8");
    } else {
      try {
        logText = run("gh", ["run", "view", runId(runRecord), "--log"]);
      } catch {
        continue;
      }
    }
    results.push(
      ...attachWorkflowRunMetadata(extractDeployResultsFromLog(logText), runRecord)
    );
  }

  fs.mkdirSync(path.dirname(path.resolve(repoRoot, output)), { recursive: true });
  fs.writeFileSync(
    path.resolve(repoRoot, output),
    `${JSON.stringify(results, null, 2)}\n`
  );
};

if (process.argv[1] === new URL(import.meta.url).pathname) {
  try {
    main();
  } catch (error) {
    console.error(error.message);
    process.exit(1);
  }
}
