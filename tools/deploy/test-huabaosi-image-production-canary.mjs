#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const script = path.join(
  repoRoot,
  "deploy/sidecar/scripts/huabaosi-image-generation-production-canary-smoke.sh"
);
const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "qintopia-aliang-canary-"));
const envFile = path.join(tmpRoot, "message-sidecar.env");
const commandLog = path.join(tmpRoot, "commands.log");
const systemctl = path.join(tmpRoot, "systemctl");
const productionRoot = path.join(
  fs.realpathSync(tmpRoot),
  "qintopia-agent-os-releases"
);
const briefId = "11111111-2222-4333-8444-555555555555";
const briefWorkItemId = "44444444-5555-4666-8777-888888888888";
const imageWorkItemId = "22222222-3333-4444-8555-666666666666";
const generatedArtifactId = "33333333-4444-4555-8666-777777777777";
const releaseSha = "0123456789abcdef0123456789abcdef01234567";
const contentHash = `sha256:${"a".repeat(64)}`;
const databaseUrl = "postgresql://prod-user:database-secret@db.example.test/qintopia";
const databaseHash = crypto.createHash("sha256").update(databaseUrl).digest("hex");
const secretValues = [
  databaseUrl,
  "https://provider.example.test/v1/",
  "provider-secret-key",
  "base-token-secret",
  "table-secret",
  "/etc/qintopia/huabaosi-profile.env",
];

const sha256File = (filePath) =>
  crypto.createHash("sha256").update(fs.readFileSync(filePath)).digest("hex");

const writeExecutable = (filePath, content) => {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content, "utf8");
  fs.chmodSync(filePath, 0o755);
};
const envLine = (key, value) => `${key}=${value}`;

const writeEnv = (extra = "", reviewerIds = "owner, trainer") => {
  fs.writeFileSync(
    envFile,
    [
      `QINTOPIA_SIDECAR_DATABASE_URL=${databaseUrl}`,
      `QINTOPIA_OPERATIONS_ALLOWED_REVIEWER_IDS=${reviewerIds}`,
      "QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED=1",
      "QINTOPIA_HUABAOSI_IMAGE_PROVIDER=openai-compatible",
      "QINTOPIA_HUABAOSI_IMAGE_MODEL=gpt-image-2",
      "QINTOPIA_HUABAOSI_IMAGE_API_BASE_URL=https://provider.example.test/v1/",
      "QINTOPIA_HUABAOSI_IMAGE_API_KEY=provider-secret-key",
      "QINTOPIA_HUABAOSI_IMAGE_HTTP_TIMEOUT_SECONDS=180",
      "QINTOPIA_HUABAOSI_IMAGE_STORAGE_BACKEND=feishu-base",
      "QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS=open.feishu.cn",
      "QINTOPIA_HUABAOSI_MEDIA_MAX_BYTES=10485760",
      "QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED=1",
      "QINTOPIA_HUABAOSI_FEISHU_MIRROR_APPROVAL=approved-huabaosi-feishu-artifact-mirror",
      envLine("QINTOPIA_HUABAOSI_FEISHU_BASE_TOKEN", "base-token-secret"),
      envLine("QINTOPIA_HUABAOSI_FEISHU_ALLOWED_BASE_TOKENS", "base-token-secret"),
      envLine("QINTOPIA_HUABAOSI_FEISHU_ARTIFACT_TABLE_ID", "table-secret"),
      envLine("QINTOPIA_HUABAOSI_FEISHU_ALLOWED_ARTIFACT_TABLE_IDS", "table-secret"),
      "QINTOPIA_HUABAOSI_FEISHU_PROFILE_ENV_PATH=/etc/qintopia/huabaosi-profile.env",
      "QINTOPIA_HUABAOSI_FEISHU_SCHEMA_VERSION=huabaosi-generated-image-v1",
      "UNRELATED_RUNTIME_KEY=ignored-without-evaluation",
      extra,
      "",
    ].join("\n"),
    "utf8"
  );
  fs.chmodSync(envFile, 0o600);
};

const fakeSidecar = (
  name,
  { mismatch = false, leak = false, starterParentMismatch = false } = {}
) => {
  const filePath = path.join(tmpRoot, name);
  const revalidatedHash = mismatch ? `sha256:${"b".repeat(64)}` : contentHash;
  const preflightExtra = leak ? ',"unexpected":"provider-secret-key"' : "";
  const starterParentWorkItemId = starterParentMismatch
    ? "55555555-6666-4777-8888-999999999999"
    : briefWorkItemId;
  writeExecutable(
    filePath,
    `#!/usr/bin/env bash
set -euo pipefail
if [[ -n "\${QIWE_TOKEN:-}" ]]; then
  echo "ambient QiWe credential reached Huabaosi child" >&2
  exit 65
fi
printf '%s\n' "$*" >> ${JSON.stringify(commandLog)}
case "\${1:-}" in
  huabaosi-image-generation-preflight)
    printf '%s\n' '${JSON.stringify({
      success: true,
      worker: "huabaosi-image-generation-worker",
      action_status: "adapter_config_ready",
      generation_enabled: true,
      adapter_compiled: true,
      adapter_mode: "production",
      config_valid: true,
      missing_configuration: [],
      safe_for_chat: false,
    }).replace(/}$/, "")}${preflightExtra}}'
    ;;
  operations-artifact-review-decision)
    printf '%s\n' '${JSON.stringify({
      success: true,
      dry_run: false,
      apply_requested: true,
      action_status: "review_recorded",
      artifact_id: briefId,
      work_item_id: briefWorkItemId,
      artifact_type: "poster_brief",
      previous_review_status: "pending",
      review_status: "approved",
      reviewer_id: "trainer",
      reason_required: true,
    })}'
    ;;
  run-xiaoman-activity-image-generation-starter-worker)
    printf '%s\n' '${JSON.stringify({
      success: true,
      action_status: "image_generation_requests_created",
      requested_work_item_id: briefWorkItemId,
      created_count: 1,
      existing_count: 0,
      work_items: [
        {
          existing: false,
          work_item_type: "image_generation_request",
          capability_key: "huabaosi.generate_image_asset",
          parent_work_item_id: starterParentWorkItemId,
          work_item_id: imageWorkItemId,
        },
      ],
    })}'
    ;;
  run-huabaosi-image-generation-worker)
    printf '%s\n' '${JSON.stringify({
      success: true,
      action_status: "generated_image_created",
      dry_run: false,
      apply_requested: true,
      work_item_id: imageWorkItemId,
      artifact_ids: [generatedArtifactId],
      artifact_preview: {
        artifact_type: "generated_image",
        review_status: "pending",
        mime_type: "image/jpeg",
        artifact_uri: `feishu-base://huabaosi-generated-image/${generatedArtifactId}`,
        width: 1024,
        height: 1024,
        byte_size: 123456,
        content_hash: contentHash,
      },
    })}'
    ;;
  huabaosi-feishu-primary-storage-revalidate)
    printf '%s\n' '${JSON.stringify({
      success: true,
      worker: "huabaosi-feishu-artifact-mirror-worker",
      action_status: "feishu_primary_storage_revalidated",
      artifact_id: generatedArtifactId,
      work_item_id: imageWorkItemId,
      schema_version: "huabaosi-generated-image-v1",
      content_hash: revalidatedHash,
      byte_size: 123456,
      width: 1024,
      height: 1024,
      external_calls_executed: true,
      database_writes_executed: false,
      sensitive_fields_redacted: true,
    })}'
    ;;
  *) exit 64 ;;
esac
`
  );
  return filePath;
};

const run = (sidecar, extraEnv = {}) =>
  spawnSync("bash", [script], {
    cwd: repoRoot,
    env: {
      ...process.env,
      QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_ENABLE: "1",
      QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_APPROVAL:
        "approved-production-image-generation-canary",
      QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_BRIEF_ARTIFACT_ID: briefId,
      QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_DATABASE_URL_SHA256: databaseHash,
      QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_RELEASE_SHA: releaseSha,
      QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_SIDECAR_SHA256: sha256File(sidecar),
      QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_TEST_MODE: "1",
      QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_ENV_FILE: envFile,
      QINTOPIA_SIDECAR_BIN: sidecar,
      SYSTEMCTL: systemctl,
      QIWE_TOKEN: "ambient-qiwe-token-must-not-reach-child",
      ...extraEnv,
    },
    encoding: "utf8",
  });

const productionRootFixture = () => {
  const fixturePath = path.join(
    productionRoot,
    releaseSha,
    "deploy/sidecar/scripts/huabaosi-image-generation-production-canary-smoke.sh"
  );
  writeExecutable(
    fixturePath,
    fs
      .readFileSync(script, "utf8")
      .replaceAll(
        'PRODUCTION_RELEASE_PARENT="/home/ubuntu/qintopia-agent-os-releases"',
        `PRODUCTION_RELEASE_PARENT="${productionRoot}"`
      )
  );
  return fixturePath;
};

const assertNoSecrets = (output) => {
  for (const secret of secretValues) {
    if (output.includes(secret)) {
      throw new Error(`production canary output leaked a sensitive value: ${secret}`);
    }
  }
};

try {
  fs.chmodSync(tmpRoot, 0o755);
  writeEnv();
  writeExecutable(
    systemctl,
    `#!/usr/bin/env bash
set -euo pipefail
case "\${1:-}" in
  cat) exit 0 ;;
  is-enabled)
    case "\${FAKE_TIMER_ENABLED_STATE:-disabled}" in
      disabled) printf '%s\n' disabled; exit 1 ;;
      enabled) printf '%s\n' enabled; exit 0 ;;
      masked) printf '%s\n' masked; exit 1 ;;
      static) printf '%s\n' static; exit 0 ;;
      *) exit 64 ;;
    esac
    ;;
  is-active)
    [[ "\${FAKE_TIMER_ACTIVE:-0}" == "1" ]] && exit 0
    exit 3
    ;;
  *) exit 64 ;;
esac
`
  );

  const sidecar = fakeSidecar("sidecar-success");
  const source = fs.readFileSync(script, "utf8");
  if (!source.includes('PATH="/usr/bin:/bin:/usr/sbin:/sbin"')) {
    throw new Error("production canary must reset PATH in production mode");
  }
  if (!source.includes('SYSTEMCTL="/usr/bin/systemctl"')) {
    throw new Error("production canary must use the fixed systemctl path");
  }
  if (!source.includes("test mode is forbidden from production release roots")) {
    throw new Error("production canary must forbid test mode in release roots");
  }
  if (!source.includes("test mode may execute only a temporary fake sidecar")) {
    throw new Error("production canary must restrict test sidecar path");
  }

  const success = run(sidecar);
  if (success.status !== 0) {
    throw new Error(`production canary success fixture failed\n${success.stderr}`);
  }
  assertNoSecrets(`${success.stdout}\n${success.stderr}`);
  const evidence = success.stdout
    .split("\n")
    .filter((line) =>
      line.startsWith("huabaosi_image_generation_production_canary_evidence=")
    )
    .map((line) => JSON.parse(line.split("=", 2)[1]));
  const phases = evidence.map((record) => record.phase);
  if (
    JSON.stringify(phases) !==
    JSON.stringify([
      "preflight",
      "brief_review",
      "request_intake",
      "generation",
      "revalidation",
    ])
  ) {
    throw new Error(`unexpected production canary phases: ${phases.join(",")}`);
  }
  if (
    evidence[3].artifact_id !== generatedArtifactId ||
    evidence[4].artifact_id !== generatedArtifactId ||
    evidence[3].content_hash !== contentHash ||
    evidence[4].content_hash !== contentHash ||
    evidence[3].review_status !== "pending" ||
    evidence[1].brief_work_item_id !== briefWorkItemId ||
    evidence[2].brief_work_item_id !== briefWorkItemId ||
    evidence[4].database_writes_executed !== false
  ) {
    throw new Error("production canary evidence did not preserve artifact identity");
  }
  if (!success.stdout.includes("one Feishu-backed JPEG remains pending human review")) {
    throw new Error("production canary did not report the retained human review gate");
  }

  const commands = fs.readFileSync(commandLog, "utf8").trim().split("\n");
  if (commands.length !== 5) {
    throw new Error(`expected five sidecar commands, got ${commands.length}`);
  }
  if (
    !commands[1].includes("operations-artifact-review-decision --apply") ||
    !commands[1].includes('"reviewer_id":"trainer"') ||
    !commands[1].includes('"expected_artifact_type":"poster_brief"') ||
    !commands[1].includes('"expected_review_status":"pending"') ||
    !commands[2].includes(`--work-item-id ${briefWorkItemId}`) ||
    !commands[3].includes(`--work-item-id ${imageWorkItemId}`) ||
    !commands[4].includes(`--artifact-id ${generatedArtifactId}`) ||
    commands.some((command) => /enable|publish|qiwe|send/i.test(command))
  ) {
    throw new Error(`unexpected production canary commands: ${commands.join(" | ")}`);
  }

  const activeTimer = run(sidecar, { FAKE_TIMER_ACTIVE: "1" });
  if (
    activeTimer.status === 0 ||
    !activeTimer.stderr.includes("timer must be inactive during one-shot canary")
  ) {
    throw new Error("active provider timer must block one-shot production canary");
  }

  const enabledTimer = run(sidecar, { FAKE_TIMER_ENABLED_STATE: "enabled" });
  if (
    enabledTimer.status === 0 ||
    !enabledTimer.stderr.includes("timer must be disabled during one-shot canary")
  ) {
    throw new Error("enabled provider timer must block one-shot production canary");
  }

  const maskedTimer = run(sidecar, { FAKE_TIMER_ENABLED_STATE: "masked" });
  if (
    maskedTimer.status === 0 ||
    !maskedTimer.stderr.includes("timer must be disabled during one-shot canary")
  ) {
    throw new Error("masked provider timer must block one-shot production canary");
  }

  const staticTimer = run(sidecar, { FAKE_TIMER_ENABLED_STATE: "static" });
  if (
    staticTimer.status === 0 ||
    !staticTimer.stderr.includes("timer must be disabled during one-shot canary")
  ) {
    throw new Error("static provider timer must block one-shot production canary");
  }

  const releaseRootFixture = productionRootFixture();
  const testModeInProductionRoot = spawnSync("bash", [releaseRootFixture], {
    cwd: repoRoot,
    env: {
      ...process.env,
      QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_ENABLE: "1",
      QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_APPROVAL:
        "approved-production-image-generation-canary",
      QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_BRIEF_ARTIFACT_ID: briefId,
      QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_DATABASE_URL_SHA256: databaseHash,
      QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_RELEASE_SHA: releaseSha,
      QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_SIDECAR_SHA256: sha256File(sidecar),
      QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_TEST_MODE: "1",
      QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_ENV_FILE: envFile,
      QINTOPIA_SIDECAR_BIN: sidecar,
      SYSTEMCTL: systemctl,
    },
    encoding: "utf8",
  });
  if (
    testModeInProductionRoot.status === 0 ||
    !testModeInProductionRoot.stderr.includes(
      "test mode is forbidden from production release roots"
    )
  ) {
    throw new Error("test mode must be rejected from production release roots");
  }

  const nonTemporarySidecar = run("/usr/bin/true");
  if (
    nonTemporarySidecar.status === 0 ||
    !nonTemporarySidecar.stderr.includes(
      "test mode may execute only a temporary fake sidecar"
    )
  ) {
    throw new Error("test mode must reject non-temporary sidecar paths");
  }

  const sidecarSymlink = path.join(tmpRoot, "sidecar-symlink");
  fs.symlinkSync("/usr/bin/true", sidecarSymlink);
  const symlinkSidecar = run(sidecarSymlink);
  if (
    symlinkSidecar.status === 0 ||
    !symlinkSidecar.stderr.includes(
      "test mode may execute only a temporary fake sidecar"
    )
  ) {
    throw new Error("test mode must reject symlink sidecar paths");
  }

  const envSymlink = path.join(tmpRoot, "message-sidecar-link.env");
  fs.symlinkSync(envFile, envSymlink);
  const symlinkEnv = run(sidecar, {
    QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_ENV_FILE: envSymlink,
  });
  if (
    symlinkEnv.status === 0 ||
    !symlinkEnv.stderr.includes("test mode may read only a temporary fake env file")
  ) {
    throw new Error("test mode must reject symlink env file paths");
  }

  const wrongApproval = run(sidecar, {
    QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_APPROVAL: "not-approved",
  });
  if (
    wrongApproval.status === 0 ||
    !wrongApproval.stderr.includes("explicit owner approval")
  ) {
    throw new Error("wrong owner approval must block production canary");
  }

  const wrongDigest = run(sidecar, {
    QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_SIDECAR_SHA256: "f".repeat(64),
  });
  if (
    wrongDigest.status === 0 ||
    !wrongDigest.stderr.includes("sidecar hash does not match")
  ) {
    throw new Error("wrong sidecar digest must block production canary");
  }

  const invalidBriefId = run(sidecar, {
    QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_BRIEF_ARTIFACT_ID: "not-a-uuid",
  });
  if (
    invalidBriefId.status === 0 ||
    !invalidBriefId.stderr.includes("brief artifact id must be a UUID")
  ) {
    throw new Error("invalid production canary brief UUID must fail closed");
  }

  writeEnv("", "owner");
  const missingReviewer = run(sidecar);
  if (
    missingReviewer.status === 0 ||
    !missingReviewer.stderr.includes(
      "trainer is not in the production reviewer allowlist"
    )
  ) {
    throw new Error("missing trainer reviewer allowlist entry must fail closed");
  }
  writeEnv();

  writeEnv("QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED=1");
  const duplicate = run(sidecar);
  if (duplicate.status === 0 || !duplicate.stderr.includes("duplicate canary key")) {
    throw new Error("duplicate production canary env key must fail closed");
  }
  writeEnv();

  const mismatchSidecar = fakeSidecar("sidecar-mismatch", { mismatch: true });
  fs.writeFileSync(commandLog, "", "utf8");
  const mismatch = run(mismatchSidecar);
  if (
    mismatch.status === 0 ||
    !mismatch.stderr.includes("revalidation returned an invalid report") ||
    mismatch.stdout.includes("production canary passed")
  ) {
    throw new Error("revalidation identity mismatch must block canary completion");
  }
  assertNoSecrets(`${mismatch.stdout}\n${mismatch.stderr}`);

  const mismatchedParentSidecar = fakeSidecar("sidecar-parent-mismatch", {
    starterParentMismatch: true,
  });
  fs.writeFileSync(commandLog, "", "utf8");
  const mismatchedParent = run(mismatchedParentSidecar);
  if (
    mismatchedParent.status === 0 ||
    !mismatchedParent.stderr.includes(
      "image request intake returned an invalid report"
    ) ||
    fs.readFileSync(commandLog, "utf8").includes("run-huabaosi-image-generation-worker")
  ) {
    throw new Error("starter parent work item mismatch must fail before generation");
  }

  const leakSidecar = fakeSidecar("sidecar-leak", { leak: true });
  const leak = run(leakSidecar);
  if (leak.status === 0 || !leak.stderr.includes("contains sensitive output")) {
    throw new Error("sensitive child output must block production canary");
  }
  assertNoSecrets(leak.stderr);
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Huabaosi image production canary test passed.");
