#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const errors = [];

const exists = (relativePath) => fs.existsSync(path.join(repoRoot, relativePath));
const readText = (relativePath) =>
  fs.readFileSync(path.join(repoRoot, relativePath), "utf8");
const addError = (message) => errors.push(message);
const normalizeWhitespace = (value) => value.replace(/\s+/g, " ").trim();
const requireFragment = (relativePath, text, fragment) => {
  if (!text.includes(fragment)) {
    addError(`${relativePath}: must include ${fragment}`);
  }
};
const forbidFragment = (relativePath, text, fragment) => {
  if (text.includes(fragment)) {
    addError(`${relativePath}: must not include ${fragment}`);
  }
};

const workflowRoot = "workflows/xiaoman-daily-case-report";
const scriptPath = `${workflowRoot}/daily_case_report.py`;
const readmePath = `${workflowRoot}/README.md`;
const manifestPath = `${workflowRoot}/workflow.yaml`;
const testPath = `${workflowRoot}/tests/test_daily_case_report.py`;
const retiredRunbookPath =
  "docs/operations/xiaoman-daily-case-report-cutover-runbook.md";

for (const required of [scriptPath, readmePath, manifestPath, testPath]) {
  if (!exists(required)) {
    addError(`${required}: missing Xiaoman daily report artifact`);
  }
}

if (exists(scriptPath)) {
  const script = readText(scriptPath);
  for (const fragment of [
    "DraftBundle",
    "CreativeMemorySignal",
    "CharacterSketch",
    "sender_person_id",
    "stable_identity_grouping",
    "private_profile_text_excluded_from_public_image",
    "public_output_style",
    "image_first_delivery",
    "pdf_default_delivery",
    "storyline_first_output",
    "_render_png_with_pillow",
    "/usr/bin/psql",
    ".draft-bundle.json",
  ]) {
    requireFragment(scriptPath, script, fragment);
  }
  for (const fragment of [
    'DEFAULT_REPORT_TITLE = "群聊案件档案"',
    'QINTOPIA_SIDECAR_DATABASE_URL") or os.environ.get("DATABASE_URL")',
    "requests.",
    "httpx.",
    "urllib.request",
  ]) {
    forbidFragment(scriptPath, script, fragment);
  }
}

if (exists(readmePath)) {
  const readme = readText(readmePath);
  const normalizedReadme = normalizeWhitespace(readme);
  for (const fragment of [
    "wx-cli inspired",
    "PNG/JPEG-style long image",
    "digest",
    "roast",
    "public draft",
    "quote map",
    "profiles-roast",
    "PDF can be produced later",
    "not the default group deliverable",
    "Production Boundary",
    "Acceptance Scenarios",
    "Pillow long-image renderer",
    "Do not download browsers",
    "node tools/deploy/check-xiaoman-daily-case-report-character-universe-local.mjs",
  ]) {
    requireFragment(readmePath, normalizedReadme, fragment);
  }
  forbidFragment(readmePath, readme, "群聊案件档案");
}

if (exists(manifestPath)) {
  const manifest = readText(manifestPath);
  for (const fragment of [
    "name: Xiaoman Wx-Cli Style Daily Report",
    "group_deliverable: image",
    "pdf_default_delivery: false",
    "wx-cli-style",
  ]) {
    requireFragment(manifestPath, manifest, fragment);
  }
}

if (exists(testPath)) {
  const tests = readText(testPath);
  for (const fragment of [
    "test_character_sketches_group_by_person_id_before_display_name",
    "test_private_memory_stays_out_of_public_image_line",
    "test_render_png_falls_back_to_pillow_when_playwright_missing",
    "image_first_delivery",
    "pdf_default_delivery",
  ]) {
    requireFragment(testPath, tests, fragment);
  }
}

if (exists(retiredRunbookPath)) {
  const runbook = readText(retiredRunbookPath);
  const normalizedRunbook = normalizeWhitespace(runbook);
  for (const fragment of [
    "Retired Systemd Design Draft",
    "Hermes cron",
    "reviewed image upload / auto-publish / QiWe send-ready chain",
    "Do not use this document as the current activation path",
    "Do not paste, retain, or commit the real `chat_id`",
  ]) {
    requireFragment(retiredRunbookPath, normalizedRunbook, fragment);
  }
  forbidFragment(retiredRunbookPath, runbook, "10859791146538059");
}

const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "xiaoman-daily-report-check-"));
const dryRun = spawnSync(
  "python3",
  [
    scriptPath,
    "--dry-run",
    "--render",
    "html",
    "--json",
    "--date",
    "2026-08-08",
    "--output-dir",
    tmpRoot,
  ],
  {
    cwd: repoRoot,
    encoding: "utf8",
    env: {
      ...process.env,
      PYTHONDONTWRITEBYTECODE: "1",
    },
  }
);

if (dryRun.status !== 0) {
  addError(
    `dry-run html render failed with status ${dryRun.status}\nstdout:\n${dryRun.stdout}\nstderr:\n${dryRun.stderr}`
  );
} else {
  let result = null;
  try {
    result = JSON.parse(dryRun.stdout);
  } catch (error) {
    addError(`dry-run stdout must be JSON: ${error.message}`);
  }
  if (result) {
    const style = result.public_output_style ?? {};
    const privacy = result.privacy_flags ?? {};
    const counts = result.draft_counts ?? {};
    if (style.schema_version !== "xiaoman-character-daily-image-v1") {
      addError(
        "public_output_style.schema_version must be xiaoman-character-daily-image-v1"
      );
    }
    for (const [field, expected] of [
      ["image_first_delivery", true],
      ["pdf_default_delivery", false],
      ["storyline_first_output", true],
      ["stable_identity_grouping", true],
      ["private_draft_boundary", true],
    ]) {
      if (style[field] !== expected) {
        addError(`public_output_style.${field} must be ${expected}`);
      }
    }
    for (const [field, expected] of [
      ["stable_identity_grouping", true],
      ["raw_member_fact_text_retained", false],
      ["private_profile_text_excluded_from_public_image", true],
      ["external_send_executed", false],
      ["requires_human_confirmation", true],
    ]) {
      if (privacy[field] !== expected) {
        addError(`privacy_flags.${field} must be ${expected}`);
      }
    }
    for (const field of [
      "digest",
      "roast",
      "public_draft",
      "quote_map",
      "profile_candidates",
      "storylines",
    ]) {
      if (!Number.isInteger(counts[field]) || counts[field] < 1) {
        addError(`draft_counts.${field} must be a positive integer`);
      }
    }
    if (!result.draft_bundle_path || !fs.existsSync(result.draft_bundle_path)) {
      addError("dry-run must write draft_bundle_path");
    } else {
      const mode = fs.statSync(result.draft_bundle_path).mode & 0o777;
      if (mode !== 0o600) {
        addError(`draft bundle mode must be 0600, got ${mode.toString(8)}`);
      }
      const bundle = JSON.parse(fs.readFileSync(result.draft_bundle_path, "utf8"));
      for (const field of [
        "digest_markdown",
        "roast_markdown",
        "public_draft_markdown",
        "quote_map",
        "profile_candidates",
        "privacy_flags",
        "draft_counts",
      ]) {
        if (!(field in bundle)) {
          addError(`draft bundle must include ${field}`);
        }
      }
    }
    const dirMode = fs.statSync(tmpRoot).mode & 0o777;
    if (dirMode !== 0o700) {
      addError(
        `dry-run output directory mode must be 0700, got ${dirMode.toString(8)}`
      );
    }
  }
}

if (errors.length > 0) {
  console.error("Xiaoman daily case-report character-universe local check failed:");
  for (const error of errors) {
    console.error(`- ${error}`);
  }
  process.exit(1);
}

console.log("Xiaoman daily case-report character-universe local check passed.");
