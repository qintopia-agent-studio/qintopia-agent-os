#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const sourceBundle = path.join(repoRoot, "agents/xiaoman/profile-bundle");
const observation = path.join(
  repoRoot,
  "deploy/sidecar/scripts/xiaoman-profile-bundle-observation-smoke.sh"
);
const temporary = fs.mkdtempSync(
  path.join(os.tmpdir(), "qintopia-xiaoman-profile-observation-")
);

const sha256 = (filePath) =>
  crypto.createHash("sha256").update(fs.readFileSync(filePath)).digest("hex");

const run = (extraEnv = {}) =>
  spawnSync("bash", [observation], {
    cwd: repoRoot,
    env: {
      ...process.env,
      QINTOPIA_XIAOMAN_PROFILE_BUNDLE_OBSERVATION_ENABLE: "1",
      QINTOPIA_XIAOMAN_PROFILE_DIR: path.join(temporary, "live"),
      QINTOPIA_XIAOMAN_PROFILE_BUNDLE_DIR: path.join(temporary, "bundle"),
      QINTOPIA_XIAOMAN_PROFILE_BUNDLE_VALUES_FILE: path.join(temporary, "values.json"),
      ...extraEnv,
    },
    encoding: "utf8",
  });

try {
  const bundle = path.join(temporary, "bundle");
  const live = path.join(temporary, "live");
  const values = path.join(temporary, "values.json");
  fs.cpSync(sourceBundle, bundle, { recursive: true });
  fs.copyFileSync(path.join(sourceBundle, "tests/fixtures/values.json"), values);
  fs.chmodSync(values, 0o600);

  const render = spawnSync(
    "python3",
    [path.join(bundle, "render.py"), "--values-file", values, "--output-dir", live],
    { cwd: repoRoot, encoding: "utf8" }
  );
  if (render.status !== 0) {
    throw new Error(`fixture render failed: ${render.stderr}`);
  }

  const manifestPath = path.join(bundle, "bundle.json");
  const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
  const files = new Map(manifest.files.map((item) => [item.target, item]));
  files.get("SOUL.md").production_source_sha256 = sha256(path.join(live, "SOUL.md"));
  files.get("profile.yaml").production_source_sha256 = sha256(
    path.join(live, "profile.yaml")
  );
  fs.writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);

  const soulBefore = fs.readFileSync(path.join(live, "SOUL.md"));
  const profileBefore = fs.readFileSync(path.join(live, "profile.yaml"));
  const success = run();
  if (success.status !== 0) {
    throw new Error(
      `expected observation to pass\nstdout:\n${success.stdout}\nstderr:\n${success.stderr}`
    );
  }
  const report = JSON.parse(success.stdout);
  if (
    report.status !== "xiaoman_profile_bundle_observation_passed" ||
    report.observation_only !== true ||
    report.live_profile_modified !== false ||
    report.symlink_created !== false ||
    report.soul_match !== true ||
    report.profile_match !== true
  ) {
    throw new Error("observation report did not retain the read-only parity boundary");
  }
  if (!fs.readFileSync(path.join(live, "SOUL.md")).equals(soulBefore)) {
    throw new Error("observation modified live SOUL.md fixture");
  }
  if (!fs.readFileSync(path.join(live, "profile.yaml")).equals(profileBefore)) {
    throw new Error("observation modified live profile.yaml fixture");
  }

  fs.appendFileSync(path.join(live, "SOUL.md"), "drift\n");
  const drift = run();
  if (drift.status === 0 || !drift.stderr.includes("source hash drifted")) {
    throw new Error("observation must fail closed on live source drift");
  }
  fs.writeFileSync(path.join(live, "SOUL.md"), soulBefore);

  fs.chmodSync(values, 0o644);
  const permissiveValues = run();
  if (
    permissiveValues.status === 0 ||
    !permissiveValues.stderr.includes("must not be group or world accessible")
  ) {
    throw new Error("observation must reject a permissive values file");
  }

  const disabled = run({
    QINTOPIA_XIAOMAN_PROFILE_BUNDLE_OBSERVATION_ENABLE: "0",
  });
  if (disabled.status === 0 || !disabled.stderr.includes("observation is disabled")) {
    throw new Error("observation must remain disabled by default");
  }
} finally {
  fs.rmSync(temporary, { recursive: true, force: true });
}

console.log("Xiaoman profile bundle observation test passed.");
