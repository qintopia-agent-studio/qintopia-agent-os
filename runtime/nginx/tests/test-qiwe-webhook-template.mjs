#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const repoRoot = process.cwd();
const templateRelative = "runtime/nginx/templates/qiwe-webhook.location.conf.template";
const disabledRelative = "runtime/nginx/templates/qiwe-webhook.disabled.conf";
const oldReviewRelative =
  "runtime/nginx/templates/qiwe-webhook.review-only.conf.template";
const errors = [];
const addError = (message) => errors.push(message);
const count = (text, fragment) => text.split(fragment).length - 1;
const read = (relative) => fs.readFileSync(path.join(repoRoot, relative), "utf8");

if (!fs.existsSync(path.join(repoRoot, templateRelative))) {
  addError(`${templateRelative}: missing deployable template`);
} else {
  const template = read(templateRelative);
  for (const marker of ["DEPLOYABLE SCAFFOLD", "DEFAULT DISABLED"]) {
    if (!template.includes(marker)) {
      addError(`${templateRelative}: missing ${marker} marker`);
    }
  }
  if (
    !/location\s*=\s*\/qiwe\/webhook\/__QIWE_PUBLIC_PATH_TOKEN__\s*\{/.test(template)
  ) {
    addError(`${templateRelative}: must use the exact secret path location`);
  }
  for (const fragment of [
    "proxy_pass http://127.0.0.1:18661/qiwe/webhook;",
    "proxy_pass_request_headers off;",
    "proxy_set_header X-Qintopia-Qiwe-Ingress-Auth __QIWE_INTERNAL_AUTH_TOKEN__;",
    "access_log off;",
    "error_log /dev/null crit;",
    "client_max_body_size 1m;",
    "client_body_timeout 2s;",
    "proxy_connect_timeout 1s;",
    "proxy_read_timeout 2s;",
    "proxy_send_timeout 2s;",
  ]) {
    if (!template.includes(fragment)) {
      addError(`${templateRelative}: missing ${fragment}`);
    }
  }
  if (count(template, "__QIWE_PUBLIC_PATH_TOKEN__") !== 1) {
    addError(`${templateRelative}: public token placeholder must occur once`);
  }
  if (count(template, "__QIWE_INTERNAL_AUTH_TOKEN__") !== 1) {
    addError(`${templateRelative}: internal token placeholder must occur once`);
  }
  if (/\$http_x_qintopia_qiwe_ingress_auth/i.test(template)) {
    addError(`${templateRelative}: must not forward the client auth header`);
  }
  if (/\b\d{15,}\b/.test(template)) {
    addError(`${templateRelative}: contains a real-looking Qiwe identifier`);
  }
}

if (!fs.existsSync(path.join(repoRoot, disabledRelative))) {
  addError(`${disabledRelative}: missing disabled include bootstrap`);
} else if (!read(disabledRelative).includes("QINTOPIA_QIWE_WEBHOOK_DISABLED")) {
  addError(`${disabledRelative}: missing disabled marker`);
}

if (fs.existsSync(path.join(repoRoot, oldReviewRelative))) {
  addError(`${oldReviewRelative}: obsolete review-only dead end must be removed`);
}

const ingressScript = "deploy/sidecar/scripts/qiwe-webhook-ingress-production.sh";
const providerScript =
  "deploy/sidecar/scripts/qiwe-webhook-provider-callback-reviewed-command.sh";
for (const relative of [ingressScript, providerScript]) {
  if (!fs.existsSync(path.join(repoRoot, relative))) {
    addError(`${relative}: missing fixed production boundary`);
  } else if ((fs.statSync(path.join(repoRoot, relative)).mode & 0o111) === 0) {
    addError(`${relative}: must be executable`);
  }
}

if (fs.existsSync(path.join(repoRoot, ingressScript))) {
  const script = read(ingressScript);
  for (const fragment of [
    "approved-production-qiwe-webhook-ingress-apply",
    "approved-production-qiwe-webhook-ingress-rollback",
    "/etc/qintopia/qiwe-webhook-ingress.env",
    "/etc/nginx/sites-available/qintopia.cn",
    "/etc/nginx/snippets/qintopia-qiwe-webhook.conf",
    "/home/ubuntu/qintopia-agent-os-releases/current",
    "nginx_test",
    "atomic_copy",
    "run_active_smoke",
    "run_disabled_smoke",
    "rollback.conf",
  ]) {
    if (!script.includes(fragment)) {
      addError(`${ingressScript}: missing ${fragment}`);
    }
  }
  for (const forbidden of ["set -x", "curl -v", "access_log on;"]) {
    if (script.includes(forbidden)) {
      addError(`${ingressScript}: forbidden secret-logging surface ${forbidden}`);
    }
  }
}

if (fs.existsSync(path.join(repoRoot, providerScript))) {
  const script = read(providerScript);
  for (const fragment of [
    "approved-production-qiwe-webhook-provider-callback-command",
    "/etc/qintopia/qiwe-webhook-provider-callback-command",
    "QINTOPIA_QIWE_WEBHOOK_PROVIDER_COMMAND_SHA256",
    "--check",
    "--execute",
    "/usr/bin/timeout",
    "/usr/sbin/runuser",
  ]) {
    if (!script.includes(fragment)) {
      addError(`${providerScript}: missing ${fragment}`);
    }
  }
  if (/manager\.qiweapi\.com|\/qw\/doApi|setCallback/i.test(script)) {
    addError(`${providerScript}: must not invent undocumented Qiwe API semantics`);
  }
}

if (errors.length > 0) {
  console.error("Qiwe authenticated webhook ingress contract check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log("Qiwe authenticated webhook ingress contract check passed.");
