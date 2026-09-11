#!/usr/bin/env node

import process from "node:process";
import { loadHermesProfileRegistry } from "./hermes-profile-registry.mjs";

const FIXED_LAUNCHER = "/usr/local/libexec/qintopia-hermes-core-launcher";

const profile = process.argv[2];
if (process.argv.length !== 3 || typeof profile !== "string") {
  console.error("hermes_core_systemd_unit=blocked");
  console.error("hermes_core_systemd_unit_error=invalid_invocation");
  process.exit(2);
}

try {
  const entry = loadHermesProfileRegistry().find(
    (candidate) => candidate.id === profile
  );
  if (!entry) {
    throw new Error("profile_registry_invalid");
  }
  process.stdout.write(
    `[Service]\nExecStart=\nExecStart=${FIXED_LAUNCHER} --profile ${entry.id} gateway run --replace\nEnvironment=HERMES_HOME=/home/ubuntu/.hermes\n`
  );
} catch (error) {
  const code =
    error instanceof Error && /^profile_registry_[a-z_]+$/.test(error.message)
      ? error.message
      : "profile_registry_invalid";
  console.error("hermes_core_systemd_unit=blocked");
  console.error(`hermes_core_systemd_unit_error=${code}`);
  process.exit(1);
}
