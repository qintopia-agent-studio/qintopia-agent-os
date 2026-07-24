#!/usr/bin/env node

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { pathToFileURL } from "node:url";

const repoRoot = process.cwd();
const builderPath = path.join(
  repoRoot,
  "tools",
  "deploy",
  "build-qiwe-production-sidecar-artifact.mjs"
);
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "build-qiwe-production-sidecar-artifact-")
);

try {
  const fixture = createFixtureRepo();
  let result = runBuilder(fixture.repoRoot, fixture.binRoot);
  assert.equal(result.status, 0, result.stderr);
  assert.match(
    result.stdout,
    /Built qintopia-message-sidecar-qiwe-production-linux-x86_64-gnu/
  );

  const artifactDir = path.join(
    fixture.repoRoot,
    "dist",
    "sidecar-artifacts",
    "qintopia-message-sidecar-qiwe-production-linux-x86_64-gnu"
  );
  const manifest = JSON.parse(
    fs.readFileSync(path.join(artifactDir, "artifact-manifest.json"), "utf8")
  );
  assert.equal(
    manifest.artifact_name,
    "qintopia-message-sidecar-qiwe-production-linux-x86_64-gnu"
  );
  assert.equal(manifest.validation.artifact_profile, "qiwe-production");
  assert.deepEqual(manifest.validation.cargo_features, ["qiwe-production-adapter"]);
  assert.equal(
    manifest.files.find((entry) => entry.path === "qintopia-message-sidecar")?.mode,
    "0755"
  );
  assert.equal(
    manifest.files.find((entry) => entry.path === "qintopia-message-sidecar.tar.gz")
      ?.mode,
    "0644"
  );
  const checksumText = fs.readFileSync(path.join(artifactDir, "SHA256SUMS"), "utf8");
  assert.match(checksumText, /artifact-manifest\.json/);
  assert.doesNotMatch(checksumText, /huabaosi-production-adapter/);

  result = runBuilder(fixture.repoRoot, fixture.binRoot, {
    FAKE_GIT_STATUS: " M docs/README.md\n",
  });
  assert.notEqual(result.status, 0);
  assert.match(
    `${result.stderr}\n${result.stdout}`,
    /dirty or unreadable git worktree/
  );
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("QiWe production sidecar artifact builder test passed.");

function runBuilder(repoFixtureRoot, binRoot, extraEnv = {}) {
  const wrapperPath = path.join(repoFixtureRoot, "run-builder.mjs");
  fs.writeFileSync(
    wrapperPath,
    [
      'Object.defineProperty(process, "platform", { value: "linux" });',
      'Object.defineProperty(process, "arch", { value: "x64" });',
      'Object.defineProperty(process, "report", {',
      "  value: {",
      "    getReport() {",
      '      return { header: { glibcVersionRuntime: "2.35" } };',
      "    },",
      "  },",
      "});",
      `await import(${JSON.stringify(pathToFileURL(builderPath).href)});`,
      "",
    ].join("\n"),
    "utf8"
  );

  return spawnSync("node", [wrapperPath], {
    cwd: repoFixtureRoot,
    encoding: "utf8",
    env: {
      ...process.env,
      ...extraEnv,
      PATH: `${binRoot}${path.delimiter}${process.env.PATH ?? ""}`,
      GITHUB_REPOSITORY: "qintopia/qintopia-agent-os",
      GITHUB_SHA: "0123456789abcdef0123456789abcdef01234567",
      GITHUB_REF_NAME: "codex/test-qiwe-production-builder",
    },
  });
}

function createFixtureRepo() {
  const repoFixtureRoot = path.join(tmpRoot, "repo");
  const binRoot = path.join(tmpRoot, "bin");
  fs.mkdirSync(path.join(repoFixtureRoot, "runtime", "sidecar"), { recursive: true });
  fs.mkdirSync(binRoot, { recursive: true });
  fs.writeFileSync(
    path.join(repoFixtureRoot, "runtime", "sidecar", "Cargo.toml"),
    '[package]\nname = "qintopia-message-sidecar"\nversion = "0.0.0"\n',
    "utf8"
  );

  writeExecutable(
    path.join(binRoot, "git"),
    [
      "#!/usr/bin/env node",
      "const args = process.argv.slice(2);",
      'if (args[0] === "status" && args[1] === "--porcelain") {',
      '  process.stdout.write(process.env.FAKE_GIT_STATUS ?? "");',
      "  process.exit(0);",
      "}",
      'if (args[0] === "rev-parse" && args[1] === "HEAD") {',
      '  process.stdout.write("0123456789abcdef0123456789abcdef01234567\\n");',
      "  process.exit(0);",
      "}",
      'if (args[0] === "branch" && args[1] === "--show-current") {',
      '  process.stdout.write("codex/test-qiwe-production-builder\\n");',
      "  process.exit(0);",
      "}",
      'process.stderr.write(`unexpected git args: ${args.join(" ")}\\n`);',
      "process.exit(1);",
      "",
    ].join("\n")
  );

  writeExecutable(
    path.join(binRoot, "cargo"),
    [
      "#!/usr/bin/env node",
      "const fs = require('node:fs');",
      "const path = require('node:path');",
      "const args = process.argv.slice(2);",
      'if (args.length === 1 && args[0] === "--version") {',
      '  process.stdout.write("cargo 1.81.0\\n");',
      "  process.exit(0);",
      "}",
      'if (args[0] !== "build") {',
      '  process.stderr.write(`unexpected cargo args: ${args.join(" ")}\\n`);',
      "  process.exit(1);",
      "}",
      "const featureIndex = args.indexOf('--features');",
      "if (featureIndex === -1 || args[featureIndex + 1] !== 'qiwe-production-adapter') {",
      '  process.stderr.write("builder must compile exactly qiwe-production-adapter\\n");',
      "  process.exit(1);",
      "}",
      "const binaryPath = path.join(process.cwd(), 'runtime', 'sidecar', 'target', 'release', 'qintopia-message-sidecar');",
      "fs.mkdirSync(path.dirname(binaryPath), { recursive: true });",
      'fs.writeFileSync(binaryPath, "fake qiwe production sidecar\\n");',
      "process.exit(0);",
      "",
    ].join("\n")
  );

  writeExecutable(
    path.join(binRoot, "rustc"),
    '#!/usr/bin/env node\nprocess.stdout.write("rustc 1.81.0\\n");\n'
  );

  writeExecutable(
    path.join(binRoot, "tar"),
    [
      "#!/usr/bin/env node",
      "const fs = require('node:fs');",
      "const path = require('node:path');",
      "const args = process.argv.slice(2);",
      "const chdirIndex = args.indexOf('-C');",
      "const outputIndex = args.indexOf('-czf');",
      "if (chdirIndex === -1 || outputIndex === -1) {",
      '  process.stderr.write(`unexpected tar args: ${args.join(" ")}\\n`);',
      "  process.exit(1);",
      "}",
      "const artifactDir = args[chdirIndex + 1];",
      "const bundlePath = args[outputIndex + 1];",
      "const binaryName = args[outputIndex + 2];",
      "const binaryPath = path.join(artifactDir, binaryName);",
      "if (!fs.existsSync(binaryPath)) {",
      '  process.stderr.write("binary missing before bundle creation\\n");',
      "  process.exit(1);",
      "}",
      "fs.writeFileSync(bundlePath, `bundle:${binaryName}\\n`);",
      "process.exit(0);",
      "",
    ].join("\n")
  );

  return { repoRoot: repoFixtureRoot, binRoot };
}

function writeExecutable(filePath, contents) {
  fs.writeFileSync(filePath, contents, "utf8");
  fs.chmodSync(filePath, 0o755);
}
