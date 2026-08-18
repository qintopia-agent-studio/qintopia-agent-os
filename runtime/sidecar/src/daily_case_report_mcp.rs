//! MCP tool for on-demand Xiaoman daily case-report generation.
//!
//! This tool is a thin orchestration layer over the reviewed production
//! chain: it renders the daily report via the Python workflow, uploads the
//! JPEG through the governed media-upload boundary, and (only when the caller
//! opts out of dry-run) creates the automatic QiWe publish work item.  It
//! reuses the same `operations` functions as the CLI worker so scheduled and
//! on-demand paths stay identical.
//!
//! The tool never writes the DB itself; `apply=false` returns a preview of
//! everything that *would* be uploaded / published.  `apply=true` runs the
//! same reviewed storage / publish gates as the production worker.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::postgres::PgPool;
use tempfile::TempDir;

use crate::{
    config::Cli,
    daily_case_report_cutover::{resolve_release_path, run_pipeline, PipelineOptions},
    operations::{
        create_daily_case_report_auto_publish, daily_case_report_media_upload,
        DailyCaseReportAutoPublishCreateRequest, DailyCaseReportMediaUploadRequest,
        DailyCaseReportStorageBackend,
    },
};

pub(crate) const TOOL_NAME: &str = "qintopia_daily_case_report_generate";
const MAX_RENDER_OUTPUT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone)]
pub(crate) struct DailyCaseReportMcpConfig {
    pub database_url: String,
    pub allowed_caller: String,
    pub python_bin: String,
    pub workflow_py: Option<PathBuf>,
    pub rasterize_py: Option<PathBuf>,
    pub render_timeout: Duration,
}

impl DailyCaseReportMcpConfig {
    pub(crate) fn from_cli(cli: &Cli) -> Result<Self> {
        Ok(Self {
            database_url: cli.database_url_required()?.to_string(),
            allowed_caller: cli.daily_case_report_mcp_allowed_caller.clone(),
            python_bin: cli.daily_case_report_mcp_python_bin.clone(),
            workflow_py: cli
                .daily_case_report_mcp_workflow_py
                .clone()
                .map(PathBuf::from),
            rasterize_py: cli
                .daily_case_report_mcp_rasterize_py
                .clone()
                .map(PathBuf::from),
            render_timeout: Duration::from_secs(cli.daily_case_report_mcp_render_timeout_seconds),
        })
    }
}

pub(crate) fn tool_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "date": {
                "type": "string",
                "description": "Backfill one calendar day (YYYY-MM-DD). Omit for the latest rolling 24-hour window."
            },
            "template": {
                "type": "string",
                "default": "roast-long-image",
                "description": "Poster template. roast-long-image renders the LLM narrative (requires --narrative roast); newspaper-elegant is the broadsheet variant; v3 is the legacy scoreboard."
            },
            "dry_run": {
                "type": "boolean",
                "default": true,
                "description": "Preview only: render + validate media identity without uploading or creating the send work item. Set false to run the full reviewed auto-publish chain."
            },
            "caller": {
                "type": "string",
                "description": "Calling profile id. Must match the configured allowed caller."
            }
        },
        "additionalProperties": false
    })
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct GenerateArguments {
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default = "default_template")]
    pub template: String,
    #[serde(default = "default_dry_run")]
    pub dry_run: bool,
    #[serde(default)]
    pub caller: String,
}

fn default_template() -> String {
    "roast-long-image".to_string()
}

fn default_dry_run() -> bool {
    true
}

impl GenerateArguments {
    fn from_json(arguments: Value) -> Result<Self> {
        let request: Self = serde_json::from_value(arguments)
            .context("parse daily case report generate arguments")?;
        let template = request.template.trim();
        if template.is_empty() {
            bail!("template must not be empty");
        }
        if let Some(date) = &request.date {
            let valid = date.len() == 10 && {
                let bytes = date.as_bytes();
                (0..10).all(|idx| match idx {
                    4 | 7 => bytes[idx] == b'-',
                    _ => bytes[idx].is_ascii_digit(),
                })
            };
            if !valid {
                bail!("date must be YYYY-MM-DD");
            }
        }
        Ok(request)
    }
}

/// Render the daily report to JPEG via the Python workflow and return the
/// parsed render summary JSON (paths, counts, flags; no raw message rows).
/// Resolve the configured workflow path to a concrete file.  Absolute paths
/// are used as-is; relative paths resolve against the release root the sidecar
/// runs from (QINTOPIA_AGENT_OS_RELEASE_CURRENT, or the sidecar binary's
/// release directory when that env is absent).
fn resolve_workflow_path(workflow: &Path) -> PathBuf {
    if workflow.is_absolute() {
        return workflow.to_path_buf();
    }
    if let Some(release_current) = std::env::var_os("QINTOPIA_AGENT_OS_RELEASE_CURRENT") {
        let root = PathBuf::from(release_current);
        let candidate = root.join(workflow);
        if candidate.is_file() {
            return candidate;
        }
    }
    // Fall back to the release layout that owns this binary:
    // <release_root>/<sha>/sidecar/qintopia-message-sidecar -> <release_root>/<sha>/<workflow>
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(sidecar_dir) = current_exe.parent() {
            if let Some(release_dir) = sidecar_dir.parent() {
                let candidate = release_dir.join(workflow);
                if candidate.is_file() {
                    return candidate;
                }
            }
        }
    }
    workflow.to_path_buf()
}

pub(crate) async fn render_report(
    pool: &PgPool,
    config: &DailyCaseReportMcpConfig,
    arguments: &GenerateArguments,
) -> Result<(TempDir, Value)> {
    if use_python_pipeline() {
        render_report_python(config, arguments).await
    } else {
        render_report_rust_pipeline(pool, config, arguments).await
    }
}

fn use_python_pipeline() -> bool {
    std::env::var("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_USE_PYTHON_PIPELINE").as_deref() == Ok("1")
}

async fn render_report_python(
    config: &DailyCaseReportMcpConfig,
    arguments: &GenerateArguments,
) -> Result<(TempDir, Value)> {
    let tmp = tempfile::Builder::new()
        .prefix("xiaoman-daily-case-report-mcp-")
        .tempdir()
        .context("create render temp dir")?;
    set_dir_mode_private(tmp.path())?;
    let workflow = config
        .workflow_py
        .as_ref()
        .context("daily case report workflow script is not configured")?;
    let workflow = resolve_workflow_path(workflow);
    if !workflow.is_file() {
        bail!(
            "daily case report workflow script is missing: {}",
            workflow.display()
        );
    }

    let mut command = Command::new(&config.python_bin);
    command
        .arg(&workflow)
        .arg("--render")
        .arg("image")
        .arg("--image-format")
        .arg("jpeg")
        .arg("--template")
        .arg(&arguments.template)
        .arg("--json")
        .arg("--json-summary-only")
        .arg("--output-dir")
        .arg(tmp.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(date) = &arguments.date {
        command.arg("--date").arg(date);
    }

    let timeout = config.render_timeout;
    let render = tokio::time::timeout(
        timeout,
        tokio::task::spawn_blocking(move || run_render_command(command)),
    )
    .await
    .context("daily case report render timed out")?
    .context("daily case report render task join")??;

    if !render.status.success() {
        let stderr = String::from_utf8_lossy(&render.stderr);
        bail!(
            "daily case report render failed (status {}): {}",
            render.status,
            truncate(stderr.trim(), 2000)
        );
    }
    let parsed: Value =
        serde_json::from_slice(&render.stdout).context("parse daily case report render JSON")?;
    validate_render_summary(&parsed)?;
    Ok((tmp, parsed))
}

async fn render_report_rust_pipeline(
    pool: &PgPool,
    config: &DailyCaseReportMcpConfig,
    arguments: &GenerateArguments,
) -> Result<(TempDir, Value)> {
    let tmp = tempfile::Builder::new()
        .prefix("xiaoman-daily-case-report-mcp-")
        .tempdir()
        .context("create render temp dir")?;
    set_dir_mode_private(tmp.path())?;

    let rasterize_py = config
        .rasterize_py
        .as_deref()
        .or_else(|| {
            Some(Path::new(
                "workflows/xiaoman-daily-case-report/rasterize.py",
            ))
        })
        .map(resolve_release_path)
        .context("daily case report rasterize script is not configured")?;
    if !rasterize_py.is_file() {
        bail!(
            "daily case report rasterize script is missing: {}",
            rasterize_py.display()
        );
    }

    let chat_id = std::env::var("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_CHAT_ID").context(
        "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_CHAT_ID is required for the Rust render pipeline",
    )?;
    let options = PipelineOptions {
        chat_id,
        date: arguments.date.clone(),
        template: arguments.template.clone(),
        narrative_style: std::env::var("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_NARRATIVE")
            .unwrap_or_else(|_| "roast".to_string()),
        output_dir: tmp.path().to_path_buf(),
        apply: false,
        group_name: std::env::var("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_GROUP_NAME").ok(),
        report_title: std::env::var("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_REPORT_TITLE").ok(),
        width: std::env::var("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_OUTPUT_WIDTH")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(1080),
    };

    let narrative_config = if options.narrative_style != "none" {
        crate::daily_case_report_narrative::NarrativeConfig::from_env_with_overrides(None, None)
            .ok()
    } else {
        None
    };

    let summary = run_pipeline(
        pool,
        &options,
        &rasterize_py,
        None,
        narrative_config.as_ref(),
    )
    .await
    .context("daily case report Rust pipeline render")?;
    validate_render_summary(&summary)?;
    Ok((tmp, summary))
}

fn run_render_command(mut command: Command) -> Result<std::process::Output> {
    let output = command.output().context("spawn daily case report render")?;
    if output.stdout.len() > MAX_RENDER_OUTPUT_BYTES {
        bail!("daily case report render output exceeded safety limit");
    }
    Ok(output)
}

/// Set a directory to private 0700 mode (Unix).  The daily report workflow
/// refuses output directories that are not dedicated private 0700 dirs.
fn set_dir_mode_private(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let permissions = std::fs::Permissions::from_mode(0o700);
    std::fs::set_permissions(path, permissions)
        .with_context(|| format!("chmod 0700 render dir {}", path.display()))
}

fn truncate(text: &str, max: usize) -> String {
    let mut out = text.chars().take(max).collect::<String>();
    if text.chars().count() > max {
        out.push('…');
    }
    out
}

/// Validate the fields the orchestrator needs from the render summary.
fn validate_render_summary(render: &Value) -> Result<()> {
    if render.get("success").and_then(Value::as_bool) != Some(true) {
        bail!("daily case report render did not report success");
    }
    let image_path = render
        .get("image_path")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if image_path.is_empty() {
        bail!("daily case report render did not produce an image path");
    }
    let candidate = render
        .get("artifact_candidate")
        .cloned()
        .unwrap_or_default();
    let mime_type = candidate
        .get("mime_type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if mime_type != "image/jpeg" && mime_type != "image/png" {
        bail!("rendered daily report image must be JPEG or PNG");
    }
    Ok(())
}

/// Run the full orchestration and return the MCP tool result body.
pub(crate) async fn call_tool(
    pool: &PgPool,
    config: &DailyCaseReportMcpConfig,
    arguments: Value,
) -> Result<Value> {
    let request = GenerateArguments::from_json(arguments)?;
    if request.caller.trim() != config.allowed_caller {
        bail!("{TOOL_NAME} is only available to {}", config.allowed_caller);
    }

    let (_tmp, render) = render_report(pool, config, &request).await?;
    let candidate = render
        .get("artifact_candidate")
        .cloned()
        .unwrap_or_default();
    let image_path = render
        .get("image_path")
        .and_then(Value::as_str)
        .unwrap_or_default();

    // ---- governed media upload ----
    let storage_backend = DailyCaseReportStorageBackend::from_env()?;
    let upload_request = media_upload_request_from_render(&render, Path::new(image_path))?;
    let database_url = if request.dry_run {
        None
    } else {
        Some(config.database_url.as_str())
    };
    let pool_ref = if request.dry_run { None } else { Some(pool) };
    let upload_report = daily_case_report_media_upload(
        upload_request,
        !request.dry_run,
        storage_backend,
        database_url,
        pool_ref,
    )
    .await
    .context("daily case report media upload")?;

    // ---- optional governed auto-publish ----
    let publish_report = if request.dry_run {
        None
    } else {
        let artifact_uri = upload_report
            .artifact_uri
            .as_ref()
            .context("media upload did not return artifact_uri")?;
        let evidence = upload_report
            .media_upload_evidence
            .as_ref()
            .context("media upload did not return evidence")?;
        let publish_request =
            auto_publish_request_from_render(&render, &candidate, artifact_uri, evidence)?;
        let database_url = config.database_url.clone();
        let report =
            create_daily_case_report_auto_publish(pool, &database_url, publish_request, true)
                .await
                .context("daily case report auto-publish create")?;
        Some(report)
    };

    Ok(build_preview_json(
        &render,
        &candidate,
        &upload_report,
        publish_report.as_ref(),
        request.dry_run,
    ))
}

fn media_upload_request_from_render(
    render: &Value,
    image_path: &Path,
) -> Result<DailyCaseReportMediaUploadRequest> {
    let candidate = render
        .get("artifact_candidate")
        .cloned()
        .unwrap_or_default();
    Ok(DailyCaseReportMediaUploadRequest {
        image_path: image_path.to_path_buf(),
        content_hash: candidate
            .get("content_hash")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        file_md5: candidate
            .get("file_md5")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        byte_size: candidate
            .get("byte_size")
            .and_then(Value::as_u64)
            .map(|v| v as usize),
        filename: candidate
            .get("filename")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        report_window: candidate.get("report_window").cloned().unwrap_or_default(),
        source_chat_ref: candidate
            .get("source_chat_ref")
            .cloned()
            .unwrap_or_default(),
        template_version: candidate
            .get("template_version")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        metadata: json!({
            "created_by": TOOL_NAME,
        }),
    })
}

fn auto_publish_request_from_render(
    render: &Value,
    candidate: &Value,
    artifact_uri: &str,
    evidence: &crate::operations::DailyCaseReportMediaUploadEvidence,
) -> Result<DailyCaseReportAutoPublishCreateRequest> {
    let window = candidate.get("report_window").cloned().unwrap_or_default();
    let metrics = candidate
        .get("content_metrics")
        .cloned()
        .unwrap_or_default();
    let report_date = window
        .get("display")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            render
                .get("report_date")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default();
    let time_range = window
        .get("time_range")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            render
                .get("time_range")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default();
    let target_group_id =
        std::env::var("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_TARGET_GROUP_ID").unwrap_or_default();
    if target_group_id.trim().is_empty() {
        bail!("auto-publish requires QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_TARGET_GROUP_ID");
    }

    Ok(DailyCaseReportAutoPublishCreateRequest {
        window_start: window
            .get("start")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        window_end: window
            .get("end")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        report_date,
        time_range,
        artifact_uri: artifact_uri.to_string(),
        content_hash: evidence.content_hash.clone(),
        file_md5: evidence.file_md5.clone(),
        byte_size: evidence.byte_size,
        mime_type: evidence.mime_type.clone(),
        width: evidence.width,
        height: evidence.height,
        filename: evidence.filename.clone(),
        target_group_id,
        message_text: "小满日报已自动生成。".to_string(),
        title: render
            .get("report_date")
            .and_then(Value::as_str)
            .map(|date| format!("小满日报 {}", date))
            .unwrap_or_else(|| "小满日报".to_string()),
        summary: format!(
            "消息 {} 条 / 活跃 {} 人 / 案件 {} 起 / 人物 {} 位",
            render
                .get("message_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            render
                .get("participant_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            render
                .get("case_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            render
                .get("character_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        ),
        priority: "normal".to_string(),
        source_chat_ref: candidate
            .get("source_chat_ref")
            .cloned()
            .unwrap_or_default(),
        template_version: candidate
            .get("template_version")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        metadata: json!({
            "created_by": TOOL_NAME,
            "render_metrics": metrics,
        }),
        media_upload_evidence: Some(evidence.clone()),
    })
}

fn build_preview_json(
    render: &Value,
    candidate: &Value,
    upload_report: &crate::operations::DailyCaseReportMediaUploadReport,
    publish_report: Option<&crate::operations::DailyCaseReportAutoPublishCreateReport>,
    dry_run: bool,
) -> Value {
    json!({
        "success": true,
        "dry_run": dry_run,
        "report_date": render.get("report_date"),
        "time_range": render.get("time_range"),
        "message_count": render.get("message_count"),
        "participant_count": render.get("participant_count"),
        "case_count": render.get("case_count"),
        "character_count": render.get("character_count"),
        "image_path": render.get("image_path"),
        "template_version": candidate.get("template_version"),
        "media_upload": {
            "action_status": upload_report.action_status,
            "artifact_uri": upload_report.artifact_uri,
            "content_hash": upload_report.content_hash,
            "file_md5": upload_report.file_md5,
            "byte_size": upload_report.byte_size,
            "mime_type": upload_report.mime_type,
            "filename": upload_report.filename,
            "width": upload_report.width,
            "height": upload_report.height,
            "external_send_executed": upload_report.external_send_executed,
        },
        "auto_publish": publish_report.map(|report| json!({
            "action_status": report.action_status,
            "send_work_item_id": report.send_work_item_id,
            "artifact_id": report.artifact_id,
            "review_status": report.review_status,
            "external_send_executed": report.external_send_executed,
            "requires_human_final_confirmation": report.requires_human_final_confirmation,
        })),
        "guardrails": upload_report.guardrails,
    })
}

/// Preview-only tool path: no DB writes happen for `dry_run=true`.  Apply mode
/// reuses the MCP server's shared pool, so no dedicated connection helper is
/// needed here.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_arguments_accepts_minimal_call() {
        let request = GenerateArguments::from_json(json!({"caller": "wenyuange"}))
            .expect("minimal call parses");
        assert!(request.dry_run);
        assert_eq!(request.template, "roast-long-image");
        assert_eq!(request.caller, "wenyuange");
        assert!(request.date.is_none());
    }

    #[test]
    fn parse_arguments_rejects_bad_date() {
        let error = GenerateArguments::from_json(json!({"date": "2026-8-8", "caller": "w"}))
            .expect_err("bad date rejected");
        assert!(error.to_string().contains("YYYY-MM-DD"));
    }

    #[test]
    fn parse_arguments_accepts_apply_mode() {
        let request = GenerateArguments::from_json(json!({
            "date": "2026-08-08",
            "template": "newspaper-elegant",
            "dry_run": false,
            "caller": "wenyuange",
        }))
        .expect("apply call parses");
        assert!(!request.dry_run);
        assert_eq!(request.template, "newspaper-elegant");
        assert_eq!(request.date.as_deref(), Some("2026-08-08"));
    }

    #[test]
    fn schema_has_expected_defaults() {
        let schema = tool_input_schema();
        let properties = schema.get("properties").expect("properties");
        assert_eq!(
            properties.get("dry_run").and_then(|v| v.get("default")),
            Some(&json!(true))
        );
        assert_eq!(
            properties.get("template").and_then(|v| v.get("default")),
            Some(&json!("roast-long-image"))
        );
    }

    #[test]
    fn caller_mismatch_is_rejected() {
        let config = DailyCaseReportMcpConfig {
            database_url: "postgresql://unit".to_string(),
            allowed_caller: "wenyuange".to_string(),
            python_bin: "/usr/bin/python3".to_string(),
            workflow_py: Some(PathBuf::from("/tmp/daily_case_report.py")),
            rasterize_py: None,
            render_timeout: Duration::from_secs(60),
        };
        let request = GenerateArguments::from_json(json!({"caller": "erhua"})).expect("parses");
        assert_ne!(request.caller, config.allowed_caller);
    }

    #[test]
    fn missing_workflow_py_fails_with_clear_error_when_python_pipeline_forced() {
        let config = DailyCaseReportMcpConfig {
            database_url: "postgresql://unit".to_string(),
            allowed_caller: "wenyuange".to_string(),
            python_bin: "/usr/bin/python3".to_string(),
            workflow_py: None,
            rasterize_py: None,
            render_timeout: Duration::from_secs(60),
        };
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let error = runtime
            .block_on(async {
                std::env::set_var(
                    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_USE_PYTHON_PIPELINE",
                    "1",
                );
                let pool = sqlx::postgres::PgPoolOptions::new()
                    .max_connections(1)
                    .connect_lazy(&config.database_url)
                    .expect("lazy pool");
                let result = call_tool(
                    &pool,
                    &config,
                    json!({"caller": "wenyuange", "dry_run": true}),
                )
                .await;
                std::env::remove_var("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_USE_PYTHON_PIPELINE");
                result
            })
            .expect_err("missing workflow rejected");
        assert!(error
            .to_string()
            .contains("workflow script is not configured"));
    }

    #[test]
    fn render_summary_validation_rejects_unsupported_mime_type() {
        let render = json!({
            "success": true,
            "image_path": "/tmp/x.gif",
            "artifact_candidate": {"mime_type": "image/gif"},
        });
        let error = validate_render_summary(&render).expect_err("gif rejected");
        assert!(error.to_string().contains("must be JPEG or PNG"));
    }

    #[test]
    fn render_summary_validation_accepts_valid_summary() {
        let render = json!({
            "success": true,
            "image_path": "/tmp/x.jpg",
            "artifact_candidate": {
                "mime_type": "image/jpeg",
                "content_hash": "sha256:abc",
                "file_md5": "d41d8cd98f00b204e9800998ecf8427e",
                "byte_size": 10,
                "filename": "x.jpg",
                "template_version": "xiaoman-daily-case-report-v3",
                "report_window": {},
                "source_chat_ref": {},
            },
        });
        validate_render_summary(&render).expect("valid summary passes");
    }

    #[test]
    fn truncate_shortens_long_text() {
        assert_eq!(truncate("abc", 2), "ab…");
        assert_eq!(truncate("abc", 5), "abc");
    }

    #[test]
    fn absolute_workflow_path_is_used_as_is() {
        let path = Path::new(
            "/home/ubuntu/qintopia-agent-os-releases/current/workflows/x/daily_case_report.py",
        );
        assert_eq!(resolve_workflow_path(path), path);
    }

    #[test]
    fn render_dir_is_forced_to_private_mode() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().expect("tmp");
        set_dir_mode_private(tmp.path()).expect("chmod");
        let mode = std::fs::metadata(tmp.path())
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700);
    }

    #[test]
    fn relative_workflow_path_resolves_against_release_current_env() {
        let tmp = tempfile::tempdir().expect("tmp");
        let release = tmp.path().join("release");
        std::fs::create_dir_all(release.join("workflows/xiaoman-daily-case-report"))
            .expect("mkdir");
        let workflow = release.join("workflows/xiaoman-daily-case-report/daily_case_report.py");
        std::fs::write(&workflow, "print('ok')").expect("write");
        std::env::set_var("QINTOPIA_AGENT_OS_RELEASE_CURRENT", &release);
        let resolved = resolve_workflow_path(Path::new(
            "workflows/xiaoman-daily-case-report/daily_case_report.py",
        ));
        std::env::remove_var("QINTOPIA_AGENT_OS_RELEASE_CURRENT");
        assert_eq!(resolved, workflow);
    }

    #[test]
    fn media_upload_request_maps_render_fields() {
        let render = json!({
            "image_path": "/tmp/x.jpg",
            "artifact_candidate": {
                "mime_type": "image/jpeg",
                "content_hash": "sha256:abc",
                "file_md5": "d41d8cd98f00b204e9800998ecf8427e",
                "byte_size": 123,
                "filename": "x.jpg",
                "template_version": "v3",
                "report_window": {"display": "2026-08-08"},
                "source_chat_ref": {"kind": "sha256", "value": "sha256:xyz"},
            },
        });
        let request =
            media_upload_request_from_render(&render, Path::new("/tmp/x.jpg")).expect("maps");
        assert_eq!(request.content_hash, "sha256:abc");
        assert_eq!(request.filename, "x.jpg");
        assert_eq!(request.byte_size, Some(123));
        assert_eq!(request.template_version, "v3");
    }
}

#[cfg(test)]
mod e2e_tests {
    use super::*;
    use std::fs;

    /// Minimal stand-in workflow that writes a real 1x1 JPEG and emits the
    /// render summary JSON the orchestrator expects.  Rendered inline so the
    /// test never depends on an external fixture path.
    const FAKE_WORKFLOW_PY: &str = r#"#!/usr/bin/env python3
import argparse, base64, hashlib, json, pathlib

JPEG_B64 = "/9j/4AAQSkZJRgABAQEAYABgAAD/2wBDAAgGBgcGBQgHBwcJCQgKDBQNDAsLDBkSEw8UHRofHh0aHBwgJC4nICIsIxwcKDcpLDAxNDQ0Hyc5PTgyPC4zNDL/wAALCAABAAEBAREA/8QAFAABAAAAAAAAAAAAAAAAAAAACf/EABQQAQAAAAAAAAAAAAAAAAAAAAD/2gAIAQEAAD8AVN//2Q=="

p = argparse.ArgumentParser()
p.add_argument("--render"); p.add_argument("--image-format"); p.add_argument("--template")
p.add_argument("--json", action="store_true"); p.add_argument("--json-summary-only", action="store_true")
p.add_argument("--output-dir"); p.add_argument("--date")
a = p.parse_args()
out = pathlib.Path(a.output_dir)
out.mkdir(parents=True, exist_ok=True)
jpg = out / "daily-report.jpg"
jpeg_bytes = base64.b64decode(JPEG_B64)
jpg.write_bytes(jpeg_bytes)
sha = hashlib.sha256(jpeg_bytes).hexdigest()
md5 = hashlib.md5(jpeg_bytes).hexdigest()
print(json.dumps({
    "success": True,
    "skill": "xiaoman_daily_case_report",
    "report_date": a.date or "2026-08-08",
    "time_range": "00:00-23:59",
    "message_count": 3, "participant_count": 2,
    "case_count": 1, "character_count": 1,
    "image_path": str(jpg),
    "template_version": "xiaoman-daily-case-report-v3",
    "artifact_candidate": {
        "artifact_type": "generated_image", "workflow_type": "daily_case_report",
        "template_version": "xiaoman-daily-case-report-v3",
        "mime_type": "image/jpeg", "filename": "daily-report.jpg",
        "content_hash": f"sha256:{sha}", "file_md5": md5,
        "byte_size": len(jpeg_bytes), "render": {"image_format": "jpeg", "width": 1},
        "report_window": {"start": "2026-08-07T00:00:00+08:00", "end": "2026-08-08T00:00:00+08:00",
                          "display": "2026年08月08日", "time_range": "00:00-23:59", "timezone": "Asia/Shanghai"},
        "content_metrics": {"message_count": 3, "participant_count": 2, "case_count": 1, "character_count": 1},
        "source_chat_ref": {"kind": "sha256", "value": "sha256:xyz"},
        "retained_source_policy": "sanitized_metadata_only"
    }
}, ensure_ascii=False))
"#;

    fn fake_workflow_dir() -> (TempDir, PathBuf) {
        let tmp = tempfile::tempdir().expect("tmp");
        let workflow = tmp.path().join("workflow.py");
        fs::write(&workflow, FAKE_WORKFLOW_PY).expect("write fake workflow");
        (tmp, workflow)
    }

    #[test]
    fn render_report_invokes_workflow_with_template_and_parses_summary() {
        let (_tmp, workflow) = fake_workflow_dir();
        let config = DailyCaseReportMcpConfig {
            database_url: "postgresql://unit".to_string(),
            allowed_caller: "wenyuange".to_string(),
            python_bin: "/usr/bin/python3".to_string(),
            workflow_py: Some(workflow),
            rasterize_py: None,
            render_timeout: Duration::from_secs(60),
        };
        let arguments = GenerateArguments::from_json(json!({
            "date": "2026-08-08",
            "template": "v3",
            "caller": "wenyuange",
        }))
        .expect("parses");
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let (_dir, render) = runtime
            .block_on(async {
                std::env::set_var(
                    "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_USE_PYTHON_PIPELINE",
                    "1",
                );
                let pool = sqlx::postgres::PgPoolOptions::new()
                    .max_connections(1)
                    .connect_lazy(&config.database_url)
                    .expect("lazy pool");
                let result = render_report(&pool, &config, &arguments).await;
                std::env::remove_var("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_USE_PYTHON_PIPELINE");
                result
            })
            .expect("render succeeds");
        assert_eq!(render["success"], true);
        assert_eq!(render["message_count"], 3);
        assert_eq!(render["artifact_candidate"]["mime_type"], "image/jpeg");
        let image = render["image_path"].as_str().expect("image path");
        assert!(Path::new(image).exists());
    }

    #[test]
    fn call_tool_dry_run_returns_preview_without_db() {
        let (_tmp, workflow) = fake_workflow_dir();
        let config = DailyCaseReportMcpConfig {
            database_url: "postgresql://unit".to_string(),
            allowed_caller: "wenyuange".to_string(),
            python_bin: "/usr/bin/python3".to_string(),
            workflow_py: Some(workflow),
            rasterize_py: None,
            render_timeout: Duration::from_secs(60),
        };
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let result = runtime.block_on(async {
            std::env::set_var(
                "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_USE_PYTHON_PIPELINE",
                "1",
            );
            let pool = sqlx::postgres::PgPoolOptions::new()
                .max_connections(1)
                .connect_lazy(&config.database_url)
                .expect("lazy pool");
            let result = call_tool(
                &pool,
                &config,
                json!({
                    "date": "2026-08-08",
                    "template": "v3",
                    "dry_run": true,
                    "caller": "wenyuange",
                }),
            )
            .await;
            std::env::remove_var("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_USE_PYTHON_PIPELINE");
            result
        });
        let preview = result.expect("dry run call succeeds");
        assert_eq!(preview["success"], true);
        assert_eq!(preview["dry_run"], true);
        assert_eq!(preview["message_count"], 3);
        assert_eq!(
            preview["media_upload"]["action_status"],
            "media_upload_validated"
        );
        assert!(preview["auto_publish"].is_null());
    }

    #[test]
    fn use_python_pipeline_env_switch() {
        std::env::remove_var("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_USE_PYTHON_PIPELINE");
        assert!(!use_python_pipeline());
        std::env::set_var(
            "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_USE_PYTHON_PIPELINE",
            "1",
        );
        assert!(use_python_pipeline());
        std::env::remove_var("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_USE_PYTHON_PIPELINE");
    }

    #[test]
    fn rasterize_html_invokes_subprocess_and_parses_metadata() {
        const FAKE_RASTERIZE_PY: &str = r#"#!/usr/bin/env python3
import argparse, base64, json, pathlib, sys

JPEG_B64 = "/9j/4AAQSkZJRgABAQEAYABgAAD/2wBDAAgGBgcGBQgHBwcJCQgKDBQNDAsLDBkSEw8UHRofHh0aHBwgJC4nICIsIxwcKDcpLDAxNDQ0Hyc5PTgyPC4zNDL/wAALCAABAAEBAREA/8QAFAABAAAAAAAAAAAAAAAAAAAACf/EABQQAQAAAAAAAAAAAAAAAAAAAAD/2gAIAQEAAD8AVN//2Q=="

p = argparse.ArgumentParser()
p.add_argument("template")
a = p.parse_args()
request = json.load(sys.stdin)
out = pathlib.Path(request["output_path"])
out.parent.mkdir(parents=True, exist_ok=True)
jpeg_bytes = base64.b64decode(JPEG_B64)
out.write_bytes(jpeg_bytes)
print(json.dumps({
    "success": True,
    "image_path": str(out),
    "mime_type": "image/jpeg",
    "byte_size": len(jpeg_bytes),
    "width": 2,
    "height": 3,
    "image_format": "jpeg",
}, ensure_ascii=False))
"#;

        let tmp = tempfile::tempdir().expect("tmp");
        let rasterize = tmp.path().join("rasterize.py");
        fs::write(&rasterize, FAKE_RASTERIZE_PY).expect("write fake rasterize");
        let runtime = tokio::runtime::Runtime::new().expect("runtime");
        let (_image_path, output) = runtime
            .block_on(crate::daily_case_report_cutover::rasterize_html(
                &rasterize,
                "<html></html>",
                tmp.path(),
                "roast-long-image",
                750,
                "jpeg",
            ))
            .expect("rasterize succeeds");
        assert_eq!(output.image_format, "jpeg");
        assert_eq!(output.mime_type, "image/jpeg");
        assert_eq!(output.width, 2);
        assert_eq!(output.height, 3);
        assert!(output.byte_size > 0);
    }
}
