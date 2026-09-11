#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import YAML from "yaml";
import { loadHermesProfileRegistry } from "./hermes-profile-registry.mjs";

const PRODUCTION_HERMES_HOME = "/home/ubuntu/.hermes";
const PRODUCTION_CANDIDATE_ROOT =
  "/home/ubuntu/.local/state/qintopia-agentos/hermes-core-staging";
const MAX_INPUT_BYTES = 1024 * 1024;

const isRecord = (value) =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const hasOwn = (value, key) =>
  isRecord(value) && Object.prototype.hasOwnProperty.call(value, key);

function failInvocation(code) {
  console.error("hermes_wecom_parity=blocked");
  console.error(`hermes_wecom_parity_error=${code}`);
  process.exitCode = 2;
}

function parseArguments(argv) {
  const values = { baselineHome: null, candidateHome: null, mode: null };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--baseline-home" && values.baselineHome === null) {
      values.baselineHome = argv[++index] ?? null;
    } else if (argument === "--candidate-home" && values.candidateHome === null) {
      values.candidateHome = argv[++index] ?? null;
    } else if (argument === "--mode" && values.mode === null) {
      values.mode = argv[++index] ?? null;
    } else {
      throw new Error("invalid_invocation");
    }
  }
  if (
    !path.isAbsolute(values.baselineHome ?? "") ||
    !path.isAbsolute(values.candidateHome ?? "") ||
    !new Set(["production", "fixture"]).has(values.mode)
  ) {
    throw new Error("invalid_invocation");
  }
  values.baselineHome = path.resolve(values.baselineHome);
  values.candidateHome = path.resolve(values.candidateHome);
  if (values.baselineHome === values.candidateHome) {
    throw new Error("homes_must_differ");
  }
  if (values.mode === "production") {
    if (values.baselineHome !== PRODUCTION_HERMES_HOME) {
      throw new Error("production_baseline_must_be_fixed");
    }
    const relativeCandidate = path.relative(
      PRODUCTION_CANDIDATE_ROOT,
      values.candidateHome
    );
    if (
      relativeCandidate === "" ||
      relativeCandidate.startsWith("..") ||
      path.isAbsolute(relativeCandidate)
    ) {
      throw new Error("production_candidate_outside_staging_root");
    }
  } else if (
    values.baselineHome === PRODUCTION_HERMES_HOME ||
    values.candidateHome === PRODUCTION_HERMES_HOME
  ) {
    throw new Error("fixture_mode_cannot_read_production");
  }
  return values;
}

function requireDirectory(directoryPath, kind) {
  let metadata;
  try {
    metadata = fs.lstatSync(directoryPath);
  } catch {
    throw new Error(`${kind}_missing`);
  }
  if (metadata.isSymbolicLink()) {
    throw new Error(`${kind}_symlink`);
  }
  if (!metadata.isDirectory()) {
    throw new Error(`${kind}_not_directory`);
  }
}

function readRegularText(filePath, kind, optional = false) {
  let metadata;
  try {
    metadata = fs.lstatSync(filePath);
  } catch {
    if (optional) {
      return null;
    }
    throw new Error(`${kind}_missing`);
  }
  if (metadata.isSymbolicLink()) {
    throw new Error(`${kind}_symlink`);
  }
  if (!metadata.isFile()) {
    throw new Error(`${kind}_not_regular`);
  }
  if (metadata.size > MAX_INPUT_BYTES) {
    throw new Error(`${kind}_too_large`);
  }
  try {
    const text = new TextDecoder("utf-8", { fatal: true }).decode(
      fs.readFileSync(filePath)
    );
    if (text.includes("\u0000")) {
      throw new Error(`${kind}_invalid_text`);
    }
    return text;
  } catch (error) {
    if (error instanceof Error && error.message === `${kind}_invalid_text`) {
      throw error;
    }
    throw new Error(`${kind}_unreadable`);
  }
}

function parseConfig(text) {
  let document;
  try {
    document = YAML.parseDocument(text, {
      prettyErrors: false,
      uniqueKeys: true,
      maxAliasCount: 0,
    });
  } catch {
    throw new Error("config_unparseable");
  }
  if (document.errors.length > 0 || document.warnings.length > 0) {
    throw new Error("config_unparseable");
  }
  const config = document.toJS();
  if (!isRecord(config)) {
    throw new Error("config_not_mapping");
  }
  return config;
}

function findWecom(config) {
  const matches = [];
  for (const parentName of ["platforms", "channel"]) {
    const parent = config[parentName];
    if (isRecord(parent) && hasOwn(parent, "wecom")) {
      matches.push(parent.wecom);
    }
  }
  if (matches.length !== 1 || !isRecord(matches[0])) {
    throw new Error(
      matches.length === 0 ? "wecom_config_missing" : "wecom_config_ambiguous"
    );
  }
  if (typeof matches[0].enabled !== "boolean") {
    throw new Error("wecom_enabled_invalid");
  }
  return matches[0];
}

function canonicalize(value) {
  if (Array.isArray(value)) {
    return value.map(canonicalize);
  }
  if (isRecord(value)) {
    return Object.fromEntries(
      Object.keys(value)
        .sort()
        .map((key) => [key, canonicalize(value[key])])
    );
  }
  if (
    value === null ||
    typeof value === "string" ||
    typeof value === "number" ||
    typeof value === "boolean"
  ) {
    return value;
  }
  throw new Error("wecom_value_type_unsupported");
}

function parseWecomEnv(text) {
  const values = new Map();
  if (text === null) {
    return values;
  }
  const assignment = /^(?:export[ \t]+)?([A-Za-z_][A-Za-z0-9_]*)[ \t]*=(.*)$/;
  for (const line of text.split(/\r?\n/)) {
    const stripped = line.trim();
    if (stripped === "" || stripped.startsWith("#")) {
      continue;
    }
    const match = assignment.exec(stripped);
    if (match === null) {
      throw new Error("env_unparseable");
    }
    const key = match[1];
    if (values.has(key)) {
      throw new Error("env_duplicate_key");
    }
    if (key.startsWith("WECOM_")) {
      values.set(key, match[2]);
    }
  }
  return values;
}

function mapsEqual(left, right) {
  if (left.size !== right.size) {
    return false;
  }
  for (const [key, value] of left) {
    if (!right.has(key) || right.get(key) !== value) {
      return false;
    }
  }
  return true;
}

function inspectHome(home, profile) {
  requireDirectory(home, "hermes_home");
  requireDirectory(path.join(home, "profiles"), "profiles_dir");
  const profileDirectory =
    profile === "default" ? home : path.join(home, "profiles", profile);
  requireDirectory(profileDirectory, "profile_dir");
  const config = parseConfig(
    readRegularText(path.join(profileDirectory, "config.yaml"), "config")
  );
  const env = parseWecomEnv(
    readRegularText(path.join(profileDirectory, ".env"), "env", true)
  );
  return { wecom: canonicalize(findWecom(config)), env };
}

function inspectProfile(baselineHome, candidateHome, registryProfile) {
  const result = {
    profile: registryProfile.id,
    expectedEnabled: registryProfile.wecom.expected_enabled,
    baselineEnabled: null,
    candidateEnabled: null,
    configMatched: false,
    envMatched: false,
    errors: [],
  };
  let baseline;
  let candidate;
  try {
    baseline = inspectHome(baselineHome, registryProfile.id);
  } catch (error) {
    result.errors.push(`baseline_${error.message}`);
    return result;
  }
  try {
    candidate = inspectHome(candidateHome, registryProfile.id);
  } catch (error) {
    result.errors.push(`candidate_${error.message}`);
    return result;
  }
  result.baselineEnabled = baseline.wecom.enabled;
  result.candidateEnabled = candidate.wecom.enabled;
  if (result.baselineEnabled !== result.expectedEnabled) {
    result.errors.push("baseline_enabled_mismatch");
  }
  if (result.candidateEnabled !== result.expectedEnabled) {
    result.errors.push("candidate_enabled_mismatch");
  }
  result.configMatched =
    JSON.stringify(baseline.wecom) === JSON.stringify(candidate.wecom);
  result.envMatched = mapsEqual(baseline.env, candidate.env);
  if (!result.configMatched) {
    result.errors.push("wecom_config_mismatch");
  }
  if (!result.envMatched) {
    result.errors.push("wecom_env_mismatch");
  }
  return result;
}

function main() {
  let args;
  try {
    args = parseArguments(process.argv.slice(2));
  } catch (error) {
    failInvocation(error instanceof Error ? error.message : "invalid_invocation");
    return;
  }

  let profiles;
  try {
    profiles = loadHermesProfileRegistry();
  } catch {
    console.log("hermes_wecom_parity=blocked");
    console.log("hermes_wecom_parity_error=profile_registry_invalid");
    process.exitCode = 1;
    return;
  }

  const results = profiles.map((profile) =>
    inspectProfile(args.baselineHome, args.candidateHome, profile)
  );
  const blocked = results.some((result) => result.errors.length > 0);
  console.log(`hermes_wecom_parity=${blocked ? "blocked" : "ready"}`);
  console.log(`hermes_wecom_parity_mode=${args.mode}`);
  console.log(`hermes_wecom_parity_profiles=${results.length}`);
  for (const result of results) {
    console.log(
      [
        `hermes_wecom_parity_profile=${result.profile}`,
        `expected_enabled=${result.expectedEnabled}`,
        `baseline_enabled=${result.baselineEnabled ?? "unknown"}`,
        `candidate_enabled=${result.candidateEnabled ?? "unknown"}`,
        `config_matched=${result.configMatched}`,
        `env_matched=${result.envMatched}`,
        `error_count=${result.errors.length}`,
      ].join(" ")
    );
    for (const error of result.errors) {
      console.log(`hermes_wecom_parity_error=${result.profile}:${error}`);
    }
  }
  process.exitCode = blocked ? 1 : 0;
}

main();
