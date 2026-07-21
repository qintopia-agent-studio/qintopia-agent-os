#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { execFileSync } from "node:child_process";
import { pathToFileURL } from "node:url";
import YAML from "yaml";

const repoRoot = process.cwd();

const run = (command, args, options = {}) =>
  (
    execFileSync(command, args, {
      cwd: repoRoot,
      encoding: "utf8",
      stdio: options.stdio ?? ["ignore", "pipe", "pipe"],
    }) ?? ""
  ).trim();

const argValue = (name, fallback = "") => {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] || "" : fallback;
};

const hasArg = (name) => process.argv.includes(name);

const splitCsv = (value) =>
  String(value || "")
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);

const unique = (items) => [...new Set(items)];

const escapeRegex = (value) => value.replace(/[.+^${}()|[\]\\]/g, "\\$&");

const globToRegex = (glob) => {
  let regex = "^";
  for (let index = 0; index < glob.length; index += 1) {
    const char = glob[index];
    const next = glob[index + 1];
    if (char === "*" && next === "*") {
      const after = glob[index + 2];
      if (after === "/") {
        regex += "(?:.*/)?";
        index += 2;
      } else {
        regex += ".*";
        index += 1;
      }
    } else if (char === "*") {
      regex += "[^/]*";
    } else if (char === "?") {
      regex += "[^/]";
    } else {
      regex += escapeRegex(char);
    }
  }
  regex += "$";
  return new RegExp(regex);
};

const matchesAny = (file, patterns) =>
  patterns.some((pattern) => globToRegex(pattern).test(file));

const readRules = (rulesPath) => {
  const absolutePath = path.resolve(repoRoot, rulesPath);
  const rules = YAML.parse(fs.readFileSync(absolutePath, "utf8"));
  if (rules?.schema_version !== 1) {
    throw new Error(`${rulesPath}: schema_version must be 1`);
  }
  if (!Array.isArray(rules.allowed_targets) || rules.allowed_targets.length === 0) {
    throw new Error(`${rulesPath}: allowed_targets must be a non-empty array`);
  }
  if (!Array.isArray(rules.rules)) {
    throw new Error(`${rulesPath}: rules must be an array`);
  }
  return rules;
};

const diffFiles = ({ baseRef, headRef }) => {
  if (process.env.RESTART_TARGET_CHANGED_FILES) {
    return process.env.RESTART_TARGET_CHANGED_FILES.split(/\r?\n/)
      .map((file) => file.trim())
      .filter(Boolean)
      .sort();
  }
  const output = run("git", ["diff", "--name-only", `${baseRef}..${headRef}`]);
  return output ? output.split("\n").filter(Boolean).sort() : [];
};

const latestPreviousReleaseTag = (headTag) => {
  const tagList = run("git", ["tag", "--list", "v[0-9]*", "--sort=-creatordate"])
    .split("\n")
    .map((tag) => tag.trim())
    .filter(Boolean);

  const headCommit = run("git", ["rev-list", "-n", "1", `${headTag}^{commit}`]);
  for (const tag of tagList) {
    if (tag === headTag) {
      continue;
    }
    const tagCommit = run("git", ["rev-list", "-n", "1", `${tag}^{commit}`]);
    try {
      run("git", ["merge-base", "--is-ancestor", tagCommit, headCommit]);
      return tag;
    } catch {
      // Skip tags that are not ancestors of the current release tag.
    }
  }
  return "";
};

const validateTargets = (targets, allowedTargets, label) => {
  const unsupported = targets.filter((target) => !allowedTargets.includes(target));
  if (unsupported.length > 0) {
    throw new Error(
      `${label} contains unsupported target(s): ${unsupported.join(",")}`
    );
  }
  return unique(targets);
};

const resolveTargets = ({ changedFiles, rules, overrideTargets = [] }) => {
  const allowedTargets = rules.allowed_targets;
  if (overrideTargets.length > 0) {
    return {
      targets: validateTargets(overrideTargets, allowedTargets, "override"),
      matched: [],
      unmatched: [],
      ignored: [],
      override: true,
    };
  }

  const matched = [];
  const ignored = [];
  const unmatched = [];
  const targets = new Set();

  for (const file of changedFiles) {
    if (matchesAny(file, rules.no_restart_paths ?? [])) {
      ignored.push(file);
      continue;
    }

    const matchingRules = rules.rules.filter((rule) =>
      matchesAny(file, rule.paths ?? [])
    );
    if (matchingRules.length > 0) {
      for (const rule of matchingRules) {
        targets.add(rule.target);
        matched.push({
          file,
          target: rule.target,
          reason: rule.reason || "",
        });
      }
      continue;
    }

    if (matchesAny(file, rules.production_adjacent_paths ?? [])) {
      unmatched.push(file);
    } else {
      ignored.push(file);
    }
  }

  validateTargets([...targets], allowedTargets, "resolved targets");
  return {
    targets: [...targets],
    matched,
    unmatched,
    ignored,
    override: false,
  };
};

const writeSummary = ({ mode, baseRef, headRef, changedFiles, resolution }) => {
  const lines = [
    "## Restart Impact",
    "",
    `Mode: ${mode}`,
    `Range: ${baseRef}..${headRef}`,
    `Resolved targets: ${resolution.targets.length ? resolution.targets.join(",") : "none"}`,
  ];
  if (resolution.override) {
    lines.push("", "Manual override was used.");
  }
  if (changedFiles.length > 0) {
    lines.push("", "Changed files:", ...changedFiles.map((file) => `- ${file}`));
  }
  if (resolution.matched.length > 0) {
    lines.push(
      "",
      "Matched restart rules:",
      ...resolution.matched.map(
        (item) =>
          `- ${item.target}: ${item.file}${item.reason ? ` (${item.reason})` : ""}`
      )
    );
  }
  if (resolution.ignored.length > 0) {
    lines.push(
      "",
      "No-restart files:",
      ...resolution.ignored.map((file) => `- ${file}`)
    );
  }
  if (resolution.unmatched.length > 0) {
    lines.push(
      "",
      "Unmatched production-adjacent files:",
      ...resolution.unmatched.map((file) => `- ${file}`)
    );
  }
  return `${lines.join("\n")}\n`;
};

const main = () => {
  const mode = argValue("--mode", "preview");
  const rulesPath = argValue("--rules", "deploy/restart-target-rules.yaml");
  const output = argValue("--output", "");
  const summaryOutput = argValue("--summary-output", "");
  const overrideTargets = splitCsv(
    argValue("--override", process.env.RELEASE_DEPLOY_RESTART_TARGETS_OVERRIDE || "")
  );
  const failOnUnmatched = argValue("--fail-on-unmatched", "true") !== "false";
  const rules = readRules(rulesPath);

  let baseRef = argValue("--base-ref", "");
  let headRef = argValue("--head-ref", "");
  const baseTag = argValue("--base-tag", "");
  const headTag = argValue("--head-tag", "");

  if (baseTag || headTag) {
    if (!headTag) {
      throw new Error("--head-tag is required when resolving Release tags");
    }
    baseRef = baseTag || latestPreviousReleaseTag(headTag);
    headRef = headTag;
    if (!baseRef) {
      throw new Error(`Could not resolve previous Release tag for ${headTag}`);
    }
  }

  if (!baseRef || !headRef) {
    throw new Error("Provide --base-ref/--head-ref or --head-tag");
  }

  const changedFiles = diffFiles({ baseRef, headRef });
  const resolution = resolveTargets({ changedFiles, rules, overrideTargets });
  const summary = writeSummary({ mode, baseRef, headRef, changedFiles, resolution });

  if (summaryOutput) {
    fs.mkdirSync(path.dirname(path.resolve(summaryOutput)), { recursive: true });
    fs.writeFileSync(summaryOutput, summary);
  }
  if (process.env.GITHUB_STEP_SUMMARY) {
    fs.appendFileSync(process.env.GITHUB_STEP_SUMMARY, `\n${summary}`);
  }

  const targetCsv = resolution.targets.join(",");
  if (output) {
    fs.mkdirSync(path.dirname(path.resolve(output)), { recursive: true });
    fs.writeFileSync(output, `${targetCsv}\n`);
  }

  if (hasArg("--github-output") && process.env.GITHUB_OUTPUT) {
    const delimiter = `restart_targets_${Date.now()}`;
    fs.appendFileSync(
      process.env.GITHUB_OUTPUT,
      [
        `restart-targets=${targetCsv}`,
        `summary<<${delimiter}`,
        summary.trimEnd(),
        delimiter,
        "",
      ].join("\n")
    );
  }

  if (resolution.unmatched.length > 0 && failOnUnmatched) {
    console.error(summary);
    console.error(
      "Restart target resolution failed for unmatched production-adjacent files."
    );
    process.exit(1);
  }

  if (!output && !hasArg("--github-output")) {
    process.stdout.write(`${targetCsv}\n`);
  }
};

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    main();
  } catch (error) {
    console.error(error.message);
    process.exit(1);
  }
}

export { globToRegex, matchesAny, readRules, resolveTargets, writeSummary };
