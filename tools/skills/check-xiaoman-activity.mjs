#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { execFileSync } from "node:child_process";
import YAML from "yaml";

const repoRoot = process.cwd();
const packageRoot = path.join(repoRoot, "skills/xiaoman-activity");
const errors = [];

const requiredFiles = [
  "README.md",
  "manifest.yaml",
  "plugin.yaml",
  "__init__.py",
  "tests/test_xiaoman_activity.py",
];

for (const file of requiredFiles) {
  if (!fs.existsSync(path.join(packageRoot, file))) {
    errors.push(`missing ${file}`);
  }
}

const manifest = YAML.parse(
  fs.readFileSync(path.join(packageRoot, "manifest.yaml"), "utf8")
);
const plugin = YAML.parse(
  fs.readFileSync(path.join(packageRoot, "plugin.yaml"), "utf8")
);

if (manifest.id !== "skills/xiaoman-activity") {
  errors.push("manifest id must be skills/xiaoman-activity");
}

const expectedTools = [
  "qintopia_xiaoman_activity_record_get",
  "qintopia_xiaoman_activity_list_by_date",
  "qintopia_xiaoman_activity_plan_table_probe",
  "qintopia_xiaoman_activity_announcement_prepare",
  "qintopia_xiaoman_activity_text_group_message_request_prepare",
  "qintopia_xiaoman_weekly_poster_workflow_prepare",
  "qintopia_xiaoman_public_reply_rewrite",
  "qintopia_xiaoman_activity_status_update",
  "qintopia_xiaoman_activity_gap_update",
  "qintopia_xiaoman_activity_phase_update",
  "qintopia_xiaoman_activity_feishu_field_update",
  "qintopia_xiaoman_activity_handoff_create",
  "qintopia_xiaoman_activity_promotion_review_draft",
  "qintopia_xiaoman_activity_material_summary",
];

for (const tool of expectedTools) {
  if (!plugin.tools?.includes(tool)) {
    errors.push(`plugin.yaml must list ${tool}`);
  }
}

try {
  execFileSync(
    "python3",
    [
      "-m",
      "unittest",
      "discover",
      "-s",
      "skills/xiaoman-activity/tests",
      "-p",
      "test_*.py",
      "-v",
    ],
    {
      cwd: repoRoot,
      env: { ...process.env, PYTHONDONTWRITEBYTECODE: "1" },
      stdio: "inherit",
    }
  );
} catch {
  errors.push("xiaoman-activity unittest failed");
}

const walk = (dir) => {
  const files = [];
  const visit = (absoluteDir) => {
    for (const entry of fs.readdirSync(absoluteDir, { withFileTypes: true })) {
      const absolutePath = path.join(absoluteDir, entry.name);
      if (entry.isDirectory()) {
        visit(absolutePath);
      } else if (entry.isFile()) {
        files.push(path.relative(packageRoot, absolutePath));
      }
    }
  };
  visit(dir);
  return files.sort();
};

for (const file of walk(packageRoot)) {
  if (file.includes("__pycache__/") || file.endsWith(".pyc")) {
    errors.push(`runtime cache file must not be committed: ${file}`);
  }
}

if (errors.length > 0) {
  console.error("Xiaoman activity skill check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log("Xiaoman activity skill check passed.");
