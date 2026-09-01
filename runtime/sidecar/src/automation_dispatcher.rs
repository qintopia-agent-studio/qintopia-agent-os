use std::{str::FromStr, time::Duration};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Datelike, Duration as ChronoDuration, Timelike, Utc};
use chrono_tz::Tz;
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::{postgres::PgPool, Row};
use tracing::{info, warn};
use uuid::Uuid;

use crate::{config::Cli, db};

const DEFAULT_TIMEZONE: &str = "Asia/Shanghai";
const MAX_CRON_SEARCH_MINUTES: i64 = 60 * 24 * 366 * 5;

#[derive(Debug, Clone)]
pub struct DispatcherOptions {
    pub once: bool,
    pub apply: bool,
    pub dry_run: bool,
    pub batch_size: i64,
    pub poll_seconds: u64,
}

#[derive(Debug, Default, Serialize)]
struct DispatchReport {
    scanned: usize,
    created: usize,
    deduped: usize,
    rejected_invalid: usize,
    dry_run: bool,
}

#[derive(Debug)]
struct DueAutomation {
    id: Uuid,
    space_id: Uuid,
    definition_key: String,
    version: i64,
    business_definition_id: Uuid,
    automation_digest: String,
    business_digest: String,
    policy_id: Uuid,
    policy_digest: String,
    trigger_config: Value,
    timezone: String,
    misfire_policy: String,
    scheduled_for: DateTime<Utc>,
}

pub async fn run(cli: &Cli, options: DispatcherOptions) -> Result<()> {
    if options.apply == options.dry_run {
        bail!("choose exactly one of --apply or --dry-run");
    }
    if options.batch_size <= 0 || options.batch_size > 500 {
        bail!("automation dispatcher batch_size must be between 1 and 500");
    }
    if !options.once && options.poll_seconds == 0 {
        bail!("automation dispatcher poll_seconds must be positive");
    }

    let database_url = cli.database_url_required()?;
    let agent_turn_runtime_ready = crate::space_agent_turn::runtime_readiness()?;
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    db::run_migrations(&pool).await?;

    loop {
        let report = dispatch_once(&pool, Utc::now(), &options, agent_turn_runtime_ready).await?;
        info!(
            scanned = report.scanned,
            created = report.created,
            deduped = report.deduped,
            rejected_invalid = report.rejected_invalid,
            dry_run = report.dry_run,
            "Space automation dispatcher batch completed"
        );
        if options.once {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(options.poll_seconds)).await;
    }
}

async fn dispatch_once(
    pool: &PgPool,
    now: DateTime<Utc>,
    options: &DispatcherOptions,
    agent_turn_runtime_ready: bool,
) -> Result<DispatchReport> {
    let mut tx = pool
        .begin()
        .await
        .context("begin automation dispatcher transaction")?;
    let rows = if options.apply {
        sqlx::query(
            r#"
            SELECT automation.id, automation.space_id, automation.definition_key,
                   automation.version::bigint AS version, automation.business_definition_id,
                   automation.definition_digest AS automation_digest,
                   business.definition_digest AS business_digest,
                   policy.id AS policy_id, policy.definition_digest AS policy_digest,
                   automation.trigger_config, automation.timezone,
                   automation.misfire_policy, automation.next_run_at
            FROM qintopia_agent_os.automation_definition_versions automation
            JOIN qintopia_agent_os.business_definition_versions business
              ON business.id = automation.business_definition_id
             AND business.space_id = automation.space_id
             AND business.status = 'active'
            JOIN qintopia_agent_os.space_policy_versions policy
              ON policy.space_id = automation.space_id
             AND policy.definition_key = 'default'
             AND policy.status = 'active'
            JOIN qintopia_agent_os.capabilities selected
              ON selected.capability_key = CASE business.execution_mode
                  WHEN 'deterministic' THEN business.definition->>'capability_key'
                  WHEN 'agent_turn' THEN 'erhua.space_agent_turn'
                  ELSE NULL
                END
             AND selected.enabled
             AND selected.provider_agent = 'erhua'
             AND selected.metadata ->> 'space_invocable' = 'true'
             AND selected.metadata ->> 'space_scope_binding' = 'work_item_space_id'
             AND selected.metadata ->> 'invocation_boundary' = 'erhua.execute_space_business'
             AND (business.execution_mode <> 'agent_turn' OR $3::boolean)
             AND 'system' = ANY(selected.allowed_callers)
             AND (
                  (business.execution_mode = 'deterministic'
                   AND selected.metadata ? 'space_execution_recipe'
                   AND 'space_automation_run' = ANY(selected.allowed_work_item_types)) OR
                  (business.execution_mode = 'agent_turn'
                   AND 'space_agent_turn' = ANY(selected.allowed_work_item_types))
                )
            WHERE automation.status = 'active'
              AND automation.trigger_kind = 'schedule'
              AND automation.next_run_at IS NOT NULL
              AND automation.next_run_at <= $1
              AND selected.capability_key = ANY(business.allowed_capabilities)
              AND COALESCE(policy.policy_config->'capability_grants', '[]'::jsonb)
                  ? selected.capability_key
              AND EXISTS (
                  SELECT 1 FROM qintopia_agent_os.capabilities capability
                  WHERE capability.capability_key = 'erhua.execute_space_business'
                    AND capability.enabled
                    AND 'system' = ANY(capability.allowed_callers)
                    AND 'space_automation_run' = ANY(capability.allowed_work_item_types)
              )
            ORDER BY automation.next_run_at, automation.id
            FOR UPDATE OF automation SKIP LOCKED
            LIMIT $2
            "#,
        )
        .bind(now)
        .bind(options.batch_size)
        .bind(agent_turn_runtime_ready)
        .fetch_all(&mut *tx)
        .await
        .context("select due automation definitions")?
    } else {
        sqlx::query(
            r#"
            SELECT automation.id, automation.space_id, automation.definition_key,
                   automation.version::bigint AS version, automation.business_definition_id,
                   automation.definition_digest AS automation_digest,
                   business.definition_digest AS business_digest,
                   policy.id AS policy_id, policy.definition_digest AS policy_digest,
                   automation.trigger_config, automation.timezone,
                   automation.misfire_policy, automation.next_run_at
            FROM qintopia_agent_os.automation_definition_versions automation
            JOIN qintopia_agent_os.business_definition_versions business
              ON business.id = automation.business_definition_id
             AND business.space_id = automation.space_id
             AND business.status = 'active'
            JOIN qintopia_agent_os.space_policy_versions policy
              ON policy.space_id = automation.space_id
             AND policy.definition_key = 'default'
             AND policy.status = 'active'
            JOIN qintopia_agent_os.capabilities selected
              ON selected.capability_key = CASE business.execution_mode
                  WHEN 'deterministic' THEN business.definition->>'capability_key'
                  WHEN 'agent_turn' THEN 'erhua.space_agent_turn'
                  ELSE NULL
                END
             AND selected.enabled
             AND selected.provider_agent = 'erhua'
             AND selected.metadata ->> 'space_invocable' = 'true'
             AND selected.metadata ->> 'space_scope_binding' = 'work_item_space_id'
             AND selected.metadata ->> 'invocation_boundary' = 'erhua.execute_space_business'
             AND (business.execution_mode <> 'agent_turn' OR $3::boolean)
             AND 'system' = ANY(selected.allowed_callers)
             AND (
                  (business.execution_mode = 'deterministic'
                   AND selected.metadata ? 'space_execution_recipe'
                   AND 'space_automation_run' = ANY(selected.allowed_work_item_types)) OR
                  (business.execution_mode = 'agent_turn'
                   AND 'space_agent_turn' = ANY(selected.allowed_work_item_types))
                )
            WHERE automation.status = 'active'
              AND automation.trigger_kind = 'schedule'
              AND automation.next_run_at IS NOT NULL
              AND automation.next_run_at <= $1
              AND selected.capability_key = ANY(business.allowed_capabilities)
              AND COALESCE(policy.policy_config->'capability_grants', '[]'::jsonb)
                  ? selected.capability_key
              AND EXISTS (
                  SELECT 1 FROM qintopia_agent_os.capabilities capability
                  WHERE capability.capability_key = 'erhua.execute_space_business'
                    AND capability.enabled
                    AND 'system' = ANY(capability.allowed_callers)
                    AND 'space_automation_run' = ANY(capability.allowed_work_item_types)
              )
            ORDER BY automation.next_run_at, automation.id
            LIMIT $2
            "#,
        )
        .bind(now)
        .bind(options.batch_size)
        .bind(agent_turn_runtime_ready)
        .fetch_all(&mut *tx)
        .await
        .context("preview due automation definitions")?
    };

    let mut report = DispatchReport {
        scanned: rows.len(),
        dry_run: options.dry_run,
        ..DispatchReport::default()
    };

    for row in rows {
        let automation = DueAutomation {
            id: row.try_get("id")?,
            space_id: row.try_get("space_id")?,
            definition_key: row.try_get("definition_key")?,
            version: row.try_get("version")?,
            business_definition_id: row.try_get("business_definition_id")?,
            automation_digest: row.try_get("automation_digest")?,
            business_digest: row.try_get("business_digest")?,
            policy_id: row.try_get("policy_id")?,
            policy_digest: row.try_get("policy_digest")?,
            trigger_config: row.try_get("trigger_config")?,
            timezone: row.try_get::<String, _>("timezone")?,
            misfire_policy: row.try_get("misfire_policy")?,
            scheduled_for: row.try_get("next_run_at")?,
        };

        let scheduled_for = automation.scheduled_for;
        let next_run_at = match validated_next_schedule_run(
            &automation.trigger_config,
            &automation.timezone,
            &automation.misfire_policy,
            now.max(scheduled_for),
        ) {
            Ok(next_run_at) => next_run_at,
            Err(error) => {
                warn!(
                    automation_id = %automation.id,
                    error = %error,
                    "rejecting invalid active Space automation schedule"
                );
                report.rejected_invalid += 1;
                if options.apply {
                    bail!(
                        "active Space automation schedule is invalid; pause it through a reviewed Space definition operation: {error}"
                    );
                }
                continue;
            }
        };

        let idempotency_key = schedule_idempotency_key(automation.id, scheduled_for);

        if options.apply {
            let result = sqlx::query(
                r#"
                INSERT INTO qintopia_agent_os.work_items
                    (space_id, work_item_type, status, requester_agent, target_agent,
                     capability_key, human_owner, priority, available_at, brief_summary,
                     purpose, source_type, source_refs, dedupe_key, idempotency_key,
                     risk_level, information_class, payload, payload_redaction_policy,
                     review_policy, metadata)
                VALUES
                    ($1, 'space_automation_run', 'queued', 'system', 'erhua',
                     'erhua.execute_space_business', '', 'normal', now(),
                     'Execute a confirmed Space automation definition.',
                     'space_automation_schedule', 'space_automation_schedule', $2,
                     $3, $3, 'medium', 'internal_ops', $4,
                     'summary_only', 'not_required', $5)
                ON CONFLICT (idempotency_key) DO NOTHING
                "#,
            )
            .bind(automation.space_id)
            .bind(json!({
                "automation_definition_id": automation.id,
                "business_definition_id": automation.business_definition_id
            }))
            .bind(&idempotency_key)
            .bind(json!({
                "automation_definition_id": automation.id,
                "automation_definition_digest": automation.automation_digest,
                "automation_key": automation.definition_key,
                "automation_version": automation.version,
                "business_definition_id": automation.business_definition_id,
                "business_definition_digest": automation.business_digest,
                "space_policy_version_id": automation.policy_id,
                "space_policy_digest": automation.policy_digest,
                "trigger": {
                    "kind": "schedule",
                    "scheduled_for_utc": scheduled_for.to_rfc3339()
                }
            }))
            .bind(json!({
                "external_send_executed": false,
                "space_bound": true,
                "dispatcher": "postgres-minute-v1"
            }))
            .execute(&mut *tx)
            .await
            .context("create scheduled automation work item")?;

            if result.rows_affected() == 1 {
                report.created += 1;
            } else {
                report.deduped += 1;
            }

            sqlx::query(
                r#"
                UPDATE qintopia_agent_os.automation_definition_versions
                SET last_dispatched_at = $2, next_run_at = $3, updated_at = now()
                WHERE id = $1 AND status = 'active'
                "#,
            )
            .bind(automation.id)
            .bind(scheduled_for)
            .bind(next_run_at)
            .execute(&mut *tx)
            .await
            .context("advance automation scheduler cursor")?;
        }
    }

    if options.apply {
        tx.commit()
            .await
            .context("commit automation dispatcher transaction")?;
    } else {
        tx.rollback()
            .await
            .context("rollback automation dispatcher preview")?;
    }
    Ok(report)
}

#[cfg(all(test, feature = "postgres-integration-tests"))]
pub(crate) async fn dispatch_once_for_integration_test(
    pool: &PgPool,
    now: DateTime<Utc>,
    options: &DispatcherOptions,
) -> Result<()> {
    dispatch_once(pool, now, options, false).await.map(|_| ())
}

fn schedule_idempotency_key(automation_id: Uuid, scheduled_for: DateTime<Utc>) -> String {
    format!(
        "automation:{automation_id}:{}",
        scheduled_for.format("%Y-%m-%dT%H:%M:00Z")
    )
}

pub(crate) fn next_schedule_run(
    trigger_config: &Value,
    timezone: &str,
    after: DateTime<Utc>,
) -> Result<DateTime<Utc>> {
    let cron_expression = trigger_config
        .get("cron")
        .and_then(Value::as_str)
        .context("schedule trigger_config.cron must be a string")?;
    let timezone = if timezone.trim().is_empty() {
        DEFAULT_TIMEZONE
    } else {
        timezone.trim()
    };
    CronSchedule::parse(cron_expression)?
        .with_timezone(timezone)?
        .next_after(after)
        .context("calculate next automation occurrence")
}

fn validated_next_schedule_run(
    trigger_config: &Value,
    timezone: &str,
    misfire_policy: &str,
    after: DateTime<Utc>,
) -> Result<DateTime<Utc>> {
    if misfire_policy != "run_once" {
        bail!("automation misfire policy must be run_once in v1");
    }
    next_schedule_run(trigger_config, timezone, after)
}

#[derive(Debug, Clone)]
struct CronSchedule {
    minute: CronField,
    hour: CronField,
    day_of_month: CronField,
    month: CronField,
    day_of_week: CronField,
    timezone: Option<Tz>,
}

impl CronSchedule {
    fn parse(expression: &str) -> Result<Self> {
        let parts = expression.split_whitespace().collect::<Vec<_>>();
        if parts.len() != 5 {
            bail!("cron expression must contain exactly five fields");
        }
        Ok(Self {
            minute: CronField::parse(parts[0], 0, 59, false)?,
            hour: CronField::parse(parts[1], 0, 23, false)?,
            day_of_month: CronField::parse(parts[2], 1, 31, false)?,
            month: CronField::parse(parts[3], 1, 12, false)?,
            day_of_week: CronField::parse(parts[4], 0, 7, true)?,
            timezone: None,
        })
    }

    fn with_timezone(mut self, timezone: &str) -> Result<Self> {
        self.timezone = Some(
            Tz::from_str(timezone)
                .with_context(|| format!("unsupported IANA timezone {timezone}"))?,
        );
        Ok(self)
    }

    fn next_after(&self, after: DateTime<Utc>) -> Result<DateTime<Utc>> {
        let timezone = self.timezone.context("cron timezone is missing")?;
        let mut candidate = after
            .with_second(0)
            .and_then(|value| value.with_nanosecond(0))
            .context("normalize schedule timestamp")?
            + ChronoDuration::minutes(1);
        for _ in 0..MAX_CRON_SEARCH_MINUTES {
            let local = candidate.with_timezone(&timezone);
            let day_match = self.day_of_month.matches(local.day());
            let weekday_match = self
                .day_of_week
                .matches(local.weekday().num_days_from_sunday());
            let date_match = match (self.day_of_month.wildcard, self.day_of_week.wildcard) {
                (true, true) => true,
                (true, false) => weekday_match,
                (false, true) => day_match,
                (false, false) => day_match || weekday_match,
            };
            if self.minute.matches(local.minute())
                && self.hour.matches(local.hour())
                && self.month.matches(local.month())
                && date_match
            {
                return Ok(candidate);
            }
            candidate += ChronoDuration::minutes(1);
        }
        bail!("cron expression has no occurrence within five years")
    }
}

#[derive(Debug, Clone)]
struct CronField {
    minimum: u32,
    allowed: Vec<bool>,
    wildcard: bool,
}

impl CronField {
    fn parse(value: &str, minimum: u32, maximum: u32, sunday_alias: bool) -> Result<Self> {
        if value.trim().is_empty() {
            bail!("cron field is empty");
        }
        let mut field = Self {
            minimum,
            allowed: vec![false; (maximum - minimum + 1) as usize],
            wildcard: false,
        };
        for segment in value.split(',') {
            let (base, step) = match segment.split_once('/') {
                Some((base, step)) => {
                    let step = parse_number(step, 1, maximum - minimum + 1)?;
                    (base, step)
                }
                None => (segment, 1),
            };
            let (start, end) = if base == "*" {
                (minimum, maximum)
            } else if let Some((start, end)) = base.split_once('-') {
                (
                    parse_number(start, minimum, maximum)?,
                    parse_number(end, minimum, maximum)?,
                )
            } else {
                let start = parse_number(base, minimum, maximum)?;
                (
                    start,
                    if segment.contains('/') {
                        maximum
                    } else {
                        start
                    },
                )
            };
            if start > end {
                bail!("cron field range must be ascending");
            }
            let mut item = start;
            while item <= end {
                let normalized = if sunday_alias && item == 7 { 0 } else { item };
                field.allowed[(normalized - minimum) as usize] = true;
                match item.checked_add(step) {
                    Some(next) if next > item => item = next,
                    _ => break,
                }
            }
        }
        if !field.allowed.iter().any(|allowed| *allowed) {
            bail!("cron field selects no values");
        }
        field.wildcard = (minimum..=maximum).all(|item| {
            let normalized = if sunday_alias && item == 7 { 0 } else { item };
            field.allowed[(normalized - minimum) as usize]
        });
        Ok(field)
    }

    fn matches(&self, value: u32) -> bool {
        value
            .checked_sub(self.minimum)
            .and_then(|index| self.allowed.get(index as usize))
            .copied()
            .unwrap_or(false)
    }
}

fn parse_number(value: &str, minimum: u32, maximum: u32) -> Result<u32> {
    let parsed = value
        .parse::<u32>()
        .with_context(|| format!("cron field value {value:?} is not numeric"))?;
    if parsed < minimum || parsed > maximum {
        bail!("cron field value {parsed} is outside {minimum}..={maximum}");
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn five_field_cron_supports_ranges_lists_and_steps() {
        let schedule = CronSchedule::parse("*/15 8-10 * * 1,3,5")
            .unwrap()
            .with_timezone("Asia/Shanghai")
            .unwrap();
        let start = Utc.with_ymd_and_hms(2026, 8, 14, 0, 1, 0).unwrap();
        assert_eq!(
            schedule.next_after(start).unwrap(),
            Utc.with_ymd_and_hms(2026, 8, 14, 0, 15, 0).unwrap()
        );
    }

    #[test]
    fn timezone_changes_the_utc_occurrence() {
        let start = Utc.with_ymd_and_hms(2026, 8, 13, 23, 59, 0).unwrap();
        let shanghai = CronSchedule::parse("0 9 * * *")
            .unwrap()
            .with_timezone("Asia/Shanghai")
            .unwrap();
        let utc = CronSchedule::parse("0 9 * * *")
            .unwrap()
            .with_timezone("UTC")
            .unwrap();
        assert_eq!(
            shanghai.next_after(start).unwrap(),
            Utc.with_ymd_and_hms(2026, 8, 14, 1, 0, 0).unwrap()
        );
        assert_eq!(
            utc.next_after(start).unwrap(),
            Utc.with_ymd_and_hms(2026, 8, 14, 9, 0, 0).unwrap()
        );
    }

    #[test]
    fn shared_schedule_helper_defaults_to_asia_shanghai() {
        let start = Utc.with_ymd_and_hms(2026, 8, 13, 23, 59, 0).unwrap();
        assert_eq!(
            next_schedule_run(&json!({"cron": "0 9 * * *"}), "", start).unwrap(),
            Utc.with_ymd_and_hms(2026, 8, 14, 1, 0, 0).unwrap()
        );
    }

    #[test]
    fn shared_schedule_helper_rejects_missing_or_non_string_cron() {
        let start = Utc.with_ymd_and_hms(2026, 8, 13, 23, 59, 0).unwrap();
        assert!(next_schedule_run(&json!({}), "UTC", start).is_err());
        assert!(next_schedule_run(&json!({"cron": 42}), "UTC", start).is_err());
    }

    #[test]
    fn dispatcher_validation_rejects_unimplemented_misfire_without_mutating_a_version() {
        let start = Utc.with_ymd_and_hms(2026, 8, 13, 23, 59, 0).unwrap();
        for policy in ["skip", "catch_up"] {
            let error = validated_next_schedule_run(
                &json!({"cron": "0 9 * * *"}),
                "Asia/Shanghai",
                policy,
                start,
            )
            .expect_err("unsupported misfire policy");
            assert!(error.to_string().contains("run_once in v1"));
        }
    }

    #[test]
    fn invalid_cron_and_timezone_fail_closed() {
        assert!(CronSchedule::parse("0 9 * *").is_err());
        assert!(CronSchedule::parse("60 9 * * *").is_err());
        assert!(CronSchedule::parse("0 9 * * *")
            .unwrap()
            .with_timezone("Mars/Olympus")
            .is_err());
    }

    #[test]
    fn idempotency_key_keeps_minute_precision() {
        let id = Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
        let due = Utc.with_ymd_and_hms(2026, 8, 14, 1, 2, 59).unwrap();
        assert_eq!(
            schedule_idempotency_key(id, due),
            "automation:11111111-1111-4111-8111-111111111111:2026-08-14T01:02:00Z"
        );
    }

    #[test]
    fn restricted_day_fields_use_standard_cron_or_semantics() {
        let schedule = CronSchedule::parse("0 0 15 * 1")
            .unwrap()
            .with_timezone("UTC")
            .unwrap();
        let sunday = Utc.with_ymd_and_hms(2026, 8, 16, 0, 0, 0).unwrap();
        assert_eq!(
            schedule.next_after(sunday).unwrap(),
            Utc.with_ymd_and_hms(2026, 8, 17, 0, 0, 0).unwrap()
        );
    }

    #[test]
    fn full_domain_step_is_not_treated_as_a_restricted_day_field() {
        let schedule = CronSchedule::parse("0 9 */1 * 1")
            .unwrap()
            .with_timezone("UTC")
            .unwrap();
        let monday_after_run = Utc.with_ymd_and_hms(2026, 8, 17, 9, 0, 0).unwrap();
        assert_eq!(
            schedule.next_after(monday_after_run).unwrap(),
            Utc.with_ymd_and_hms(2026, 8, 24, 9, 0, 0).unwrap()
        );
    }
}
