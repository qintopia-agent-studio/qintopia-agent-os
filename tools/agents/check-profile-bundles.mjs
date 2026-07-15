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
const readJson = (relativePath) => JSON.parse(readText(relativePath));

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

const xiaomanBundleRoot = "agents/xiaoman/profile-bundle";
const xiaomanBundleFiles = [
  `${xiaomanBundleRoot}/README.md`,
  `${xiaomanBundleRoot}/bundle.json`,
  `${xiaomanBundleRoot}/render.py`,
  `${xiaomanBundleRoot}/templates/SOUL.md.template`,
  `${xiaomanBundleRoot}/templates/profile.yaml.template`,
  `${xiaomanBundleRoot}/tests/fixtures/values.json`,
  `${xiaomanBundleRoot}/tests/test_render.py`,
];
for (const file of xiaomanBundleFiles) {
  if (!exists(file)) {
    addError(`${file}: required Xiaoman observation bundle file is missing`);
  }
}

for (const file of [
  `${xiaomanBundleRoot}/render.py`,
  "deploy/sidecar/scripts/xiaoman-profile-bundle-observation-smoke.sh",
  "tools/deploy/test-xiaoman-profile-bundle-observation.mjs",
]) {
  if (exists(file) && (fs.statSync(path.join(repoRoot, file)).mode & 0o111) === 0) {
    addError(`${file}: must be executable`);
  }
}

if (exists(`${xiaomanBundleRoot}/bundle.json`)) {
  const bundle = readJson(`${xiaomanBundleRoot}/bundle.json`);
  const inputs = bundle.inputs ?? [];
  const files = bundle.files ?? [];
  const expectedInputs = new Set([
    "QINTOPIA_XIAOMAN_OPERATIONS_OWNER_NAME",
    "QINTOPIA_XIAOMAN_OPERATIONS_OWNER_WECOM_TARGET",
    "QINTOPIA_XIAOMAN_TECHNICAL_OWNER_NAME",
    "QINTOPIA_XIAOMAN_TECHNICAL_HOME_CHANNEL",
  ]);
  const actualInputs = new Set(inputs.map((item) => item.name));
  if (bundle.status !== "observation-only") {
    addError(`${xiaomanBundleRoot}/bundle.json: status must remain observation-only`);
  }
  if (
    actualInputs.size !== expectedInputs.size ||
    [...expectedInputs].some((name) => !actualInputs.has(name))
  ) {
    addError(`${xiaomanBundleRoot}/bundle.json: input allowlist mismatch`);
  }
  if (
    files.length !== 2 ||
    !files.some((item) => item.target === "SOUL.md") ||
    !files.some((item) => item.target === "profile.yaml")
  ) {
    addError(`${xiaomanBundleRoot}/bundle.json: file allowlist mismatch`);
  }
  if (
    bundle.production_boundary?.live_profile_changes !== false ||
    bundle.production_boundary?.external_sends !== false ||
    bundle.production_boundary?.database_writes !== false ||
    bundle.production_boundary?.network_access !== false
  ) {
    addError(`${xiaomanBundleRoot}/bundle.json: production boundary must be read-only`);
  }
}

const xiaomanObservationPath =
  "deploy/sidecar/scripts/xiaoman-profile-bundle-observation-smoke.sh";
if (exists(xiaomanObservationPath)) {
  const observation = readText(xiaomanObservationPath);
  for (const fragment of [
    'default_values_file="/etc/qintopia/xiaoman-profile-bundle-values.json"',
    'observer_uid="$(id -u)"',
    "requires root and a root-owned values file",
    "must be owned by the observing user",
    'cmp -s "${profile_dir}/SOUL.md" "${rendered_dir}/SOUL.md"',
    'cmp -s "${profile_dir}/profile.yaml" "${rendered_dir}/profile.yaml"',
  ]) {
    if (!observation.includes(fragment)) {
      addError(`${xiaomanObservationPath}: missing ${fragment}`);
    }
  }
  for (const fragment of ["systemctl", "curl ", "wget ", "ln -s", "eval "]) {
    if (observation.includes(fragment)) {
      addError(`${xiaomanObservationPath}: must not contain ${fragment}`);
    }
  }
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
  for (const fragment of ["agents/xiaoman/profile-bundle"]) {
    if (!deployBundle.includes(fragment)) {
      addError(`tools/deploy/build-deploy-bundle.mjs: missing ${fragment}`);
    }
  }
  if (!deployBundle.includes(String.raw`profile-bundle\/tests`)) {
    addError(
      "tools/deploy/build-deploy-bundle.mjs: Xiaoman profile bundle tests must be excluded"
    );
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
