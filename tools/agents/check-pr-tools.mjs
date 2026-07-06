#!/usr/bin/env node

import process from "node:process";
import { run } from "./run-command.mjs";
import { validatePrBody } from "./pr-body.mjs";

const errors = [];

const pipeOutput = run("node", ["-e", "process.stdout.write(' ok\\n')"]);
if (pipeOutput !== "ok") {
  errors.push(`run() must trim piped stdout; received ${JSON.stringify(pipeOutput)}`);
}

const inheritOutput = run("node", ["-e", ""], { stdio: "inherit" });
if (inheritOutput !== "") {
  errors.push(
    `run() must return an empty string for inherited stdio; received ${JSON.stringify(
      inheritOutput
    )}`
  );
}

const incompleteBody = [
  "## Summary",
  "",
  "## Planning",
  "",
  "- [ ] Read `AGENTS.md`",
  "",
  "## Domain",
  "",
  "- [ ] tools",
  "",
  "## Validation",
  "",
  "Commands run:",
  "",
  "```text",
  "",
  "```",
  "",
  "## Production Boundary",
  "",
  "- [ ] Does not touch production boundary",
  "",
  "## Architecture / Tooling Boundary",
  "",
  "- [ ] Uses only approved language/tooling families",
  "",
  "## Changelog",
  "",
  "- [ ] Updated `CHANGELOG.md`",
].join("\n");

if (validatePrBody(incompleteBody).length === 0) {
  errors.push("validatePrBody() must reject an empty pull request template");
}

const completeBody = [
  "## Summary",
  "",
  "Standardize pull request creation.",
  "",
  "## Planning",
  "",
  "- [x] Read `AGENTS.md`",
  "",
  "Branch: feature/example",
  "",
  "## Domain",
  "",
  "- [x] tools",
  "",
  "## Validation",
  "",
  "Commands run:",
  "",
  "```text",
  "pnpm tools:ci:check",
  "```",
  "",
  "## Production Boundary",
  "",
  "- [x] Does not touch production boundary",
  "",
  "Notes:",
  "CI-only change.",
  "",
  "## Architecture / Tooling Boundary",
  "",
  "- [x] Uses only approved language/tooling families",
  "",
  "## Changelog",
  "",
  "- [x] Updated `CHANGELOG.md`",
].join("\n");

if (validatePrBody(completeBody).length > 0) {
  errors.push("validatePrBody() must accept a completed pull request body");
}

if (errors.length > 0) {
  console.error("PR tool check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log("PR tool check passed.");
