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
const applyCreativeProfiles = requireFile(
  "workflows/xiaoman-daily-case-report/apply_creative_profile_candidates.py"
);
const applyCreativeProfilesTests = requireFile(
  "workflows/xiaoman-daily-case-report/tests/test_apply_creative_profile_candidates.py"
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
  ["def _build_quote_map(", "daily workflow wx-cli-style quote map"],
  ["def _build_wiki_bundle(", "daily workflow wx-cli-style wiki bundle"],
  ["def _build_run_manifest(", "daily workflow wx-cli-style run manifest"],
  ["def _render_review_report(", "daily workflow wx-cli-style review report"],
  ["def _is_time_bucket_topic(", "daily workflow excludes time-bucket wiki topics"],
  ["def _relationship_hints(", "daily workflow same-topic relationship hints"],
  ["def _relationship_candidates(", "daily workflow relationship render candidates"],
  ['"schema_version": "xiaoman-character-universe-v1"', "universe schema"],
  ['"source": "daily_case_report_second_pass"', "universe source"],
  ['"raw_messages_included": False', "raw message exclusion flag"],
  ['"profile_fact_text_included": False', "profile fact exclusion flag"],
  ['"memes": memes', "universe meme candidates"],
  ['"callbacks": callbacks', "universe callback candidates"],
  ['"relationships": relationships', "universe relationship candidates"],
  [
    '"creative_profile_candidates": creative_profile_candidates',
    "universe creative-profile candidates",
  ],
  [
    '"creative_profile_candidate_policy": {',
    "universe creative-profile candidate policy",
  ],
  [
    '"evidence_anchor": character_anchor(character)',
    "creative-profile candidates carry safe evidence anchors",
  ],
  [
    '"recurrence_evidence_count": character_evidence_count(character)',
    "creative-profile candidates carry recurrence evidence count",
  ],
  [
    '"minimum_recurrence_met": character_evidence_count(character) >= 2',
    "creative-profile candidates expose minimum recurrence gate",
  ],
  [
    '"profile_upgrade_status": character_upgrade_status(character)',
    "creative-profile candidates expose upgrade status",
  ],
  [
    '"blocked_reason": (',
    "creative-profile candidates preserve daily-note block reason",
  ],
  [
    '"writes_member_profile_snapshots": False',
    "creative-profile candidates do not write snapshots",
  ],
  [
    '"public_surface_allowed": False',
    "creative-profile candidates are not public-surface allowed",
  ],
  ['relation": "co_discusses_topic"', "same-topic relationship edge"],
  ["memory_weight_label", "public-safe memory weight label"],
  ["meme_seed", "public-safe meme seed"],
  ["arc_label", "daily character arc label"],
  ['"daily_report_markdown_path"', "Markdown report output path"],
  ['"character_universe_path"', "character universe output path"],
  ['"quote_map_path"', "quote map output path"],
  ['"wiki_bundle_path"', "wiki bundle output path"],
  ['"run_manifest_path"', "run manifest output path"],
  ['"review_report_path"', "review report output path"],
  ['"private_review_bundle"', "private review bundle summary"],
  [".character-universe.json", "private universe file output"],
  [".quote-map.json", "private quote-map file output"],
  [".wiki-bundle.json", "private wiki-bundle file output"],
  [".run-manifest.json", "private run-manifest file output"],
  [".review.md", "private review report file output"],
  ['"schema_version": "xiaoman-daily-quote-map-v1"', "quote-map schema"],
  ['"schema_version": "xiaoman-daily-wiki-bundle-v1"', "wiki-bundle schema"],
  ['"schema_version": "xiaoman-daily-run-manifest-v1"', "run-manifest schema"],
  [
    '"source": "wx_cli_style_daily_migration"',
    "private review bundle migration source",
  ],
  [
    '"latest_chat_records_preserved": True',
    "run manifest preserves latest chat records",
  ],
  [
    '"public_surface_allowed": False',
    "private review outputs are not public-surface allowed",
  ],
  [
    '"raw_message_rows_included": False',
    "private review outputs exclude raw message rows",
  ],
  [
    "except Exception:\n            character_memory = {}",
    "character memory soft dependency",
  ],
]) {
  requireIncludes(workflow, fragment, label);
}

for (const [fragment, label] of [
  [
    'APPLY_APPROVAL = "approved-production-xiaoman-creative-profile-candidates"',
    "creative-profile apply approval phrase",
  ],
  ['PROFILE_KIND = "creative_profile"', "creative-profile apply kind"],
  [
    'PROFILE_VERSION = "xiaoman-daily-creative-profile-v1"',
    "creative-profile apply version",
  ],
  [
    'item.get("profile_upgrade_status") != "eligible_for_review"',
    "creative-profile apply rejects non-eligible candidates",
  ],
  [
    'item.get("minimum_recurrence_met") is not True',
    "creative-profile apply requires minimum recurrence",
  ],
  [
    'item.get("public_surface_allowed") is not False',
    "creative-profile apply keeps public surface false",
  ],
  [
    'person_ids_included": False',
    "creative-profile apply sanitized report excludes person ids",
  ],
  ["member_facts_fact_text", "creative-profile apply do-not-disclose fact text"],
  [
    "exact owner approval is required for apply",
    "creative-profile apply fails closed without approval",
  ],
]) {
  requireIncludes(applyCreativeProfiles, fragment, label);
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
    "test_character_universe_exports_public_safe_memes_relationships_and_callbacks",
    "universe memes/relationships/callbacks regression test",
  ],
  [
    "test_hot_topics_include_case_storylines_as_wiki_topics",
    "case-storyline wiki topic regression test",
  ],
  [
    "test_private_review_bundle_exports_quote_map_wiki_and_run_manifest",
    "wx-cli-style private review bundle regression test",
  ],
  [
    'self.assertTrue(universe["creative_profile_candidates"])',
    "creative-profile candidate regression test",
  ],
  [
    "test_character_profile_candidate_keeps_single_day_signal_as_daily_note",
    "creative-profile daily-note-only regression test",
  ],
  [
    'candidate["profile_upgrade_status"], "daily_note_only"',
    "creative-profile upgrade gate assertion",
  ],
  [
    'self.assertFalse(candidate["minimum_recurrence_met"])',
    "creative-profile minimum recurrence assertion",
  ],
  [
    'self.assertIn("不能升级为长期人物画像", candidate["blocked_reason"])',
    "creative-profile blocked reason assertion",
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
  [
    'self.assertTrue(Path(result["quote_map_path"]).is_file())',
    "quote-map output file assertion",
  ],
  [
    'self.assertTrue(Path(result["wiki_bundle_path"]).is_file())',
    "wiki-bundle output file assertion",
  ],
  [
    'self.assertTrue(Path(result["run_manifest_path"]).is_file())',
    "run-manifest output file assertion",
  ],
  [
    'self.assertTrue(Path(result["review_report_path"]).is_file())',
    "review report output file assertion",
  ],
]) {
  requireIncludes(workflowTests, fragment, label);
}

for (const [fragment, label] of [
  [
    "test_daily_note_only_candidate_is_rejected_for_apply_payload",
    "creative-profile apply rejects daily notes",
  ],
  [
    "test_payload_requires_reviewed_person_id_not_display_name_guessing",
    "creative-profile apply requires reviewed person id",
  ],
  [
    "test_apply_uses_fixed_psql_stdin_and_does_not_echo_database_url",
    "creative-profile apply psql boundary test",
  ],
  [
    "test_main_dry_run_reports_sanitized_counts_only",
    "creative-profile apply sanitized report test",
  ],
  [
    "test_main_apply_requires_exact_owner_approval_before_database_url",
    "creative-profile apply approval test",
  ],
]) {
  requireIncludes(applyCreativeProfilesTests, fragment, label);
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
  [
    'private_review_bundle = rendered.get("private_review_bundle") or {}',
    "worker private review bundle safe summary read",
  ],
  ['"character_universe": {', "worker safe universe metadata"],
  ['"private_review_bundle": {', "worker safe private review bundle metadata"],
  [
    '"raw_messages_included": character_universe.get("raw_messages_included") is True',
    "worker raw flag binding",
  ],
  [
    '"profile_fact_text_included": character_universe.get("profile_fact_text_included") is True',
    "worker profile fact flag binding",
  ],
  [
    '"creative_profile_candidate_count": len(character_universe.get("creative_profile_candidates") or [])',
    "worker creative-profile candidate count",
  ],
  [
    '"creative_profile_public_surface_allowed": (',
    "worker creative-profile public-surface flag",
  ],
  [
    '"quote_map_entry_count": private_review_bundle.get("quote_map_entry_count", 0)',
    "worker quote-map count only",
  ],
  [
    '"wiki_counts": private_review_bundle.get("wiki_counts") or {}',
    "worker wiki counts only",
  ],
  ['"meme_count": len(character_universe.get("memes") or [])', "worker meme count"],
  [
    '"callback_count": len(character_universe.get("callbacks") or [])',
    "worker callback count",
  ],
  [
    '"relationship_count": len(character_universe.get("relationships") or [])',
    "worker relationship count",
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
  [
    'if universe.get("creative_profile_public_surface_allowed") is not False:',
    "worker-run creative-profile public-surface guard",
  ],
  [
    'if review_bundle.get("public_surface_allowed") is not False:',
    "worker-run private review bundle public-surface guard",
  ],
  [
    'if review_bundle.get("raw_message_rows_included") is not False:',
    "worker-run private review bundle raw rows guard",
  ],
  [
    'print(f"{key}_worker_private_review_bundle_quote_map_entry_count=',
    "worker-run quote-map count evidence",
  ],
  [
    'print(f"{key}_worker_private_review_bundle_wiki_people_count=',
    "worker-run wiki people count evidence",
  ],
  [
    'print(f"{key}_worker_character_universe_creative_profile_candidate_count=',
    "worker-run creative-profile candidate count evidence",
  ],
  [
    'print(f"{key}_worker_character_universe_creative_profile_public_surface_allowed=false")',
    "worker-run creative-profile public-surface evidence",
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
    "xiaoman_daily_case_report_worker_character_universe_(?:raw_messages_included|profile_fact_text_included|creative_profile_public_surface_allowed)",
    "runner allowlist for universe privacy flags",
  ],
  [
    "xiaoman_daily_case_report_worker_character_universe_(?:people_count|topic_count|event_count|meme_count|callback_count|relationship_count|creative_profile_candidate_count|storyline_candidate_count|edge_count)",
    "runner allowlist for universe counts",
  ],
  [
    "xiaoman_daily_case_report_worker_private_review_bundle_(?:public_surface_allowed|raw_message_rows_included|profile_fact_text_included)",
    "runner allowlist for private review bundle privacy flags",
  ],
  [
    "xiaoman_daily_case_report_worker_private_review_bundle_(?:quote_map_entry_count|wiki_people_count|wiki_event_count|wiki_storyline_count)",
    "runner allowlist for private review bundle counts",
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
  requireIncludes(
    text,
    "xiaoman_daily_case_report_worker_character_universe_creative_profile_public_surface_allowed=false",
    label
  );
  requireIncludes(
    text,
    "xiaoman_daily_case_report_worker_private_review_bundle_public_surface_allowed=false",
    label
  );
  requireIncludes(
    text,
    "xiaoman_daily_case_report_worker_private_review_bundle_quote_map_entry_count=13",
    label
  );
  requireIncludes(
    text,
    "xiaoman_daily_case_report_worker_character_universe_meme_count=4",
    label
  );
  requireIncludes(
    text,
    "xiaoman_daily_case_report_worker_character_universe_callback_count=4",
    label
  );
  requireIncludes(
    text,
    "xiaoman_daily_case_report_worker_character_universe_relationship_count=2",
    label
  );
  requireIncludes(
    text,
    "xiaoman_daily_case_report_worker_character_universe_creative_profile_candidate_count=4",
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
