#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.mkdtempSync(
  path.join(os.tmpdir(), "qintopia-qiwe-webhook-ingress-test-")
);
const sha = "1234567890abcdef1234567890abcdef12345678";
const publicToken = crypto
  .createHash("sha384")
  .update("qintopia-ingress-public-test-fixture")
  .digest("base64url");
const internalToken = crypto
  .createHash("sha384")
  .update("qintopia-ingress-internal-test-fixture")
  .digest("base64url");
const releaseDir = path.join(tmpRoot, "releases", sha);
const templateDir = path.join(releaseDir, "runtime/nginx/templates");
const configFile = path.join(tmpRoot, "etc/qintopia/qiwe-webhook-ingress.env");
const siteFile = path.join(tmpRoot, "etc/nginx/sites-available/qintopia.cn");
const snippetFile = path.join(tmpRoot, "etc/nginx/snippets/qintopia-qiwe-webhook.conf");
const stateDir = path.join(tmpRoot, "state");
const binDir = path.join(tmpRoot, "bin");
const callLog = path.join(tmpRoot, "calls.log");
const ingressScript = path.join(
  repoRoot,
  "deploy/sidecar/scripts/qiwe-webhook-ingress-production.sh"
);
const providerScript = path.join(
  repoRoot,
  "deploy/sidecar/scripts/qiwe-webhook-provider-callback-reviewed-command.sh"
);

const writeFile = (filePath, content, mode = 0o600) => {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content, { encoding: "utf8", mode });
  fs.chmodSync(filePath, mode);
};
const writeExecutable = (name, content) => {
  const filePath = path.join(binDir, name);
  writeFile(filePath, content, 0o755);
  return filePath;
};
const copyTemplate = (name) => {
  fs.mkdirSync(templateDir, { recursive: true });
  fs.copyFileSync(
    path.join(repoRoot, "runtime/nginx/templates", name),
    path.join(templateDir, name)
  );
};

const assertNoSecrets = (result, label) => {
  const output = `${result.stdout || ""}${result.stderr || ""}`;
  for (const secret of [publicToken, internalToken]) {
    if (output.includes(secret)) {
      throw new Error(`${label} leaked a server-local ingress secret`);
    }
  }
};

try {
  copyTemplate("qiwe-webhook.location.conf.template");
  copyTemplate("qiwe-webhook.disabled.conf");
  const disabled = fs.readFileSync(
    path.join(templateDir, "qiwe-webhook.disabled.conf"),
    "utf8"
  );
  writeFile(
    configFile,
    `QINTOPIA_QIWE_WEBHOOK_PUBLIC_PATH_TOKEN=${publicToken}\nQIWE_WEBHOOK_AUTH_TOKEN=${internalToken}\n`,
    0o600
  );
  writeFile(
    siteFile,
    "server {\n  include /etc/nginx/snippets/qintopia-qiwe-webhook.conf;\n}\n",
    0o600
  );
  writeFile(snippetFile, disabled, 0o600);

  const fakeNginx = writeExecutable(
    "nginx",
    `#!/usr/bin/env bash
set -euo pipefail
printf 'nginx-test\n' >> ${JSON.stringify(callLog)}
if [[ "\${FAKE_NGINX_FAIL_ACTIVE:-0}" == "1" ]] && ! grep -Fq 'QINTOPIA_QIWE_WEBHOOK_DISABLED' "\${FAKE_SNIPPET_FILE}"; then
  exit 1
fi
[[ "\${1:-}" == "-t" ]]
`
  );
  const fakeSystemctl = writeExecutable(
    "systemctl",
    `#!/usr/bin/env bash
set -euo pipefail
[[ "\${1:-}" == "reload" && "\${2:-}" == "nginx.service" ]]
printf 'nginx-reload\n' >> ${JSON.stringify(callLog)}
`
  );
  const fakeCurl = writeExecutable(
    "curl",
    `#!/usr/bin/env bash
set -euo pipefail
printf 'curl-argv:%s\n' "$*" >> ${JSON.stringify(callLog)}
config="$(cat)"
if [[ "$config" == *'url = "http://127.0.0.1:18661/qiwe/webhook"'* ]]; then
  if [[ "$config" == *${JSON.stringify(`header = "X-Qintopia-Qiwe-Ingress-Auth: ${internalToken}"`)}* ]]; then
    printf '200'
  else
    printf '401'
  fi
  exit 0
fi
if [[ "$config" == *'/qiwe/webhook/qintopia-invalid-ingress-probe"'* ]]; then
  printf '404'
  exit 0
fi
if [[ "$config" == *${JSON.stringify(`/qiwe/webhook/${publicToken}"`)}* ]] && \
   grep -Fq ${JSON.stringify(`location = /qiwe/webhook/${publicToken} {`)} "\${FAKE_SNIPPET_FILE}" && \
   grep -Fq ${JSON.stringify(`proxy_set_header X-Qintopia-Qiwe-Ingress-Auth ${internalToken};`)} "\${FAKE_SNIPPET_FILE}"; then
  printf '200'
else
  printf '404'
fi
`
  );

  const baseEnv = {
    ...process.env,
    QINTOPIA_QIWE_WEBHOOK_INGRESS_TEST_MODE: "1",
    QINTOPIA_QIWE_WEBHOOK_INGRESS_RELEASE_CURRENT: releaseDir,
    QINTOPIA_QIWE_WEBHOOK_INGRESS_CONFIG_FILE: configFile,
    QINTOPIA_QIWE_WEBHOOK_INGRESS_SITE_FILE: siteFile,
    QINTOPIA_QIWE_WEBHOOK_INGRESS_SNIPPET_FILE: snippetFile,
    QINTOPIA_QIWE_WEBHOOK_INGRESS_STATE_DIR: stateDir,
    QINTOPIA_QIWE_WEBHOOK_INGRESS_NGINX_BIN: fakeNginx,
    QINTOPIA_QIWE_WEBHOOK_INGRESS_SYSTEMCTL_BIN: fakeSystemctl,
    QINTOPIA_QIWE_WEBHOOK_INGRESS_CURL_BIN: fakeCurl,
    QINTOPIA_QIWE_WEBHOOK_INGRESS_RELEASE_SHA: sha,
    FAKE_SNIPPET_FILE: snippetFile,
  };
  const runIngress = (action, approval, extraEnv = {}) =>
    spawnSync("bash", [ingressScript, action], {
      cwd: repoRoot,
      env: {
        ...baseEnv,
        QINTOPIA_QIWE_WEBHOOK_INGRESS_APPROVAL: approval,
        ...extraEnv,
      },
      encoding: "utf8",
    });

  let result = runIngress("--apply", "approved-production-qiwe-webhook-ingress-apply");
  assertNoSecrets(result, "apply");
  if (result.status !== 0) {
    throw new Error(`ingress apply failed\n${result.stdout}\n${result.stderr}`);
  }
  const rendered = fs.readFileSync(snippetFile, "utf8");
  if (
    !rendered.includes(`location = /qiwe/webhook/${publicToken} {`) ||
    !rendered.includes(
      `proxy_set_header X-Qintopia-Qiwe-Ingress-Auth ${internalToken};`
    )
  ) {
    throw new Error("ingress apply did not render the exact authenticated route");
  }
  if (!fs.existsSync(path.join(stateDir, "rollback.conf"))) {
    throw new Error("ingress apply did not retain fixed rollback state");
  }

  const rollbackFile = path.join(stateDir, "rollback.conf");
  writeFile(rollbackFile, `${disabled}\nadd_header X-Unreviewed true;\n`, 0o600);
  result = runIngress(
    "--rollback",
    "approved-production-qiwe-webhook-ingress-rollback"
  );
  assertNoSecrets(result, "noncanonical rollback");
  if (result.status === 0 || fs.readFileSync(snippetFile, "utf8") !== rendered) {
    throw new Error("ingress rollback accepted a noncanonical retained include");
  }
  writeFile(rollbackFile, disabled, 0o600);

  const sameTokenActive = fs
    .readFileSync(path.join(templateDir, "qiwe-webhook.location.conf.template"), "utf8")
    .replace("__QIWE_PUBLIC_PATH_TOKEN__", publicToken)
    .replace("__QIWE_INTERNAL_AUTH_TOKEN__", publicToken);
  writeFile(snippetFile, sameTokenActive, 0o600);
  result = runIngress(
    "--rollback",
    "approved-production-qiwe-webhook-ingress-rollback"
  );
  assertNoSecrets(result, "same-token current include");
  if (result.status === 0 || fs.readFileSync(snippetFile, "utf8") !== sameTokenActive) {
    throw new Error("ingress rollback accepted equal public and internal tokens");
  }
  writeFile(snippetFile, rendered, 0o600);

  result = runIngress(
    "--rollback",
    "approved-production-qiwe-webhook-ingress-rollback"
  );
  assertNoSecrets(result, "rollback");
  if (result.status !== 0) {
    throw new Error(`ingress rollback failed\n${result.stdout}\n${result.stderr}`);
  }
  if (
    !fs.readFileSync(snippetFile, "utf8").includes("QINTOPIA_QIWE_WEBHOOK_DISABLED")
  ) {
    throw new Error("ingress rollback did not restore the disabled include");
  }

  result = runIngress("--apply", "approved-production-qiwe-webhook-ingress-apply", {
    FAKE_NGINX_FAIL_ACTIVE: "1",
  });
  assertNoSecrets(result, "failed apply");
  if (result.status === 0) {
    throw new Error("ingress apply accepted a failing nginx candidate");
  }
  if (
    !fs.readFileSync(snippetFile, "utf8").includes("QINTOPIA_QIWE_WEBHOOK_DISABLED")
  ) {
    throw new Error("failed ingress apply did not restore the previous include");
  }

  writeFile(
    configFile,
    `QINTOPIA_QIWE_WEBHOOK_PUBLIC_PATH_TOKEN=short\nQIWE_WEBHOOK_AUTH_TOKEN=${internalToken}\n`,
    0o600
  );
  result = runIngress("--apply", "approved-production-qiwe-webhook-ingress-apply");
  assertNoSecrets(result, "invalid config");
  if (result.status === 0) {
    throw new Error("ingress apply accepted a low-entropy public path token");
  }
  writeFile(
    configFile,
    `QINTOPIA_QIWE_WEBHOOK_PUBLIC_PATH_TOKEN=${publicToken}\nQIWE_WEBHOOK_AUTH_TOKEN=${internalToken}\n`,
    0o600
  );

  writeFile(
    configFile,
    `QINTOPIA_QIWE_WEBHOOK_PUBLIC_PATH_TOKEN=${"a".repeat(
      64
    )}\nQIWE_WEBHOOK_AUTH_TOKEN=${internalToken}\n`,
    0o600
  );
  result = runIngress("--apply", "approved-production-qiwe-webhook-ingress-apply");
  assertNoSecrets(result, "obvious placeholder config");
  if (result.status === 0) {
    throw new Error("ingress apply accepted an obvious placeholder path token");
  }
  writeFile(
    configFile,
    `QINTOPIA_QIWE_WEBHOOK_PUBLIC_PATH_TOKEN=${publicToken}\nQIWE_WEBHOOK_AUTH_TOKEN=${internalToken}\n`,
    0o600
  );

  const providerCommand = path.join(
    tmpRoot,
    "etc/qintopia/qiwe-webhook-provider-callback-command"
  );
  const providerOutput = path.join(tmpRoot, "provider-callback-url.txt");
  writeFile(
    providerCommand,
    `[[ -z "\${QINTOPIA_QIWE_REVIEWED_CALLBACK_URL:-}" ]]
IFS= read -r callback_url
printf '%s' "$callback_url" > ${JSON.stringify(providerOutput)}
printf '%s\n' "$callback_url"
printf '%s\n' "$callback_url" >&2
`,
    0o555
  );
  const providerSha = crypto
    .createHash("sha256")
    .update(fs.readFileSync(providerCommand))
    .digest("hex");
  const fakeRunuser = writeExecutable(
    "runuser",
    `#!/usr/bin/env bash
set -euo pipefail
printf 'runuser-argv:%s\n' "$*" >> ${JSON.stringify(callLog)}
[[ "\${1:-}" == "-u" ]]
shift 2
[[ "\${1:-}" == "--" ]]
shift
exec "$@"
`
  );
  const fakeTimeout = writeExecutable(
    "timeout",
    `#!/usr/bin/env bash
set -euo pipefail
printf 'timeout-argv:%s\n' "$*" >> ${JSON.stringify(callLog)}
shift
exec "$@"
`
  );
  const providerEnv = {
    ...process.env,
    QINTOPIA_QIWE_WEBHOOK_PROVIDER_COMMAND_TEST_MODE: "1",
    QINTOPIA_QIWE_WEBHOOK_PROVIDER_COMMAND_FILE: providerCommand,
    QINTOPIA_QIWE_WEBHOOK_PROVIDER_CONFIG_FILE: configFile,
    QINTOPIA_QIWE_WEBHOOK_PROVIDER_RUNUSER_BIN: fakeRunuser,
    QINTOPIA_QIWE_WEBHOOK_PROVIDER_TIMEOUT_BIN: fakeTimeout,
    QINTOPIA_QIWE_WEBHOOK_PROVIDER_USER: "test-provider",
    QINTOPIA_QIWE_WEBHOOK_PROVIDER_COMMAND_SHA256: providerSha,
  };
  result = spawnSync("bash", [providerScript, "--check"], {
    cwd: repoRoot,
    env: providerEnv,
    encoding: "utf8",
  });
  assertNoSecrets(result, "provider check");
  if (result.status !== 0 || fs.existsSync(providerOutput)) {
    throw new Error("provider command check did not remain non-executing");
  }
  fs.chmodSync(configFile, 0o644);
  result = spawnSync("bash", [providerScript, "--check"], {
    cwd: repoRoot,
    env: providerEnv,
    encoding: "utf8",
  });
  assertNoSecrets(result, "provider config mode rejection");
  if (result.status === 0) {
    throw new Error("provider command accepted a non-secret ingress config mode");
  }
  fs.chmodSync(configFile, 0o600);
  result = spawnSync("bash", [providerScript, "--execute"], {
    cwd: repoRoot,
    env: {
      ...providerEnv,
      QINTOPIA_QIWE_WEBHOOK_PROVIDER_COMMAND_APPROVAL:
        "approved-production-qiwe-webhook-provider-callback-command",
    },
    encoding: "utf8",
  });
  assertNoSecrets(result, "provider execute");
  if (result.status !== 0) {
    throw new Error(`provider command boundary failed\n${result.stderr}`);
  }
  if (
    fs.readFileSync(providerOutput, "utf8") !==
    `https://qintopia.cn/qiwe/webhook/${publicToken}`
  ) {
    throw new Error("provider command did not receive the fixed callback URL boundary");
  }

  const calls = fs.readFileSync(callLog, "utf8");
  if (!calls.includes("nginx-test") || !calls.includes("nginx-reload")) {
    throw new Error("ingress action did not validate and reload nginx");
  }
  for (const secret of [publicToken, internalToken]) {
    if (calls.includes(secret)) {
      throw new Error(
        "ingress or provider callback secret leaked through process argv"
      );
    }
  }
} finally {
  fs.rmSync(tmpRoot, { recursive: true, force: true });
}

console.log("Qiwe authenticated webhook ingress activation tests passed.");
