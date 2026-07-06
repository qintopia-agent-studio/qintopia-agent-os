#!/usr/bin/env node

import fs from "node:fs";
import process from "node:process";
import { validatePrBody } from "../agents/pr-body.mjs";

const eventPath = process.env.GITHUB_EVENT_PATH ?? "";

if (process.env.GITHUB_EVENT_NAME !== "pull_request") {
  console.log("PR body check skipped outside pull_request events.");
  process.exit(0);
}

if (!eventPath || !fs.existsSync(eventPath)) {
  console.error("GITHUB_EVENT_PATH is missing; cannot validate PR body.");
  process.exit(1);
}

const event = JSON.parse(fs.readFileSync(eventPath, "utf8"));
const body = event.pull_request?.body ?? "";
const errors = validatePrBody(body);

if (errors.length > 0) {
  console.error("PR body check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log("PR body check passed.");
