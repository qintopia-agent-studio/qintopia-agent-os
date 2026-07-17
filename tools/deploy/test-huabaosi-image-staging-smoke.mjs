#!/usr/bin/env node

import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const smokePath = path.join(
  repoRoot,
  "deploy/sidecar/scripts/huabaosi-image-generation-staging-smoke.sh"
);
const evidenceChecker = path.join(
  repoRoot,
  "tools/deploy/check-huabaosi-image-staging-evidence.mjs"
);
const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), "huabaosi-staging-smoke-"));
const workItemId = "11111111-2222-4333-8444-555555555555";
const databaseUrl =
  "postgres://qintopia:staging-secret@127.0.0.1:55432/qintopia_staging";
const databaseHash = crypto
  .createHash("sha256")
  .update(databaseUrl, "utf8")
  .digest("hex");
const markerPath = path.join(tempRoot, "env-executed");
const commandLogPath = path.join(tempRoot, "commands.log");
const fakeSidecarPath = path.join(tempRoot, "fake-sidecar.sh");
const httpStorageSidecarPath = path.join(tempRoot, "http-storage-sidecar.sh");
const leakingSidecarPath = path.join(tempRoot, "leaking-sidecar.sh");
const stagingEnvPath = path.join(tempRoot, "message-sidecar-staging.env");
const feishuEnvLine = (suffix, value) =>
  `QINTOPIA_HUABAOSI_${"FEISHU"}_${suffix}=${value}`;

const writeFile = (filePath, content, mode) => {
  fs.writeFileSync(filePath, content, mode === undefined ? undefined : { mode });
};

writeFile(
  fakeSidecarPath,
  `#!/usr/bin/env bash
set -euo pipefail
printf '%s\\n' "$*" >>"${commandLogPath}"
if [[ -n "\${QINTOPIA_UNRELATED_RUNTIME_SECRET:-}" || -n "\${QIWE_TOKEN:-}" ]]; then
  echo "ambient secret reached child process" >&2
  exit 23
fi
case "$1" in
  huabaosi-image-generation-preflight)
    printf '%s\\n' '{"success":true,"worker":"huabaosi-image-generation-worker","action_status":"adapter_config_ready","generation_enabled":true,"adapter_compiled":true,"config_valid":true,"missing_configuration":[],"safe_for_chat":false}'
    ;;
  run-huabaosi-image-generation-worker)
    artifact_uri="\${QINTOPIA_FAKE_ARTIFACT_URI:-feishu-base://huabaosi-generated-image/22222222-3333-4444-8555-666666666666}"
    printf '{"success":true,"worker":"huabaosi-image-generation-worker","dry_run":false,"apply_requested":true,"action_status":"generated_image_created","artifact_ids":["22222222-3333-4444-8555-666666666666"],"artifact_preview":{"artifact_type":"generated_image","review_status":"pending","mime_type":"image/jpeg","width":1024,"height":1024,"byte_size":123456,"content_hash":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","artifact_uri":"%s"},"safe_for_chat":false}\\n' "$artifact_uri"
    ;;
  *)
    echo "unexpected command: $*" >&2
    exit 99
    ;;
esac
`,
  0o700
);

writeFile(
  leakingSidecarPath,
  `#!/usr/bin/env bash
set -euo pipefail
case "$1" in
  huabaosi-image-generation-preflight)
    printf '{"success":true,"leak":"%s"}\\n' "$QINTOPIA_SIDECAR_DATABASE_URL"
    ;;
  *)
    printf '%s\\n' '{"success":true}'
    ;;
esac
`,
  0o700
);

writeFile(
  httpStorageSidecarPath,
  `#!/usr/bin/env bash
set -euo pipefail
case "$1" in
  huabaosi-image-generation-preflight)
    printf '%s\\n' '{"success":true,"worker":"huabaosi-image-generation-worker","action_status":"adapter_config_ready","generation_enabled":true,"adapter_compiled":true,"config_valid":true,"missing_configuration":[],"safe_for_chat":false}'
    ;;
  run-huabaosi-image-generation-worker)
    printf '%s\\n' '{"success":true,"worker":"huabaosi-image-generation-worker","dry_run":false,"apply_requested":true,"action_status":"generated_image_created","artifact_ids":["22222222-3333-4444-8555-666666666666"],"artifact_preview":{"artifact_type":"generated_image","review_status":"pending","mime_type":"image/jpeg","width":1024,"height":1024,"byte_size":123456,"content_hash":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","artifact_uri":"https://media.example/generated/22222222-3333-4444-8555-666666666666.jpg"},"safe_for_chat":false}'
    ;;
esac
`,
  0o700
);

const envContent = () => `
QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED=1
QINTOPIA_SIDECAR_DATABASE_URL=${databaseUrl}
QINTOPIA_HUABAOSI_IMAGE_PROVIDER=openai-compatible
QINTOPIA_HUABAOSI_IMAGE_MODEL=gpt-image-2
QINTOPIA_HUABAOSI_IMAGE_API_BASE_URL=https://image.example/v1
QINTOPIA_HUABAOSI_IMAGE_API_KEY="$(touch ${markerPath})"
QINTOPIA_HUABAOSI_IMAGE_STORAGE_BACKEND=feishu-base
QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED=1
QINTOPIA_HUABAOSI_FEISHU_MIRROR_APPROVAL=approved-huabaosi-feishu-artifact-mirror
QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA=0123456789abcdef0123456789abcdef01234567
QINTOPIA_DEPLOYED_COMMIT_SHA=0123456789abcdef0123456789abcdef01234567
QINTOPIA_HUABAOSI_FEISHU_DATABASE_URL_SHA256=${databaseHash}
${feishuEnvLine("BASE_TOKEN", "baseTokenFixture")}
${feishuEnvLine("ALLOWED_BASE_TOKENS", "baseTokenFixture")}
${feishuEnvLine("ARTIFACT_TABLE_ID", "tblFixture")}
${feishuEnvLine("ALLOWED_ARTIFACT_TABLE_IDS", "tblFixture")}
QINTOPIA_HUABAOSI_FEISHU_PROFILE_ENV_PATH=/home/ubuntu/.hermes/profiles/huabaosi/.env
QINTOPIA_HUABAOSI_FEISHU_SCHEMA_VERSION=huabaosi-generated-image-v1
QINTOPIA_HUABAOSI_MEDIA_MAX_BYTES=10485760
QINTOPIA_QIWE_IMAGE_SEND_ENABLED=1
QINTOPIA_QIWE_IMAGE_SEND_WEBHOOK_READY=1
QIWE_API_URL=https://qiwe.example
QIWE_TOKEN=staging-qiwe-token-must-not-reach-huabaosi-child
QIWE_GUID=staging-qiwe-guid-must-not-reach-huabaosi-child
QINTOPIA_QIWE_IMAGE_SEND_ALLOWED_HOSTS=qiwe.example
QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS=staging-group
`;

writeFile(stagingEnvPath, envContent());

const runSmoke = (overrides = {}) =>
  spawnSync("bash", [smokePath], {
    cwd: repoRoot,
    env: {
      ...process.env,
      QINTOPIA_HUABAOSI_IMAGE_STAGING_SMOKE_ENABLE: "1",
      QINTOPIA_HUABAOSI_IMAGE_STAGING_APPROVAL: "approved-staging-image-generation",
      QINTOPIA_HUABAOSI_IMAGE_STAGING_ENV_FILE: stagingEnvPath,
      QINTOPIA_HUABAOSI_IMAGE_STAGING_WORK_ITEM_ID: workItemId,
      QINTOPIA_HUABAOSI_IMAGE_STAGING_DATABASE_URL_SHA256: databaseHash,
      QINTOPIA_SIDECAR_BIN: fakeSidecarPath,
      QINTOPIA_UNRELATED_RUNTIME_SECRET: "ambient-unrelated-secret",
      QIWE_TOKEN: "ambient-qiwe-secret",
      ...overrides,
    },
    encoding: "utf8",
  });

let result = runSmoke();
assert.equal(result.status, 0, result.stderr);
assert.match(
  result.stdout,
  /Huabaosi image staging smoke passed: one generated_image remains pending human review; Feishu Base stored the final JPEG/
);
assert.equal(result.stderr, "");
assert.match(result.stdout, /huabaosi_image_generation_staging_evidence=/);
assert.doesNotMatch(result.stdout, /artifact_uri/);
assert.doesNotMatch(result.stdout, /feishu-base:\/\/huabaosi-generated-image/);
assert.equal(fs.existsSync(markerPath), false, "env file command was executed");
assert.deepEqual(fs.readFileSync(commandLogPath, "utf8").trim().split("\n"), [
  "huabaosi-image-generation-preflight",
  `run-huabaosi-image-generation-worker --once --work-item-id ${workItemId} --apply`,
]);
const evidenceFile = path.join(tempRoot, "huabaosi-staging-evidence.txt");
fs.writeFileSync(evidenceFile, result.stdout, "utf8");
const evidenceCheck = spawnSync("node", [evidenceChecker, evidenceFile], {
  cwd: repoRoot,
  encoding: "utf8",
});
assert.equal(evidenceCheck.status, 0, evidenceCheck.stderr);

const rawEvidenceFile = path.join(tempRoot, "raw-huabaosi-staging-evidence.txt");
fs.writeFileSync(
  rawEvidenceFile,
  `${result.stdout}\n{"artifact_uri":"feishu-base://huabaosi-generated-image/22222222-3333-4444-8555-666666666666"}\n`,
  "utf8"
);
const rawEvidenceCheck = spawnSync("node", [evidenceChecker, rawEvidenceFile], {
  cwd: repoRoot,
  encoding: "utf8",
});
assert.notEqual(rawEvidenceCheck.status, 0);

fs.rmSync(commandLogPath, { force: true });
result = runSmoke({
  QINTOPIA_HUABAOSI_IMAGE_STAGING_DATABASE_URL_SHA256: "0".repeat(64),
});
assert.notEqual(result.status, 0);
assert.match(
  result.stderr,
  /staging database URL hash does not match the approved command/
);
assert.equal(fs.existsSync(commandLogPath), false);

writeFile(
  stagingEnvPath,
  envContent().replace(
    `QINTOPIA_HUABAOSI_FEISHU_DATABASE_URL_SHA256=${databaseHash}`,
    `QINTOPIA_HUABAOSI_FEISHU_DATABASE_URL_SHA256=${"0".repeat(64)}`
  )
);
result = runSmoke();
assert.notEqual(result.status, 0);
assert.match(
  result.stderr,
  /QINTOPIA_HUABAOSI_FEISHU_DATABASE_URL_SHA256 must match the approved staging database hash/
);

writeFile(
  stagingEnvPath,
  `${envContent()}
UNSUPPORTED_ENV=value
`
);
result = runSmoke();
assert.notEqual(result.status, 0);
assert.match(result.stderr, /staging env contains an unsupported key/);

writeFile(stagingEnvPath, envContent());
result = runSmoke({ QINTOPIA_SIDECAR_BIN: leakingSidecarPath });
assert.notEqual(result.status, 0);
assert.match(result.stderr, /contains forbidden sensitive output/);

result = runSmoke({ QINTOPIA_SIDECAR_BIN: httpStorageSidecarPath });
assert.notEqual(result.status, 0);
assert.doesNotMatch(result.stdout, /Feishu Base stored the final JPEG/);

fs.rmSync(tempRoot, { recursive: true, force: true });

console.log("Huabaosi image staging smoke test passed.");
