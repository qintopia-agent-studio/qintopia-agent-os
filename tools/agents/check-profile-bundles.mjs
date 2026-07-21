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

const erhuaWeatherBroadcastPath =
  "skills/qintopia-weather/scripts/qintopia-erhua-weather-broadcast.py";

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

if (!exists(erhuaWeatherBroadcastPath)) {
  addError(
    `${erhuaWeatherBroadcastPath}: required Erhua weather broadcast asset is missing`
  );
} else {
  const broadcastScript = readText(erhuaWeatherBroadcastPath);
  if (
    (fs.statSync(path.join(repoRoot, erhuaWeatherBroadcastPath)).mode & 0o111) ===
    0
  ) {
    addError(`${erhuaWeatherBroadcastPath}: must be executable`);
  }
  for (const fragment of [
    'WEATHER_ARGUMENTS = {"intent": "general", "hours": 24}',
    'payload.get("morning_broadcast")',
    'payload.get("daily_forecast")',
  ]) {
    if (!broadcastScript.includes(fragment)) {
      addError(`${erhuaWeatherBroadcastPath}: missing ${fragment}`);
    }
  }
  for (const fragment of ["QIWE_TOKEN", "QiWeAdapter", "/msg/send"]) {
    if (broadcastScript.includes(fragment)) {
      addError(
        `${erhuaWeatherBroadcastPath}: must not own QiWe delivery (${fragment})`
      );
    }
  }
}

if (exists("agents/erhua/profile.template.yaml")) {
  const template = readYaml("agents/erhua/profile.template.yaml");
  const reviewedSources = template.reviewed_script_sources ?? [];
  const weatherSource = reviewedSources.find(
    (item) => item?.runtime_name === "qintopia-erhua-weather-broadcast.py"
  );
  if (weatherSource?.source !== erhuaWeatherBroadcastPath) {
    addError(
      "agents/erhua/profile.template.yaml: weather broadcast source must use the release-owned asset"
    );
  }
  if (!String(weatherSource?.activation ?? "").includes("pending")) {
    addError(
      "agents/erhua/profile.template.yaml: weather broadcast activation must remain pending"
    );
  }
}

if (!exists("docs/operations/profile-bundles/m10f-profile-template-plan.md")) {
  addError("docs/operations/profile-bundles/m10f-profile-template-plan.md is required");
}

const xiaomanBundleRoot = "agents/xiaoman/profile-bundle";
const xiaomanBundleFiles = [
  `${xiaomanBundleRoot}/README.md`,
  `${xiaomanBundleRoot}/bundle.json`,
  `${xiaomanBundleRoot}/migrate_values.py`,
  `${xiaomanBundleRoot}/render.py`,
  `${xiaomanBundleRoot}/templates/SOUL.md.template`,
  `${xiaomanBundleRoot}/templates/profile.yaml.template`,
  `${xiaomanBundleRoot}/tests/fixtures/values.json`,
  `${xiaomanBundleRoot}/tests/test_migrate_values.py`,
  `${xiaomanBundleRoot}/tests/test_render.py`,
];
for (const file of xiaomanBundleFiles) {
  if (!exists(file)) {
    addError(`${file}: required Xiaoman observation bundle file is missing`);
  }
}

for (const file of [
  `${xiaomanBundleRoot}/migrate_values.py`,
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
  const expectedSourceHashes = new Map([
    ["SOUL.md", "4b54c777e09102385665554829df7b1665bde57d28b4c5bc5ce34fd1d052801e"],
    [
      "profile.yaml",
      "b34f56b16eac72dc561faef1178d8242705000376561327054e9a15809c2de09",
    ],
  ]);
  for (const item of files) {
    if (expectedSourceHashes.get(item.target) !== item.production_source_sha256) {
      addError(
        `${xiaomanBundleRoot}/bundle.json: source hash mismatch for ${item.target}`
      );
    }
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
    bundle.production_boundary?.network_access !== false ||
    bundle.production_boundary?.server_config_write !== "manual-root-only"
  ) {
    addError(`${xiaomanBundleRoot}/bundle.json: production boundary must be read-only`);
  }
}

if (exists(`${xiaomanBundleRoot}/migrate_values.py`)) {
  const migration = readText(`${xiaomanBundleRoot}/migrate_values.py`);
  for (const fragment of [
    'LIVE_SOUL_PATH = Path("/home/ubuntu/.hermes/profiles/xiaoman/SOUL.md")',
    'LIVE_PROFILE_PATH = Path("/home/ubuntu/.hermes/profiles/xiaoman/profile.yaml")',
    'OUTPUT_PATH = Path("/etc/qintopia/xiaoman-profile-bundle-values.json")',
    'APPROVAL_PHRASE = "approved-xiaoman-profile-values-migration"',
    "if effective_uid != 0:",
    "reviewed production source hash mismatch",
    "rendered SOUL.md parity mismatch",
    "rendered profile.yaml parity mismatch",
    "os.link(temporary_path, path, follow_symlinks=False)",
    '"live_profile_modified": False',
    '"external_send_executed": False',
  ]) {
    if (!migration.includes(fragment)) {
      addError(`${xiaomanBundleRoot}/migrate_values.py: missing ${fragment}`);
    }
  }
  for (const fragment of [
    "requests",
    "urllib",
    "socket",
    "subprocess",
    "--output",
    "--source",
  ]) {
    if (migration.includes(fragment)) {
      addError(`${xiaomanBundleRoot}/migrate_values.py: must not contain ${fragment}`);
    }
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
