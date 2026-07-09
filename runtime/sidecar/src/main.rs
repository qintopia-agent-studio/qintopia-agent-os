mod collaboration;
mod config;
mod consumer;
mod context_mcp_server;
mod context_tools;
mod daily_digest_publisher;
mod db;
mod embedding_worker;
mod event;
mod event_signal;
mod evidence;
mod graph_projection;
mod group_message_send;
mod health;
mod identity_backfill;
mod identity_bootstrap;
mod knowledge;
mod mcp_server;
mod member_profile;
mod message_search;
mod operations;
mod raw_archive;
mod smoke;
mod workbench;
mod xiaoman_activity;

use anyhow::Result;
use clap::Parser;
use config::{Cli, Command};

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let command = cli.command.clone();
    match command {
        Command::Check => health::check(&cli).await,
        Command::Migrate => migrate(&cli).await,
        Command::Run => consumer::run(cli).await,
        Command::RunEmbeddingWorker { check_only } => embedding_worker::run(cli, check_only).await,
        Command::RunIdentityWorker {
            check_only,
            batch_size,
            poll_seconds,
            chat_id,
            member_map_ttl_seconds,
        } => {
            identity_backfill::run_worker(
                &cli,
                identity_backfill::IdentityWorkerOptions {
                    check_only,
                    batch_size,
                    poll_seconds,
                    chat_id,
                    member_map_ttl_seconds,
                },
            )
            .await
        }
        Command::RunMemberProfileWorker {
            check_only,
            quiet,
            batch_size,
            poll_seconds,
            chat_id,
        } => {
            member_profile::run_profile_worker(
                &cli,
                member_profile::ProfileWorkerOptions {
                    check_only,
                    quiet,
                    batch_size,
                    poll_seconds,
                    chat_id,
                },
            )
            .await
        }
        Command::RunGraphProjectionWorker {
            check_only,
            batch_size,
            poll_seconds,
            chat_id,
        } => {
            graph_projection::run_worker(
                &cli,
                graph_projection::GraphProjectionWorkerOptions {
                    check_only,
                    batch_size,
                    poll_seconds,
                    chat_id,
                },
            )
            .await
        }
        Command::RunEventSignalWorker {
            check_only,
            once,
            chat_id,
            date,
            poll_seconds,
            limit,
        } => {
            event_signal::run_worker(
                &cli,
                event_signal::EventSignalWorkerOptions {
                    check_only,
                    once,
                    chat_id,
                    date,
                    poll_seconds,
                    limit,
                },
            )
            .await
        }
        Command::RunRawArchiveWorker {
            check_only,
            batch_size,
            poll_seconds,
            chat_id,
        } => {
            raw_archive::run_worker(
                &cli,
                raw_archive::ArchiveWorkerOptions {
                    check_only,
                    batch_size,
                    poll_seconds,
                    chat_id,
                },
            )
            .await
        }
        Command::Smoke {
            timeout_seconds,
            poll_interval_ms,
        } => smoke::run(&cli, timeout_seconds, poll_interval_ms).await,
        Command::InspectMessage {
            platform,
            message_id,
        } => smoke::inspect_message(&cli, &platform, &message_id).await,
        Command::McpMessageStore => mcp_server::run(&cli).await,
        Command::McpContext => context_mcp_server::run(&cli).await,
        Command::ImportKnowledgeSnapshot {
            public_jsonl,
            internal_jsonl,
            member_scoped_jsonl,
            source_key,
            source_title,
        } => {
            knowledge::run_import_snapshot(
                &cli,
                public_jsonl,
                internal_jsonl,
                member_scoped_jsonl,
                source_key,
                source_title,
            )
            .await
        }
        Command::SearchMessageStore {
            query,
            search_mode,
            chat_id,
            sender_id,
            chat_type,
            message_kind,
            since,
            until,
            limit,
            caller,
            purpose,
        } => {
            let request = message_search::SearchRequest {
                query: query.unwrap_or_default(),
                search_mode,
                chat_id: chat_id.unwrap_or_default(),
                sender_id: sender_id.unwrap_or_default(),
                chat_type: chat_type.unwrap_or_default(),
                message_kind: message_kind.unwrap_or_default(),
                since,
                until,
                limit,
                caller,
                purpose,
            };
            message_search::run_cli(&cli, request).await
        }
        Command::IdentityBackfill {
            apply,
            dry_run,
            refresh,
            limit,
            chat_id,
            sender_id,
            request_delay_ms,
        } => {
            identity_backfill::run(
                &cli,
                identity_backfill::BackfillOptions {
                    apply,
                    dry_run,
                    refresh,
                    limit,
                    chat_id,
                    sender_id,
                    request_delay_ms,
                },
            )
            .await
        }
        Command::IdentityBootstrapPersons {
            apply,
            dry_run,
            chat_id,
            limit,
        } => {
            identity_bootstrap::run(
                &cli,
                identity_bootstrap::BootstrapOptions {
                    apply,
                    dry_run,
                    chat_id,
                    limit,
                },
            )
            .await
        }
        Command::MemberProfile {
            apply,
            dry_run,
            chat_id,
            limit,
        } => {
            member_profile::run_profile(
                &cli,
                member_profile::ProfileOptions {
                    apply,
                    dry_run,
                    chat_id,
                    limit,
                },
            )
            .await
        }
        Command::DailyDigest {
            apply,
            dry_run,
            quiet,
            chat_id,
            date,
        } => {
            member_profile::run_digest(
                &cli,
                member_profile::DigestOptions {
                    apply,
                    dry_run,
                    quiet,
                    chat_id,
                    date,
                },
            )
            .await
        }
        Command::EventSignal {
            apply,
            dry_run,
            chat_id,
            date,
            limit,
        } => {
            event_signal::run(
                &cli,
                event_signal::EventSignalOptions {
                    apply,
                    dry_run,
                    chat_id,
                    date,
                    limit,
                },
            )
            .await
        }
        Command::AgentosDailyDigestWorker {
            dry_run,
            once,
            quiet,
            chat_id,
            date,
            poll_seconds,
        } => {
            member_profile::run_digest_worker(
                &cli,
                member_profile::DigestWorkerOptions {
                    dry_run,
                    once,
                    quiet,
                    chat_id,
                    date,
                    poll_seconds,
                },
            )
            .await
        }
        Command::DailyDigestPublish {
            apply,
            dry_run,
            digest_id,
            actor_agent,
        } => {
            daily_digest_publisher::run(
                &cli,
                daily_digest_publisher::PublishOptions {
                    apply,
                    dry_run,
                    digest_id,
                    actor_agent,
                },
            )
            .await
        }
        Command::XiaomanActivity {
            operation,
            payload_json,
            fixture_path,
            use_feishu_base,
            apply,
            dry_run,
        } => {
            xiaoman_activity::run(
                &cli,
                operation,
                payload_json,
                apply,
                dry_run,
                fixture_path,
                use_feishu_base,
            )
            .await
        }
        Command::RunXiaomanActivitySignalWorker {
            check_only,
            once,
            apply,
            batch_size,
            poll_seconds,
        } => {
            xiaoman_activity::run_signal_worker(
                &cli,
                xiaoman_activity::SignalWorkerOptions {
                    check_only,
                    once,
                    apply,
                    batch_size,
                    poll_seconds,
                },
            )
            .await
        }
        Command::RunXiaomanActivityPromotionStarterWorker {
            check_only,
            once,
            apply,
            batch_size,
            work_item_id,
        } => {
            operations::run_xiaoman_activity_promotion_starter_worker(
                &cli,
                check_only,
                once,
                apply,
                batch_size,
                work_item_id,
            )
            .await
        }
        Command::OperationsWorkItemCreate {
            payload_json,
            apply,
            dry_run,
        } => operations::run_create(&cli, payload_json, apply, dry_run).await,
        Command::OperationsCapabilityList { use_db } => {
            operations::run_capability_list(&cli, use_db).await
        }
        Command::OperationsReadinessCheck { profile, strict } => {
            operations::run_readiness_check(&cli, profile, strict)
        }
        Command::OperationsRequestPlan { payload_json } => {
            operations::run_request_plan(payload_json)
        }
        Command::OperationsRequestSubmit {
            payload_json,
            apply,
            dry_run,
        } => operations::run_request_submit(&cli, payload_json, apply, dry_run).await,
        Command::OperationsWorkflowStart {
            payload_json,
            apply,
            dry_run,
        } => operations::run_workflow_start(&cli, payload_json, apply, dry_run).await,
        Command::OperationsArtifactReviewDecision {
            payload_json,
            apply,
            dry_run,
        } => operations::run_review_decision(&cli, payload_json, apply, dry_run).await,
        Command::OperationsGroupMessageConfirm {
            payload_json,
            apply,
            dry_run,
        } => operations::run_group_message_confirm(&cli, payload_json, apply, dry_run).await,
        Command::OperationsWorkbenchEventRecord {
            payload_json,
            apply,
            dry_run,
        } => operations::run_workbench_event_record(&cli, payload_json, apply, dry_run).await,
        Command::OperationsWorkbenchEventProcess {
            event_id,
            apply,
            dry_run,
        } => operations::run_workbench_event_process(&cli, event_id, apply, dry_run).await,
        Command::RunWorkbenchEventWorker {
            once,
            event_id,
            apply,
            dry_run,
        } => operations::run_workbench_event_worker(&cli, once, event_id, apply, dry_run).await,
        Command::OperationsWorkItemStatus { work_item_id } => {
            operations::run_work_item_status(&cli, work_item_id).await
        }
        Command::OperationsWorkflowSync {
            work_item_id,
            apply,
            dry_run,
        } => operations::run_workflow_sync(&cli, work_item_id, apply, dry_run).await,
        Command::RunWorkflowSyncWorker {
            once,
            work_item_id,
            apply,
            dry_run,
        } => operations::run_workflow_sync_worker(&cli, once, work_item_id, apply, dry_run).await,
        Command::RunCollaborationWorker {
            work_item_type,
            once,
            work_item_id,
            apply,
            dry_run,
            fixture_mode,
        } => {
            collaboration::run(
                &cli,
                work_item_type,
                once,
                work_item_id,
                apply,
                dry_run,
                fixture_mode,
            )
            .await
        }
        Command::RunEvidenceWorker {
            once,
            work_item_id,
            apply,
            dry_run,
            fixture_mode,
        } => evidence::run(&cli, once, work_item_id, apply, dry_run, fixture_mode).await,
        Command::RunGroupMessageSendWorker {
            once,
            work_item_id,
            apply,
            dry_run,
            fixture_mode,
        } => group_message_send::run(&cli, once, work_item_id, apply, dry_run, fixture_mode).await,
        Command::RunWorkbenchMirrorWorker {
            once,
            work_item_id,
            apply,
            dry_run,
            fixture_mode,
        } => workbench::run(&cli, once, work_item_id, apply, dry_run, fixture_mode).await,
        Command::RunDailyDigestPublisherWorker {
            check_only,
            once,
            batch_size,
            poll_seconds,
            actor_agent,
        } => {
            daily_digest_publisher::run_worker(
                &cli,
                daily_digest_publisher::PublisherWorkerOptions {
                    check_only,
                    once,
                    batch_size,
                    poll_seconds,
                    actor_agent,
                },
            )
            .await
        }
        Command::GraphProjection {
            apply,
            dry_run,
            chat_id,
            limit,
        } => {
            graph_projection::run(
                &cli,
                graph_projection::GraphProjectionOptions {
                    apply,
                    dry_run,
                    chat_id,
                    limit,
                },
            )
            .await
        }
        Command::RawArchive {
            apply,
            dry_run,
            chat_id,
            limit,
        } => {
            raw_archive::run(
                &cli,
                raw_archive::ArchiveOptions {
                    apply,
                    dry_run,
                    chat_id,
                    limit,
                },
            )
            .await
        }
    }
}

async fn migrate(cli: &Cli) -> Result<()> {
    let database_url = cli.database_url_required()?;
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    db::run_migrations(&pool).await?;
    println!("migrations applied");
    Ok(())
}
