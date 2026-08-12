#!/usr/bin/env node

import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const script = path.join(
  repoRoot,
  "deploy/sidecar/scripts/apply-xiaoman-creative-profile-candidates-production.sh"
);

const source = fs.readFileSync(script, "utf8");
for (const fragment of [
  'APPROVAL="approved-production-xiaoman-creative-profile-candidates"',
  'ENV_FILE="/etc/qintopia/message-sidecar.env"',
  'RELEASE_CURRENT="/home/ubuntu/qintopia-agent-os-releases/current"',
  'PAYLOAD_FILE="/home/ubuntu/.local/state/qintopia-agentos/xiaoman-creative-profile-candidates/reviewed-payload.json"',
  "reviewed payload SHA-256 mismatch",
  '--approval "$APPROVAL"',
  "allowed = {",
  "QINTOPIA_SIDECAR_DATABASE_URL",
  "QINTOPIA_MESSAGE_STORE_DATABASE_URL",
]) {
  assert.match(source, new RegExp(fragment.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
}
for (const forbidden of [
  "$1",
  '. "$ENV_FILE"',
  'source "$ENV_FILE"',
  "set -a",
  "eval ",
  "curl ",
  "ssh ",
  "QINTOPIA_XIAOMAN_CREATIVE_PROFILE_CANDIDATES_PAYLOAD_FILE",
]) {
  assert.ok(!source.includes(forbidden), `forbidden fragment ${forbidden}`);
}

const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "qintopia-creative-profile-apply-")
);
try {
  const fixture = path.join(tmpRoot, "script.sh");
  const fakeRoot = path.join(tmpRoot, "fake-root");
  const envFile = path.join(fakeRoot, "etc/qintopia/message-sidecar.env");
  const releaseCurrent = path.join(fakeRoot, "release/current");
  const payloadFile = path.join(
    fakeRoot,
    "state/xiaoman-creative-profile-candidates/reviewed-payload.json"
  );
  const fakePython = path.join(tmpRoot, "python3");
  const commandLog = path.join(tmpRoot, "command.log");
  fs.mkdirSync(path.dirname(envFile), { recursive: true });
  fs.mkdirSync(path.join(releaseCurrent, "workflows/xiaoman-daily-case-report"), {
    recursive: true,
  });
  fs.mkdirSync(path.dirname(payloadFile), { recursive: true });
  fs.writeFileSync(
    envFile,
    "QINTOPIA_SIDECAR_DATABASE_URL=postgresql://unit\n",
    "utf8"
  );
  fs.writeFileSync(
    path.join(
      releaseCurrent,
      "workflows/xiaoman-daily-case-report/apply_creative_profile_candidates.py"
    ),
    "# fixture\n",
    "utf8"
  );
  fs.writeFileSync(payloadFile, '{"schema_version":1}\n', "utf8");
  fs.writeFileSync(
    fakePython,
    `#!/usr/bin/env bash\nprintf '%s\\n' "$*" > ${JSON.stringify(commandLog)}\nprintf '{"success":true}\\n'\n`,
    { mode: 0o755 }
  );
  fs.writeFileSync(
    fixture,
    source
      .replace('ENV_FILE="/etc/qintopia/message-sidecar.env"', `ENV_FILE="${envFile}"`)
      .replace(
        'RELEASE_CURRENT="/home/ubuntu/qintopia-agent-os-releases/current"',
        `RELEASE_CURRENT="${releaseCurrent}"`
      )
      .replace(
        'PAYLOAD_FILE="/home/ubuntu/.local/state/qintopia-agentos/xiaoman-creative-profile-candidates/reviewed-payload.json"',
        `PAYLOAD_FILE="${payloadFile}"`
      )
      .replace('PYTHON_BIN="/usr/bin/python3"', `PYTHON_BIN="${fakePython}"`),
    { mode: 0o755 }
  );

  let result = spawnSync("bash", [fixture], {
    env: {
      ...process.env,
      QINTOPIA_XIAOMAN_CREATIVE_PROFILE_CANDIDATES_APPLY:
        "approved-production-xiaoman-creative-profile-candidates",
      QINTOPIA_XIAOMAN_CREATIVE_PROFILE_CANDIDATES_PAYLOAD_SHA256: crypto
        .createHash("sha256")
        .update(fs.readFileSync(payloadFile))
        .digest("hex"),
    },
    encoding: "utf8",
  });
  assert.equal(result.status, 0, result.stderr);
  assert.match(
    fs.readFileSync(commandLog, "utf8"),
    /apply_creative_profile_candidates\.py --payload-json .*reviewed-payload\.json --apply --approval approved-production-xiaoman-creative-profile-candidates/
  );

  result = spawnSync("bash", [fixture], {
    env: {
      ...process.env,
      QINTOPIA_XIAOMAN_CREATIVE_PROFILE_CANDIDATES_APPLY:
        "approved-production-xiaoman-creative-profile-candidates",
      QINTOPIA_XIAOMAN_CREATIVE_PROFILE_CANDIDATES_PAYLOAD_SHA256: "0".repeat(64),
    },
    encoding: "utf8",
  });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /reviewed payload SHA-256 mismatch/);

  result = spawnSync("bash", [fixture], {
    env: {
      ...process.env,
      QINTOPIA_XIAOMAN_CREATIVE_PROFILE_CANDIDATES_PAYLOAD_SHA256: crypto
        .createHash("sha256")
        .update(fs.readFileSync(payloadFile))
        .digest("hex"),
    },
    encoding: "utf8",
  });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /explicit owner approval/);

  console.log("Xiaoman creative-profile candidates production apply test passed.");
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}
