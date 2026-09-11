#!/usr/bin/env node

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFileSync, spawnSync } from "node:child_process";
import { smokeHermesCli } from "./build-hermes-core-artifact.mjs";

const repoRoot = process.cwd();
const builder = path.join(repoRoot, "tools/deploy/build-hermes-core-artifact.mjs");
const tempRoot = fs.realpathSync.native(
  fs.mkdtempSync(path.join(os.tmpdir(), "hermes-core-builder-"))
);
const tag = "v9.9.9";
const sourceSha256 = "a".repeat(64);

try {
  const source = path.join(tempRoot, "source");
  const output = path.join(tempRoot, "artifact");
  fs.mkdirSync(path.join(source, "hermes_cli"), { recursive: true });
  fs.writeFileSync(
    path.join(source, "pyproject.toml"),
    "[project]\nname = 'hermes-agent'\nversion = '9.9.9'\n"
  );
  fs.writeFileSync(path.join(source, "uv.lock"), "version = 1\n");
  fs.writeFileSync(path.join(source, "hermes_cli", "__init__.py"), "\n");
  fs.writeFileSync(
    path.join(source, "hermes_cli", "main.py"),
    "print('builder fixture')\n"
  );
  execFileSync("git", ["init", "--quiet", source]);
  execFileSync("git", [
    "-C",
    source,
    "config",
    "user.email",
    "fixture@example.invalid",
  ]);
  execFileSync("git", ["-C", source, "config", "user.name", "Fixture"]);
  execFileSync("git", [
    "-C",
    source,
    "remote",
    "add",
    "origin",
    "https://github.com/NousResearch/hermes-agent.git",
  ]);
  execFileSync("git", ["-C", source, "add", "."]);
  execFileSync("git", ["-C", source, "commit", "--quiet", "-m", "fixture"]);
  const commit = execFileSync("git", ["-C", source, "rev-parse", "HEAD"], {
    encoding: "utf8",
  }).trim();
  assert.match(commit, /^[0-9a-f]{40}$/);
  execFileSync("git", ["-C", source, "tag", tag]);

  const fakeUv = path.join(tempRoot, "uv");
  fs.writeFileSync(
    fakeUv,
    "#!/bin/sh\n" +
      "set -eu\n" +
      'if [ "$1" = export ]; then\n' +
      '  output=""\n' +
      "  while [ $# -gt 0 ]; do\n" +
      '    if [ "$1" = --output-file ]; then output="$2"; shift 2; else shift; fi\n' +
      "  done\n" +
      '  : > "$output"\n' +
      "  exit 0\n" +
      "fi\n" +
      'if [ "$1" = pip ] && [ "$2" = sync ]; then exit 0; fi\n' +
      "exit 64\n"
  );
  fs.chmodSync(fakeUv, 0o755);

  const python = execFileSync("sh", ["-c", "command -v python3"], {
    encoding: "utf8",
  }).trim();
  smokeHermesCli(python, source, tempRoot);
  fs.writeFileSync(
    path.join(source, "hermes_cli", "main.py"),
    "raise RuntimeError('broken CLI fixture')\n"
  );
  assert.throws(
    () => smokeHermesCli(python, source, tempRoot),
    /hermes_cli_smoke_failed/
  );
  fs.writeFileSync(
    path.join(source, "hermes_cli", "main.py"),
    "print('builder fixture')\n"
  );
  const result = spawnSync(
    process.execPath,
    [
      builder,
      "--source-dir",
      source,
      "--output-dir",
      output,
      "--tag",
      tag,
      "--commit",
      commit,
      "--source-archive-sha256",
      sourceSha256,
      "--previous-version",
      "9.9.8",
      "--builder-image-digest",
      "c".repeat(64),
      "--python",
      python,
      "--uv",
      fakeUv,
    ],
    { cwd: repoRoot, encoding: "utf8" }
  );
  if (process.platform !== "linux" || process.arch !== "x64") {
    assert.equal(result.status, 1);
    assert.match(result.stderr, /builder_platform_unsupported/);
    assert.equal(fs.existsSync(output), false);
    console.log(
      "Hermes CLI success/failure probes and non-Linux artifact rejection passed; Linux artifact build requires Linux CI."
    );
  } else {
    assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
    const manifest = JSON.parse(
      fs.readFileSync(path.join(output, "artifact-manifest.json"), "utf8")
    );
    assert.equal(manifest.commit_sha, commit);
    assert.equal(manifest.runtime.kind, "release-local-venv");
    assert.equal(
      fs.lstatSync(path.join(output, manifest.runtime.interpreter_path)).isFile(),
      true
    );
    assert.equal(
      fs.lstatSync(path.join(output, manifest.runtime.launcher_path)).mode & 0o111,
      0o111
    );
    assert.equal(
      fs.existsSync(path.join(output, manifest.runtime.site_packages_path)),
      true
    );
    console.log("Hermes core artifact builder test passed.");
  }
} finally {
  makeWritable(tempRoot);
  fs.rmSync(tempRoot, { recursive: true, force: true });
}

function makeWritable(directory) {
  if (!fs.existsSync(directory)) return;
  const metadata = fs.lstatSync(directory);
  if (metadata.isDirectory()) {
    for (const name of fs.readdirSync(directory))
      makeWritable(path.join(directory, name));
    fs.chmodSync(directory, 0o700);
  } else if (!metadata.isSymbolicLink()) {
    fs.chmodSync(directory, 0o600);
  }
}
