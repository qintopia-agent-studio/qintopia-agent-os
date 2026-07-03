#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { execFileSync } from "node:child_process";

const repoRoot = process.cwd();
const errors = [];

const textExtensions = new Set([
  ".bash",
  ".c",
  ".conf",
  ".css",
  ".env",
  ".example",
  ".gitignore",
  ".html",
  ".js",
  ".json",
  ".lock",
  ".md",
  ".mjs",
  ".py",
  ".rs",
  ".sh",
  ".sql",
  ".toml",
  ".ts",
  ".txt",
  ".yaml",
  ".yml",
]);

const allowedSecretLikeFiles = new Set(["runtime/sidecar/.env.example"]);

const ignoredPathParts = new Set([
  ".git",
  "node_modules",
  "target",
  ".pnpm-store",
  ".venv",
  "venv",
  "__pycache__",
]);

const runtimeStatePathParts = new Set([
  "sessions",
  "cache",
  "logs",
  "tmp",
  "secrets",
  "image_cache",
  "audio_cache",
  "sandboxes",
  "pairing",
]);

const blockedBasenames = new Set([
  ".env",
  "auth.json",
  "auth.lock",
  "gateway.pid",
  "gateway.lock",
  "gateway_state.json",
  "state.db",
  "state.db-shm",
  "state.db-wal",
  ".hermes_history",
  ".skills_prompt_snapshot.json",
  "feishu_seen_message_ids.json",
]);

const blockedFilePatterns = [
  /^\.env\./,
  /\.(pem|key|p12|pfx|secret)$/i,
  /\.(sqlite|sqlite3|db|db-shm|db-wal|log|pid)$/i,
  /request_dump/i,
];

const privateKeyPattern = /-----BEGIN (?:RSA |DSA |EC |OPENSSH |PGP )?PRIVATE KEY-----/;
const credentialAssignmentPattern =
  /\b(api[_-]?key|access[_-]?token|refresh[_-]?token|tenant[_-]?access[_-]?token|auth[_-]?token|client[_-]?secret|app[_-]?secret|secret|password)\b\s*[:=]\s*["']?([A-Za-z0-9_./+=:@-]{32,})["']?/gi;

const placeholderPattern =
  /^(replace[-_]?with|example|dummy|fake|test|unused|placeholder|changeme|redacted|xxxx|your[-_]?)/i;
const envVarReferencePattern = /^\$\{?[A-Z0-9_]+\}?$/;

const normalizePath = (relativePath) => relativePath.split(path.sep).join("/");

const addError = (message) => {
  errors.push(message);
};

const readGitFiles = () => {
  const output = execFileSync(
    "git",
    ["ls-files", "--cached", "--others", "--exclude-standard"],
    {
      cwd: repoRoot,
      encoding: "utf8",
    }
  );
  return output
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean)
    .map(normalizePath)
    .sort();
};

const isTextFile = (relativePath) => {
  const basename = path.basename(relativePath);
  if (
    [
      ".env.example",
      ".editorconfig",
      ".gitattributes",
      ".gitignore",
      ".prettierignore",
      ".prettierrc",
    ].includes(basename)
  ) {
    return true;
  }
  return textExtensions.has(path.extname(relativePath).toLowerCase());
};

const shouldSkip = (relativePath) => {
  const parts = relativePath.split("/");
  return parts.some((part) => ignoredPathParts.has(part));
};

const isPlaceholder = (value) => {
  const trimmed = value.trim().replace(/^["']|["']$/g, "");
  return (
    placeholderPattern.test(trimmed) ||
    envVarReferencePattern.test(trimmed) ||
    trimmed.includes("...")
  );
};

const inspectPath = (relativePath) => {
  const basename = path.basename(relativePath);
  const parts = relativePath.split("/");

  if (allowedSecretLikeFiles.has(relativePath)) {
    return;
  }

  if (
    blockedBasenames.has(basename) ||
    blockedFilePatterns.some((pattern) => pattern.test(basename))
  ) {
    addError(`${relativePath}: blocked secret or runtime-state file path`);
  }

  if (parts.some((part) => runtimeStatePathParts.has(part))) {
    addError(`${relativePath}: blocked live runtime-state directory`);
  }
};

const inspectContent = (relativePath) => {
  if (!isTextFile(relativePath)) {
    return;
  }

  const absolutePath = path.join(repoRoot, relativePath);
  let content = "";
  try {
    content = fs.readFileSync(absolutePath, "utf8");
  } catch {
    return;
  }

  if (privateKeyPattern.test(content)) {
    addError(`${relativePath}: private key material detected`);
  }

  let match;
  while ((match = credentialAssignmentPattern.exec(content)) !== null) {
    const value = match[2];
    if (!isPlaceholder(value)) {
      const line = content.slice(0, match.index).split(/\r?\n/).length;
      addError(`${relativePath}:${line}: high-confidence credential assignment`);
    }
  }
};

for (const relativePath of readGitFiles()) {
  if (shouldSkip(relativePath)) {
    continue;
  }
  inspectPath(relativePath);
  inspectContent(relativePath);
}

if (errors.length > 0) {
  console.error("Secret and runtime-state check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log("Secret and runtime-state check passed.");
