//! Daily case-report collection preview (PR 2 of the Rust migration plan
//! `docs/plans/active/xiaoman-daily-case-report-rust-migration.md`).
//!
//! Mirrors the query semantics of `workflows/xiaoman-daily-case-report/collector.py`
//! (psycopg variant) and emits a sanitized JSON summary only: counts, SHA-256
//! hashes, byte sizes, and presence flags. Raw message text, sender names, chat
//! ids, and person ids never leave the process.

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{PgExecutor, PgPool};

use crate::config::Cli;

const WORKER_ID: &str = "daily-case-report-collect-preview";
const PROTOCOL: &str = "daily_case_report_collect_preview_v1";
const MEMORY_LOOKBACK_DAYS: i64 = 90;
const MAX_ITEMS: usize = 200;
const MEMORY_FACT_TYPES: [&str; 7] = [
    "activity_organizer",
    "activity_participation",
    "content_story_lead",
    "operation_signal",
    "resource_scout",
    "service_need",
    "unresolved_question",
];
const CREATIVE_PROFILE_KIND: &str = "creative_profile";
const CREATIVE_PROFILE_VERSION: &str = "xiaoman-daily-creative-profile-v1";

#[derive(Debug, Clone)]
pub struct CollectPreviewArgs {
    pub chat_id: Option<String>,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

impl CollectPreviewArgs {
    pub fn parse(chat_id: Option<String>, start: &str, end: &str) -> Result<Self> {
        let start = DateTime::parse_from_rfc3339(start)
            .with_context(|| "window start must be RFC 3339")?
            .with_timezone(&Utc);
        let end = DateTime::parse_from_rfc3339(end)
            .with_context(|| "window end must be RFC 3339")?
            .with_timezone(&Utc);
        if end <= start {
            bail!("window end must be after window start");
        }
        Ok(Self {
            chat_id,
            start,
            end,
        })
    }
}

#[derive(Debug, Serialize)]
struct WindowSummary {
    start_rfc3339: String,
    end_rfc3339: String,
    chat_id_sha256: Option<String>,
    memory_lookback_days: i64,
}

#[derive(Debug, Serialize)]
struct MessageItem {
    id_sha256: String,
    text_byte_count: usize,
    report_time: String,
    sender_person_id_present: bool,
}

#[derive(Debug, Serialize)]
struct MessagesSummary {
    count: usize,
    distinct_sender_count: usize,
    senders_with_person_id_count: usize,
    total_text_byte_count: usize,
    first_report_time: Option<String>,
    last_report_time: Option<String>,
    items_truncated: bool,
    items: Vec<MessageItem>,
}

#[derive(Debug, Serialize)]
struct CharacterMemoryPerson {
    person_id_sha256: String,
    recent_fact_count: i32,
    lifetime_fact_count: i32,
    dominant_fact_type: String,
}

#[derive(Debug, Serialize)]
struct CharacterMemorySummary {
    person_count: usize,
    total_recent_fact_count: i64,
    total_lifetime_fact_count: i64,
    persons: Vec<CharacterMemoryPerson>,
}

#[derive(Debug, Serialize)]
struct CreativeProfilePerson {
    person_id_sha256: String,
    communication_style_present: bool,
    safe_reply_hints_key_count: i32,
}

#[derive(Debug, Serialize)]
struct CreativeProfileSummary {
    person_count: usize,
    persons: Vec<CreativeProfilePerson>,
}

#[derive(Debug, Serialize)]
pub struct CollectPreviewReport {
    success: bool,
    worker: &'static str,
    action_status: &'static str,
    protocol: &'static str,
    safe_for_chat: bool,
    window: WindowSummary,
    messages: MessagesSummary,
    character_memory: CharacterMemorySummary,
    creative_profile_memory: CreativeProfileSummary,
    limitations: [&'static str; 4],
}

type MessageTuple = (String, String, String, DateTime<Utc>, Option<String>);
type CharacterMemoryTuple = (String, i32, i32, String);
type CreativeProfileTuple = (String, bool, i32);

#[derive(Debug, Clone, PartialEq)]
struct MessageRow {
    id: String,
    sender_id: String,
    text: String,
    report_time: DateTime<Utc>,
    sender_person_id: Option<String>,
}

impl From<MessageTuple> for MessageRow {
    fn from((id, sender_id, text, report_time, sender_person_id): MessageTuple) -> Self {
        Self {
            id,
            sender_id,
            text,
            report_time,
            sender_person_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct CharacterMemoryRow {
    person_id: String,
    recent_fact_count: i32,
    lifetime_fact_count: i32,
    dominant_fact_type: String,
}

impl From<CharacterMemoryTuple> for CharacterMemoryRow {
    fn from(
        (person_id, recent_fact_count, lifetime_fact_count, dominant_fact_type): CharacterMemoryTuple,
    ) -> Self {
        Self {
            person_id,
            recent_fact_count,
            lifetime_fact_count,
            dominant_fact_type,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct CreativeProfileRow {
    person_id: String,
    communication_style_present: bool,
    safe_reply_hints_key_count: i32,
}

impl From<CreativeProfileTuple> for CreativeProfileRow {
    fn from(
        (person_id, communication_style_present, safe_reply_hints_key_count): CreativeProfileTuple,
    ) -> Self {
        Self {
            person_id,
            communication_style_present,
            safe_reply_hints_key_count,
        }
    }
}

const MESSAGES_SQL: &str = r#"
    SELECT
        m.id::text AS id,
        COALESCE(m.sender_id, '') AS sender_id,
        COALESCE(m.text, '') AS text,
        COALESCE(m.sent_at, m.received_at) AS report_time,
        m.sender_person_id::text AS sender_person_id
    FROM qintopia_messages.messages m
    WHERE m.platform = 'qiwe'
      AND m.chat_type = 'group'
      AND m.message_kind = 'text'
      AND NULLIF(BTRIM(m.text), '') IS NOT NULL
      AND COALESCE(m.sent_at, m.received_at) >= $1
      AND COALESCE(m.sent_at, m.received_at) < $2
      AND ($3::text IS NULL OR m.chat_id = $3)
    ORDER BY COALESCE(m.sent_at, m.received_at) ASC
"#;

const CHARACTER_MEMORY_SQL: &str = r#"
    WITH facts AS (
        SELECT
            mf.person_id::text AS person_id,
            mf.fact_type,
            mf.observed_at
        FROM qintopia_identity.member_facts mf
        WHERE mf.person_id::text = ANY($1::text[])
          AND mf.revoked_at IS NULL
          AND mf.fact_type = ANY($2::text[])
          AND mf.observed_at < $3
    ),
    role_counts AS (
        SELECT person_id, fact_type, count(*)::int AS fact_count
        FROM facts
        GROUP BY person_id, fact_type
    ),
    dominant AS (
        SELECT DISTINCT ON (person_id) person_id, fact_type
        FROM role_counts
        ORDER BY person_id, fact_count DESC, fact_type ASC
    )
    SELECT
        facts.person_id,
        count(*) FILTER (WHERE facts.observed_at >= $4)::int AS recent_fact_count,
        count(*)::int AS lifetime_fact_count,
        dominant.fact_type AS dominant_fact_type
    FROM facts
    JOIN dominant ON dominant.person_id = facts.person_id
    GROUP BY facts.person_id, dominant.fact_type
"#;

const CREATIVE_PROFILE_SQL: &str = r#"
    SELECT DISTINCT ON (s.person_id)
        s.person_id::text AS person_id,
        (s.communication_style IS NOT NULL) AS communication_style_present,
        COALESCE(
            (SELECT count(*)::int FROM jsonb_object_keys(s.safe_reply_hints)),
            0
        ) AS safe_reply_hints_key_count
    FROM qintopia_identity.member_profile_snapshots s
    WHERE s.person_id::text = ANY($1::text[])
      AND s.profile_kind = $2
      AND s.profile_version = $3
      AND s.status = 'active'
      AND s.reviewed_at IS NOT NULL
      AND s.generated_at < $4
      AND COALESCE((s.do_not_disclose->>'public_surface_allowed')::boolean, false) = false
      AND COALESCE((s.safe_reply_hints->>'public_surface_allowed')::boolean, false) = false
    ORDER BY s.person_id, s.reviewed_at DESC NULLS LAST, s.generated_at DESC
"#;

fn is_uuid_shape(value: &str) -> bool {
    (32..=36).contains(&value.len()) && value.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

fn sha256_str(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

async fn fetch_messages(
    executor: impl PgExecutor<'_>,
    args: &CollectPreviewArgs,
) -> Result<Vec<MessageRow>> {
    let rows: Vec<MessageTuple> = sqlx::query_as(MESSAGES_SQL)
        .bind(args.start)
        .bind(args.end)
        .bind(args.chat_id.as_deref())
        .fetch_all(executor)
        .await
        .with_context(|| "daily case-report message collection query failed")?;
    Ok(rows.into_iter().map(MessageRow::from).collect())
}

async fn fetch_character_memory(
    executor: impl PgExecutor<'_>,
    person_ids: &[String],
    end: DateTime<Utc>,
) -> Result<Vec<CharacterMemoryRow>> {
    let memory_start = end - Duration::days(MEMORY_LOOKBACK_DAYS);
    let rows: Vec<CharacterMemoryTuple> = sqlx::query_as(CHARACTER_MEMORY_SQL)
        .bind(person_ids)
        .bind(&MEMORY_FACT_TYPES[..])
        .bind(end)
        .bind(memory_start)
        .fetch_all(executor)
        .await
        .with_context(|| "daily case-report character memory query failed")?;
    Ok(rows.into_iter().map(CharacterMemoryRow::from).collect())
}

async fn fetch_creative_profiles(
    executor: impl PgExecutor<'_>,
    person_ids: &[String],
    end: DateTime<Utc>,
) -> Result<Vec<CreativeProfileRow>> {
    let rows: Vec<CreativeProfileTuple> = sqlx::query_as(CREATIVE_PROFILE_SQL)
        .bind(person_ids)
        .bind(CREATIVE_PROFILE_KIND)
        .bind(CREATIVE_PROFILE_VERSION)
        .bind(end)
        .fetch_all(executor)
        .await
        .with_context(|| "daily case-report creative profile query failed")?;
    Ok(rows.into_iter().map(CreativeProfileRow::from).collect())
}

fn collected_person_ids(messages: &[MessageRow]) -> Vec<String> {
    let mut person_ids: Vec<String> = messages
        .iter()
        .filter_map(|row| row.sender_person_id.clone())
        .filter(|person_id| is_uuid_shape(person_id))
        .collect();
    person_ids.sort();
    person_ids.dedup();
    person_ids
}

pub async fn collect_preview(
    pool: &PgPool,
    args: &CollectPreviewArgs,
) -> Result<CollectPreviewReport> {
    let messages = fetch_messages(pool, args).await?;
    let person_ids = collected_person_ids(&messages);
    let (character_rows, creative_rows) = if person_ids.is_empty() {
        (Vec::new(), Vec::new())
    } else {
        (
            fetch_character_memory(pool, &person_ids, args.end).await?,
            fetch_creative_profiles(pool, &person_ids, args.end).await?,
        )
    };
    Ok(build_report(
        args,
        &messages,
        &character_rows,
        &creative_rows,
    ))
}

fn build_report(
    args: &CollectPreviewArgs,
    messages: &[MessageRow],
    character_rows: &[CharacterMemoryRow],
    creative_rows: &[CreativeProfileRow],
) -> CollectPreviewReport {
    let mut distinct_senders: Vec<&str> =
        messages.iter().map(|row| row.sender_id.as_str()).collect();
    distinct_senders.sort_unstable();
    distinct_senders.dedup();

    let items: Vec<MessageItem> = messages
        .iter()
        .take(MAX_ITEMS)
        .map(|row| MessageItem {
            id_sha256: sha256_str(&row.id),
            text_byte_count: row.text.len(),
            report_time: row.report_time.to_rfc3339(),
            sender_person_id_present: row.sender_person_id.is_some(),
        })
        .collect();

    let mut character_persons: Vec<CharacterMemoryPerson> = character_rows
        .iter()
        .map(|row| CharacterMemoryPerson {
            person_id_sha256: sha256_str(&row.person_id),
            recent_fact_count: row.recent_fact_count,
            lifetime_fact_count: row.lifetime_fact_count,
            dominant_fact_type: row.dominant_fact_type.clone(),
        })
        .collect();
    character_persons.sort_by(|a, b| a.person_id_sha256.cmp(&b.person_id_sha256));

    let mut creative_persons: Vec<CreativeProfilePerson> = creative_rows
        .iter()
        .map(|row| CreativeProfilePerson {
            person_id_sha256: sha256_str(&row.person_id),
            communication_style_present: row.communication_style_present,
            safe_reply_hints_key_count: row.safe_reply_hints_key_count,
        })
        .collect();
    creative_persons.sort_by(|a, b| a.person_id_sha256.cmp(&b.person_id_sha256));

    CollectPreviewReport {
        success: true,
        worker: WORKER_ID,
        action_status: "collect_preview_ok",
        protocol: PROTOCOL,
        safe_for_chat: false,
        window: WindowSummary {
            start_rfc3339: args.start.to_rfc3339(),
            end_rfc3339: args.end.to_rfc3339(),
            chat_id_sha256: args.chat_id.as_deref().map(sha256_str),
            memory_lookback_days: MEMORY_LOOKBACK_DAYS,
        },
        messages: MessagesSummary {
            count: messages.len(),
            distinct_sender_count: distinct_senders.len(),
            senders_with_person_id_count: messages
                .iter()
                .filter(|row| row.sender_person_id.is_some())
                .count(),
            total_text_byte_count: messages.iter().map(|row| row.text.len()).sum(),
            first_report_time: messages.first().map(|row| row.report_time.to_rfc3339()),
            last_report_time: messages.last().map(|row| row.report_time.to_rfc3339()),
            items_truncated: messages.len() > MAX_ITEMS,
            items,
        },
        character_memory: CharacterMemorySummary {
            person_count: character_persons.len(),
            total_recent_fact_count: character_persons
                .iter()
                .map(|person| i64::from(person.recent_fact_count))
                .sum(),
            total_lifetime_fact_count: character_persons
                .iter()
                .map(|person| i64::from(person.lifetime_fact_count))
                .sum(),
            persons: character_persons,
        },
        creative_profile_memory: CreativeProfileSummary {
            person_count: creative_persons.len(),
            persons: creative_persons,
        },
        limitations: [
            "preview emits counts, hashes, and byte sizes only; raw text and identifiers never leave the process",
            "message items are capped at 200 with items_truncated marking the remainder",
            "collection mirrors collector.py semantics; analysis and rendering are later migration phases",
            "no user-visible output; safe_for_chat is always false",
        ],
    }
}

pub async fn run_collect_preview_cli(
    cli: &Cli,
    chat_id: Option<String>,
    start: &str,
    end: &str,
) -> Result<()> {
    let args = CollectPreviewArgs::parse(chat_id, start, end)?;
    let database_url = cli.database_url_required()?;
    let pool: PgPool = crate::db::connect(database_url, cli.db_max_connections).await?;
    let report = collect_preview(&pool, &args).await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed_args() -> CollectPreviewArgs {
        CollectPreviewArgs::parse(
            Some("chat-secret-1".to_string()),
            "2026-08-15T00:00:00+08:00",
            "2026-08-16T00:00:00+08:00",
        )
        .expect("fixture window must parse")
    }

    fn person_a() -> String {
        "aaaaaaaa-1111-4111-8111-aaaaaaaaaaaa".to_string()
    }

    fn person_b() -> String {
        "bbbbbbbb-2222-4222-8222-bbbbbbbbbbbb".to_string()
    }

    fn sample_messages() -> Vec<MessageRow> {
        vec![
            MessageRow {
                id: "message-id-1".to_string(),
                sender_id: "sender-1".to_string(),
                text: "secret text one".to_string(),
                report_time: DateTime::parse_from_rfc3339("2026-08-15T01:00:00+00:00")
                    .unwrap()
                    .with_timezone(&Utc),
                sender_person_id: Some(person_a()),
            },
            MessageRow {
                id: "message-id-2".to_string(),
                sender_id: "sender-1".to_string(),
                text: "secret text two".to_string(),
                report_time: DateTime::parse_from_rfc3339("2026-08-15T02:00:00+00:00")
                    .unwrap()
                    .with_timezone(&Utc),
                sender_person_id: None,
            },
            MessageRow {
                id: "message-id-3".to_string(),
                sender_id: "sender-2".to_string(),
                text: "secret text three".to_string(),
                report_time: DateTime::parse_from_rfc3339("2026-08-15T03:00:00+00:00")
                    .unwrap()
                    .with_timezone(&Utc),
                sender_person_id: Some(person_b()),
            },
        ]
    }

    #[test]
    fn report_matches_golden_fixture_shape() {
        let character_rows = vec![
            CharacterMemoryRow {
                person_id: person_a(),
                recent_fact_count: 2,
                lifetime_fact_count: 9,
                dominant_fact_type: "activity_organizer".to_string(),
            },
            CharacterMemoryRow {
                person_id: person_b(),
                recent_fact_count: 0,
                lifetime_fact_count: 3,
                dominant_fact_type: "service_need".to_string(),
            },
        ];
        let creative_rows = vec![CreativeProfileRow {
            person_id: person_a(),
            communication_style_present: true,
            safe_reply_hints_key_count: 4,
        }];

        let report = build_report(
            &fixed_args(),
            &sample_messages(),
            &character_rows,
            &creative_rows,
        );
        let actual = serde_json::to_value(&report).expect("report must serialize");

        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../fixtures/daily_case_report_collect_preview.json"
        ))
        .expect("golden fixture must parse");
        assert_eq!(
            actual, fixture,
            "report must match the golden fixture field-for-field"
        );
    }

    #[test]
    fn report_never_contains_raw_private_values() {
        let report = build_report(
            &fixed_args(),
            &sample_messages(),
            &[CharacterMemoryRow {
                person_id: person_a(),
                recent_fact_count: 1,
                lifetime_fact_count: 5,
                dominant_fact_type: "operation_signal".to_string(),
            }],
            &[],
        );
        let serialized = serde_json::to_string(&report).expect("report must serialize");
        for forbidden in [
            "secret text",
            "message-id-1",
            "sender-1",
            "chat-secret-1",
            &person_a(),
            &person_b(),
        ] {
            assert!(
                !serialized.contains(forbidden),
                "sanitized report must not contain {forbidden}"
            );
        }
    }

    #[test]
    fn items_are_bounded_and_marked_truncated() {
        let args = fixed_args();
        let mut rows = Vec::new();
        for index in 0..(MAX_ITEMS + 5) {
            rows.push(MessageRow {
                id: format!("message-id-{index}"),
                sender_id: "sender-1".to_string(),
                text: "x".to_string(),
                report_time: args.start,
                sender_person_id: None,
            });
        }
        let report = build_report(&args, &rows, &[], &[]);
        assert_eq!(report.messages.items.len(), MAX_ITEMS);
        assert!(report.messages.items_truncated);
        assert_eq!(report.messages.count, MAX_ITEMS + 5);
    }

    #[test]
    fn uuid_shape_matches_python_filter() {
        assert!(is_uuid_shape("aaaaaaaa-1111-4111-8111-aaaaaaaaaaaa"));
        assert!(is_uuid_shape("aaaaaaaa111141118111aaaaaaaaaaaa"));
        assert!(!is_uuid_shape(""));
        assert!(!is_uuid_shape("short"));
        assert!(!is_uuid_shape("zzzzzzzz-1111-4111-8111-zzzzzzzzzzzz"));
        assert!(!is_uuid_shape(&"a".repeat(37)));
    }

    #[test]
    fn window_requires_end_after_start() {
        let result = CollectPreviewArgs::parse(
            None,
            "2026-08-16T00:00:00+08:00",
            "2026-08-15T00:00:00+08:00",
        );
        assert!(result.is_err());
    }

    #[cfg(feature = "postgres-integration-tests")]
    fn integration_database_url() -> String {
        assert_eq!(
            std::env::var("QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE").as_deref(),
            Ok("1"),
            "PostgreSQL integration test requires the explicit apply-smoke guard"
        );
        let database_url = std::env::var("QINTOPIA_SIDECAR_DATABASE_URL")
            .expect("PostgreSQL integration test requires QINTOPIA_SIDECAR_DATABASE_URL");
        let parsed = url::Url::parse(&database_url).expect("integration database URL must parse");
        assert!(matches!(
            parsed.host_str(),
            Some("127.0.0.1" | "localhost" | "::1")
        ));
        assert_eq!(parsed.path().trim_start_matches('/'), "qintopia_test");
        database_url
    }

    #[cfg(feature = "postgres-integration-tests")]
    #[tokio::test]
    #[ignore = "requires disposable PostgreSQL"]
    async fn postgres_collection_matches_python_semantics() {
        let database_url = integration_database_url();
        let pool = crate::db::connect(&database_url, 2)
            .await
            .expect("integration database must connect");
        let mut tx = pool.begin().await.expect("test transaction must begin");

        sqlx::query("CREATE SCHEMA IF NOT EXISTS qintopia_messages")
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("CREATE SCHEMA IF NOT EXISTS qintopia_identity")
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS qintopia_messages.messages (
                id uuid PRIMARY KEY,
                platform text NOT NULL,
                message_id text NOT NULL,
                event_id text NOT NULL,
                chat_id text NOT NULL,
                chat_type text NOT NULL,
                sender_id text NOT NULL,
                sender_name text,
                message_kind text NOT NULL,
                text text,
                sent_at timestamptz,
                received_at timestamptz NOT NULL,
                sender_person_id uuid
            )",
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS qintopia_identity.member_facts (
                person_id uuid NOT NULL,
                fact_type text NOT NULL,
                fact_text text NOT NULL,
                evidence_type text NOT NULL,
                observed_at timestamptz NOT NULL,
                revoked_at timestamptz
            )",
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS qintopia_identity.member_profile_snapshots (
                person_id uuid NOT NULL,
                profile_kind text NOT NULL,
                profile_version text NOT NULL,
                summary text NOT NULL,
                status text NOT NULL,
                reviewed_at timestamptz,
                generated_at timestamptz NOT NULL,
                do_not_disclose jsonb,
                safe_reply_hints jsonb,
                communication_style jsonb
            )",
        )
        .execute(&mut *tx)
        .await
        .unwrap();

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS qintopia_identity.persons (
                id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
                display_name text NOT NULL
            )",
        )
        .execute(&mut *tx)
        .await
        .unwrap();

        let person_a = "aaaaaaaa-1111-4111-8111-aaaaaaaaaaaa";
        let person_b = "bbbbbbbb-2222-4222-8222-bbbbbbbbbbbb";
        let test_chat = "daily-report-itest-chat";

        // Real schemas reference persons; satisfy the foreign keys inside the
        // rolled-back test transaction.
        sqlx::query(
            "INSERT INTO qintopia_identity.persons (id, display_name)
             VALUES ($1::uuid, 'itest-person-a'), ($2::uuid, 'itest-person-b')
             ON CONFLICT (id) DO NOTHING",
        )
        .bind(person_a)
        .bind(person_b)
        .execute(&mut *tx)
        .await
        .unwrap();
        let start = DateTime::parse_from_rfc3339("2026-08-15T00:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc);
        let end = DateTime::parse_from_rfc3339("2026-08-16T00:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc);

        // In-window qiwe group text: included.
        sqlx::query(
            "INSERT INTO qintopia_messages.messages
             (id, platform, message_id, event_id, chat_id, chat_type, sender_id, sender_name, message_kind, text, sent_at, received_at, sender_person_id)
             VALUES
             ('11111111-1111-4111-8111-111111111111', 'qiwe', 'mid-1', 'eid-1', $2, 'group', 'sender-1', 'name-1', 'text', 'hello daily', '2026-08-15T10:00:00+00:00', '2026-08-15T10:00:01+00:00', $1::uuid)",
        )
        .bind(person_a)
        .bind(test_chat)
        .execute(&mut *tx)
        .await
        .unwrap();
        // In-window text with only received_at: included via COALESCE.
        sqlx::query(
            "INSERT INTO qintopia_messages.messages
             (id, platform, message_id, event_id, chat_id, chat_type, sender_id, sender_name, message_kind, text, sent_at, received_at, sender_person_id)
             VALUES
             ('22222222-2222-4222-8222-222222222222', 'qiwe', 'mid-2', 'eid-2', $2, 'group', 'sender-2', NULL, 'text', 'second message', NULL, '2026-08-15T11:00:00+00:00', $1::uuid)",
        )
        .bind(person_b)
        .bind(test_chat)
        .execute(&mut *tx)
        .await
        .unwrap();
        // Blank text, non-text kind, non-qiwe platform, private chat, out-of-window: excluded.
        sqlx::query(
            "INSERT INTO qintopia_messages.messages
             (id, platform, message_id, event_id, chat_id, chat_type, sender_id, sender_name, message_kind, text, sent_at, received_at, sender_person_id)
             VALUES
             ('33333333-3333-4333-8333-333333333333', 'qiwe', 'mid-3', 'eid-3', $1, 'group', 'sender-3', 'n', 'text', '   ', '2026-08-15T12:00:00+00:00', '2026-08-15T12:00:01+00:00', NULL),
             ('44444444-4444-4444-8444-444444444444', 'qiwe', 'mid-4', 'eid-4', $1, 'group', 'sender-3', 'n', 'image', 'media', '2026-08-15T12:05:00+00:00', '2026-08-15T12:05:01+00:00', NULL),
             ('55555555-5555-4555-8555-555555555555', 'wecom', 'mid-5', 'eid-5', $1, 'group', 'sender-3', 'n', 'text', 'other platform', '2026-08-15T12:10:00+00:00', '2026-08-15T12:10:01+00:00', NULL),
             ('66666666-6666-4666-8666-666666666666', 'qiwe', 'mid-6', 'eid-6', $1, 'private', 'sender-3', 'n', 'text', 'private chat', '2026-08-15T12:15:00+00:00', '2026-08-15T12:15:01+00:00', NULL),
             ('77777777-7777-4777-8777-777777777777', 'qiwe', 'mid-7', 'eid-7', $1, 'group', 'sender-3', 'n', 'text', 'out of window', '2026-08-17T12:00:00+00:00', '2026-08-17T12:00:01+00:00', NULL)",
        )
        .bind(test_chat)
        .execute(&mut *tx)
        .await
        .unwrap();

        // Character memory: person_a has a dominant type by count; revoked and
        // non-reviewed fact types are excluded; the 2020 fact is outside the lookback.
        sqlx::query(
            "INSERT INTO qintopia_identity.member_facts (person_id, fact_type, fact_text, evidence_type, observed_at, revoked_at)
             VALUES
             ($1::uuid, 'activity_organizer', 'f', 'observation', '2026-08-10T00:00:00+00:00', NULL),
             ($1::uuid, 'activity_organizer', 'f', 'observation', '2026-08-11T00:00:00+00:00', NULL),
             ($1::uuid, 'service_need', 'f', 'observation', '2026-08-12T00:00:00+00:00', NULL),
             ($1::uuid, 'service_need', 'f', 'observation', '2020-01-01T00:00:00+00:00', NULL),
             ($1::uuid, 'operation_signal', 'f', 'observation', '2026-08-12T00:00:00+00:00', '2026-08-13T00:00:00+00:00'),
             ($1::uuid, 'not_a_reviewed_type', 'f', 'observation', '2026-08-12T00:00:00+00:00', NULL),
             ($2::uuid, 'resource_scout', 'f', 'observation', '2026-08-12T00:00:00+00:00', NULL)",
        )
        .bind(person_a)
        .bind(person_b)
        .execute(&mut *tx)
        .await
        .unwrap();

        // Creative profile: latest reviewed snapshot wins; public-surface rows excluded.
        sqlx::query(
            "INSERT INTO qintopia_identity.member_profile_snapshots
             (person_id, profile_kind, profile_version, summary, status, reviewed_at, generated_at, do_not_disclose, safe_reply_hints, communication_style)
             VALUES
             ($1::uuid, 'creative_profile', 'xiaoman-daily-creative-profile-v1', 's', 'active', '2026-08-14T00:00:00+00:00', '2026-08-13T00:00:00+00:00', '{}'::jsonb, '{\"a\":1,\"b\":2}'::jsonb, '{\"tone\":\"dry\"}'::jsonb),
             ($1::uuid, 'creative_profile', 'xiaoman-daily-creative-profile-v1', 's', 'active', NULL, '2026-08-14T00:00:00+00:00', '{}'::jsonb, '{}'::jsonb, '{}'::jsonb),
             ($2::uuid, 'creative_profile', 'xiaoman-daily-creative-profile-v1', 's', 'active', '2026-08-14T00:00:00+00:00', '2026-08-13T00:00:00+00:00', '{\"public_surface_allowed\":true}'::jsonb, '{}'::jsonb, '{}'::jsonb)",
        )
        .bind(person_a)
        .bind(person_b)
        .execute(&mut *tx)
        .await
        .unwrap();

        let args = CollectPreviewArgs {
            chat_id: Some(test_chat.to_string()),
            start,
            end,
        };
        let messages = fetch_messages(&mut *tx, &args)
            .await
            .expect("message collection must succeed");
        let person_ids = collected_person_ids(&messages);
        assert_eq!(person_ids.len(), 2);
        let character_rows = fetch_character_memory(&mut *tx, &person_ids, end)
            .await
            .expect("character memory must succeed");
        let creative_rows = fetch_creative_profiles(&mut *tx, &person_ids, end)
            .await
            .expect("creative profiles must succeed");
        let report = build_report(&args, &messages, &character_rows, &creative_rows);
        let value = serde_json::to_value(&report).unwrap();

        assert_eq!(value["messages"]["count"], 2);
        assert_eq!(value["messages"]["distinct_sender_count"], 2);
        assert_eq!(value["messages"]["senders_with_person_id_count"], 2);
        assert_eq!(
            value["messages"]["total_text_byte_count"],
            "hello daily".len() + "second message".len()
        );
        assert_eq!(value["messages"]["items"].as_array().unwrap().len(), 2);

        let character_persons = value["character_memory"]["persons"].as_array().unwrap();
        assert_eq!(character_persons.len(), 2);
        let person_a_entry = character_persons
            .iter()
            .find(|entry| entry["person_id_sha256"] == sha256_str(person_a))
            .expect("person_a memory must exist");
        // 4 valid facts (2 organizer + 2 service_need); revoked and unknown types excluded.
        assert_eq!(person_a_entry["lifetime_fact_count"], 4);
        // The 2020 fact is older than the 90-day lookback.
        assert_eq!(person_a_entry["recent_fact_count"], 3);
        assert_eq!(person_a_entry["dominant_fact_type"], "activity_organizer");

        let creative_persons = value["creative_profile_memory"]["persons"]
            .as_array()
            .unwrap();
        // person_b is excluded because its snapshot allows the public surface.
        assert_eq!(creative_persons.len(), 1);
        assert_eq!(
            creative_persons[0]["person_id_sha256"],
            sha256_str(person_a)
        );
        assert_eq!(creative_persons[0]["safe_reply_hints_key_count"], 2);
        assert_eq!(creative_persons[0]["communication_style_present"], true);

        let serialized = serde_json::to_string(&report).unwrap();
        for forbidden in [
            "hello daily",
            "second message",
            test_chat,
            person_a,
            person_b,
        ] {
            assert!(
                !serialized.contains(forbidden),
                "integration report must not contain {forbidden}"
            );
        }

        tx.rollback()
            .await
            .expect("test transaction must roll back");
    }
}
