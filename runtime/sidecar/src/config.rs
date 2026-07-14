use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "qintopia-message-sidecar")]
#[command(about = "Persist Qintopia QiWe/Hermes message events from NATS to Postgres.")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    #[arg(
        long,
        env = "QINTOPIA_SIDECAR_NATS_URL",
        default_value = "nats://127.0.0.1:4222"
    )]
    pub nats_url: String,

    #[arg(
        long,
        env = "QINTOPIA_SIDECAR_NATS_STREAM",
        default_value = "QINTOPIA_QIWE_MESSAGES"
    )]
    pub nats_stream: String,

    #[arg(
        long,
        env = "QINTOPIA_SIDECAR_RAW_SUBJECT",
        default_value = "qintopia.qiwe.raw"
    )]
    pub raw_subject: String,

    #[arg(
        long,
        env = "QINTOPIA_SIDECAR_MESSAGE_SUBJECT",
        default_value = "qintopia.qiwe.message"
    )]
    pub message_subject: String,

    #[arg(
        long,
        env = "QINTOPIA_SIDECAR_CONSUMER",
        default_value = "qintopia-message-sidecar"
    )]
    pub consumer: String,

    #[arg(long, env = "QINTOPIA_SIDECAR_DATABASE_URL")]
    pub database_url: Option<String>,

    #[arg(long, env = "QINTOPIA_SIDECAR_BATCH_SIZE", default_value_t = 25)]
    pub batch_size: usize,

    #[arg(long, env = "QINTOPIA_SIDECAR_NAK_DELAY_SECONDS", default_value_t = 30)]
    pub nak_delay_seconds: u64,

    #[arg(long, env = "QINTOPIA_SIDECAR_DB_MAX_CONNECTIONS", default_value_t = 5)]
    pub db_max_connections: u32,

    #[arg(
        long,
        env = "QINTOPIA_EMBEDDING_BASE_URL",
        default_value = "https://livecool.net"
    )]
    pub embedding_base_url: String,

    #[arg(long, env = "QINTOPIA_EMBEDDING_API_KEY")]
    pub embedding_api_key: Option<String>,

    #[arg(long, env = "QINTOPIA_MESSAGE_EMBEDDING_ENDPOINT")]
    pub message_embedding_endpoint: Option<String>,

    #[arg(
        long,
        env = "QINTOPIA_MESSAGE_EMBEDDING_MODEL",
        default_value = "text-embedding-3-small"
    )]
    pub message_embedding_model: String,

    #[arg(
        long,
        env = "QINTOPIA_MESSAGE_EMBEDDING_BATCH_SIZE",
        default_value_t = 10
    )]
    pub message_embedding_batch_size: i64,

    #[arg(
        long,
        env = "QINTOPIA_MESSAGE_EMBEDDING_POLL_SECONDS",
        default_value_t = 10
    )]
    pub message_embedding_poll_seconds: u64,

    #[arg(
        long,
        env = "QINTOPIA_MESSAGE_EMBEDDING_REQUEST_DELAY_MS",
        default_value_t = 0
    )]
    pub message_embedding_request_delay_ms: u64,

    #[arg(
        long,
        env = "QINTOPIA_MESSAGE_EMBEDDING_MAX_ATTEMPTS",
        default_value_t = 5
    )]
    pub message_embedding_max_attempts: i32,

    #[arg(
        long,
        env = "QINTOPIA_MESSAGE_STORE_MCP_ALLOWED_CALLER",
        default_value = "wenyuange"
    )]
    pub message_store_mcp_allowed_caller: String,

    #[arg(long, env = "QINTOPIA_CONTEXT_MCP_ALLOWED_CALLERS")]
    pub context_mcp_allowed_callers: Option<String>,

    #[arg(long, env = "QINTOPIA_ERHUA_TRAINER_USER_IDS", default_value = "")]
    pub erhua_trainer_user_ids: String,

    #[arg(
        long,
        env = "QIWE_API_URL",
        default_value = "http://manager.qiweapi.com/qiwe/api/qw/doApi"
    )]
    pub qiwe_api_url: String,

    #[arg(long, env = "QIWE_TOKEN")]
    pub qiwe_token: Option<String>,

    #[arg(long, env = "QIWE_GUID")]
    pub qiwe_guid: Option<String>,

    #[arg(
        long,
        env = "QINTOPIA_IDENTITY_MEMBER_MAP_TTL_SECONDS",
        default_value_t = 1200
    )]
    pub identity_member_map_ttl_seconds: u64,

    #[arg(long, env = "QINTOPIA_PROFILE_TARGET_CHAT_IDS", default_value = "")]
    pub profile_target_chat_ids: String,

    #[arg(
        long,
        env = "QINTOPIA_PROFILE_EXCLUDED_CHANNEL_USER_IDS",
        default_value = ""
    )]
    pub profile_excluded_channel_user_ids: String,

    #[arg(
        long,
        env = "QINTOPIA_PROFILE_EXCLUDED_DISPLAY_NAMES",
        default_value = "秦托邦小客服"
    )]
    pub profile_excluded_display_names: String,

    #[arg(long, env = "QINTOPIA_CHAT_METADATA_JSON")]
    pub chat_metadata_json: Option<String>,

    #[arg(long, env = "QINTOPIA_DAILY_DIGEST_TIME", default_value = "03:00")]
    pub daily_digest_time: String,

    #[arg(
        long,
        env = "QINTOPIA_DAILY_DIGEST_TIMEZONE",
        default_value = "Asia/Shanghai"
    )]
    pub daily_digest_timezone: String,

    #[arg(
        long,
        env = "QINTOPIA_DAILY_DIGEST_OWNER_AGENT",
        default_value = "xiaoman"
    )]
    pub daily_digest_owner_agent: String,

    #[arg(long, env = "QINTOPIA_DAILY_DIGEST_FEISHU_PARENT_NODE")]
    pub daily_digest_feishu_parent_node: Option<String>,

    #[arg(
        long,
        env = "QINTOPIA_DAILY_DIGEST_ALLOWED_FEISHU_PARENT_NODES",
        default_value = ""
    )]
    pub daily_digest_allowed_feishu_parent_nodes: String,

    #[arg(long, env = "QINTOPIA_DAILY_DIGEST_FEISHU_BASE_TOKEN")]
    pub daily_digest_feishu_base_token: Option<String>,

    #[arg(
        long,
        env = "QINTOPIA_DAILY_DIGEST_ALLOWED_FEISHU_BASE_TOKENS",
        default_value = ""
    )]
    pub daily_digest_allowed_feishu_base_tokens: String,

    #[arg(long, env = "QINTOPIA_DAILY_DIGEST_FEISHU_DAILY_TABLE_ID")]
    pub daily_digest_feishu_daily_table_id: Option<String>,

    #[arg(long, env = "QINTOPIA_DAILY_DIGEST_FEISHU_SIGNAL_TABLE_ID")]
    pub daily_digest_feishu_signal_table_id: Option<String>,

    #[arg(long, env = "QINTOPIA_DAILY_DIGEST_FEISHU_ARCHIVE_TABLE_ID")]
    pub daily_digest_feishu_archive_table_id: Option<String>,

    #[arg(
        long,
        env = "QINTOPIA_DAILY_DIGEST_FEISHU_PROFILE_ENV_PATH",
        default_value = "/home/ubuntu/.hermes/profiles/xiaoman/.env"
    )]
    pub daily_digest_feishu_profile_env_path: String,

    #[arg(long, env = "QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_BASE_TOKEN")]
    pub xiaoman_activity_feishu_base_token: Option<String>,

    #[arg(
        long,
        env = "QINTOPIA_XIAOMAN_ACTIVITY_ALLOWED_FEISHU_BASE_TOKENS",
        default_value = ""
    )]
    pub xiaoman_activity_allowed_feishu_base_tokens: String,

    #[arg(long, env = "QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_PLAN_TABLE_ID")]
    pub xiaoman_activity_feishu_plan_table_id: Option<String>,

    #[arg(long, env = "QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_OCCURRENCE_TABLE_ID")]
    pub xiaoman_activity_feishu_occurrence_table_id: Option<String>,

    #[arg(
        long,
        env = "QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_PROFILE_ENV_PATH",
        default_value = "/home/ubuntu/.hermes/profiles/xiaoman/.env"
    )]
    pub xiaoman_activity_feishu_profile_env_path: String,

    #[arg(
        long,
        env = "QINTOPIA_OPERATIONS_ALLOWED_GROUP_ALIASES",
        default_value = ""
    )]
    pub operations_allowed_group_aliases: String,

    #[arg(
        long,
        env = "QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS",
        default_value = ""
    )]
    pub operations_allowed_group_ids: String,

    #[arg(
        long,
        env = "QINTOPIA_OPERATIONS_ALLOWED_REVIEWER_IDS",
        default_value = ""
    )]
    pub operations_allowed_reviewer_ids: String,

    #[arg(
        long,
        env = "QINTOPIA_OPERATIONS_ALLOWED_CONFIRMER_IDS",
        default_value = ""
    )]
    pub operations_allowed_confirmer_ids: String,

    #[arg(
        long,
        env = "QINTOPIA_OPERATIONS_ALLOWED_OWNER_IDS",
        default_value = ""
    )]
    pub operations_allowed_owner_ids: String,

    #[arg(
        long,
        env = "QINTOPIA_OPERATIONS_ALLOWED_ATTACHMENT_HOSTS",
        default_value = ""
    )]
    pub operations_allowed_attachment_hosts: String,

    #[arg(
        long,
        env = "QINTOPIA_DAILY_DIGEST_PUBLISHER_AGENT",
        default_value = "xiaoman"
    )]
    pub daily_digest_publisher_agent: String,

    #[arg(long, env = "QINTOPIA_DAILY_DIGEST_DISPATCH_RULES_JSON")]
    pub daily_digest_dispatch_rules_json: Option<String>,

    #[arg(
        long,
        env = "QINTOPIA_DAILY_DIGEST_DISPATCH_RULES_PATH",
        default_value = "config/agentos/daily-digest-dispatch-rules.json"
    )]
    pub daily_digest_dispatch_rules_path: String,

    #[arg(
        long,
        env = "QINTOPIA_RAW_MESSAGE_HOT_RETENTION_DAYS",
        default_value_t = 30
    )]
    pub raw_message_hot_retention_days: i64,

    #[arg(long, env = "QINTOPIA_RAW_ARCHIVE_FORMAT", default_value = "jsonl.zst")]
    pub raw_archive_format: String,

    #[arg(long, env = "QINTOPIA_RAW_ARCHIVE_DIR")]
    pub raw_archive_dir: Option<String>,

    #[arg(long, env = "QINTOPIA_GRAPH_BACKEND", default_value = "sql")]
    pub graph_backend: String,

    #[arg(long, env = "QINTOPIA_AGE_ENABLED", default_value_t = false)]
    pub age_enabled: bool,

    #[arg(
        long,
        env = "QINTOPIA_AGE_GRAPH_NAME",
        default_value = "qintopia_profile_graph"
    )]
    pub age_graph_name: String,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    /// Check NATS JetStream and optional Postgres connectivity.
    Check,
    /// Run Postgres migrations and exit.
    Migrate,
    /// Start the sidecar consumer loop.
    Run,
    /// Start the message embedding worker loop.
    RunEmbeddingWorker {
        /// Validate config and database connectivity, then exit without calling the embedding API.
        #[arg(long)]
        check_only: bool,
    },
    /// Start the QiWe sender identity worker loop.
    RunIdentityWorker {
        /// Run one batch and exit.
        #[arg(long)]
        check_only: bool,

        /// Distinct chat/sender pairs to process per batch.
        #[arg(
            long,
            env = "QINTOPIA_IDENTITY_WORKER_BATCH_SIZE",
            default_value_t = 10
        )]
        batch_size: i64,

        /// Delay between worker batches.
        #[arg(
            long,
            env = "QINTOPIA_IDENTITY_WORKER_POLL_SECONDS",
            default_value_t = 60
        )]
        poll_seconds: u64,

        /// Restrict the worker to one QiWe group/chat id.
        #[arg(long, env = "QINTOPIA_IDENTITY_WORKER_CHAT_ID")]
        chat_id: Option<String>,

        /// TTL for the in-process QiWe room member map cache.
        #[arg(
            long,
            env = "QINTOPIA_IDENTITY_MEMBER_MAP_TTL_SECONDS",
            default_value_t = 1200
        )]
        member_map_ttl_seconds: u64,
    },
    /// Start the Agent OS member profile worker loop.
    RunMemberProfileWorker {
        /// Run one batch and exit.
        #[arg(long)]
        check_only: bool,

        /// With --check-only, print only aggregate counts instead of candidate fact details.
        #[arg(long)]
        quiet: bool,

        /// Maximum messages to scan per batch.
        #[arg(
            long,
            env = "QINTOPIA_MEMBER_PROFILE_WORKER_BATCH_SIZE",
            default_value_t = 500
        )]
        batch_size: i64,

        /// Delay between worker batches.
        #[arg(
            long,
            env = "QINTOPIA_MEMBER_PROFILE_WORKER_POLL_SECONDS",
            default_value_t = 300
        )]
        poll_seconds: u64,

        /// Restrict the worker to one configured QiWe group/chat id.
        #[arg(long, env = "QINTOPIA_MEMBER_PROFILE_WORKER_CHAT_ID")]
        chat_id: Option<String>,
    },
    /// Start the SQL graph projection worker loop.
    RunGraphProjectionWorker {
        /// Run one batch and exit.
        #[arg(long)]
        check_only: bool,

        /// Maximum facts to project per batch.
        #[arg(
            long,
            env = "QINTOPIA_GRAPH_PROJECTION_WORKER_BATCH_SIZE",
            default_value_t = 500
        )]
        batch_size: i64,

        /// Delay between worker batches.
        #[arg(
            long,
            env = "QINTOPIA_GRAPH_PROJECTION_WORKER_POLL_SECONDS",
            default_value_t = 300
        )]
        poll_seconds: u64,

        /// Restrict the worker to one configured QiWe group/chat id.
        #[arg(long, env = "QINTOPIA_GRAPH_PROJECTION_WORKER_CHAT_ID")]
        chat_id: Option<String>,
    },
    /// Start the Agent OS event signal extraction worker loop.
    RunEventSignalWorker {
        /// Run one dry-run cycle and exit.
        #[arg(long)]
        check_only: bool,

        /// Run one apply cycle and exit.
        #[arg(long)]
        once: bool,

        /// Restrict to one configured QiWe group/chat id.
        #[arg(long, env = "QINTOPIA_EVENT_SIGNAL_WORKER_CHAT_ID")]
        chat_id: Option<String>,

        /// Signal date in YYYY-MM-DD. Defaults to yesterday in the configured timezone.
        #[arg(long)]
        date: Option<chrono::NaiveDate>,

        /// Delay between worker cycles.
        #[arg(
            long,
            env = "QINTOPIA_EVENT_SIGNAL_WORKER_POLL_SECONDS",
            default_value_t = 300
        )]
        poll_seconds: u64,

        /// Maximum messages to scan per cycle.
        #[arg(
            long,
            env = "QINTOPIA_EVENT_SIGNAL_WORKER_BATCH_SIZE",
            default_value_t = 2000
        )]
        limit: i64,
    },
    /// Start the raw message retention/archive worker loop.
    RunRawArchiveWorker {
        /// Run one batch and exit.
        #[arg(long)]
        check_only: bool,

        /// Maximum messages to archive per batch.
        #[arg(
            long,
            env = "QINTOPIA_RAW_ARCHIVE_WORKER_BATCH_SIZE",
            default_value_t = 1000
        )]
        batch_size: i64,

        /// Delay between worker batches.
        #[arg(
            long,
            env = "QINTOPIA_RAW_ARCHIVE_WORKER_POLL_SECONDS",
            default_value_t = 3600
        )]
        poll_seconds: u64,

        /// Restrict the worker to one configured QiWe group/chat id.
        #[arg(long, env = "QINTOPIA_RAW_ARCHIVE_WORKER_CHAT_ID")]
        chat_id: Option<String>,
    },
    /// Publish a test event and verify that the running sidecar persists it.
    Smoke {
        #[arg(
            long,
            env = "QINTOPIA_SIDECAR_SMOKE_TIMEOUT_SECONDS",
            default_value_t = 30
        )]
        timeout_seconds: u64,

        #[arg(
            long,
            env = "QINTOPIA_SIDECAR_SMOKE_POLL_INTERVAL_MS",
            default_value_t = 500
        )]
        poll_interval_ms: u64,
    },
    /// Inspect a persisted message by platform message id.
    InspectMessage {
        #[arg(long, default_value = "qiwe")]
        platform: String,

        #[arg(long)]
        message_id: String,
    },
    /// Run the read-only message store MCP server over stdio.
    McpMessageStore,
    /// Run the Agent-facing Qintopia context MCP server over stdio.
    McpContext,
    /// Import newline-delimited Qintopia knowledge snapshot files into Postgres.
    ImportKnowledgeSnapshot {
        #[arg(long)]
        public_jsonl: Option<String>,

        #[arg(long)]
        internal_jsonl: Option<String>,

        #[arg(long)]
        member_scoped_jsonl: Option<String>,

        #[arg(long, default_value = "qintopia-knowledge-snapshot")]
        source_key: String,

        #[arg(long, default_value = "Qintopia knowledge snapshot")]
        source_title: String,
    },
    /// Search the message store from the CLI using the same logic as the MCP tool.
    SearchMessageStore {
        #[arg(long)]
        query: Option<String>,

        #[arg(long, default_value = "hybrid")]
        search_mode: crate::message_search::SearchMode,

        #[arg(long)]
        chat_id: Option<String>,

        #[arg(long)]
        sender_id: Option<String>,

        #[arg(long)]
        chat_type: Option<String>,

        #[arg(long)]
        message_kind: Option<String>,

        #[arg(long)]
        since: Option<chrono::DateTime<chrono::Utc>>,

        #[arg(long)]
        until: Option<chrono::DateTime<chrono::Utc>>,

        #[arg(long)]
        limit: Option<i64>,

        #[arg(long, default_value = "wenyuange")]
        caller: String,

        #[arg(long)]
        purpose: String,
    },
    /// Resolve QiWe sender identities and optionally backfill captured messages.
    IdentityBackfill {
        /// Apply changes. Without this flag the command is a dry run.
        #[arg(long)]
        apply: bool,

        /// Force dry-run mode even if --apply is not present.
        #[arg(long)]
        dry_run: bool,

        /// Re-resolve identities even when messages already have sender names and identity links.
        #[arg(long)]
        refresh: bool,

        /// Maximum distinct chat/sender pairs to scan.
        #[arg(long)]
        limit: Option<i64>,

        /// Restrict backfill to one QiWe group/chat id.
        #[arg(long)]
        chat_id: Option<String>,

        /// Restrict backfill to one QiWe sender id.
        #[arg(long)]
        sender_id: Option<String>,

        /// Delay between QiWe identity API requests.
        #[arg(long, default_value_t = 0)]
        request_delay_ms: u64,
    },
    /// Create one person per QiWe channel identity and link captured messages.
    IdentityBootstrapPersons {
        /// Apply changes. Without this flag the command is a dry run.
        #[arg(long)]
        apply: bool,

        /// Force dry-run mode even if --apply is not present.
        #[arg(long)]
        dry_run: bool,

        /// Restrict bootstrap to one QiWe group/chat id.
        #[arg(long)]
        chat_id: Option<String>,

        /// Maximum channel identities to bootstrap.
        #[arg(long)]
        limit: Option<i64>,
    },
    /// Dry-run or apply member profile extraction for target QiWe group messages.
    MemberProfile {
        /// Apply changes. Without this flag the command is a dry run.
        #[arg(long)]
        apply: bool,

        /// Force dry-run mode even if --apply is not present.
        #[arg(long)]
        dry_run: bool,

        /// Restrict to one configured QiWe group/chat id.
        #[arg(long)]
        chat_id: Option<String>,

        /// Maximum messages to scan.
        #[arg(long)]
        limit: Option<i64>,
    },
    /// Generate a daily group operations digest draft.
    DailyDigest {
        /// Apply changes. Without this flag the command is a dry run.
        #[arg(long)]
        apply: bool,

        /// Force dry-run mode even if --apply is not present.
        #[arg(long)]
        dry_run: bool,

        /// Print only aggregate counts instead of the generated markdown.
        #[arg(long)]
        quiet: bool,

        /// Restrict to one configured QiWe group/chat id.
        #[arg(long)]
        chat_id: Option<String>,

        /// Digest date in YYYY-MM-DD. Defaults to yesterday in the configured timezone.
        #[arg(long)]
        date: Option<chrono::NaiveDate>,
    },
    /// Generate V2 structured event signal candidates and accepted events.
    EventSignal {
        /// Apply changes. Without this flag the command is a dry run.
        #[arg(long)]
        apply: bool,

        /// Force dry-run mode even if --apply is not present.
        #[arg(long)]
        dry_run: bool,

        /// Restrict to one configured QiWe group/chat id.
        #[arg(long)]
        chat_id: Option<String>,

        /// Signal date in YYYY-MM-DD. Defaults to yesterday in the configured timezone.
        #[arg(long)]
        date: Option<chrono::NaiveDate>,

        /// Maximum messages to scan.
        #[arg(long)]
        limit: Option<i64>,
    },
    /// Start the Agent OS daily community event radar worker loop.
    AgentosDailyDigestWorker {
        /// Run one dry-run cycle and exit.
        #[arg(long)]
        dry_run: bool,

        /// Run one apply cycle and exit.
        #[arg(long)]
        once: bool,

        /// Print only aggregate counts for one-shot runs.
        #[arg(long)]
        quiet: bool,

        /// Restrict to one configured QiWe group/chat id.
        #[arg(long)]
        chat_id: Option<String>,

        /// Digest date in YYYY-MM-DD. Defaults to yesterday in the configured timezone.
        #[arg(long)]
        date: Option<chrono::NaiveDate>,

        /// Delay between schedule checks.
        #[arg(
            long,
            env = "QINTOPIA_DAILY_DIGEST_WORKER_POLL_SECONDS",
            default_value_t = 60
        )]
        poll_seconds: u64,
    },
    /// Publish a generated daily digest through the narrow Agent OS publisher boundary.
    DailyDigestPublish {
        /// Apply changes. Without this flag the command is a dry run.
        #[arg(long)]
        apply: bool,

        /// Force dry-run mode even if --apply is not present.
        #[arg(long)]
        dry_run: bool,

        /// Digest outbox row id to publish.
        #[arg(long)]
        digest_id: uuid::Uuid,

        /// Actor agent requesting publication.
        #[arg(long, default_value = "xiaoman")]
        actor_agent: String,
    },
    /// Validate a Xiaoman activity worker payload from Hermes qintopia-tools.
    XiaomanActivity {
        /// Controlled activity operation name.
        operation: String,

        /// JSON payload emitted by qintopia_xiaoman_activity_* wrappers.
        #[arg(long)]
        payload_json: String,

        /// Local activity fixture for replay acceptance. Does not write production data.
        #[arg(long, env = "QINTOPIA_XIAOMAN_ACTIVITY_FIXTURE_PATH")]
        fixture_path: Option<std::path::PathBuf>,

        /// Read from the allowlisted Feishu Base activity tables instead of a local fixture.
        #[arg(long, env = "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE")]
        use_feishu_base: bool,

        /// Apply changes. Currently accepted only for read-only operations.
        #[arg(long)]
        apply: bool,

        /// Force dry-run mode even if --apply is not present.
        #[arg(long)]
        dry_run: bool,
    },
    /// Scan Xiaoman activity event_signals and submit signal-ingest work items.
    RunXiaomanActivitySignalWorker {
        /// Scan without writing AgentOS work items.
        #[arg(long)]
        check_only: bool,

        /// Process one batch and exit.
        #[arg(long)]
        once: bool,

        /// Apply AgentOS work item writes. Without this flag the worker previews only.
        #[arg(long)]
        apply: bool,

        /// Maximum event_signals to scan per batch.
        #[arg(long, default_value_t = 25)]
        batch_size: i64,

        /// Poll interval for long-running mode.
        #[arg(long, default_value_t = 300)]
        poll_seconds: u64,
    },
    /// Add missing evidence/visual child work items for Xiaoman activity requests.
    RunXiaomanActivityPromotionStarterWorker {
        /// Scan without writing AgentOS child work items.
        #[arg(long)]
        check_only: bool,

        /// Process one batch and exit.
        #[arg(long)]
        once: bool,

        /// Apply AgentOS child work item writes. Without this flag the worker previews only.
        #[arg(long)]
        apply: bool,

        /// Maximum parent work items to scan per batch.
        #[arg(long, default_value_t = 25)]
        batch_size: i64,

        /// Process one specific Xiaoman activity request work item.
        #[arg(long)]
        work_item_id: Option<uuid::Uuid>,
    },
    /// Add awaiting_publish group-message requests for approved Xiaoman activity posters.
    RunXiaomanActivitySendRequestStarterWorker {
        /// Scan without writing AgentOS group-message work items.
        #[arg(long)]
        check_only: bool,

        /// Process one batch and exit.
        #[arg(long)]
        once: bool,

        /// Apply AgentOS group-message work item writes. Without this flag the worker previews only.
        #[arg(long)]
        apply: bool,

        /// Maximum parent work items to scan per batch.
        #[arg(long, default_value_t = 25)]
        batch_size: i64,

        /// Process one specific Xiaoman parent or visual child work item.
        #[arg(long)]
        work_item_id: Option<uuid::Uuid>,

        /// Target allowlisted group alias for the future final-confirmed send.
        #[arg(long, default_value = "community_activity_group")]
        target_group_alias: String,

        /// Safe message text summary for final confirmation. This is not sent by this worker.
        #[arg(long, default_value = "活动海报已审核，请确认是否发送。")]
        message_text: String,
    },
    /// Add image-generation requests for approved Xiaoman activity poster briefs.
    RunXiaomanActivityImageGenerationStarterWorker {
        /// Scan without writing AgentOS image-generation work items.
        #[arg(long)]
        check_only: bool,

        /// Process one batch and exit.
        #[arg(long)]
        once: bool,

        /// Apply AgentOS image-generation work item writes. Without this flag the worker previews only.
        #[arg(long)]
        apply: bool,

        /// Maximum approved visual artifacts to scan per batch.
        #[arg(long, default_value_t = 25)]
        batch_size: i64,

        /// Process one specific visual work item or approved poster brief artifact.
        #[arg(long)]
        work_item_id: Option<uuid::Uuid>,
    },
    /// Create a capability-governed AgentOS operations work item.
    OperationsWorkItemCreate {
        /// JSON payload for the generic capability/work item request.
        #[arg(long)]
        payload_json: String,

        /// Apply changes. Without this flag the command only validates and previews.
        #[arg(long)]
        apply: bool,

        /// Force dry-run mode even if --apply is not present.
        #[arg(long)]
        dry_run: bool,
    },
    /// List AgentOS capabilities available for governed cross-Agent work.
    OperationsCapabilityList {
        /// Load capabilities from Postgres instead of the built-in offline registry.
        #[arg(long)]
        use_db: bool,
    },
    /// Check production readiness for AgentOS operations-control-plane rollout.
    OperationsReadinessCheck {
        /// Readiness profile to check: production or apply_smoke.
        #[arg(long, default_value = "production")]
        profile: String,

        /// Return a non-zero exit code when required readiness checks fail.
        #[arg(long)]
        strict: bool,
    },
    /// Plan a non-technical natural-language request into a governed work item.
    OperationsRequestPlan {
        /// JSON payload with actor_agent, request_text, and optional source refs.
        #[arg(long)]
        payload_json: String,
    },
    /// Plan and optionally create a governed work item from a non-technical request.
    OperationsRequestSubmit {
        /// JSON payload with actor_agent, request_text, and optional source refs.
        #[arg(long)]
        payload_json: String,

        /// Apply changes. Without this flag the command only validates and previews.
        #[arg(long)]
        apply: bool,

        /// Force dry-run mode even if --apply is not present.
        #[arg(long)]
        dry_run: bool,
    },
    /// Start a governed multi-step operations workflow.
    OperationsWorkflowStart {
        /// JSON payload for workflow_type, actor_agent, request_text, and source refs.
        #[arg(long)]
        payload_json: String,

        /// Apply changes. Without this flag the command only validates and previews.
        #[arg(long)]
        apply: bool,

        /// Force dry-run mode even if --apply is not present.
        #[arg(long)]
        dry_run: bool,
    },
    /// Record a human review decision for an operations artifact.
    OperationsArtifactReviewDecision {
        /// JSON payload for the artifact review decision.
        #[arg(long)]
        payload_json: String,

        /// Apply changes. Without this flag the command only validates and previews.
        #[arg(long)]
        apply: bool,

        /// Force dry-run mode even if --apply is not present.
        #[arg(long)]
        dry_run: bool,
    },
    /// Record human final confirmation for an Erhua group-message request.
    OperationsGroupMessageConfirm {
        /// JSON payload for the group-message confirmation decision.
        #[arg(long)]
        payload_json: String,

        /// Apply changes. Without this flag the command only validates and previews.
        #[arg(long)]
        apply: bool,

        /// Force dry-run mode even if --apply is not present.
        #[arg(long)]
        dry_run: bool,
    },
    /// Record a validated human workbench event without mutating work item state.
    OperationsWorkbenchEventRecord {
        /// JSON payload for the human workbench event.
        #[arg(long)]
        payload_json: String,

        /// Apply changes. Without this flag the command only validates and previews.
        #[arg(long)]
        apply: bool,

        /// Force dry-run mode even if --apply is not present.
        #[arg(long)]
        dry_run: bool,
    },
    /// Process one recorded human workbench event through policy-checked AgentOS commands.
    OperationsWorkbenchEventProcess {
        /// work_item_events.id for a human_workbench_event_recorded event.
        #[arg(long)]
        event_id: uuid::Uuid,

        /// Apply changes. Without this flag the command only validates and previews.
        #[arg(long)]
        apply: bool,

        /// Force dry-run mode even if --apply is not present.
        #[arg(long)]
        dry_run: bool,
    },
    /// Process recorded human workbench events through policy-checked AgentOS commands.
    RunWorkbenchEventWorker {
        /// Run one processable event and exit.
        #[arg(long)]
        once: bool,

        /// Process one specific human_workbench_event_recorded event.
        #[arg(long)]
        event_id: Option<uuid::Uuid>,

        /// Apply changes. Without this flag the worker previews the next event.
        #[arg(long)]
        apply: bool,

        /// Force dry-run mode even if --apply is not present.
        #[arg(long)]
        dry_run: bool,
    },
    /// Read a parent/child operations work item status tree.
    OperationsWorkItemStatus {
        /// Parent or child work item id to inspect.
        #[arg(long)]
        work_item_id: uuid::Uuid,
    },
    /// Sync one workflow parent summary from its child work item states.
    OperationsWorkflowSync {
        /// Parent or child work item id to summarize.
        #[arg(long)]
        work_item_id: uuid::Uuid,

        /// Apply changes. Without this flag the command previews the parent summary.
        #[arg(long)]
        apply: bool,

        /// Force dry-run mode even if --apply is not present.
        #[arg(long)]
        dry_run: bool,
    },
    /// Sync workflow parent summaries from child work item states.
    RunWorkflowSyncWorker {
        /// Run one workflow parent and exit.
        #[arg(long)]
        once: bool,

        /// Sync one specific parent or child work item instead of the oldest syncable parent.
        #[arg(long)]
        work_item_id: Option<uuid::Uuid>,

        /// Apply changes. Without this flag the worker previews the next workflow parent.
        #[arg(long)]
        apply: bool,

        /// Force dry-run mode even if --apply is not present.
        #[arg(long)]
        dry_run: bool,
    },
    /// Claim visual collaboration work items and create draft artifacts.
    RunCollaborationWorker {
        /// Work item type to process. Currently supports visual_asset_request.
        #[arg(long, default_value = "visual_asset_request")]
        work_item_type: String,

        /// Run one batch and exit.
        #[arg(long)]
        once: bool,

        /// Process one specific work item instead of the oldest claimable item.
        #[arg(long)]
        work_item_id: Option<uuid::Uuid>,

        /// Apply changes. Without this flag the worker previews the next item.
        #[arg(long)]
        apply: bool,

        /// Force dry-run mode even if --apply is not present.
        #[arg(long)]
        dry_run: bool,

        /// Use deterministic local fixture input instead of reading Postgres.
        #[arg(long)]
        fixture_mode: bool,
    },
    /// Generate review-pending image artifacts from approved Huabaosi poster briefs.
    RunHuabaosiImageGenerationWorker {
        /// Process one work item and exit.
        #[arg(long)]
        once: bool,

        /// Restrict processing to one image-generation work item.
        #[arg(long)]
        work_item_id: Option<uuid::Uuid>,

        /// Apply the configured provider adapter. Disabled by default and requires owner-reviewed configuration.
        #[arg(long)]
        apply: bool,

        /// Force preview mode even if --apply is absent.
        #[arg(long)]
        dry_run: bool,

        /// Use a deterministic local preview without database writes or network calls.
        #[arg(long)]
        fixture_mode: bool,
    },
    /// Validate Huabaosi image adapter configuration without opening network or database connections.
    HuabaosiImageGenerationPreflight,
    /// Preview-sanitize one Huabaosi WeCom event from stdin without writing, sending, or generating assets.
    HuabaosiWecomShadowCapture,
    /// Preview Huabaosi WeCom gateway policy for one stdin event without writing, sending, or generating assets.
    HuabaosiWecomPolicyPreview,
    /// Validate the disabled QiWe async image-upload/send contract without network or database access.
    QiweImageSendPreflight,
    /// Claim one reviewed QiWe image-send request and submit its asynchronous URL upload.
    RunQiweImageSendWorker {
        /// Process one work item and exit.
        #[arg(long)]
        once: bool,

        /// Restrict processing to one group-message work item.
        #[arg(long)]
        work_item_id: Option<uuid::Uuid>,

        /// Apply the external upload adapter. Requires explicit reviewed enablement.
        #[arg(long)]
        apply: bool,

        /// Preview one eligible work item without claiming, writing, or networking.
        #[arg(long)]
        dry_run: bool,
    },
    /// Process one bounded QiWe cmd=20000 callback read from stdin.
    ProcessQiweImageSendCallback {
        /// Apply callback correlation and at-most-once external image send.
        #[arg(long)]
        apply: bool,

        /// Validate callback shape only; do not open Postgres or network connections.
        #[arg(long)]
        dry_run: bool,
    },
    /// Claim Wenyuange evidence requests and create internal evidence artifacts.
    RunEvidenceWorker {
        /// Run one batch and exit.
        #[arg(long)]
        once: bool,

        /// Process one specific evidence work item instead of the oldest claimable item.
        #[arg(long)]
        work_item_id: Option<uuid::Uuid>,

        /// Apply changes. Apply only writes AgentOS artifacts/events; it does not call external search.
        #[arg(long)]
        apply: bool,

        /// Force dry-run mode even if --apply is not present.
        #[arg(long)]
        dry_run: bool,

        /// Use deterministic local fixture input instead of reading Postgres.
        #[arg(long)]
        fixture_mode: bool,
    },
    /// Validate queued Erhua group-message requests without sending them.
    RunGroupMessageSendWorker {
        /// Run one batch and exit.
        #[arg(long)]
        once: bool,

        /// Process one specific group-message work item instead of the oldest claimable item.
        #[arg(long)]
        work_item_id: Option<uuid::Uuid>,

        /// Apply changes. Apply only records send-readiness; it does not send.
        #[arg(long)]
        apply: bool,

        /// Force dry-run mode even if --apply is not present.
        #[arg(long)]
        dry_run: bool,

        /// Use deterministic local fixture input instead of reading Postgres.
        #[arg(long)]
        fixture_mode: bool,
    },
    /// Build sanitized Feishu Task mirror payloads without calling Feishu.
    RunWorkbenchMirrorWorker {
        /// Run one batch and exit.
        #[arg(long)]
        once: bool,

        /// Mirror one specific work item instead of the oldest mirrorable item.
        #[arg(long)]
        work_item_id: Option<uuid::Uuid>,

        /// Apply changes. Apply only records a dry-run workbench ref.
        #[arg(long)]
        apply: bool,

        /// Force dry-run mode even if --apply is not present.
        #[arg(long)]
        dry_run: bool,

        /// Use deterministic local fixture input instead of reading Postgres.
        #[arg(long)]
        fixture_mode: bool,
    },
    /// Start the narrow Feishu Base publisher worker for generated daily digests.
    RunDailyDigestPublisherWorker {
        /// Run one dry-run batch and exit.
        #[arg(long)]
        check_only: bool,

        /// Run one apply batch and exit.
        #[arg(long)]
        once: bool,

        /// Maximum digest rows to publish per batch.
        #[arg(
            long,
            env = "QINTOPIA_DAILY_DIGEST_PUBLISHER_BATCH_SIZE",
            default_value_t = 10
        )]
        batch_size: i64,

        /// Delay between publisher batches.
        #[arg(
            long,
            env = "QINTOPIA_DAILY_DIGEST_PUBLISHER_POLL_SECONDS",
            default_value_t = 120
        )]
        poll_seconds: u64,

        /// Actor agent requesting publication.
        #[arg(
            long,
            env = "QINTOPIA_DAILY_DIGEST_PUBLISHER_AGENT",
            default_value = "xiaoman"
        )]
        actor_agent: String,
    },
    /// Project member profile facts and summaries into qintopia_graph SQL tables.
    GraphProjection {
        /// Apply changes. Without this flag the command is a dry run.
        #[arg(long)]
        apply: bool,

        /// Force dry-run mode even if --apply is not present.
        #[arg(long)]
        dry_run: bool,

        /// Restrict to one configured QiWe group/chat id.
        #[arg(long)]
        chat_id: Option<String>,

        /// Maximum facts to project.
        #[arg(long)]
        limit: Option<i64>,
    },
    /// Archive raw messages older than the hot retention window.
    RawArchive {
        /// Apply changes. Without this flag the command is a dry run.
        #[arg(long)]
        apply: bool,

        /// Force dry-run mode even if --apply is not present.
        #[arg(long)]
        dry_run: bool,

        /// Restrict to one configured QiWe group/chat id.
        #[arg(long)]
        chat_id: Option<String>,

        /// Maximum messages to archive.
        #[arg(long)]
        limit: Option<i64>,
    },
}

impl Cli {
    pub fn database_url_required(&self) -> anyhow::Result<&str> {
        self.database_url
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("QINTOPIA_SIDECAR_DATABASE_URL is required"))
    }

    pub fn embedding_api_key_required(&self) -> anyhow::Result<&str> {
        self.embedding_api_key.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "QINTOPIA_EMBEDDING_API_KEY is required for the message embedding worker"
            )
        })
    }

    pub fn profile_target_chat_ids(&self) -> Vec<String> {
        csv_items(&self.profile_target_chat_ids)
    }

    pub fn profile_excluded_channel_user_ids(&self) -> Vec<String> {
        csv_items(&self.profile_excluded_channel_user_ids)
    }

    pub fn profile_excluded_display_names(&self) -> Vec<String> {
        csv_items(&self.profile_excluded_display_names)
    }

    #[allow(dead_code)]
    pub fn daily_digest_allowed_feishu_parent_nodes(&self) -> Vec<String> {
        csv_items(&self.daily_digest_allowed_feishu_parent_nodes)
    }

    pub fn daily_digest_allowed_feishu_base_tokens(&self) -> Vec<String> {
        csv_items(&self.daily_digest_allowed_feishu_base_tokens)
    }

    pub fn xiaoman_activity_allowed_feishu_base_tokens(&self) -> Vec<String> {
        csv_items(&self.xiaoman_activity_allowed_feishu_base_tokens)
    }

    pub fn operations_allowed_reviewer_ids(&self) -> Vec<String> {
        csv_items(&self.operations_allowed_reviewer_ids)
    }

    pub fn operations_allowed_confirmer_ids(&self) -> Vec<String> {
        csv_items(&self.operations_allowed_confirmer_ids)
    }

    pub fn operations_allowed_owner_ids(&self) -> Vec<String> {
        csv_items(&self.operations_allowed_owner_ids)
    }

    pub fn operations_allowed_attachment_hosts(&self) -> Vec<String> {
        csv_items(&self.operations_allowed_attachment_hosts)
    }
}

fn csv_items(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
        .collect()
}
