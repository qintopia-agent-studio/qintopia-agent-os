#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import YAML from "yaml";
import { loadHermesProfileRegistry } from "./hermes-profile-registry.mjs";

const PRODUCTION_HERMES_HOME = "/home/ubuntu/.hermes";
const MAX_INPUT_BYTES = 1024 * 1024;
const ENV_KEY_BY_BINDING = Object.freeze({
  bot_id: "WECOM_BOT_ID",
  secret: "WECOM_SECRET",
});
const WECOM_CONFIG_LOCATIONS = Object.freeze([
  Object.freeze({ parent: "platforms", label: "platforms.wecom" }),
  Object.freeze({ parent: "channel", label: "channel.wecom" }),
]);
const SAFE_INVOCATION_ERRORS = new Set([
  "invalid_invocation",
  "production_home_must_be_fixed",
  "staging_home_is_production_home",
]);
const REGULAR_INPUT_ERRORS = Object.freeze({
  config: Object.freeze({
    missing: "config_missing",
    symlink: "config_symlink",
    notRegular: "config_not_regular",
    tooLarge: "config_too_large",
    invalidText: "config_invalid_text",
    unreadable: "config_unreadable",
  }),
  env: Object.freeze({
    missing: "env_missing",
    symlink: "env_symlink",
    notRegular: "env_not_regular",
    tooLarge: "env_too_large",
    invalidText: "env_invalid_text",
    unreadable: "env_unreadable",
  }),
});

const hasOwn = (value, key) =>
  value !== null &&
  typeof value === "object" &&
  Object.prototype.hasOwnProperty.call(value, key);

const isRecord = (value) =>
  value !== null && typeof value === "object" && !Array.isArray(value);

const fixedError = (code) => ({ code });

function parseArguments(argv) {
  let hermesHome = null;
  let mode = null;

  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--help") {
      return { help: true };
    }
    if (argument === "--hermes-home" && hermesHome === null) {
      hermesHome = argv[++index] ?? null;
      continue;
    }
    if (argument === "--mode" && mode === null) {
      mode = argv[++index] ?? null;
      continue;
    }
    throw new Error("invalid_invocation");
  }

  if (hermesHome === null || !path.isAbsolute(hermesHome)) {
    throw new Error("invalid_invocation");
  }
  hermesHome = path.resolve(hermesHome);
  if (mode === null) {
    mode = hermesHome === PRODUCTION_HERMES_HOME ? "production" : "staging";
  }
  if (!new Set(["production", "staging"]).has(mode)) {
    throw new Error("invalid_invocation");
  }
  if (mode === "production" && hermesHome !== PRODUCTION_HERMES_HOME) {
    throw new Error("production_home_must_be_fixed");
  }
  if (mode === "staging" && hermesHome === PRODUCTION_HERMES_HOME) {
    throw new Error("staging_home_is_production_home");
  }

  return { hermesHome, mode, help: false };
}

function readRegularText(filePath, kind) {
  const errors = REGULAR_INPUT_ERRORS[kind];
  if (errors === undefined) {
    return { ok: false, error: fixedError("input_kind_invalid") };
  }
  let metadata;
  try {
    metadata = fs.lstatSync(filePath);
  } catch {
    return { ok: false, error: fixedError(errors.missing) };
  }
  if (metadata.isSymbolicLink()) {
    return { ok: false, error: fixedError(errors.symlink) };
  }
  if (!metadata.isFile()) {
    return { ok: false, error: fixedError(errors.notRegular) };
  }
  if (!Number.isSafeInteger(metadata.size) || metadata.size > MAX_INPUT_BYTES) {
    return { ok: false, error: fixedError(errors.tooLarge) };
  }

  try {
    const bytes = fs.readFileSync(filePath);
    const text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
    if (text.includes("\u0000")) {
      return { ok: false, error: fixedError(errors.invalidText) };
    }
    return { ok: true, text };
  } catch {
    return { ok: false, error: fixedError(errors.unreadable) };
  }
}

function readDirectory(directoryPath, kind) {
  let metadata;
  try {
    metadata = fs.lstatSync(directoryPath);
  } catch {
    return fixedError(`${kind}_missing`);
  }
  if (metadata.isSymbolicLink()) {
    return fixedError(`${kind}_symlink`);
  }
  if (!metadata.isDirectory()) {
    return fixedError(`${kind}_not_directory`);
  }
  return null;
}

function parseConfig(text) {
  try {
    const documents = YAML.parseAllDocuments(text, {
      prettyErrors: false,
      uniqueKeys: true,
    });
    if (
      documents.length !== 1 ||
      documents.some(
        (document) => document.errors.length > 0 || document.warnings.length > 0
      )
    ) {
      return { ok: false, error: fixedError("config_unparseable") };
    }
    const value = documents[0].toJS();
    if (!isRecord(value)) {
      return { ok: false, error: fixedError("config_not_mapping") };
    }
    return { ok: true, value };
  } catch {
    return { ok: false, error: fixedError("config_unparseable") };
  }
}

function findWecomConfig(config) {
  const matches = [];
  for (const location of WECOM_CONFIG_LOCATIONS) {
    const parent = config[location.parent];
    if (isRecord(parent) && hasOwn(parent, "wecom")) {
      matches.push({ label: location.label, value: parent.wecom });
    }
  }

  if (matches.length === 0) {
    return { ok: false, error: fixedError("wecom_config_missing") };
  }
  if (matches.length !== 1) {
    return { ok: false, error: fixedError("wecom_config_ambiguous") };
  }
  if (!isRecord(matches[0].value)) {
    return { ok: false, error: fixedError("wecom_config_not_mapping") };
  }
  if (!hasOwn(matches[0].value, "enabled")) {
    return { ok: false, error: fixedError("wecom_enabled_missing") };
  }
  if (typeof matches[0].value.enabled !== "boolean") {
    return { ok: false, error: fixedError("wecom_enabled_not_boolean") };
  }
  if (hasOwn(matches[0].value, "extra") && !isRecord(matches[0].value.extra)) {
    return { ok: false, error: fixedError("wecom_extra_not_mapping") };
  }
  return {
    ok: true,
    label: matches[0].label,
    value: matches[0].value,
  };
}

function hasConfigBinding(wecomConfig, binding) {
  if (hasOwn(wecomConfig, binding)) {
    return true;
  }
  return isRecord(wecomConfig.extra) && hasOwn(wecomConfig.extra, binding);
}

function parseEnvKeys(text) {
  const keys = new Set();
  const lines = text.split(/\r?\n/);
  const assignment = /^(?:export[ \t]+)?([A-Za-z_][A-Za-z0-9_]*)[ \t]*=/;

  for (const line of lines) {
    const stripped = line.trim();
    if (stripped === "" || stripped.startsWith("#")) {
      continue;
    }
    const match = assignment.exec(stripped);
    if (match === null) {
      return { ok: false, error: fixedError("env_unparseable") };
    }
    const key = match[1];
    if (keys.has(key)) {
      return { ok: false, error: fixedError("env_duplicate_key") };
    }
    keys.add(key);
  }
  return { ok: true, keys };
}

function inspectProfile(hermesHome, profile, expectedEnabled, requiredBindings) {
  const result = {
    profile,
    expected_enabled: expectedEnabled,
    actual_enabled: null,
    config_parsed: false,
    wecom_config: "unknown",
    bindings: "unknown",
    env_file: "not_checked",
    required_keys: [],
    present_keys: [],
    errors: [],
  };
  const profileDirectory =
    profile === "default" ? hermesHome : path.join(hermesHome, "profiles", profile);
  const profileDirectoryError = readDirectory(profileDirectory, "profile_dir");
  if (profileDirectoryError !== null) {
    result.errors.push(profileDirectoryError.code);
    return result;
  }

  const configPath = path.join(profileDirectory, "config.yaml");
  const configInput = readRegularText(configPath, "config");
  if (!configInput.ok) {
    result.errors.push(configInput.error.code);
    return result;
  }
  const parsedConfig = parseConfig(configInput.text);
  if (!parsedConfig.ok) {
    result.errors.push(parsedConfig.error.code);
    return result;
  }
  result.config_parsed = true;

  const wecom = findWecomConfig(parsedConfig.value);
  if (!wecom.ok) {
    if (wecom.error.code === "wecom_config_missing" && !expectedEnabled) {
      result.actual_enabled = false;
      result.wecom_config = "absent_disabled";
      result.bindings = "not_required_disabled";
      result.env_file = "not_checked_disabled";
      return result;
    }
    result.errors.push(wecom.error.code);
    return result;
  }
  result.wecom_config = wecom.label;
  result.actual_enabled = wecom.value.enabled;
  if (result.actual_enabled !== expectedEnabled) {
    result.errors.push("wecom_enabled_mismatch");
  }

  if (!result.actual_enabled) {
    result.bindings = "not_required_disabled";
    result.env_file = "not_checked_disabled";
    return result;
  }

  const configBindings = {};
  const requiredEnvKeys = [];
  for (const binding of requiredBindings) {
    configBindings[binding] = hasConfigBinding(wecom.value, binding);
    if (!configBindings[binding]) {
      requiredEnvKeys.push(ENV_KEY_BY_BINDING[binding]);
    }
  }
  result.required_keys = requiredEnvKeys;

  const envPath = path.join(profileDirectory, ".env");
  const envInput = readRegularText(envPath, "env");
  if (!envInput.ok) {
    if (envInput.error.code === "env_missing" && requiredEnvKeys.length === 0) {
      result.env_file = "not_required_inline_bindings";
    } else {
      result.errors.push(envInput.error.code);
    }
    result.bindings = Object.entries(configBindings)
      .map(([binding, present]) => `${binding}:${present ? "config" : "missing"}`)
      .join(",");
    return result;
  }
  result.env_file = "present";
  const parsedEnv = parseEnvKeys(envInput.text);
  if (!parsedEnv.ok) {
    result.errors.push(parsedEnv.error.code);
    result.bindings = Object.entries(configBindings)
      .map(([binding, present]) => `${binding}:${present ? "config" : "env"}`)
      .join(",");
    return result;
  }
  for (const key of requiredEnvKeys) {
    if (parsedEnv.keys.has(key)) {
      result.present_keys.push(key);
    } else {
      result.errors.push(`required_key_missing:${key}`);
    }
  }
  result.bindings = Object.entries(configBindings)
    .map(([binding, present]) => `${binding}:${present ? "config" : "env"}`)
    .join(",");
  return result;
}

function printReport(report) {
  const blocked = report.errors.length > 0;
  console.log(`hermes_wecom_readiness=${blocked ? "blocked" : "ready"}`);
  console.log(`hermes_wecom_mode=${report.mode}`);
  console.log(`hermes_wecom_profiles=${report.profiles.length}`);
  for (const profile of report.profiles) {
    const actual =
      profile.actual_enabled === null ? "unknown" : String(profile.actual_enabled);
    const requiredKeys =
      profile.required_keys.length === 0 ? "none" : profile.required_keys.join(",");
    const presentKeys =
      profile.present_keys.length === 0 ? "none" : profile.present_keys.join(",");
    console.log(
      [
        `hermes_wecom_profile=${profile.profile}`,
        `expected_enabled=${profile.expected_enabled}`,
        `actual_enabled=${actual}`,
        `config_parsed=${profile.config_parsed}`,
        `wecom_config=${profile.wecom_config}`,
        `bindings=${profile.bindings}`,
        `env_file=${profile.env_file}`,
        `required_keys=${requiredKeys}`,
        `present_keys=${presentKeys}`,
        `error_count=${profile.errors.length}`,
      ].join(" ")
    );
    for (const error of profile.errors) {
      console.log(`hermes_wecom_error=${profile.profile}:${error}`);
    }
  }
  for (const error of report.errors) {
    console.log(`hermes_wecom_error=${error}`);
  }
}

function buildReport(argumentsValue) {
  let registry;
  try {
    registry = loadHermesProfileRegistry();
  } catch {
    return {
      mode: argumentsValue.mode,
      profiles: [],
      errors: ["profile_registry_invalid"],
    };
  }
  const homeError = readDirectory(argumentsValue.hermesHome, "hermes_home");
  if (homeError !== null) {
    return {
      mode: argumentsValue.mode,
      profiles: [],
      errors: [homeError.code],
    };
  }
  const profilesDirectoryError = readDirectory(
    path.join(argumentsValue.hermesHome, "profiles"),
    "profiles_dir"
  );
  if (profilesDirectoryError !== null) {
    return {
      mode: argumentsValue.mode,
      profiles: [],
      errors: [profilesDirectoryError.code],
    };
  }

  const profiles = registry.map((entry) =>
    inspectProfile(
      argumentsValue.hermesHome,
      entry.id,
      entry.wecom.expected_enabled,
      entry.wecom.required_bindings
    )
  );
  return {
    mode: argumentsValue.mode,
    profiles,
    errors: profiles.flatMap((profile) => profile.errors),
  };
}

function main() {
  let argumentsValue;
  try {
    argumentsValue = parseArguments(process.argv.slice(2));
  } catch (error) {
    const errorCode =
      error instanceof Error && SAFE_INVOCATION_ERRORS.has(error.message)
        ? error.message
        : "invalid_invocation";
    console.error("hermes_wecom_readiness=blocked");
    console.error(`hermes_wecom_error=${errorCode}`);
    process.exitCode = 2;
    return;
  }
  if (argumentsValue.help) {
    console.log(
      "usage: check-hermes-wecom-readiness.mjs --hermes-home /absolute/path [--mode production|staging]"
    );
    return;
  }

  const report = buildReport(argumentsValue);
  printReport(report);
  process.exitCode = report.errors.length > 0 ? 1 : 0;
}

main();
