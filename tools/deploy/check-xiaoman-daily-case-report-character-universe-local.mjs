#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const repoRoot = process.cwd();
const errors = [];

const readText = (relativePath) =>
  fs.readFileSync(path.join(repoRoot, relativePath), "utf8");
const exists = (relativePath) => fs.existsSync(path.join(repoRoot, relativePath));
const addError = (message) => errors.push(message);
const requireFile = (relativePath) => {
  if (!exists(relativePath)) {
    addError(`${relativePath}: required file is missing`);
    return "";
  }
  return readText(relativePath);
};
const requireIncludes = (text, fragment, label) => {
  if (!text.includes(fragment)) {
    addError(`${label}: missing ${JSON.stringify(fragment)}`);
  }
};
const requireNotIncludes = (text, fragment, label) => {
  if (text.includes(fragment)) {
    addError(`${label}: forbidden ${JSON.stringify(fragment)}`);
  }
};

const workflow = requireFile(
  "workflows/xiaoman-daily-case-report/daily_case_report.py"
);
const workflowTests = requireFile(
  "workflows/xiaoman-daily-case-report/tests/test_daily_case_report.py"
);
const worker = requireFile(
  "deploy/sidecar/scripts/xiaoman-daily-case-report-auto-publish-worker.sh"
);
const workerRunEvidence = requireFile(
  "deploy/sidecar/scripts/production-worker-run-evidence-smoke.sh"
);
const runner = requireFile("deploy/runner/qintopia-agent-os-deploy-runner");
const runnerTest = requireFile("tools/deploy/test-production-observation-runner.mjs");
const workerEvidenceTest = requireFile(
  "tools/deploy/test-production-worker-run-evidence-smoke.mjs"
);
const backfillWorkerTest = requireFile(
  "tools/deploy/test_xiaoman_daily_case_report_backfill_worker.py"
);
const readme = requireFile("workflows/xiaoman-daily-case-report/README.md");
const plan = requireFile(
  "docs/plans/active/xiaoman-character-universe-daily-report.md"
);
const observationRunbook = requireFile(
  "docs/operations/production-runtime-observation-runbook.md"
);
const cronRunbook = requireFile(
  "docs/operations/xiaoman-daily-case-report-hermes-cron-runbook.md"
);

for (const [fragment, label] of [
  ["MEMORY_FACT_ROLE_LABELS", "daily workflow role-memory labels"],
  ["m.sender_person_id::text AS sender_person_id", "daily workflow person binding"],
  ["def _fetch_character_memory(", "daily workflow character memory query"],
  ["def _compute_characters(", "daily workflow character cards"],
  ["def _build_character_universe(", "daily workflow universe builder"],
  ['"schema_version": "xiaoman-character-universe-v1"', "universe schema"],
  ['"source": "daily_case_report_second_pass"', "universe source"],
  ['"raw_messages_included": False', "raw message exclusion flag"],
  ['"profile_fact_text_included": False', "profile fact exclusion flag"],
  ['"daily_report_markdown_path"', "Markdown report output path"],
  ['"character_universe_path"', "character universe output path"],
  [".character-universe.json", "private universe file output"],
  [
    "except Exception:\n            character_memory = {}",
    "character memory soft dependency",
  ],
]) {
  requireIncludes(workflow, fragment, label);
}

for (const [fragment, label] of [
  [
    "test_build_report_keeps_latest_messages_when_character_memory_fails",
    "character memory failure regression test",
  ],
  [
    "test_character_universe_uses_curated_second_pass_nodes",
    "universe second-pass regression test",
  ],
  [
    "test_render_html_mode_returns_existing_html_deliverable",
    "HTML/Markdown/universe output regression test",
  ],
  [
    'self.assertFalse(universe["raw_messages_included"])',
    "raw message exclusion assertion",
  ],
  [
    'self.assertFalse(universe["profile_fact_text_included"])',
    "profile fact exclusion assertion",
  ],
]) {
  requireIncludes(workflowTests, fragment, label);
}

for (const [fragment, label] of [
  [
    'content_metrics = candidate.get("content_metrics") or {}',
    "worker content metrics",
  ],
  [
    'character_universe = rendered.get("character_universe") or {}',
    "worker universe read",
  ],
  ['"character_universe": {', "worker safe universe metadata"],
  [
    '"raw_messages_included": character_universe.get("raw_messages_included") is True',
    "worker raw flag binding",
  ],
  [
    '"profile_fact_text_included": character_universe.get("profile_fact_text_included") is True',
    "worker profile fact flag binding",
  ],
  [
    '"storyline_candidate_count": len(character_universe.get("storyline_candidates") or [])',
    "worker storyline count",
  ],
  [
    '"$PYTHON_BIN" - "$render_report" "$upload_report" "$publish_report"',
    "worker final summary render binding",
  ],
]) {
  requireIncludes(worker, fragment, label);
}

for (const [fragment, label] of [
  [
    'print(f"{key}_worker_character_universe_schema_version=',
    "worker-run schema evidence",
  ],
  [
    'print(f"{key}_worker_character_universe_raw_messages_included=false")',
    "worker-run raw flag evidence",
  ],
  [
    'print(f"{key}_worker_character_universe_profile_fact_text_included=false")',
    "worker-run profile fact flag evidence",
  ],
  [
    'if universe.get("raw_messages_included") is not False:',
    "worker-run raw flag guard",
  ],
  [
    'if universe.get("profile_fact_text_included") is not False:',
    "worker-run profile fact guard",
  ],
]) {
  requireIncludes(workerRunEvidence, fragment, label);
}

for (const [fragment, label] of [
  [
    "xiaoman_daily_case_report_worker_character_universe_(?:schema_version|source)",
    "runner allowlist for universe labels",
  ],
  [
    "xiaoman_daily_case_report_worker_character_universe_(?:raw_messages_included|profile_fact_text_included)",
    "runner allowlist for universe privacy flags",
  ],
  [
    "xiaoman_daily_case_report_worker_character_universe_(?:people_count|topic_count|event_count|storyline_candidate_count|edge_count)",
    "runner allowlist for universe counts",
  ],
]) {
  requireIncludes(runner, fragment, label);
}

for (const [text, label] of [
  [runnerTest, "production observation runner fixture"],
  [workerEvidenceTest, "worker-run evidence fixture"],
]) {
  requireIncludes(
    text,
    "xiaoman_daily_case_report_worker_character_universe_schema_version=xiaoman-character-universe-v1",
    label
  );
  requireIncludes(
    text,
    "xiaoman_daily_case_report_worker_character_universe_raw_messages_included=false",
    label
  );
  requireIncludes(
    text,
    "xiaoman_daily_case_report_worker_character_universe_profile_fact_text_included=false",
    label
  );
}

for (const [fragment, label] of [
  [
    '"daily_report_markdown": rendered.get("daily_report_markdown")',
    "worker test markdown retention guard",
  ],
  ['"people": character_universe.get("people")', "worker test raw universe node guard"],
]) {
  requireIncludes(backfillWorkerTest, fragment, label);
  requireNotIncludes(worker, fragment, "worker retained payload");
}

for (const [text, label] of [
  [readme, "workflow README"],
  [plan, "migration plan"],
  [observationRunbook, "production observation runbook"],
  [cronRunbook, "Hermes cron runbook"],
]) {
  requireIncludes(text, "character-universe", label);
}

for (const [text, label] of [
  [readme, "workflow README"],
  [plan, "migration plan"],
  [observationRunbook, "production observation runbook"],
]) {
  requireIncludes(text, "raw", label);
}
for (const fragment of ["Markdown", "people labels", "story labels", "excerpts"]) {
  requireIncludes(cronRunbook, fragment, "Hermes cron runbook private-output boundary");
}

if (errors.length > 0) {
  process.stderr.write(
    `Xiaoman daily case report character-universe local check failed:\n${errors
      .map((error) => `- ${error}`)
      .join("\n")}\n`
  );
  process.exit(1);
}

process.stdout.write(
  "Xiaoman daily case report character-universe local check passed.\n"
);
