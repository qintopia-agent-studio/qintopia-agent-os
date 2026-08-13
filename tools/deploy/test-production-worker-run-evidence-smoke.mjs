#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const repoRoot = process.cwd();
const tmpRoot = fs.mkdtempSync(path.join(os.tmpdir(), "qintopia-worker-run-evidence-"));
const sourceScript = path.join(
  repoRoot,
  "deploy/sidecar/scripts/production-worker-run-evidence-smoke.sh"
);

const check = (condition, message) => {
  if (!condition) {
    throw new Error(message);
  }
};

const writeFile = (relativePath, content, mode = 0o600) => {
  const filePath = path.join(tmpRoot, relativePath);
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content, "utf8");
  fs.chmodSync(filePath, mode);
  return filePath;
};

const writeSummary = (task, worker, dateField = "date") =>
  writeFile(
    `state/${task}/latest-summary.json`,
    `${JSON.stringify(
      {
        schema_version: 1,
        worker,
        requires_human_confirmation: true,
        external_send_executed: false,
        safe_for_member_chat: false,
        [dateField]: "2026-08-10",
      },
      null,
      2
    )}\n`
  );

const run = (target) =>
  spawnSync("bash", [testScript, target], {
    cwd: repoRoot,
    env: {
      ...process.env,
      QINTOPIA_PRODUCTION_WORKER_RUN_EVIDENCE_ENABLE: "1",
    },
    encoding: "utf8",
  });

const expectStatus = (result, status, label) => {
  check(
    result.status === status,
    `${label} exited ${result.status}, expected ${status}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`
  );
};

const expectNoLeak = (result, label) => {
  const combined = `${result.stdout}\n${result.stderr}`;
  for (const forbidden of [
    "postgres://",
    "secret-token",
    "raw worker output",
    "group-id-fixture",
  ]) {
    check(!combined.includes(forbidden), `${label} leaked ${forbidden}`);
  }
};

const python = spawnSync("which", ["python3"], { encoding: "utf8" }).stdout.trim();
check(Boolean(python), "python3 is required for the worker-run evidence fixture");

const sourceBody = fs.readFileSync(sourceScript, "utf8");
for (const forbidden of [
  "systemctl",
  "timer_name",
  "service_name",
  "ExecMainStartTimestampUSec",
]) {
  check(!sourceBody.includes(forbidden), `script still depends on ${forbidden}`);
}

const testScript = path.join(tmpRoot, "production-worker-run-evidence-smoke.sh");
const testBody = sourceBody
  .replaceAll("/usr/bin/python3", python)
  .replaceAll(
    "/home/ubuntu/.local/state/qintopia-agentos",
    path.join(tmpRoot, "state")
  );
fs.writeFileSync(testScript, testBody, "utf8");
fs.chmodSync(testScript, 0o755);

let result = run("xiaoman-daily-case-report-worker-run");
expectStatus(result, 0, "missing Hermes log");
check(
  result.stdout.trim() === "xiaoman_daily_case_report_worker_run_result=not_started",
  `missing log emitted unexpected evidence\n${result.stdout}`
);

writeFile("state/xiaoman-daily-case-report/hermes-cron.log", "");
result = run("xiaoman-daily-case-report-worker-run");
expectStatus(result, 0, "empty Hermes log");
check(
  result.stdout.trim() === "xiaoman_daily_case_report_worker_run_result=not_started",
  `empty log emitted unexpected evidence\n${result.stdout}`
);

writeFile(
  "state/xiaoman-daily-case-report/hermes-cron.log",
  [
    "2026-08-10T01:30:00Z xiaoman-daily-case-report run=ok",
    JSON.stringify(
      {
        success: true,
        worker: "xiaoman-daily-case-report-auto-publish-worker",
        media_uploaded: true,
        auto_publish_created: true,
        external_send_executed: false,
        content_metrics: {
          message_count: 118,
          participant_count: 24,
          case_count: 6,
          character_count: 4,
          hot_topic_count: 3,
        },
        character_universe: {
          schema_version: "xiaoman-character-universe-v1",
          source: "daily_case_report_second_pass",
          retained_source_policy: "curated_summary_only",
          raw_messages_included: false,
          profile_fact_text_included: false,
          people_count: 4,
          topic_count: 3,
          event_count: 6,
          meme_count: 4,
          callback_count: 4,
          relationship_count: 2,
          expressive_label_candidate_count: 3,
          reviewed_public_expressive_label_count: 1,
          unreviewed_expressive_labels_public_surface_allowed: false,
          creative_profile_candidate_count: 4,
          creative_profile_public_surface_allowed: false,
          creative_universe_candidate_count: 6,
          creative_universe_public_surface_allowed: false,
          storyline_candidate_count: 5,
          edge_count: 7,
        },
        public_output_style: {
          schema_version: "xiaoman-daily-public-output-style-v1",
          character_daily_layout: true,
          storyline_first: true,
          cast_notes_enabled: true,
          meme_callback_section_enabled: true,
          relationship_section_enabled: true,
          owner_reviewed_expressive_labels_only: true,
          image_first_delivery: true,
          pdf_default_delivery: false,
          roast_review_boundary: true,
          private_draft_only: true,
          public_surface_contains_private_draft: false,
        },
        private_review_bundle: {
          schema_version: "xiaoman-daily-private-review-bundle-v1",
          source: "wx_cli_style_daily_migration",
          public_surface_allowed: false,
          review_required: true,
          raw_message_rows_included: false,
          profile_fact_text_included: false,
          raw_message_payload_read: false,
          attachment_public_surface_allowed: false,
          quote_map_entry_count: 13,
          wiki_counts: {
            people: 4,
            events: 6,
            storylines: 5,
          },
          draft_counts: {
            roast_profile_candidate_count: 4,
            ordinary_digest_local_life_note_count: 2,
            storyline_timeline_count: 6,
            lookback_callback_count: 9,
          },
        },
      },
      null,
      2
    ),
    "raw worker output with group-id-fixture",
    "",
  ].join("\n")
);
result = run("xiaoman-daily-case-report-worker-run");
expectStatus(result, 0, "daily case report success summary");
expectNoLeak(result, "daily case report success summary");
check(
  result.stdout.includes("xiaoman_daily_case_report_worker_run_result=success") &&
    result.stdout.includes("xiaoman_daily_case_report_worker_character_count=4") &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_character_universe_schema_version=xiaoman-character-universe-v1"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_character_universe_raw_messages_included=false"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_character_universe_profile_fact_text_included=false"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_character_universe_meme_count=4"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_character_universe_callback_count=4"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_character_universe_relationship_count=2"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_character_universe_expressive_label_candidate_count=3"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_character_universe_reviewed_public_expressive_label_count=1"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_character_universe_unreviewed_expressive_labels_public_surface_allowed=false"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_character_universe_creative_profile_candidate_count=4"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_character_universe_creative_profile_public_surface_allowed=false"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_character_universe_creative_universe_candidate_count=6"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_character_universe_creative_universe_public_surface_allowed=false"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_character_universe_storyline_candidate_count=5"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_public_output_style_schema_version=xiaoman-daily-public-output-style-v1"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_public_output_style_character_daily_layout=true"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_public_output_style_storyline_first=true"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_public_output_style_cast_notes_enabled=true"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_public_output_style_meme_callback_section_enabled=true"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_public_output_style_relationship_section_enabled=true"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_public_output_style_owner_reviewed_expressive_labels_only=true"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_public_output_style_image_first_delivery=true"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_public_output_style_pdf_default_delivery=false"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_public_output_style_roast_review_boundary=true"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_public_output_style_private_draft_only=true"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_public_output_style_public_surface_contains_private_draft=false"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_private_review_bundle_public_surface_allowed=false"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_private_review_bundle_review_required=true"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_private_review_bundle_raw_message_rows_included=false"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_private_review_bundle_profile_fact_text_included=false"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_private_review_bundle_raw_message_payload_read=false"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_private_review_bundle_attachment_public_surface_allowed=false"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_private_review_bundle_quote_map_entry_count=13"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_private_review_bundle_wiki_people_count=4"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_private_review_bundle_wiki_event_count=6"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_private_review_bundle_wiki_storyline_count=5"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_private_review_bundle_draft_roast_profile_candidate_count=4"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_private_review_bundle_draft_ordinary_digest_local_life_note_count=2"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_private_review_bundle_draft_storyline_timeline_count=6"
    ) &&
    result.stdout.includes(
      "xiaoman_daily_case_report_worker_private_review_bundle_draft_lookback_callback_count=9"
    ),
  `daily case report success emitted unexpected evidence\n${result.stdout}`
);

writeFile(
  "state/xiaoman-daily-case-report/hermes-cron.log",
  [
    "2026-08-10T01:30:00Z xiaoman-daily-case-report run=ok",
    "raw worker output with group-id-fixture",
    "2026-08-10T01:31:00Z xiaoman-weekly-preview run=ok",
    JSON.stringify(
      {
        worker: "xiaoman-daily-case-report-auto-publish-worker",
        content_metrics: { character_count: 99 },
        character_universe: {
          schema_version: "xiaoman-character-universe-v1",
          source: "daily_case_report_second_pass",
          raw_messages_included: false,
          profile_fact_text_included: false,
        },
      },
      null,
      2
    ),
    "",
  ].join("\n")
);
result = run("xiaoman-daily-case-report-worker-run");
expectStatus(result, 0, "daily summary stops at next sentinel");
expectNoLeak(result, "daily summary stops at next sentinel");
check(
  result.stdout.includes("xiaoman_daily_case_report_worker_run_result=success") &&
    result.stdout.includes("xiaoman_daily_case_report_worker_summary_present=false") &&
    !result.stdout.includes("xiaoman_daily_case_report_worker_character_count=99"),
  `daily parser crossed next sentinel\n${result.stdout}`
);

writeFile(
  "state/xiaoman-daily-case-report/hermes-cron.log",
  [
    "2026-08-10T01:30:00Z xiaoman-daily-case-report run=ok",
    JSON.stringify(
      {
        worker: "xiaoman-daily-case-report-auto-publish-worker",
        content_metrics: { character_count: 1 },
        character_universe: {
          raw_messages_included: true,
          profile_fact_text_included: false,
        },
      },
      null,
      2
    ),
    "",
  ].join("\n")
);
result = run("xiaoman-daily-case-report-worker-run");
expectStatus(result, 1, "unsafe daily case report summary");
expectNoLeak(result, "unsafe daily case report summary");
check(
  result.stdout.trim() ===
    "xiaoman_daily_case_report_worker_run_error=daily_case_report_summary_invalid",
  `unsafe daily summary emitted unexpected evidence\n${result.stdout}`
);

writeFile(
  "state/xiaoman-daily-case-report/hermes-cron.log",
  [
    "2026-08-10T01:30:00Z xiaoman-daily-case-report run=ok",
    JSON.stringify(
      {
        worker: "xiaoman-daily-case-report-auto-publish-worker",
        content_metrics: { character_count: 1 },
        character_universe: {
          raw_messages_included: false,
          profile_fact_text_included: false,
          creative_profile_public_surface_allowed: false,
        },
        private_review_bundle: {
          public_surface_allowed: true,
          review_required: true,
          raw_message_rows_included: false,
          profile_fact_text_included: false,
          raw_message_payload_read: false,
          attachment_public_surface_allowed: false,
        },
      },
      null,
      2
    ),
    "",
  ].join("\n")
);
result = run("xiaoman-daily-case-report-worker-run");
expectStatus(result, 1, "public private review bundle summary");
expectNoLeak(result, "public private review bundle summary");
check(
  result.stdout.trim() ===
    "xiaoman_daily_case_report_worker_run_error=daily_case_report_summary_invalid",
  `unsafe private review bundle emitted unexpected evidence\n${result.stdout}`
);

writeFile(
  "state/xiaoman-daily-case-report/hermes-cron.log",
  [
    "2026-08-10T01:30:00Z xiaoman-daily-case-report run=ok",
    JSON.stringify(
      {
        worker: "xiaoman-daily-case-report-auto-publish-worker",
        content_metrics: { character_count: 1 },
        character_universe: {
          raw_messages_included: false,
          profile_fact_text_included: false,
          creative_profile_public_surface_allowed: false,
          creative_universe_public_surface_allowed: false,
          unreviewed_expressive_labels_public_surface_allowed: false,
        },
        public_output_style: {
          character_daily_layout: true,
          storyline_first: true,
          cast_notes_enabled: true,
          meme_callback_section_enabled: true,
          relationship_section_enabled: true,
          owner_reviewed_expressive_labels_only: true,
          image_first_delivery: true,
          pdf_default_delivery: false,
          roast_review_boundary: true,
          private_draft_only: true,
          public_surface_contains_private_draft: false,
        },
        private_review_bundle: {
          public_surface_allowed: false,
          review_required: true,
          raw_message_rows_included: false,
          profile_fact_text_included: false,
          raw_message_payload_read: true,
          attachment_public_surface_allowed: false,
        },
      },
      null,
      2
    ),
    "",
  ].join("\n")
);
result = run("xiaoman-daily-case-report-worker-run");
expectStatus(result, 1, "raw payload private review bundle summary");
expectNoLeak(result, "raw payload private review bundle summary");
check(
  result.stdout.trim() ===
    "xiaoman_daily_case_report_worker_run_error=daily_case_report_summary_invalid",
  `unsafe raw payload review bundle emitted unexpected evidence\n${result.stdout}`
);

writeFile(
  "state/erhua-morning-brief/hermes-cron.log",
  [
    "raw worker output with postgres://secret@example.invalid/qintopia",
    "2026-08-10T01:30:00Z erhua-morning-brief run=ok",
    "QIWE_TOKEN=secret-token",
    "",
  ].join("\n")
);
result = run("erhua-morning-brief-worker-run");
expectStatus(result, 1, "Erhua success requires summary");
expectNoLeak(result, "Erhua success requires summary");
check(
  result.stdout.trim() === "erhua_morning_brief_worker_run_error=summary_invalid",
  `Erhua missing summary emitted unexpected evidence\n${result.stdout}`
);

writeFile(
  "state/erhua-morning-brief/hermes-cron.log",
  [
    "raw worker output with postgres://secret@example.invalid/qintopia",
    "2026-08-10T01:30:00Z erhua-morning-brief run=ok",
    JSON.stringify(
      {
        success: true,
        worker: "erhua-morning-brief-worker",
        brief_date: "2026-08-10",
        activity_publishable_count: 1,
        sunday_no_publishable_activity_followup: false,
        ai_news_item_count: 5,
        artifact_created: true,
        artifact_id: "group-id-fixture",
        work_item_id: "secret-token",
        requires_human_confirmation: true,
        external_send_executed: false,
        send_request_created: false,
      },
      null,
      2
    ),
    JSON.stringify(
      {
        success: true,
        worker: "erhua-morning-brief-auto-publish",
        qiwe_text_send_action_status: "text_send_executed",
        work_item_id: "group-id-fixture",
        external_send_executed: true,
      },
      null,
      2
    ),
    "",
  ].join("\n")
);
result = run("erhua-morning-brief-worker-run");
expectStatus(result, 0, "Erhua success summary");
expectNoLeak(result, "Erhua success log");
check(
  result.stdout.includes("erhua_morning_brief_worker_run_result=success") &&
    result.stdout.includes("erhua_morning_brief_worker_run_epoch=1786325400") &&
    result.stdout.includes("erhua_morning_brief_worker_summary_present=true") &&
    result.stdout.includes("erhua_morning_brief_worker_artifact_created=true") &&
    result.stdout.includes("erhua_morning_brief_worker_activity_publishable_count=1") &&
    result.stdout.includes("erhua_morning_brief_worker_ai_news_item_count=5") &&
    result.stdout.includes(
      "erhua_morning_brief_worker_sunday_no_publishable_activity_followup=false"
    ) &&
    result.stdout.includes(
      "erhua_morning_brief_worker_auto_publish_summary_present=true"
    ) &&
    result.stdout.includes(
      "erhua_morning_brief_worker_auto_publish_external_send_executed=true"
    ),
  `Erhua success emitted unexpected evidence\n${result.stdout}`
);

writeFile(
  "state/xiaoman-weekly-preview/hermes-cron.log",
  [
    "2026-08-10T00:00:00Z xiaoman-weekly-preview run=failed exit=7",
    "raw worker output with group-id-fixture",
    "2026-08-10T01:30:00Z xiaoman-weekly-preview run=ok",
    "",
  ].join("\n")
);
writeSummary("xiaoman-weekly-preview", "xiaoman-weekly-preview-worker", "week_start");
result = run("xiaoman-weekly-preview-worker-run");
expectStatus(result, 0, "weekly preview success log");
expectNoLeak(result, "weekly preview success log");
check(
  result.stdout.includes("xiaoman_weekly_preview_worker_run_result=success") &&
    result.stdout.includes("xiaoman_weekly_preview_worker_summary_present=true") &&
    result.stdout.includes("xiaoman_weekly_preview_worker_summary_date=2026-08-10"),
  `weekly preview success emitted unexpected evidence\n${result.stdout}`
);

writeFile(
  "state/xiaoman-weekly-recruitment/hermes-cron.log",
  [
    "2026-08-10T01:30:00Z xiaoman-weekly-recruitment run=ok",
    "2026-08-10T01:40:00Z xiaoman-weekly-recruitment run=failed exit=2",
    "",
  ].join("\n")
);
writeSummary("xiaoman-weekly-recruitment", "xiaoman-weekly-recruitment-worker");
result = run("xiaoman-weekly-recruitment-worker-run");
expectStatus(result, 1, "latest failed sentinel");
expectNoLeak(result, "latest failed sentinel");
check(
  result.stdout.trim() === "xiaoman_weekly_recruitment_worker_run_error=worker_failed",
  `latest failed sentinel emitted unexpected evidence\n${result.stdout}`
);

writeFile(
  "state/xiaoman-weekly-plan-confirmation/hermes-cron.log",
  "2026-08-10T01:30:00Z xiaoman-weekly-plan-confirmation run=ok\n"
);
writeSummary("xiaoman-weekly-plan-confirmation", "unexpected-worker");
result = run("xiaoman-weekly-plan-confirmation-worker-run");
expectStatus(result, 1, "invalid weekly summary");
expectNoLeak(result, "invalid weekly summary");
check(
  result.stdout.trim() ===
    "xiaoman_weekly_plan_confirmation_worker_run_error=summary_invalid",
  `invalid summary emitted unexpected evidence\n${result.stdout}`
);

console.log("production worker-run evidence smoke fixture passed");
