#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import YAML from "yaml";

const repoRoot = process.cwd();
const errors = [];

const addError = (message) => {
  errors.push(message);
};

const exists = (relativePath) => fs.existsSync(path.join(repoRoot, relativePath));

const readText = (relativePath) =>
  fs.readFileSync(path.join(repoRoot, relativePath), "utf8");

const readYaml = (relativePath) => YAML.parse(readText(relativePath));

if (exists("agents/erhua/config.template.yaml")) {
  const overlay = readYaml("agents/erhua/config.template.yaml");
  const expected = {
    profile_overlay_version: 1,
    agent_id: "erhua",
    managed: {
      model: {
        default: "gpt-5.5",
        provider: "custom:livecool.net",
        base_url: "",
      },
      custom_provider: {
        name: "Livecool.net",
        base_url: "https://livecool.net/v1",
        model: "gpt-5.5",
        key_env: "LIVECOOL_API_KEY",
        api_mode: "chat_completions",
      },
    },
  };
  if (JSON.stringify(overlay) !== JSON.stringify(expected)) {
    addError(
      "agents/erhua/config.template.yaml: must match the approved field-limited overlay"
    );
  }
  if (/\bapi_key\s*:/.test(readText("agents/erhua/config.template.yaml"))) {
    addError(
      "agents/erhua/config.template.yaml: must not contain an inline credential"
    );
  }
}

const forbiddenRuntimeMounts = new Set([
  ".env",
  "auth.json",
  "auth.lock",
  "gateway.pid",
  "gateway.lock",
  "gateway_state.json",
  "state.db",
  "state.db-shm",
  "state.db-wal",
  "sessions",
  "logs",
  "cache",
  "memories",
  "pairing",
]);

if (!exists("docs/operations/profile-bundles/m10f-profile-template-plan.md")) {
  addError("docs/operations/profile-bundles/m10f-profile-template-plan.md is required");
}

const agentRegistry = readYaml("registry/agents.yaml");
for (const entry of agentRegistry.entries ?? []) {
  const templatePath = `${entry.path}/profile.template.yaml`;
  if (!exists(templatePath)) {
    continue;
  }

  const template = readYaml(templatePath);
  const requiredFiles = template.runtime_mounts?.required_files ?? [];
  const optionalFiles = template.runtime_mounts?.optional_files ?? [];
  const allMounts = [...requiredFiles, ...optionalFiles].map(String);

  for (const mount of allMounts) {
    const firstPart = mount.split("/")[0];
    if (forbiddenRuntimeMounts.has(mount) || forbiddenRuntimeMounts.has(firstPart)) {
      addError(`${templatePath}: runtime mount must not include live state ${mount}`);
    }
  }

  for (const excluded of forbiddenRuntimeMounts) {
    const excludedState = template.excluded_runtime_state ?? [];
    const hasExclusion = excludedState.some((item) => String(item).includes(excluded));
    if ([".env", "sessions", "logs", "cache", "state.db"].includes(excluded)) {
      if (!hasExclusion) {
        addError(`${templatePath}: excluded_runtime_state should mention ${excluded}`);
      }
    }
  }
}

if (exists("tools/deploy/build-deploy-bundle.mjs")) {
  const deployBundle = readText("tools/deploy/build-deploy-bundle.mjs");
  const forbiddenFragments = [
    "agents/erhua/SOUL.md",
    "agents/erhua/config.yaml",
    "agents/xiaoman/SOUL.md",
    "agents/xiaoman/config.yaml",
    "agents/wenyuange/SOUL.md",
    "agents/wenyuange/config.yaml",
    "agents/huabaosi/SOUL.md",
    "agents/huabaosi/config.yaml",
    "agents/silaoshi/SOUL.md",
    "agents/silaoshi/config.yaml",
    "agents/guanerye/SOUL.md",
    "agents/guanerye/config.yaml",
  ];

  for (const fragment of forbiddenFragments) {
    if (deployBundle.includes(fragment)) {
      addError(
        `tools/deploy/build-deploy-bundle.mjs: unreviewed live profile file must not be packaged (${fragment})`
      );
    }
  }
  for (const required of [
    "agents/erhua/config.template.yaml",
    "runtime/hermes/render_profile_overlay.py",
    "runtime/hermes/migrate_erhua_livecool_env.py",
    "runtime/hermes/profile_transaction.py",
    "runtime/hermes/verify_runtime_provider.py",
  ]) {
    if (!deployBundle.includes(required)) {
      addError(
        `tools/deploy/build-deploy-bundle.mjs: missing reviewed Erhua input ${required}`
      );
    }
  }
}

if (errors.length > 0) {
  console.error("Profile bundle check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log("Profile bundle check passed.");
