#!/usr/bin/env node

import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";

const root = process.cwd();
const templates = path.join(root, "runtime/nginx/templates");
const http = fs.readFileSync(
  path.join(templates, "management-ui-http.conf.template"),
  "utf8"
);
const https = fs.readFileSync(
  path.join(templates, "management-ui-https.conf.template"),
  "utf8"
);

const count = (source, pattern) => [...source.matchAll(pattern)].length;
assert.equal(count(http, /\bserver_name agentos\.qintopia\.cn;/g), 1);
assert.equal(count(https, /\bserver_name agentos\.qintopia\.cn;/g), 2);
assert.match(http, /listen 80;/);
assert.match(http, /if \(\$host != "agentos\.qintopia\.cn"\) \{ return 421; \}/);
assert.doesNotMatch(
  http,
  /listen 443|proxy_pass|ssl_certificate|qintopia\.cn\/\$request_uri/
);
assert.match(http, /location \^~ \/\.well-known\/acme-challenge\//);
assert.match(http, /location \/\s*\{\s*return 404;/);

for (const required of [
  "listen 443 ssl;",
  "ssl_certificate /etc/letsencrypt/live/qintopia-management-ui/fullchain.pem;",
  "ssl_certificate_key /etc/letsencrypt/live/qintopia-management-ui/privkey.pem;",
  'if ($ssl_server_name != "agentos.qintopia.cn") { return 421; }',
  'if ($host != "agentos.qintopia.cn") { return 421; }',
  "proxy_pass http://127.0.0.1:18780;",
  "proxy_set_header Host agentos.qintopia.cn;",
  'proxy_set_header X-Forwarded-For "";',
  'proxy_set_header X-Forwarded-Host "";',
  'proxy_set_header X-Forwarded-Proto "";',
  'proxy_set_header Forwarded "";',
  "proxy_intercept_errors on;",
  "error_page 502 503 504 = @management_ui_unavailable;",
  "return 503;",
]) {
  assert.ok(https.includes(required), `management HTTPS route is missing ${required}`);
}
assert.equal(count(https, /proxy_pass /g), 1);
assert.equal(
  count(https, /if \(\$host != "agentos\.qintopia\.cn"\) \{ return 421; \}/g),
  2
);
assert.equal(count(https, /return 503;/g), 1);
assert.doesNotMatch(https, /cos|www\.qintopia\.cn|proxy_set_header Origin|18877/i);

console.log("Management UI Nginx template contract test passed.");
