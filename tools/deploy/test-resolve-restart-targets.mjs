#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import process from "node:process";

const runResolver = (files, extraArgs = []) =>
  spawnSync(
    "node",
    [
      "tools/deploy/resolve-restart-targets.mjs",
      "--base-ref",
      "base",
      "--head-ref",
      "head",
      ...extraArgs,
    ],
    {
      cwd: process.cwd(),
      env: {
        ...process.env,
        RESTART_TARGET_CHANGED_FILES: files.join("\n"),
      },
      encoding: "utf8",
    }
  );

const assertSuccess = (name, files, expectedTargets, extraArgs = []) => {
  const result = runResolver(files, extraArgs);
  if (result.status !== 0) {
    throw new Error(
      `${name}: expected success, got ${result.status}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`
    );
  }
  const actual = result.stdout.trim();
  const expected = expectedTargets.join(",");
  if (actual !== expected) {
    throw new Error(`${name}: expected ${expected}, got ${actual}`);
  }
};

const assertFailure = (name, files) => {
  const result = runResolver(files);
  if (result.status === 0) {
    throw new Error(`${name}: expected failure, got success ${result.stdout}`);
  }
  if (!result.stderr.includes("unmatched production-adjacent")) {
    throw new Error(`${name}: failure did not mention unmatched files`);
  }
};

assertSuccess("docs-only", ["docs/operations/production-deploy-runner.md"], []);
assertSuccess("nested-readme", ["deploy/runner/README.md"], []);
assertSuccess("ci-metadata", [".github/workflows/ci.yml", "package.json"], []);
assertSuccess(
  "xiaoman-observation-profile-bundle",
  [
    "agents/xiaoman/agent.yaml",
    "agents/xiaoman/profile.template.yaml",
    "agents/xiaoman/profile-bundle/bundle.json",
    "agents/xiaoman/profile-bundle/templates/SOUL.md.template",
    "runtime/hermes/manifest.yaml",
  ],
  []
);
assertSuccess(
  "erhua-only",
  ["skills/qintopia-tools/variants/erhua/__init__.py"],
  ["hermes-erhua"]
);
assertSuccess(
  "sidecar-only",
  ["runtime/sidecar/src/context_tools.rs"],
  ["hermes-erhua", "qintopia-system-services"]
);
assertSuccess(
  "postgres-context-contract",
  ["skills/postgres-context/fixtures/answer-context-prepare.json"],
  ["hermes-erhua", "qintopia-system-services"]
);
assertSuccess(
  "erhua-and-sidecar",
  ["agents/erhua/agent.yaml", "runtime/sidecar/src/context_tools.rs"],
  ["hermes-erhua", "qintopia-system-services"]
);
assertSuccess(
  "override",
  ["agents/erhua/agent.yaml"],
  ["hermes-wenyuange"],
  ["--override", "hermes-wenyuange"]
);
assertFailure("unknown-agent", ["agents/newagent/agent.yaml"]);
assertFailure("unknown-skill", ["skills/new-skill/manifest.yaml"]);

console.log("Restart target resolver tests passed.");
