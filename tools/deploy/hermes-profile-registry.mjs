#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import YAML from "yaml";

const MAX_REGISTRY_BYTES = 1024 * 1024;
const SCRIPT_DIRECTORY = path.dirname(fileURLToPath(import.meta.url));
export const PROFILE_REGISTRY_PATH = path.resolve(
  SCRIPT_DIRECTORY,
  "../../runtime/hermes/profile-registry.yaml"
);
const PROFILE_KEYS = new Set([
  "id",
  "agent_manifest",
  "systemd_user_service",
  "wecom",
  "qiwe_platform",
]);
const WECOM_KEYS = new Set(["expected_enabled", "required_bindings"]);
const QIWE_KEYS = new Set(["preserve"]);
const ALLOWED_BINDINGS = new Set(["bot_id", "secret"]);
const FIXED_PROFILE_CONTRACT = Object.freeze([
  Object.freeze(["default", "hermes-gateway.service", true, false]),
  Object.freeze(["erhua", "hermes-gateway-erhua.service", false, true]),
  Object.freeze(["guanerye", "hermes-gateway-guanerye.service", true, false]),
  Object.freeze(["huabaosi", "hermes-gateway-huabaosi.service", true, false]),
  Object.freeze(["silaoshi", "hermes-gateway-silaoshi.service", true, false]),
  Object.freeze(["wenyuange", "hermes-gateway-wenyuange.service", false, false]),
  Object.freeze(["xiaoman", "hermes-gateway-xiaoman.service", true, false]),
]);

const isRecord = (value) =>
  value !== null && typeof value === "object" && !Array.isArray(value);

function hasExactKeys(value, expected) {
  const keys = Object.keys(value);
  return keys.length === expected.size && keys.every((key) => expected.has(key));
}

function readRegistry() {
  let metadata;
  try {
    metadata = fs.lstatSync(PROFILE_REGISTRY_PATH);
  } catch {
    throw new Error("profile_registry_missing");
  }
  if (metadata.isSymbolicLink() || !metadata.isFile()) {
    throw new Error("profile_registry_not_regular");
  }
  if (metadata.size <= 0 || metadata.size > MAX_REGISTRY_BYTES) {
    throw new Error("profile_registry_size_invalid");
  }

  let document;
  try {
    document = YAML.parseDocument(fs.readFileSync(PROFILE_REGISTRY_PATH, "utf8"), {
      prettyErrors: false,
      uniqueKeys: true,
    });
  } catch {
    throw new Error("profile_registry_unparseable");
  }
  if (document.errors.length > 0 || document.warnings.length > 0) {
    throw new Error("profile_registry_unparseable");
  }
  const registry = document.toJS();
  if (
    !isRecord(registry) ||
    !hasExactKeys(registry, new Set(["schema_version", "profiles"])) ||
    registry.schema_version !== 1 ||
    !Array.isArray(registry.profiles) ||
    registry.profiles.length !== 7
  ) {
    throw new Error("profile_registry_invalid");
  }
  return registry;
}

export function loadHermesProfileRegistry() {
  const registry = readRegistry();
  const ids = new Set();
  const services = new Set();

  for (const entry of registry.profiles) {
    if (
      !isRecord(entry) ||
      !hasExactKeys(entry, PROFILE_KEYS) ||
      typeof entry.id !== "string" ||
      !/^[a-z][a-z0-9]*$/.test(entry.id) ||
      entry.agent_manifest !== `agents/${entry.id}/agent.yaml` ||
      typeof entry.systemd_user_service !== "string" ||
      !/^hermes-gateway(?:-[a-z][a-z0-9]*)?\.service$/.test(
        entry.systemd_user_service
      ) ||
      !isRecord(entry.wecom) ||
      !hasExactKeys(entry.wecom, WECOM_KEYS) ||
      typeof entry.wecom.expected_enabled !== "boolean" ||
      !Array.isArray(entry.wecom.required_bindings) ||
      entry.wecom.required_bindings.length === 0 ||
      entry.wecom.required_bindings.some(
        (binding) => typeof binding !== "string" || !ALLOWED_BINDINGS.has(binding)
      ) ||
      new Set(entry.wecom.required_bindings).size !==
        entry.wecom.required_bindings.length ||
      !isRecord(entry.qiwe_platform) ||
      !hasExactKeys(entry.qiwe_platform, QIWE_KEYS) ||
      typeof entry.qiwe_platform.preserve !== "boolean" ||
      ids.has(entry.id) ||
      services.has(entry.systemd_user_service)
    ) {
      throw new Error("profile_registry_invalid");
    }
    ids.add(entry.id);
    services.add(entry.systemd_user_service);
  }

  const actualContract = registry.profiles.map((entry) => [
    entry.id,
    entry.systemd_user_service,
    entry.wecom.expected_enabled,
    entry.qiwe_platform.preserve,
  ]);
  if (JSON.stringify(actualContract) !== JSON.stringify(FIXED_PROFILE_CONTRACT)) {
    throw new Error("profile_registry_contract_mismatch");
  }

  return registry.profiles;
}

function main() {
  if (
    process.argv.length !== 3 ||
    !new Set(["--services", "--service-map"]).has(process.argv[2])
  ) {
    console.error("hermes_profile_registry=blocked");
    console.error("hermes_profile_registry_error=invalid_invocation");
    process.exitCode = 2;
    return;
  }
  try {
    for (const profile of loadHermesProfileRegistry()) {
      if (process.argv[2] === "--service-map") {
        console.log(`${profile.id}\t${profile.systemd_user_service}`);
      } else {
        console.log(profile.systemd_user_service);
      }
    }
  } catch (error) {
    const code =
      error instanceof Error && /^profile_registry_[a-z_]+$/.test(error.message)
        ? error.message
        : "profile_registry_invalid";
    console.error("hermes_profile_registry=blocked");
    console.error(`hermes_profile_registry_error=${code}`);
    process.exitCode = 1;
  }
}

if (
  process.argv[1] &&
  path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url))
) {
  main();
}
