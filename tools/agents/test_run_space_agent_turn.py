#!/usr/bin/env python3

from __future__ import annotations

import ast
import importlib.util
import json
import os
import socket
import stat
import sys
import tempfile
import threading
import time
import unittest
import uuid
from pathlib import Path
from types import SimpleNamespace
from typing import Any, Callable

RUNNER_PATH = Path(__file__).with_name("run-space-agent-turn.py")
SPEC = importlib.util.spec_from_file_location("qintopia_space_agent_turn_runner", RUNNER_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError("unable to load Space agent-turn runner")
runner = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = runner
SPEC.loader.exec_module(runner)

WORK_ITEM_ID = "123e4567-e89b-42d3-a456-426614174000"
CLAIM_TOKEN = "c" * 64
RUNNER_TOKEN = "r" * 64
CAPABILITY_ONE = "erhua.space.subject_identity_lookup"
CAPABILITY_TWO = "erhua.space.context_summary"


class DripResponse:
    def __init__(self, payload: bytes, delay_seconds: float) -> None:
        self.payload = payload
        self.delay_seconds = delay_seconds


class FakeUnixJsonServer:
    def __init__(
        self,
        path: str,
        handler: Callable[[dict[str, Any]], dict[str, Any] | bytes | DripResponse],
    ) -> None:
        self.path = path
        self.handler = handler
        self.requests: list[dict[str, Any]] = []
        self.errors: list[BaseException] = []
        self._stop = threading.Event()
        self._listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        try:
            self._listener.bind(path)
        except BaseException:
            self._listener.close()
            raise
        self._listener.listen(8)
        self._listener.settimeout(0.05)
        self._thread = threading.Thread(target=self._serve, daemon=True)
        self._thread.start()

    def _serve(self) -> None:
        while not self._stop.is_set():
            try:
                connection, _ = self._listener.accept()
            except TimeoutError:
                continue
            except OSError:
                if not self._stop.is_set():
                    self.errors.append(RuntimeError("fake socket accept failed"))
                return
            try:
                connection.settimeout(2)
                request = bytearray()
                while b"\n" not in request:
                    chunk = connection.recv(16 * 1024)
                    if not chunk:
                        raise RuntimeError("fake socket request was not newline framed")
                    request.extend(chunk)
                    if len(request) > runner.MAX_MESSAGE_BYTES:
                        raise RuntimeError("fake socket request exceeded the limit")
                newline = request.index(b"\n")
                if newline != len(request) - 1:
                    raise RuntimeError("fake socket request contained trailing data")
                decoded = json.loads(bytes(request[:newline]).decode("utf-8"))
                if not isinstance(decoded, dict):
                    raise RuntimeError("fake socket request root was not an object")
                self.requests.append(decoded)
                response = self.handler(decoded)
                encoded = (
                    response.payload
                    if isinstance(response, DripResponse)
                    else response
                    if isinstance(response, bytes)
                    else json.dumps(
                        response,
                        ensure_ascii=False,
                        separators=(",", ":"),
                        allow_nan=False,
                    ).encode("utf-8")
                    + b"\n"
                )
                try:
                    if isinstance(response, DripResponse):
                        for byte in encoded:
                            connection.sendall(bytes([byte]))
                            time.sleep(response.delay_seconds)
                    else:
                        connection.sendall(encoded)
                except (BrokenPipeError, ConnectionResetError):
                    pass
            except BaseException as error:
                self.errors.append(error)
            finally:
                connection.close()

    def close(self) -> None:
        self._stop.set()
        self._listener.close()
        self._thread.join(timeout=2)
        try:
            os.unlink(self.path)
        except FileNotFoundError:
            pass

    def __enter__(self) -> "FakeUnixJsonServer":
        return self

    def __exit__(self, error_type, _error, _traceback) -> None:
        self.close()
        if error_type is None and self.errors:
            raise AssertionError(f"fake socket errors: {self.errors!r}")


def capability(key: str) -> dict[str, Any]:
    return {
        "capability_key": key,
        "input_schema": {
            "type": "object",
            "properties": {"subject": {"type": "string"}},
            "additionalProperties": False,
        },
        "output_schema": {
            "type": "object",
            "properties": {"resolved": {"type": "boolean"}},
            "additionalProperties": False,
        },
        "risk_level": "low",
        "review_policy": "not_required",
    }


def claim_response(capabilities: list[dict[str, Any]] | None = None) -> dict[str, Any]:
    return {
        "schema_version": 1,
        "claimed": True,
        "runner_identity": runner.RUNNER_IDENTITY,
        "work_item_id": WORK_ITEM_ID,
        "claim_token": CLAIM_TOKEN,
        "claim_expires_at": "2099-01-01T00:00:00Z",
        "goal": "Resolve the trigger subjects and return a bounded result.",
        "trigger": {"event_type": "group_member_add", "subject_user_ids": ["u-1"]},
        "output_contract": {
            "type": "object",
            "properties": {"message": {"type": "string"}},
            "required": ["message"],
            "additionalProperties": False,
        },
        "output_contract_sha256": "a" * 64,
        "capabilities": capabilities
        if capabilities is not None
        else [capability(CAPABILITY_ONE)],
    }


def completion_response(decision: dict[str, Any]) -> dict[str, Any]:
    return {"schema_version": 1, "accepted": True, "decision": decision}


def invoke_response(request: dict[str, Any], output: dict[str, Any]) -> dict[str, Any]:
    return {
        "schema_version": 1,
        "accepted": True,
        "status": "completed",
        "call_id": request["call_id"],
        "capability_key": request["capability_key"],
        "output": output,
        "output_sha256": "b" * 64,
        "replayed": False,
    }


def finish_response(status: str = "completed") -> dict[str, Any]:
    return {
        "schema_version": 1,
        "accepted": True,
        "status": status,
        "external_send_executed": None,
        "automatic_retry_allowed": False,
    }


class SpaceAgentTurnRunnerTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(prefix="qst-", dir="/tmp")
        self.broker_path = str(Path(self.temporary.name) / "broker.sock")
        self.completion_path = str(Path(self.temporary.name) / "completion.sock")
        self.environment = {
            runner.ENABLE_ENV: "1",
            runner.BROKER_SOCKET_ENV: self.broker_path,
            runner.COMPLETION_SOCKET_ENV: self.completion_path,
            runner.TOKEN_ENV: RUNNER_TOKEN,
            runner.TIMEOUT_ENV: "2",
        }

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def assert_failure(self, code: str, operation: Callable[[], Any]) -> None:
        with self.assertRaises(runner.RunnerFailure) as caught:
            operation()
        self.assertEqual(caught.exception.code, code)

    def run_runner(self) -> dict[str, Any]:
        return runner.run_once(
            self.environment,
            endpoint_validator=lambda _path: None,
        )

    def test_default_disabled_only_once_and_malformed_environment(self) -> None:
        self.assert_failure("configuration_error", lambda: runner.run_once({}))
        self.assertEqual(runner.main([]), 2)
        invalid_environments = [
            {**self.environment, runner.ENABLE_ENV: "yes"},
            {**self.environment, runner.BROKER_SOCKET_ENV: "relative.sock"},
            {
                **self.environment,
                runner.COMPLETION_SOCKET_ENV: self.broker_path,
            },
            {**self.environment, runner.TOKEN_ENV: "short"},
            {**self.environment, runner.TOKEN_ENV: "x" * 31 + " "},
            {**self.environment, runner.TOKEN_ENV: "\u5bc6" * 32},
            {**self.environment, runner.TOKEN_ENV: "\udcff" * 32},
            {**self.environment, runner.BROKER_SOCKET_ENV: "/tmp/\udcff.sock"},
            {**self.environment, runner.TIMEOUT_ENV: "0"},
            {**self.environment, runner.TIMEOUT_ENV: "61"},
            {**self.environment, runner.TIMEOUT_ENV: "1.5"},
        ]
        for environment in invalid_environments:
            with self.subTest(environment=environment):
                self.assert_failure(
                    "configuration_error", lambda: runner.run_once(environment)
                )

        without_timeout = dict(self.environment)
        without_timeout.pop(runner.TIMEOUT_ENV)
        config = runner.RunnerConfig.from_environment(without_timeout)
        self.assertEqual(config.timeout_seconds, 45)

    def test_idle_claim_does_not_contact_completion(self) -> None:
        def broker_handler(request: dict[str, Any]) -> dict[str, Any]:
            self.assertEqual(request["operation"], "space_agent_turn_claim")
            self.assertEqual(request["runner_token"], RUNNER_TOKEN)
            return {
                "schema_version": 1,
                "claimed": False,
                "runner_identity": runner.RUNNER_IDENTITY,
            }

        with FakeUnixJsonServer(self.broker_path, broker_handler) as broker:
            result = self.run_runner()
        self.assertEqual(result, {"schema_version": 1, "status": "idle"})
        self.assertEqual(len(broker.requests), 1)
        self.assertFalse(Path(self.completion_path).exists())

    def test_direct_final_finishes_without_capability_usage(self) -> None:
        final_output = {"message": "Welcome"}
        finishes: list[dict[str, Any]] = []

        def broker_handler(request: dict[str, Any]) -> dict[str, Any]:
            if request["operation"] == "space_agent_turn_claim":
                return claim_response()
            self.assertEqual(request["operation"], "space_agent_turn_finish")
            finishes.append(request)
            return finish_response()

        def completion_handler(request: dict[str, Any]) -> dict[str, Any]:
            self.assertEqual(request["history"], [])
            self.assertNotIn("claim_token", request)
            return completion_response({"kind": "final", "output": final_output})

        with FakeUnixJsonServer(self.broker_path, broker_handler), FakeUnixJsonServer(
            self.completion_path, completion_handler
        ):
            result = self.run_runner()
        self.assertEqual(result["status"], "completed")
        self.assertEqual(result["capability_call_count"], 0)
        self.assertEqual(
            finishes[0]["result"],
            {"outcome": "succeeded", "output": final_output, "capability_usage": []},
        )

    def test_multi_round_invocation_records_exact_history_and_receipt_usage(self) -> None:
        call_ids = [str(uuid.UUID(int=index)) for index in (1, 2, 3)]
        decisions = [
            {
                "kind": "capability_call",
                "call_id": call_ids[0],
                "capability_key": CAPABILITY_ONE,
                "input": {"subject": "u-1"},
            },
            {
                "kind": "capability_call",
                "call_id": call_ids[1],
                "capability_key": CAPABILITY_ONE,
                "input": {"subject": "u-2"},
            },
            {
                "kind": "capability_call",
                "call_id": call_ids[2],
                "capability_key": CAPABILITY_TWO,
                "input": {"subject": "event"},
            },
            {"kind": "final", "output": {"message": "Resolved"}},
        ]
        outputs = {
            call_ids[0]: {"resolved": True, "name": "A"},
            call_ids[1]: {"resolved": False},
            call_ids[2]: {"resolved": True, "summary": "bounded"},
        }
        finishes: list[dict[str, Any]] = []
        completion_requests: list[dict[str, Any]] = []

        def broker_handler(request: dict[str, Any]) -> dict[str, Any]:
            operation = request["operation"]
            if operation == "space_agent_turn_claim":
                return claim_response(
                    [capability(CAPABILITY_ONE), capability(CAPABILITY_TWO)]
                )
            if operation == "space_agent_turn_invoke":
                return invoke_response(request, outputs[request["call_id"]])
            finishes.append(request)
            return finish_response()

        def completion_handler(request: dict[str, Any]) -> dict[str, Any]:
            completion_requests.append(request)
            return completion_response(decisions[len(completion_requests) - 1])

        with FakeUnixJsonServer(self.broker_path, broker_handler), FakeUnixJsonServer(
            self.completion_path, completion_handler
        ):
            result = self.run_runner()

        self.assertEqual(result["capability_call_count"], 3)
        self.assertEqual(completion_requests[0]["history"], [])
        self.assertEqual(
            completion_requests[3]["history"],
            [
                {
                    "call_id": call_ids[index],
                    "capability_key": decisions[index]["capability_key"],
                    "input": decisions[index]["input"],
                    "output": outputs[call_ids[index]],
                }
                for index in range(3)
            ],
        )
        self.assertEqual(
            finishes[0]["result"]["capability_usage"],
            [
                {"capability_key": CAPABILITY_TWO, "call_count": 1},
                {"capability_key": CAPABILITY_ONE, "call_count": 2},
            ],
        )

    def test_duplicate_invalid_and_unknown_calls_fail_before_invoke(self) -> None:
        valid_call_id = str(uuid.UUID(int=10))
        cases = [
            {
                "kind": "capability_call",
                "call_id": "not-a-uuid",
                "capability_key": CAPABILITY_ONE,
                "input": {},
            },
            {
                "kind": "capability_call",
                "call_id": str(uuid.UUID(int=11)),
                "capability_key": "erhua.space.not_authorized",
                "input": {},
            },
        ]
        for decision in cases:
            with self.subTest(decision=decision):
                self._assert_completion_decision_rejected([decision], invoke_count=0)

        duplicate_decisions = [
            {
                "kind": "capability_call",
                "call_id": valid_call_id,
                "capability_key": CAPABILITY_ONE,
                "input": {"subject": "first"},
            },
            {
                "kind": "capability_call",
                "call_id": valid_call_id,
                "capability_key": CAPABILITY_ONE,
                "input": {"subject": "again"},
            },
        ]
        self._assert_completion_decision_rejected(duplicate_decisions, invoke_count=1)

    def _assert_completion_decision_rejected(
        self, decisions: list[dict[str, Any]], invoke_count: int
    ) -> None:
        counts = {"completion": 0, "invoke": 0, "failed_finish": 0}

        def broker_handler(request: dict[str, Any]) -> dict[str, Any]:
            if request["operation"] == "space_agent_turn_claim":
                return claim_response()
            if request["operation"] == "space_agent_turn_invoke":
                counts["invoke"] += 1
                return invoke_response(request, {"resolved": True})
            counts["failed_finish"] += 1
            self.assertEqual(request["result"]["outcome"], "failed")
            return finish_response("failed")

        def completion_handler(_request: dict[str, Any]) -> dict[str, Any]:
            decision = decisions[counts["completion"]]
            counts["completion"] += 1
            return completion_response(decision)

        with FakeUnixJsonServer(self.broker_path, broker_handler), FakeUnixJsonServer(
            self.completion_path, completion_handler
        ):
            self.assert_failure("completion_rejected", self.run_runner)
        self.assertEqual(counts["invoke"], invoke_count)
        self.assertEqual(counts["failed_finish"], 1)

    def test_malformed_duplicate_oversized_and_non_finite_wire_json(self) -> None:
        malformed_responses = [
            (b'{"schema_version":1,"claimed":\n', "json_protocol_error"),
            (
                b'{"schema_version":1,"schema_version":1,"claimed":false,'
                b'"runner_identity":"erhua-space-agent-runner-v1"}\n',
                "json_protocol_error",
            ),
            (b'{"schema_version":1,"claimed":NaN}\n', "json_protocol_error"),
            (
                b'{"padding":"' + b"x" * runner.MAX_MESSAGE_BYTES + b'"}\n',
                "socket_protocol_error",
            ),
        ]
        for response, expected_code in malformed_responses:
            with self.subTest(response=response[:40]):
                with FakeUnixJsonServer(
                    self.broker_path, lambda _request, raw=response: raw
                ):
                    self.assert_failure(
                        expected_code,
                        self.run_runner,
                    )

    def test_json_depth_node_key_string_and_number_limits(self) -> None:
        nested: dict[str, Any] = {}
        cursor = nested
        for _ in range(runner.MAX_JSON_DEPTH + 1):
            child: dict[str, Any] = {}
            cursor["child"] = child
            cursor = child
        bounded_failures = [
            nested,
            {"nodes": [None] * runner.MAX_JSON_NODES},
            {"k" * (runner.MAX_KEY_BYTES + 1): True},
            {"text": "x" * (runner.MAX_STRING_BYTES + 1)},
            {"number": float("inf")},
        ]
        for value in bounded_failures:
            with self.subTest(kind=next(iter(value))):
                self.assert_failure(
                    "json_protocol_error",
                    lambda candidate=value: runner._encode_json_line(candidate),
                )

    def test_completion_and_invoke_rejections_attempt_failed_finish(self) -> None:
        def rejected_completion(_request: dict[str, Any]) -> dict[str, Any]:
            return {"schema_version": 1, "accepted": False, "decision": None}

        self._assert_failure_after_claim(
            rejected_completion, None, "completion_rejected", expected_invokes=0
        )

        call_id = str(uuid.UUID(int=20))

        def capability_decision(_request: dict[str, Any]) -> dict[str, Any]:
            return completion_response(
                {
                    "kind": "capability_call",
                    "call_id": call_id,
                    "capability_key": CAPABILITY_ONE,
                    "input": {},
                }
            )

        def rejected_invoke(request: dict[str, Any]) -> dict[str, Any]:
            response = invoke_response(request, {})
            response["accepted"] = False
            return response

        self._assert_failure_after_claim(
            capability_decision,
            rejected_invoke,
            "capability_invoke_rejected",
            expected_invokes=1,
        )

    def _assert_failure_after_claim(
        self,
        completion_handler: Callable[[dict[str, Any]], dict[str, Any] | bytes],
        invoke_handler: Callable[[dict[str, Any]], dict[str, Any]] | None,
        expected_code: str,
        expected_invokes: int,
    ) -> None:
        counts = {"invoke": 0, "failed_finish": 0}

        def broker_handler(request: dict[str, Any]) -> dict[str, Any]:
            if request["operation"] == "space_agent_turn_claim":
                return claim_response()
            if request["operation"] == "space_agent_turn_invoke":
                counts["invoke"] += 1
                if invoke_handler is None:
                    raise AssertionError("invoke was not expected")
                return invoke_handler(request)
            counts["failed_finish"] += 1
            self.assertEqual(
                request["result"],
                {"outcome": "failed", "failure_code": expected_code},
            )
            return finish_response("failed")

        with FakeUnixJsonServer(self.broker_path, broker_handler), FakeUnixJsonServer(
            self.completion_path, completion_handler
        ):
            self.assert_failure(expected_code, self.run_runner)
        self.assertEqual(counts["invoke"], expected_invokes)
        self.assertEqual(counts["failed_finish"], 1)

    def test_sixteen_completion_round_ceiling(self) -> None:
        counts = {"completion": 0, "invoke": 0, "failed_finish": 0}

        def broker_handler(request: dict[str, Any]) -> dict[str, Any]:
            if request["operation"] == "space_agent_turn_claim":
                return claim_response()
            if request["operation"] == "space_agent_turn_invoke":
                counts["invoke"] += 1
                return invoke_response(request, {"resolved": True})
            counts["failed_finish"] += 1
            self.assertEqual(
                request["result"]["failure_code"], "completion_round_limit"
            )
            return finish_response("failed")

        def completion_handler(request: dict[str, Any]) -> dict[str, Any]:
            index = counts["completion"]
            self.assertEqual(len(request["history"]), index)
            counts["completion"] += 1
            return completion_response(
                {
                    "kind": "capability_call",
                    "call_id": str(uuid.UUID(int=100 + index)),
                    "capability_key": CAPABILITY_ONE,
                    "input": {"subject": str(index)},
                }
            )

        with FakeUnixJsonServer(self.broker_path, broker_handler), FakeUnixJsonServer(
            self.completion_path, completion_handler
        ):
            self.assert_failure(
                "completion_round_limit", self.run_runner
            )
        self.assertEqual(counts["completion"], runner.MAX_COMPLETION_ROUNDS)
        self.assertEqual(counts["invoke"], runner.MAX_COMPLETION_ROUNDS)
        self.assertEqual(counts["failed_finish"], 1)

    def test_malformed_completion_triggers_best_effort_failed_finish(self) -> None:
        self._assert_failure_after_claim(
            lambda _request: b'{"schema_version":1,"accepted":true,\n',
            None,
            "json_protocol_error",
            expected_invokes=0,
        )

    def test_expired_finish_response_is_accepted_as_a_failed_terminal_status(self) -> None:
        finish_requests: list[dict[str, Any]] = []

        def expired_response() -> dict[str, Any]:
            response = finish_response("failed")
            response["failure_code"] = "runner_claim_expired_unknown"
            return response

        def broker_handler(request: dict[str, Any]) -> dict[str, Any]:
            if request["operation"] == "space_agent_turn_claim":
                return claim_response()
            finish_requests.append(request)
            return expired_response()

        with FakeUnixJsonServer(self.broker_path, broker_handler), FakeUnixJsonServer(
            self.completion_path,
            lambda _request: completion_response(
                {"kind": "final", "output": {"message": "late"}}
            ),
        ):
            self.assert_failure("broker_finish_rejected", self.run_runner)
        self.assertEqual(len(finish_requests), 1)
        self.assertEqual(finish_requests[0]["result"]["outcome"], "succeeded")

        invalid = expired_response()
        invalid["failure_code"] = "unexpected"
        self.assert_failure(
            "broker_finish_rejected", lambda: runner._validate_finish_response(invalid)
        )

    def test_socket_timeout_is_one_end_to_end_deadline(self) -> None:
        idle = json.dumps(
            {
                "schema_version": 1,
                "claimed": False,
                "runner_identity": runner.RUNNER_IDENTITY,
            },
            separators=(",", ":"),
        ).encode("utf-8") + b"\n"
        environment = {**self.environment, runner.TIMEOUT_ENV: "1"}

        with FakeUnixJsonServer(
            self.broker_path,
            lambda _request: DripResponse(idle, delay_seconds=0.05),
        ):
            started = time.monotonic()
            self.assert_failure(
                "socket_error",
                lambda: runner.run_once(
                    environment,
                    endpoint_validator=lambda _path: None,
                ),
            )
            elapsed = time.monotonic() - started
        self.assertLess(elapsed, 2.5)

    def test_socket_endpoint_requires_protected_foreign_owned_group_socket(self) -> None:
        path = "/run/qintopia-agentos-agent-turn/space-agent-turn.sock"
        parent = str(Path(path).parent)
        runner_uid = 501
        runner_gid = 502

        def metadata(mode: int, uid: int, gid: int) -> SimpleNamespace:
            return SimpleNamespace(st_mode=mode, st_uid=uid, st_gid=gid)

        valid_parent = metadata(stat.S_IFDIR | 0o750, 0, runner_gid)
        valid_socket = metadata(stat.S_IFSOCK | 0o660, 100, runner_gid)

        def validate(parent_value: SimpleNamespace, socket_value: SimpleNamespace) -> None:
            runner._validate_socket_endpoint(
                path,
                lstat=lambda target: parent_value if target == parent else socket_value,
                geteuid=lambda: runner_uid,
                getgid=lambda: runner_gid,
            )

        validate(valid_parent, valid_socket)
        invalid_endpoints = [
            (metadata(stat.S_IFLNK | 0o750, 0, runner_gid), valid_socket),
            (metadata(stat.S_IFDIR | 0o740, 0, runner_gid), valid_socket),
            (metadata(stat.S_IFDIR | 0o750, runner_uid, runner_gid), valid_socket),
            (metadata(stat.S_IFDIR | 0o750, 0, runner_gid + 1), valid_socket),
            (valid_parent, metadata(stat.S_IFLNK | 0o660, 100, runner_gid)),
            (valid_parent, metadata(stat.S_IFSOCK | 0o600, 100, runner_gid)),
            (valid_parent, metadata(stat.S_IFSOCK | 0o660, runner_uid, runner_gid)),
            (valid_parent, metadata(stat.S_IFSOCK | 0o660, 100, runner_gid + 1)),
        ]
        for parent_value, socket_value in invalid_endpoints:
            with self.subTest(
                parent_mode=oct(parent_value.st_mode),
                socket_mode=oct(socket_value.st_mode),
                parent_uid=parent_value.st_uid,
                socket_uid=socket_value.st_uid,
                parent_gid=parent_value.st_gid,
                socket_gid=socket_value.st_gid,
            ):
                self.assert_failure(
                    "socket_endpoint_rejected",
                    lambda parent_candidate=parent_value, socket_candidate=socket_value: validate(
                        parent_candidate, socket_candidate
                    ),
                )

        self.assert_failure(
            "socket_endpoint_rejected",
            lambda: runner._validate_socket_endpoint(
                path,
                lstat=lambda _target: (_ for _ in ()).throw(FileNotFoundError()),
                geteuid=lambda: runner_uid,
                getgid=lambda: runner_gid,
            ),
        )

    def test_interrupts_are_not_swallowed_or_reported_as_runner_failures(self) -> None:
        requests: list[dict[str, Any]] = []

        def request(
            _path: str, payload: dict[str, Any], _timeout: int
        ) -> dict[str, Any]:
            requests.append(payload)
            if payload["operation"] == "space_agent_turn_claim":
                return claim_response()
            raise KeyboardInterrupt

        with self.assertRaises(KeyboardInterrupt):
            runner.run_once(self.environment, socket_request=request)
        self.assertEqual(
            [payload["operation"] for payload in requests],
            ["space_agent_turn_claim", "space_agent_turn_complete"],
        )

    def test_static_runner_boundary_has_no_external_clients_subprocesses_or_writes(self) -> None:
        source = RUNNER_PATH.read_text(encoding="utf-8")
        tree = ast.parse(source)
        imported_roots: set[str] = set()
        forbidden_calls: list[str] = []
        forbidden_imports = {
            "aiohttp",
            "asyncpg",
            "ftplib",
            "http",
            "psycopg",
            "requests",
            "sqlite3",
            "subprocess",
            "urllib",
        }
        mutating_names = {
            "open",
            "mkdir",
            "makedirs",
            "remove",
            "rename",
            "rmdir",
            "unlink",
            "write_bytes",
            "write_text",
        }
        for node in ast.walk(tree):
            if isinstance(node, ast.Import):
                imported_roots.update(alias.name.split(".")[0] for alias in node.names)
            elif isinstance(node, ast.ImportFrom) and node.module:
                imported_roots.add(node.module.split(".")[0])
            elif isinstance(node, ast.Call):
                if isinstance(node.func, ast.Name) and node.func.id == "open":
                    forbidden_calls.append("open")
                elif (
                    isinstance(node.func, ast.Attribute)
                    and node.func.attr in mutating_names
                ):
                    forbidden_calls.append(node.func.attr)
        self.assertFalse(imported_roots & forbidden_imports)
        self.assertEqual(forbidden_calls, [])
        self.assertIn("socket.AF_UNIX", source)
        self.assertNotIn("socket.AF_INET", source)
        self.assertNotIn("DATABASE_URL", source)
        self.assertNotIn("QIWE_API", source)


if __name__ == "__main__":
    unittest.main(verbosity=2)
