#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import YAML from "yaml";

const repoRoot = process.cwd();
const errors = [];

const readText = (relativePath) =>
  fs.readFileSync(path.join(repoRoot, relativePath), "utf8");

const readYaml = (relativePath) => YAML.parse(readText(relativePath));

const exists = (relativePath) => fs.existsSync(path.join(repoRoot, relativePath));

const addError = (message) => {
  errors.push(message);
};

const asArray = (value) => (Array.isArray(value) ? value : []);

const requireNonEmptyArray = (file, template, field) => {
  const value = template[field];
  if (!Array.isArray(value) || value.length === 0) {
    addError(`${file}: ${field} must be a non-empty array`);
  }
};

const requiredAgentIds = new Set([
  "agents/default",
  "agents/erhua",
  "agents/xiaoman",
  "agents/wenyuange",
  "agents/silaoshi",
  "agents/guanerye",
  "agents/huabaosi",
]);

const agentRegistry = readYaml("registry/agents.yaml");
const entries = agentRegistry.entries ?? [];
const entryIds = new Set(entries.map((entry) => entry.id));

for (const requiredId of requiredAgentIds) {
  if (!entryIds.has(requiredId)) {
    addError(`registry/agents.yaml: missing required Agent ${requiredId}`);
  }
}

if (entryIds.has("agents/xiaoqin")) {
  addError("registry/agents.yaml: xiaoqin must not be registered as an active Agent");
}

const activePackageIds = new Set();
for (const registryPath of [
  "registry/agents.yaml",
  "registry/skills.yaml",
  "registry/workflows.yaml",
  "registry/mcp.yaml",
  "registry/runtime.yaml",
  "registry/deploy.yaml",
]) {
  const registry = readYaml(registryPath);
  for (const entry of registry.entries ?? []) {
    activePackageIds.add(entry.id);
  }
}

const deprecatedIds = new Set(
  (readYaml("registry/deprecated.yaml").entries ?? []).map((entry) => entry.id)
);

for (const entry of entries) {
  const agentDir = entry.path;
  const agentId = entry.id.replace(/^agents\//, "");

  for (const requiredFile of [
    "README.md",
    "agent.yaml",
    "profile.template.yaml",
    "capabilities.md",
    "runtime-notes.md",
    "docs/source-snapshot.md",
  ]) {
    const file = `${agentDir}/${requiredFile}`;
    if (!exists(file)) {
      addError(`${agentDir}: missing ${requiredFile}`);
    }
  }

  if (!exists(entry.manifest) || !exists(`${agentDir}/profile.template.yaml`)) {
    continue;
  }

  const manifest = readYaml(entry.manifest);
  const templatePath = `${agentDir}/profile.template.yaml`;
  const template = readYaml(templatePath);

  if (manifest.id !== entry.id) {
    addError(`${entry.manifest}: id must match registry entry ${entry.id}`);
  }

  if (template.agent_id !== agentId) {
    addError(`${templatePath}: agent_id must be ${agentId}`);
  }

  if (template.runtime !== "hermes") {
    addError(`${templatePath}: runtime must be hermes`);
  }

  if (!template.profile_template_version) {
    addError(`${templatePath}: profile_template_version is required`);
  }

  for (const field of [
    "purpose",
    "prompt_sections",
    "allowed_capabilities",
    "forbidden_actions",
    "excluded_runtime_state",
    "dry_run_expectations",
  ]) {
    requireNonEmptyArray(templatePath, template, field);
  }

  if (!template.runtime_mounts || typeof template.runtime_mounts !== "object") {
    addError(`${templatePath}: runtime_mounts object is required`);
  } else if (!Array.isArray(template.runtime_mounts.required_files)) {
    addError(`${templatePath}: runtime_mounts.required_files must be an array`);
  }

  for (const capability of asArray(template.allowed_capabilities)) {
    if (typeof capability !== "string") {
      addError(`${templatePath}: allowed_capabilities entries must be strings`);
      continue;
    }

    if (capability.startsWith("deprecated/")) {
      addError(`${templatePath}: must not allow deprecated capability ${capability}`);
    }

    const packageLike = /^(agents|skills|workflows|mcp|runtime|deploy)\//.test(
      capability
    );
    if (packageLike && !activePackageIds.has(capability)) {
      addError(`${templatePath}: allowed capability ${capability} is not registered`);
    }
  }

  for (const dependency of manifest.dependencies ?? []) {
    if (deprecatedIds.has(dependency)) {
      addError(
        `${entry.manifest}: must not depend on deprecated package ${dependency}`
      );
    }
  }

  if (entry.id === "agents/huabaosi") {
    if (entry.status !== "draft") {
      addError(
        "registry/agents.yaml: agents/huabaosi must remain draft until approved"
      );
    }
    if (manifest.source?.disposition !== "review-pool") {
      addError("agents/huabaosi/agent.yaml: source disposition must be review-pool");
    }
    if (template.status !== "review-pool") {
      addError("agents/huabaosi/profile.template.yaml: status must be review-pool");
    }
  }
}

if (errors.length > 0) {
  console.error("Agent check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log("Agent check passed.");
