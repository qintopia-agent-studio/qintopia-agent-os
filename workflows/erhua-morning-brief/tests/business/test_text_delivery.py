"""Deterministic business-level coverage for the Erhua morning brief text path."""

from __future__ import annotations

import http.server
import json
import os
import sys
import hashlib
from pathlib import Path
import socket
import signal
import subprocess
import threading
import time
import uuid
from typing import Any

import allure
import pytest


ROOT = Path(__file__).resolve().parents[4]
WORKFLOW_DIR = ROOT / "workflows" / "erhua-morning-brief"
SCRIPT = WORKFLOW_DIR / "morning_brief.py"
FIXTURES = WORKFLOW_DIR / "tests" / "fixtures"
CRON_REGISTRY = ROOT / "runtime" / "hermes" / "cron" / "reviewed-cron-jobs.json"
BRIDGE_TEST = "qiwe_text_send::tests::postgres_erhua_morning_brief_text_bridge"
BRIDGE_ENABLE = "QINTOPIA_QIWE_TEXT_SEND_TEST_BRIDGE_ENABLE"


SCENARIOS = [
    {
        "id": "success",
        "title": "正常文本发布",
        "response_mode": "success",
        "expected_action_status": "text_send_executed",
        "expected_current_status": "completed",
        "confirm": True,
        "repeat": False,
    },
    {
        "id": "duplicate-send",
        "title": "重复执行发送 worker 不重复发送",
        "response_mode": "success",
        "expected_action_status": "text_send_executed",
        "expected_current_status": "completed",
        "confirm": True,
        "repeat": True,
    },
    {
        "id": "missing-confirmation",
        "title": "缺少最终确认时不发送",
        "response_mode": "success",
        "expected_action_status": "no_claimable_text_group_message_request",
        "expected_current_status": "none",
        "confirm": False,
        "repeat": False,
    },
    {
        "id": "ambiguous-send",
        "title": "请求后响应断开进入不确定终态",
        "response_mode": "disconnect_after_request",
        "expected_action_status": "text_send_outcome_ambiguous",
        "expected_current_status": "failed",
        "confirm": True,
        "repeat": True,
    },
]


class _FakeQiweServer(http.server.ThreadingHTTPServer):
    allow_reuse_address = True

    def __init__(self, response_mode: str):
        self.response_mode = response_mode
        self.records: list[dict[str, Any]] = []
        self.records_lock = threading.Lock()
        super().__init__(("127.0.0.1", 0), _FakeQiweHandler)


class _FakeQiweHandler(http.server.BaseHTTPRequestHandler):
    server: _FakeQiweServer

    def do_POST(self) -> None:  # noqa: N802 - inherited HTTPServer API
        try:
            content_length = int(self.headers.get("Content-Length", "0"))
            self.connection.settimeout(5)
            if not 0 < content_length <= 1_000_000:
                self.send_error(413)
                return
            body = self.rfile.read(content_length)
            try:
                payload = json.loads(body.decode("utf-8"))
            except (UnicodeDecodeError, json.JSONDecodeError):
                payload = None
            record = {
                "path": self.path,
                "payload": payload,
                "body_bytes": len(body),
            }
            with self.server.records_lock:
                self.server.records.append(record)

            if (self.path != "/qiwe/api/qw/doApi"
                    or not isinstance(payload, dict)
                    or payload.get("method") != "/msg/sendHyperText"
                    or self.headers.get("x-qiwei-token") != "test-only-token"):
                self.send_error(400, "Unexpected test request")
                return

            if self.server.response_mode == "disconnect_after_request":
                self.close_connection = True
                try:
                    self.connection.shutdown(socket.SHUT_RDWR)
                except OSError:
                    pass
                self.connection.close()
                return

            response = b'{"code":0,"data":{"msgUniqueIdentifier":"synthetic-message-id"}}'
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(response)))
            self.end_headers()
            self.wfile.write(response)
        except (BrokenPipeError, ConnectionResetError):
            self.close_connection = True

    def log_message(self, _format: str, *_args: Any) -> None:
        return


def _run_dir() -> Path:
    value = os.environ.get("QINTOPIA_TEST_RUN_DIR", "").strip()
    if value:
        path = Path(value).resolve()
        path.mkdir(parents=True, exist_ok=True)
        return path
    raise RuntimeError("Run this scenario through pnpm test:business")


def _sidecar_bin() -> str:
    value = os.environ.get("QINTOPIA_TEST_SIDECAR_BIN", "").strip()
    if not value:
        raise AssertionError("QINTOPIA_TEST_SIDECAR_BIN is required for the business case")
    return value


def _base_env(target_group_id: str) -> dict[str, str]:
    if os.environ.get("QINTOPIA_TEST_MODE") != "1":
        raise RuntimeError("Business scenarios require the local test runner")
    context = json.loads((_run_dir() / "context.json").read_text())
    from urllib.parse import urlparse
    db = urlparse(os.environ.get("QINTOPIA_SIDECAR_DATABASE_URL", ""))
    if db.hostname != "127.0.0.1" or db.path != "/qintopia_test" or db.port != context.get("database_port"):
        raise RuntimeError("Database must match this run's disposable instance")
    env = os.environ.copy()
    env.update(
        {
            "QINTOPIA_TEST_MODE": "1",
            "QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE": "1",
            "QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS": target_group_id,
            "QINTOPIA_OPERATIONS_ALLOWED_GROUP_ALIASES": "community_activity_group",
            "QINTOPIA_OPERATIONS_ALLOWED_REVIEWER_IDS": "business-test-reviewer",
            "QINTOPIA_OPERATIONS_ALLOWED_CONFIRMER_IDS": "business-test-confirmer",
        }
    )
    return env


def _scrub(value: str) -> str:
    database_url = os.environ.get("QINTOPIA_SIDECAR_DATABASE_URL", "")
    return value.replace(database_url, "<local-test-database>") if database_url else value


def _record_step(case_dir: Path, steps: list[dict[str, Any]], name: str, data: Any) -> None:
    step = {"name": name, "data": data}
    steps.append(step)
    (case_dir / "steps.json").write_text(
        json.dumps(steps, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    allure.attach(
        json.dumps(step, ensure_ascii=False, indent=2),
        name=name,
        attachment_type=allure.attachment_type.JSON,
    )


def _run_command(
    command: list[str],
    *,
    env: dict[str, str],
    case_dir: Path,
    steps: list[dict[str, Any]],
    name: str,
    timeout: int = 180,
) -> subprocess.CompletedProcess[str]:
    process = subprocess.Popen(
        command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
        env=env, cwd=str(_run_dir()), start_new_session=True,
    )
    previous_term = signal.getsignal(signal.SIGTERM)
    def interrupt(_signum, _frame):
        raise KeyboardInterrupt("test process interrupted")
    signal.signal(signal.SIGTERM, interrupt)
    try:
        stdout, stderr = process.communicate(timeout=timeout)
        completed = subprocess.CompletedProcess(command, process.returncode, stdout, stderr)
    except subprocess.TimeoutExpired as exc:
        _record_step(case_dir, steps, name, {
            "command": command, "timeout_seconds": timeout, "timed_out": True,
        })
        raise RuntimeError(f"{name} timed out after {timeout}s") from exc
    finally:
        # Kill the whole command group, including Cargo's test executable, even
        # when its direct parent already exited or the outer harness interrupts us.
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.communicate(timeout=5)
        signal.signal(signal.SIGTERM, previous_term)
    _record_step(
        case_dir,
        steps,
        name,
        {
            "command": command,
            "returncode": completed.returncode,
            "stdout": _scrub(completed.stdout),
            "stderr": _scrub(completed.stderr),
        },
    )
    return completed


def _json_stdout(completed: subprocess.CompletedProcess[str], name: str) -> dict[str, Any]:
    if completed.returncode != 0:
        raise AssertionError(
            f"{name} failed with {completed.returncode}: {_scrub(completed.stderr)}"
        )
    try:
        parsed = json.loads(completed.stdout)
    except json.JSONDecodeError as exc:
        raise AssertionError(f"{name} returned invalid JSON: {completed.stdout}") from exc
    if not isinstance(parsed, dict):
        raise AssertionError(f"{name} returned a non-object JSON value")
    return parsed


def _nested_report(report: dict[str, Any], key: str, name: str) -> dict[str, Any]:
    action = report.get(key)
    if not isinstance(action, dict):
        raise AssertionError(f"{name} did not include {key}")
    stdout = action.get("stdout")
    if not isinstance(stdout, str):
        raise AssertionError(f"{name} did not include {key}.stdout")
    try:
        value = json.loads(stdout)
    except json.JSONDecodeError as exc:
        raise AssertionError(f"{name} returned invalid {key} JSON: {stdout}") from exc
    if not isinstance(value, dict):
        raise AssertionError(f"{name} returned a non-object {key} report")
    return value


def _brief_command(sidecar: str) -> list[str]:
    return [
        sys.executable,
        str(SCRIPT),
        "--date",
        "2026-08-08",
        "--activity-fixture",
        str(FIXTURES / "activity-one.json"),
        "--news-fixture",
        str(FIXTURES / "qunmind-ai-report.md"),
        "--weather-fixture",
        str(FIXTURES / "weather.json"),
        "--sidecar-bin",
        sidecar,
        "--json",
    ]


def _create_artifact(
    *,
    sidecar: str,
    source_ref: str,
    target_group_id: str,
    env: dict[str, str],
    case_dir: Path,
    steps: list[dict[str, Any]],
    brief_text: str,
) -> str:
    command = _brief_command(sidecar) + [
        "--prepare-artifact",
        "--execute-artifact-create",
        "--apply-artifact-create",
        "--source-record-ref",
        source_ref,
    ]
    report = _json_stdout(
        _run_command(
            command,
            env=env,
            case_dir=case_dir,
            steps=steps,
            name="create_text_announcement_artifact",
        ),
        "create_text_announcement_artifact",
    )
    if report.get("morning_brief_text") != brief_text:
        raise AssertionError("artifact request did not use the generated morning brief text")
    artifact_report = _nested_report(report, "artifact_create", "artifact create")
    artifact_id = artifact_report.get("artifact_id")
    if not isinstance(artifact_id, str) or not artifact_id:
        raise AssertionError("artifact create did not return artifact_id")
    if artifact_report.get("review_status") != "pending":
        raise AssertionError("new text artifact must start pending")
    return artifact_id


def _approve_artifact(
    *,
    sidecar: str,
    artifact_id: str,
    env: dict[str, str],
    case_dir: Path,
    steps: list[dict[str, Any]],
) -> None:
    payload = {
        "artifact_id": artifact_id,
        "reviewer_id": "business-test-reviewer",
        "decision": "approved",
        "expected_artifact_type": "text_announcement",
        "expected_review_status": "pending",
        "reason": "业务案例审核二花早报文本",
        "source": "erhua_morning_brief_business_test",
    }
    completed = _run_command(
        [
            sidecar,
            "operations-artifact-review-decision",
            "--payload-json",
            json.dumps(payload, ensure_ascii=False, separators=(",", ":")),
            "--apply",
        ],
        env=env,
        case_dir=case_dir,
        steps=steps,
        name="approve_text_artifact",
    )
    report = _json_stdout(completed, "approve_text_artifact")
    if report.get("review_status") != "approved":
        raise AssertionError("text artifact was not approved")


def _create_send_request(
    *,
    sidecar: str,
    artifact_id: str,
    target_group_id: str,
    brief_text: str,
    env: dict[str, str],
    case_dir: Path,
    steps: list[dict[str, Any]],
) -> str:
    command = _brief_command(sidecar) + [
        "--prepare-send-request",
        "--approved-artifact-id",
        artifact_id,
        "--target-group-id",
        target_group_id,
        "--target-group-alias",
        "community_activity_group",
        "--execute-send-request",
        "--apply-send-request",
    ]
    report = _json_stdout(
        _run_command(
            command,
            env=env,
            case_dir=case_dir,
            steps=steps,
            name="create_group_message_request",
        ),
        "create_group_message_request",
    )
    if report.get("morning_brief_text") != brief_text:
        raise AssertionError("send request did not use the generated morning brief text")
    send_report = _nested_report(report, "send_request", "send request")
    work_item_id = send_report.get("work_item_id")
    if not isinstance(work_item_id, str) or not work_item_id:
        raise AssertionError("send request did not return work_item_id")
    return work_item_id


def _confirm_request(
    *,
    sidecar: str,
    work_item_id: str,
    env: dict[str, str],
    case_dir: Path,
    steps: list[dict[str, Any]],
) -> None:
    payload = {
        "work_item_id": work_item_id,
        "confirmer_id": "business-test-confirmer",
        "decision": "confirmed",
        "reason": "业务案例确认发送二花早报",
        "source": "erhua_morning_brief_business_test",
    }
    completed = _run_command(
        [
            sidecar,
            "operations-group-message-confirm",
            "--payload-json",
            json.dumps(payload, ensure_ascii=False, separators=(",", ":")),
            "--apply",
        ],
        env=env,
        case_dir=case_dir,
        steps=steps,
        name="record_final_confirmation",
    )
    report = _json_stdout(completed, "record_final_confirmation")
    if report.get("current_status") != "queued" or report.get("send_executed") is not False:
        raise AssertionError("final confirmation did not queue the request")


def _record_send_ready(
    *,
    sidecar: str,
    work_item_id: str,
    env: dict[str, str],
    case_dir: Path,
    steps: list[dict[str, Any]],
) -> dict[str, Any]:
    completed = _run_command(
        [
            sidecar,
            "run-group-message-send-worker",
            "--once",
            "--work-item-id",
            work_item_id,
            "--apply",
        ],
        env=env,
        case_dir=case_dir,
        steps=steps,
        name="record_group_message_send_ready",
    )
    return _json_stdout(completed, "record_group_message_send_ready")


def _run_text_bridge(
    *,
    case_dir: Path,
    fake_server: _FakeQiweServer,
    work_item_id: str,
    target_group_id: str,
    expected_action_status: str,
    expected_current_status: str,
    env: dict[str, str],
    steps: list[dict[str, Any]],
    invocation: str,
) -> dict[str, Any]:
    input_path = case_dir / f"bridge-input-{invocation}.json"
    output_path = case_dir / f"bridge-output-{invocation}.json"
    input_data = {
        "work_item_id": work_item_id,
        "api_url": f"http://127.0.0.1:{fake_server.server_address[1]}/qiwe/api/qw/doApi",
        "target_group_id": target_group_id,
        "expected_action_status": expected_action_status,
        "expected_current_status": expected_current_status,
        "output_path": str(output_path),
    }
    input_path.write_text(json.dumps(input_data, indent=2), encoding="utf-8")
    bridge_env = env.copy()
    bridge_env.update(
        {
            "QINTOPIA_TEST_RUN_DIR": str(_run_dir()),
            "QINTOPIA_TEST_BRIDGE_INPUT": str(input_path),
            BRIDGE_ENABLE: "1",
            "RUST_MIN_STACK": "33554432",
        }
    )
    completed = _run_command(
        [
            "cargo",
            "test",
            "--quiet",
            "--locked",
            "--manifest-path",
            str(ROOT / "runtime" / "sidecar" / "Cargo.toml"),
            "--features",
            "postgres-integration-tests",
            BRIDGE_TEST,
            "--",
            "--ignored",
            "--exact",
            "--nocapture",
        ],
        env=bridge_env,
        case_dir=case_dir,
        steps=steps,
        name=f"qiwe_text_send_bridge_{invocation}",
        timeout=900,
    )
    if completed.returncode != 0:
        raise AssertionError(f"QiWe text bridge failed: {_scrub(completed.stderr)}")
    if "1 passed; 0 failed" not in completed.stdout:
        raise RuntimeError("Rust bridge did not execute exactly one passing test")
    if not output_path.exists():
        raise AssertionError("QiWe text bridge did not write output evidence")
    evidence = json.loads(output_path.read_text(encoding="utf-8"))
    if evidence.get("action_status") != expected_action_status:
        raise AssertionError(f"unexpected bridge action status: {evidence}")
    if evidence.get("current_status") != expected_current_status:
        raise AssertionError(f"unexpected bridge current status: {evidence}")
    return evidence


def _wait_for_request_count(server: _FakeQiweServer, count: int, timeout: float = 5.0) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        with server.records_lock:
            if len(server.records) >= count:
                return
        time.sleep(0.02)
    with server.records_lock:
        actual = len(server.records)
    raise AssertionError(f"fake QiWe server expected at least {count} requests, got {actual}")


def _assert_fake_request(
    server: _FakeQiweServer, target_group_id: str, brief_text: str, expected_count: int
) -> None:
    with server.records_lock:
        records = list(server.records)
    assert len(records) == expected_count, records
    if expected_count == 0:
        return
    for record in records:
        assert record["path"] == "/qiwe/api/qw/doApi"
        payload = record["payload"]
        assert isinstance(payload, dict)
        assert payload["method"] == "/msg/sendHyperText"
        params = payload["params"]
        assert params["toId"] == target_group_id
        assert params["content"] == [{"type": "text", "text": brief_text}]


@allure.epic("Agent OS 本地业务测试")
@allure.feature("二花早报")
@allure.story("文本发布长链路")
@pytest.mark.parametrize("scenario", SCENARIOS, ids=[item["id"] for item in SCENARIOS])
def test_text_delivery(scenario: dict[str, Any]) -> None:
    """Exercise generated content through the reviewed text-send boundary."""
    allure.dynamic.title(scenario["title"])
    allure.dynamic.parameter("scenario", scenario["id"])
    allure.dynamic.description("真实早报文本、审核、确认、PostgreSQL与发送逻辑；模拟活动/新闻/天气与QiWe响应。未验证卡片、真实定时调度或真实送达。")
    run_dir = _run_dir()
    case_dir = run_dir / "erhua-morning-brief" / scenario["id"]
    case_dir.mkdir(parents=True, exist_ok=True)
    steps: list[dict[str, Any]] = []
    target_group_id = f"business-test-{scenario['id']}-{uuid.uuid4().hex[:12]}"
    env = _base_env(target_group_id)
    sidecar = _sidecar_bin()

    with allure.step("生成二花早报文本"):
        brief = _json_stdout(
            _run_command(
                _brief_command(sidecar),
                env=env,
                case_dir=case_dir,
                steps=steps,
                name="generate_morning_brief",
            ),
            "generate_morning_brief",
        )
        assert brief.get("success") is True
        assert brief.get("activity_publishable_count") == 1
        brief_text = brief.get("morning_brief_text")
        assert isinstance(brief_text, str) and brief_text.strip()
        assert brief.get("external_send_executed") is False

    source_ref = f"erhua_morning_brief:business:{scenario['id']}:{uuid.uuid4().hex[:12]}"
    with allure.step("创建并审核文本 artifact"):
        artifact_id = _create_artifact(
            sidecar=sidecar,
            source_ref=source_ref,
            target_group_id=target_group_id,
            env=env,
            case_dir=case_dir,
            steps=steps,
            brief_text=brief_text,
        )
        _approve_artifact(
            sidecar=sidecar,
            artifact_id=artifact_id,
            env=env,
            case_dir=case_dir,
            steps=steps,
        )

    with allure.step("创建群消息 work item"):
        work_item_id = _create_send_request(
            sidecar=sidecar,
            artifact_id=artifact_id,
            target_group_id=target_group_id,
            brief_text=brief_text,
            env=env,
            case_dir=case_dir,
            steps=steps,
        )

    with _FakeQiweServer(scenario["response_mode"]) as fake_server:
        server_thread = threading.Thread(target=fake_server.serve_forever, daemon=True)
        server_thread.start()
        try:
            if scenario["confirm"]:
                with allure.step("记录最终确认"):
                    _confirm_request(
                        sidecar=sidecar,
                        work_item_id=work_item_id,
                        env=env,
                        case_dir=case_dir,
                        steps=steps,
                    )

                with allure.step("记录 send-ready"):
                    ready_report = _record_send_ready(
                        sidecar=sidecar,
                        work_item_id=work_item_id,
                        env=env,
                        case_dir=case_dir,
                        steps=steps,
                    )
                    assert ready_report.get("action_status") == "send_ready_recorded"
                    assert ready_report.get("send_executed") is False
            else:
                with allure.step("确认缺失时保持 awaiting_publish"):
                    ready_report = _record_send_ready(
                        sidecar=sidecar,
                        work_item_id=work_item_id,
                        env=env,
                        case_dir=case_dir,
                        steps=steps,
                    )
                    assert ready_report.get("action_status") == "no_claimable_group_message_request"

            first = _run_text_bridge(
                case_dir=case_dir,
                fake_server=fake_server,
                work_item_id=work_item_id,
                target_group_id=target_group_id,
                expected_action_status=scenario["expected_action_status"],
                expected_current_status=scenario["expected_current_status"],
                env=env,
                steps=steps,
                invocation="first",
            )
            expected_external = (None if scenario["response_mode"] == "disconnect_after_request"
                                 else bool(scenario["confirm"]))
            assert first["external_send_executed"] is expected_external
            outcomes = first["send_events"]
            assert len(outcomes) == (1 if scenario["confirm"] else 0)
            if outcomes:
                assert outcomes[0]["external_send_executed"] is expected_external
                assert outcomes[0]["external_send_outcome"] == ("unknown" if expected_external is None else "sent")
                if expected_external is None:
                    assert outcomes[0]["automatic_retry_allowed"] is False
            assert first["artifact_bound"] is scenario["confirm"]
            assert first["approved_artifact_content_hash"] == "sha256:" + hashlib.sha256(brief_text.encode()).hexdigest()
            assert first["persisted_status"] == (scenario["expected_current_status"] if scenario["confirm"] else "awaiting_publish")
            assert first["events"].count("qiwe_text_send_executed") == (1 if scenario["response_mode"] == "success" and scenario["confirm"] else 0)

            if scenario["repeat"]:
                second_status = "no_claimable_text_group_message_request"
                second = _run_text_bridge(
                    case_dir=case_dir,
                    fake_server=fake_server,
                    work_item_id=work_item_id,
                    target_group_id=target_group_id,
                    expected_action_status=second_status,
                    expected_current_status="none",
                    env=env,
                    steps=steps,
                    invocation="second",
                )
                assert second["external_send_executed"] is False
                assert second["send_events"] == first["send_events"]
                assert second["artifact_bound"] is False
                assert second["persisted_status"] == first["persisted_status"]
                assert second["events"] == first["events"]

            expected_requests = 1 if scenario["confirm"] else 0
            if expected_requests:
                _wait_for_request_count(fake_server, expected_requests)
            _assert_fake_request(fake_server, target_group_id, brief_text, expected_requests)
            _record_step(
                case_dir,
                steps,
                "assert_fake_qiwe_request",
                {
                    "expected_request_count": expected_requests,
                    "actual_request_count": len(fake_server.records),
                    "target_group_id": target_group_id,
                    "message_text": brief_text,
                },
            )
        finally:
            fake_server.shutdown()
            server_thread.join(timeout=5)
            fake_server.server_close()

    _record_step(
        case_dir,
        steps,
        "case_result",
        {
            "scenario": scenario["id"],
            "worker_action_status": first["action_status"],
            "worker_current_status": first["current_status"],
            "external_send_executed": first["external_send_executed"],
            "real_path": [
                "morning_brief.py",
                "operations-text-announcement-artifact-create",
                "operations-artifact-review-decision",
                "operations-work-item-create",
                "operations-group-message-confirm" if scenario["confirm"] else "confirmation omitted",
                "run-group-message-send-worker",
                "qiwe_text_send bridge",
            ],
            "simulated_boundary": "loopback QiWe HTTP response",
        },
    )


@allure.epic("Agent OS 本地业务测试")
@allure.feature("二花早报")
@allure.story("调度配置契约")
@allure.label("test_type", "configuration")
def test_morning_brief_cron_declaration() -> None:
    """Check the reviewed declaration only; this does not run a scheduler."""
    registry = json.loads(CRON_REGISTRY.read_text(encoding="utf-8"))
    jobs = registry.get("reviewed_jobs", [])
    matches = [
        job
        for job in jobs
        if isinstance(job, dict)
        and job.get("profile") == "erhua"
        and job.get("name") == "二花·每日早报"
    ]
    assert len(matches) == 1
    job = matches[0]
    wrapper = ROOT / "runtime/hermes/scripts" / job["script"]
    assert wrapper.is_file()
    # Repository sources are 0644; the reviewed installer applies executable mode.
    syntax = subprocess.run(["bash", "-n", str(wrapper)], capture_output=True, text=True, timeout=10)
    assert syntax.returncode == 0, syntax.stderr
    declaration = {
        "config_only": True,
        "profile": job.get("profile"),
        "name": job.get("name"),
        "schedule_expr": job.get("schedule_expr"),
        "script": job.get("script"),
        "no_agent": job.get("no_agent"),
        "origin_platform": job.get("origin_platform"),
    }
    assert declaration == {
        "config_only": True,
        "profile": "erhua",
        "name": "二花·每日早报",
        "schedule_expr": "10 8 * * *",
        "script": "qintopia_erhua_morning_brief.sh",
        "no_agent": True,
        "origin_platform": "wecom",
    }
    case_dir = _run_dir() / "erhua-morning-brief" / "cron-contract"
    case_dir.mkdir(parents=True, exist_ok=True)
    _record_step(case_dir, [], "review_cron_registry", declaration)

