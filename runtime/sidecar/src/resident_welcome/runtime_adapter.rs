//! Fixed local process boundary for independently registered welcome Agent tools.
//! Only the controlled host constructs task context; no database or channel secrets cross it.
use super::digest;
use anyhow::{ensure, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use uuid::Uuid;

const OUTPUT_LIMIT: usize = 15 * 1024 * 1024;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Response {
    schema_version: u32,
    call_id: Uuid,
    task_ref: Uuid,
    agent: String,
    operation: String,
    runtime: String,
    request_sha256: String,
    output_sha256: String,
    plugin_source_sha256: String,
    plugin_id: String,
    plugin_version: String,
    tool_name: String,
    output: Value,
}

pub(super) struct Invocation {
    pub output: Value,
    pub evidence: Value,
}

pub(super) async fn invoke(
    work: Uuid,
    agent: &str,
    operation: &str,
    input: Value,
) -> Result<Invocation> {
    ensure!(
        matches!(
            (agent, operation),
            ("anan", "request_card" | "prepare")
                | ("huabaosi", "render_card")
                | ("erhua", "forward_review" | "forward")
        ),
        "agent_runtime_operation_denied"
    );
    let request = json!({"schema_version":1,"call_id":Uuid::new_v4(),"agent":agent,"operation":operation,
        "trusted_context":{"task_ref":work,"target_agent":agent,"capability_key":"resident_welcome.coordinate","source_type":"resident_welcome","input":input},"arguments":{}});
    tokio::task::spawn_blocking(move || run(request)).await?
}

fn run(request: Value) -> Result<Invocation> {
    let python = std::env::var("QINTOPIA_WELCOME_RENDER_PYTHON")
        .map_err(|_| anyhow::anyhow!("explicit_renderer_python_required"))?;
    ensure!(
        std::path::Path::new(&python).is_absolute(),
        "absolute_renderer_python_required"
    );
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let runner = root.join("workflows/resident-welcome/scripts/local_agent_runtime.py");
    let agent = request["agent"].as_str().unwrap();
    let plugin = root.join(format!("agents/{agent}/welcome_runtime.py"));
    let plugin_hash =
        digest(&std::fs::read(plugin).map_err(|_| anyhow::anyhow!("agent_runtime_unavailable"))?);
    let raw = serde_json::to_vec(&request)?;
    ensure!(raw.len() <= 64 * 1024, "agent_runtime_input_too_large");
    let request_hash = digest(&raw);
    let mut child = Command::new(python)
        .arg(runner)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| anyhow::anyhow!("agent_runtime_unavailable"))?;
    let process_id = child.id();
    let write = child
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("agent_runtime_unavailable"))?
        .write_all(&raw);
    if write.is_err() {
        let _ = child.kill();
        let _ = child.wait();
        anyhow::bail!("agent_runtime_unavailable");
    }
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("agent_runtime_unavailable"))?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .take((OUTPUT_LIMIT + 1) as u64)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() > Duration::from_secs(20) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            anyhow::bail!("agent_runtime_timeout");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let bytes = reader
        .join()
        .map_err(|_| anyhow::anyhow!("agent_runtime_failed"))??;
    ensure!(
        status.success() && !bytes.is_empty() && bytes.len() <= OUTPUT_LIMIT,
        "agent_runtime_failed"
    );
    let mut limits = crate::strict_json::registry_json_limits(OUTPUT_LIMIT);
    limits.max_string_bytes = OUTPUT_LIMIT;
    let result: Response = serde_json::from_value(crate::strict_json::parse_strict_bounded_slice(
        &bytes, limits,
    )?)?;
    ensure!(
        result.schema_version == 1
            && json!(result.call_id) == request["call_id"]
            && json!(result.task_ref) == request["trusted_context"]["task_ref"]
            && result.agent == agent
            && result.operation == request["operation"]
            && result.runtime == "local_scripted_agent_runtime"
            && result.request_sha256 == request_hash
            && result.output_sha256 == digest(&serde_json::to_vec(&result.output)?)
            && result.plugin_source_sha256 == plugin_hash
            && result.plugin_id == format!("qintopia-welcome-{agent}")
            && result.plugin_version == "0.1.0"
            && result.tool_name == format!("qintopia_welcome_{}", result.operation),
        "agent_runtime_result_mismatch"
    );
    let mut output_binding = result.output.clone();
    if let Some(fields) = output_binding.as_object_mut() {
        fields.remove("content_base64");
    }
    Ok(Invocation {
        output: result.output,
        evidence: json!({"call_id":result.call_id,"work_item_id":result.task_ref,"output_binding":output_binding,
        "agent":result.agent,"operation":result.operation,"runtime":result.runtime,"process_id":process_id,
        "request_sha256":request_hash,"output_sha256":result.output_sha256,"plugin_source_sha256":plugin_hash,
        "plugin_id":result.plugin_id,"plugin_version":result.plugin_version,"tool_name":result.tool_name}),
    })
}
