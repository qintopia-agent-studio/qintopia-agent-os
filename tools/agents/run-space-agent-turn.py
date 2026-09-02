#!/usr/bin/env python3
"""Run one bounded Space agent turn through local Unix-socket brokers."""

from __future__ import annotations

import json
import math
import os
import re
import socket
import stat
import sys
import time
import uuid
from dataclasses import dataclass
from datetime import datetime
from typing import Any, Callable, Mapping

ENABLE_ENV = "QINTOPIA_SPACE_AGENT_TURN_RUNNER_ENABLED"
BROKER_SOCKET_ENV = "QINTOPIA_SPACE_AGENT_TURN_RUNNER_SOCKET"
COMPLETION_SOCKET_ENV = "QINTOPIA_SPACE_AGENT_TURN_COMPLETION_SOCKET"
TOKEN_ENV = "QINTOPIA_SPACE_AGENT_TURN_RUNNER_TOKEN"
TIMEOUT_ENV = "QINTOPIA_SPACE_AGENT_TURN_SOCKET_TIMEOUT_SECONDS"

RUNNER_IDENTITY = "erhua-space-agent-runner-v1"
PROTOCOL_VERSION = 1
MAX_MESSAGE_BYTES = 128 * 1024
MAX_JSON_DEPTH = 16
MAX_JSON_NODES = 5_000
MAX_STRING_BYTES = 64 * 1024
MAX_KEY_BYTES = 128
MAX_CAPABILITIES = 32
MAX_COMPLETION_ROUNDS = 16
MAX_SOCKET_PATH_BYTES = 100
DEFAULT_TIMEOUT_SECONDS = 45
_CAPABILITY_KEY = re.compile(r"^[a-z0-9][a-z0-9_.-]{0,127}$")
_SHA256 = re.compile(r"^[0-9a-f]{64}$")


class RunnerFailure(RuntimeError):
    """A sanitized terminal failure suitable for the broker audit trail."""

    def __init__(self, code: str):
        super().__init__(code)
        self.code = code


@dataclass(frozen=True)
class RunnerConfig:
    broker_socket: str
    completion_socket: str
    runner_token: str
    timeout_seconds: int

    @classmethod
    def from_environment(cls, environment: Mapping[str, str]) -> "RunnerConfig":
        enabled = environment.get(ENABLE_ENV, "")
        if not isinstance(enabled, str) or enabled not in {"", "0", "1"} or enabled != "1":
            raise RunnerFailure("configuration_error")

        broker_socket = _validate_socket_path(environment.get(BROKER_SOCKET_ENV, ""))
        completion_socket = _validate_socket_path(
            environment.get(COMPLETION_SOCKET_ENV, "")
        )
        if broker_socket == completion_socket:
            raise RunnerFailure("configuration_error")

        runner_token = environment.get(TOKEN_ENV, "")
        if (
            not isinstance(runner_token, str)
            or not runner_token.isascii()
            or not 32 <= len(runner_token) <= 512
            or any(character.isspace() for character in runner_token)
        ):
            raise RunnerFailure("configuration_error")

        timeout_value = environment.get(TIMEOUT_ENV, "")
        try:
            timeout_seconds = (
                int(timeout_value) if timeout_value else DEFAULT_TIMEOUT_SECONDS
            )
        except ValueError as error:
            raise RunnerFailure("configuration_error") from error
        if not 1 <= timeout_seconds <= 60:
            raise RunnerFailure("configuration_error")

        return cls(
            broker_socket=broker_socket,
            completion_socket=completion_socket,
            runner_token=runner_token,
            timeout_seconds=timeout_seconds,
        )


SocketRequest = Callable[[str, dict[str, Any], int], dict[str, Any]]
EndpointValidator = Callable[[str], None]


def run_once(
    environment: Mapping[str, str] | None = None,
    socket_request: SocketRequest | None = None,
    endpoint_validator: EndpointValidator | None = None,
) -> dict[str, Any]:
    """Claim and execute at most one Space agent-turn work item."""

    config = RunnerConfig.from_environment(
        os.environ if environment is None else environment
    )
    if socket_request is not None:
        request = socket_request
    else:
        def request(
            path: str, payload: dict[str, Any], timeout: int
        ) -> dict[str, Any]:
            return _socket_request(
                path,
                payload,
                timeout,
                endpoint_validator=endpoint_validator,
            )
    claim = _validate_claim_response(
        request(
            config.broker_socket,
            {
                "operation": "space_agent_turn_claim",
                "schema_version": PROTOCOL_VERSION,
                "runner_identity": RUNNER_IDENTITY,
                "runner_token": config.runner_token,
            },
            config.timeout_seconds,
        )
    )
    if not claim["claimed"]:
        return {"schema_version": PROTOCOL_VERSION, "status": "idle"}

    history: list[dict[str, Any]] = []
    call_ids: set[str] = set()
    successful_receipts: list[tuple[str, str, str]] = []
    capability_keys = {
        descriptor["capability_key"] for descriptor in claim["capabilities"]
    }
    terminal_acknowledged = False
    try:
        for _ in range(MAX_COMPLETION_ROUNDS):
            completion = _validate_completion_response(
                request(
                    config.completion_socket,
                    {
                        "operation": "space_agent_turn_complete",
                        "schema_version": PROTOCOL_VERSION,
                        "runner_identity": RUNNER_IDENTITY,
                        "runner_token": config.runner_token,
                        "work_item_id": claim["work_item_id"],
                        "goal": claim["goal"],
                        "trigger": claim["trigger"],
                        "output_contract": claim["output_contract"],
                        "capabilities": claim["capabilities"],
                        "history": history,
                    },
                    config.timeout_seconds,
                ),
                capability_keys,
                call_ids,
            )
            decision = completion["decision"]
            if decision["kind"] == "final":
                usage_counts: dict[str, int] = {}
                for _call_id, key, _output_sha256 in successful_receipts:
                    usage_counts[key] = usage_counts.get(key, 0) + 1
                usage = [
                    {"capability_key": key, "call_count": usage_counts[key]}
                    for key in sorted(usage_counts)
                ]
                finish = _validate_finish_response(
                    request(
                        config.broker_socket,
                        _finish_request(
                            config,
                            claim,
                            {
                                "outcome": "succeeded",
                                "output": decision["output"],
                                "capability_usage": usage,
                            },
                        ),
                        config.timeout_seconds,
                    )
                )
                terminal_acknowledged = True
                if finish["status"] != "completed":
                    raise RunnerFailure("broker_finish_rejected")
                return {
                    "schema_version": PROTOCOL_VERSION,
                    "status": "completed",
                    "capability_call_count": len(successful_receipts),
                }

            call_id = decision["call_id"]
            capability_key = decision["capability_key"]
            invoked = _validate_invoke_response(
                request(
                    config.broker_socket,
                    {
                        "operation": "space_agent_turn_invoke",
                        "schema_version": PROTOCOL_VERSION,
                        "runner_identity": RUNNER_IDENTITY,
                        "runner_token": config.runner_token,
                        "work_item_id": claim["work_item_id"],
                        "claim_token": claim["claim_token"],
                        "call_id": call_id,
                        "capability_key": capability_key,
                        "input": decision["input"],
                    },
                    config.timeout_seconds,
                ),
                call_id,
                capability_key,
            )
            call_ids.add(call_id)
            successful_receipts.append(
                (call_id, capability_key, invoked["output_sha256"])
            )
            history.append(
                {
                    "call_id": call_id,
                    "capability_key": capability_key,
                    "input": decision["input"],
                    "output": invoked["output"],
                }
            )

        raise RunnerFailure("completion_round_limit")
    except Exception as error:
        if not terminal_acknowledged:
            failure_code = (
                error.code if isinstance(error, RunnerFailure) else "runner_internal_error"
            )
            _best_effort_failed_finish(request, config, claim, failure_code)
        if isinstance(error, RunnerFailure):
            raise
        raise RunnerFailure("runner_internal_error") from None


def _finish_request(
    config: RunnerConfig, claim: dict[str, Any], result: dict[str, Any]
) -> dict[str, Any]:
    return {
        "operation": "space_agent_turn_finish",
        "schema_version": PROTOCOL_VERSION,
        "runner_identity": RUNNER_IDENTITY,
        "runner_token": config.runner_token,
        "work_item_id": claim["work_item_id"],
        "claim_token": claim["claim_token"],
        "result": result,
    }


def _best_effort_failed_finish(
    request: SocketRequest,
    config: RunnerConfig,
    claim: dict[str, Any],
    failure_code: str,
) -> None:
    try:
        request(
            config.broker_socket,
            _finish_request(
                config,
                claim,
                {"outcome": "failed", "failure_code": failure_code},
            ),
            config.timeout_seconds,
        )
    except Exception:
        return


def _socket_request(
    socket_path: str,
    payload: dict[str, Any],
    timeout_seconds: int,
    endpoint_validator: EndpointValidator | None = None,
) -> dict[str, Any]:
    deadline = time.monotonic() + timeout_seconds
    encoded = _encode_json_line(payload)
    validator = endpoint_validator or _validate_socket_endpoint
    validator(socket_path)
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
            client.settimeout(_remaining_timeout(deadline))
            client.connect(socket_path)
            client.settimeout(_remaining_timeout(deadline))
            client.sendall(encoded)
            response = bytearray()
            while True:
                client.settimeout(_remaining_timeout(deadline))
                chunk = client.recv(min(16 * 1024, MAX_MESSAGE_BYTES + 1 - len(response)))
                if not chunk:
                    raise RunnerFailure("socket_protocol_error")
                response.extend(chunk)
                if len(response) > MAX_MESSAGE_BYTES:
                    raise RunnerFailure("socket_protocol_error")
                newline = response.find(b"\n")
                if newline >= 0:
                    if newline != len(response) - 1:
                        raise RunnerFailure("socket_protocol_error")
                    raw = bytes(response[:newline])
                    if raw.endswith(b"\r"):
                        raw = raw[:-1]
                    return _strict_json_object(raw)
    except RunnerFailure:
        raise
    except (OSError, TimeoutError):
        raise RunnerFailure("socket_error") from None


def _remaining_timeout(deadline: float) -> float:
    remaining = deadline - time.monotonic()
    if remaining <= 0:
        raise RunnerFailure("socket_error")
    return remaining


def _encode_json_line(value: dict[str, Any]) -> bytes:
    _validate_json_limits(value)
    try:
        encoded = json.dumps(
            value,
            ensure_ascii=False,
            separators=(",", ":"),
            allow_nan=False,
        ).encode("utf-8")
    except (TypeError, ValueError, UnicodeError):
        raise RunnerFailure("json_protocol_error") from None
    if not encoded or len(encoded) + 1 > MAX_MESSAGE_BYTES:
        raise RunnerFailure("json_protocol_error")
    return encoded + b"\n"


def _strict_json_object(raw: bytes) -> dict[str, Any]:
    if not raw or len(raw) > MAX_MESSAGE_BYTES:
        raise RunnerFailure("json_protocol_error")

    def pairs_hook(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
        result: dict[str, Any] = {}
        for key, value in pairs:
            if key in result:
                raise RunnerFailure("json_protocol_error")
            result[key] = value
        return result

    def parse_float(value: str) -> float:
        parsed = float(value)
        if not math.isfinite(parsed):
            raise RunnerFailure("json_protocol_error")
        return parsed

    try:
        value = json.loads(
            raw.decode("utf-8"),
            object_pairs_hook=pairs_hook,
            parse_float=parse_float,
            parse_constant=lambda _value: _raise_json_error(),
        )
    except RunnerFailure:
        raise
    except (UnicodeError, ValueError, TypeError, RecursionError):
        raise RunnerFailure("json_protocol_error") from None
    if not isinstance(value, dict):
        raise RunnerFailure("json_protocol_error")
    _validate_json_limits(value)
    return value


def _raise_json_error() -> None:
    raise RunnerFailure("json_protocol_error")


def _validate_json_limits(value: Any) -> None:
    nodes = 0

    def walk(item: Any, depth: int) -> None:
        nonlocal nodes
        nodes += 1
        if nodes > MAX_JSON_NODES or depth > MAX_JSON_DEPTH:
            raise RunnerFailure("json_protocol_error")
        if isinstance(item, str):
            if len(item.encode("utf-8")) > MAX_STRING_BYTES:
                raise RunnerFailure("json_protocol_error")
        elif isinstance(item, dict):
            for key, child in item.items():
                if not isinstance(key, str) or len(key.encode("utf-8")) > MAX_KEY_BYTES:
                    raise RunnerFailure("json_protocol_error")
                walk(child, depth + 1)
        elif isinstance(item, list):
            for child in item:
                walk(child, depth + 1)
        elif isinstance(item, float):
            if not math.isfinite(item):
                raise RunnerFailure("json_protocol_error")
        elif item is not None and not isinstance(item, (bool, int)):
            raise RunnerFailure("json_protocol_error")

    try:
        walk(value, 0)
    except (UnicodeError, RecursionError):
        raise RunnerFailure("json_protocol_error") from None


def _validate_claim_response(value: dict[str, Any]) -> dict[str, Any]:
    _require_protocol(value)
    claimed = value.get("claimed")
    if type(claimed) is not bool or value.get("runner_identity") != RUNNER_IDENTITY:
        raise RunnerFailure("broker_claim_rejected")
    if not claimed:
        _require_exact_keys(value, {"schema_version", "claimed", "runner_identity"})
        return value

    _require_exact_keys(
        value,
        {
            "schema_version",
            "claimed",
            "runner_identity",
            "work_item_id",
            "claim_token",
            "claim_expires_at",
            "goal",
            "trigger",
            "output_contract",
            "output_contract_sha256",
            "capabilities",
        },
    )
    _require_uuid(value["work_item_id"], "broker_claim_rejected")
    claim_token = value["claim_token"]
    if (
        not isinstance(claim_token, str)
        or not 32 <= len(claim_token.encode("utf-8")) <= 512
        or any(character.isspace() for character in claim_token)
    ):
        raise RunnerFailure("broker_claim_rejected")
    _require_timestamp(value["claim_expires_at"], "broker_claim_rejected")
    goal = value["goal"]
    if (
        not isinstance(goal, str)
        or not goal.strip()
        or len(goal.encode("utf-8")) > 16_000
        or not isinstance(value["trigger"], dict)
        or not isinstance(value["output_contract"], dict)
        or not isinstance(value["output_contract_sha256"], str)
        or not _SHA256.fullmatch(value["output_contract_sha256"])
    ):
        raise RunnerFailure("broker_claim_rejected")
    capabilities = value["capabilities"]
    if not isinstance(capabilities, list) or not 0 <= len(capabilities) <= MAX_CAPABILITIES:
        raise RunnerFailure("broker_claim_rejected")
    seen: set[str] = set()
    for descriptor in capabilities:
        if not isinstance(descriptor, dict):
            raise RunnerFailure("broker_claim_rejected")
        _require_exact_keys(
            descriptor,
            {
                "capability_key",
                "input_schema",
                "output_schema",
                "risk_level",
                "review_policy",
            },
        )
        key = descriptor["capability_key"]
        if (
            not isinstance(key, str)
            or not _CAPABILITY_KEY.fullmatch(key)
            or key in seen
            or not isinstance(descriptor["input_schema"], dict)
            or not isinstance(descriptor["output_schema"], dict)
            or not _bounded_label(descriptor["risk_level"])
            or not _bounded_label(descriptor["review_policy"])
        ):
            raise RunnerFailure("broker_claim_rejected")
        seen.add(key)
    return value


def _validate_completion_response(
    value: dict[str, Any], capability_keys: set[str], call_ids: set[str]
) -> dict[str, Any]:
    _require_exact_keys(value, {"schema_version", "accepted", "decision"})
    _require_protocol(value)
    if value["accepted"] is not True or not isinstance(value["decision"], dict):
        raise RunnerFailure("completion_rejected")
    decision = value["decision"]
    if decision.get("kind") == "final":
        _require_exact_keys(decision, {"kind", "output"})
        if not isinstance(decision["output"], dict):
            raise RunnerFailure("completion_rejected")
        return value
    if decision.get("kind") != "capability_call":
        raise RunnerFailure("completion_rejected")
    _require_exact_keys(decision, {"kind", "call_id", "capability_key", "input"})
    call_id = decision["call_id"]
    _require_uuid(call_id, "completion_rejected")
    if (
        call_id in call_ids
        or not isinstance(decision["capability_key"], str)
        or decision["capability_key"] not in capability_keys
        or not isinstance(decision["input"], dict)
    ):
        raise RunnerFailure("completion_rejected")
    return value


def _validate_invoke_response(
    value: dict[str, Any], call_id: str, capability_key: str
) -> dict[str, Any]:
    _require_exact_keys(
        value,
        {
            "schema_version",
            "accepted",
            "status",
            "call_id",
            "capability_key",
            "output",
            "output_sha256",
            "replayed",
        },
    )
    _require_protocol(value)
    if (
        value["accepted"] is not True
        or value["status"] != "completed"
        or value["call_id"] != call_id
        or value["capability_key"] != capability_key
        or not isinstance(value["output"], dict)
        or not isinstance(value["output_sha256"], str)
        or not _SHA256.fullmatch(value["output_sha256"])
        or type(value["replayed"]) is not bool
    ):
        raise RunnerFailure("capability_invoke_rejected")
    return value


def _validate_finish_response(value: dict[str, Any]) -> dict[str, Any]:
    common = {
        "schema_version",
        "accepted",
        "status",
        "external_send_executed",
        "automatic_retry_allowed",
    }
    if set(value) == common | {"failure_code"}:
        if (
            value["status"] != "failed"
            or value["failure_code"] != "runner_claim_expired_unknown"
        ):
            raise RunnerFailure("broker_finish_rejected")
    else:
        _require_exact_keys(value, common)
    _require_protocol(value)
    if (
        value["accepted"] is not True
        or value["status"] not in {"completed", "failed"}
        or value["external_send_executed"] is not None
        or value["automatic_retry_allowed"] is not False
    ):
        raise RunnerFailure("broker_finish_rejected")
    return value


def _require_protocol(value: dict[str, Any]) -> None:
    if type(value.get("schema_version")) is not int or value["schema_version"] != 1:
        raise RunnerFailure("protocol_version_rejected")


def _require_exact_keys(value: dict[str, Any], expected: set[str]) -> None:
    if set(value) != expected:
        raise RunnerFailure("protocol_fields_rejected")


def _require_uuid(value: Any, code: str) -> None:
    if not isinstance(value, str):
        raise RunnerFailure(code)
    try:
        parsed = uuid.UUID(value)
    except (ValueError, AttributeError):
        raise RunnerFailure(code) from None
    if str(parsed) != value:
        raise RunnerFailure(code)


def _require_timestamp(value: Any, code: str) -> None:
    if not isinstance(value, str) or len(value) > 64:
        raise RunnerFailure(code)
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        raise RunnerFailure(code) from None
    if parsed.tzinfo is None:
        raise RunnerFailure(code)


def _bounded_label(value: Any) -> bool:
    return (
        isinstance(value, str)
        and bool(value)
        and len(value.encode("utf-8")) <= 128
        and all(character.isprintable() for character in value)
    )


def _validate_socket_path(value: Any) -> str:
    try:
        encoded = value.encode("utf-8")
    except (AttributeError, UnicodeError):
        raise RunnerFailure("configuration_error") from None
    if (
        not value
        or "\x00" in value
        or not os.path.isabs(value)
        or os.path.normpath(value) != value
        or os.path.dirname(value) == os.path.sep
        or len(encoded) > MAX_SOCKET_PATH_BYTES
    ):
        raise RunnerFailure("configuration_error")
    return value


def _validate_socket_endpoint(
    value: str,
    *,
    lstat: Callable[[str], os.stat_result] | None = None,
    geteuid: Callable[[], int] | None = None,
    getgid: Callable[[], int] | None = None,
) -> None:
    inspect = lstat or os.lstat
    effective_uid = (geteuid or os.geteuid)()
    primary_gid = (getgid or os.getgid)()
    parent = os.path.dirname(value)
    try:
        parent_metadata = inspect(parent)
        socket_metadata = inspect(value)
    except OSError:
        raise RunnerFailure("socket_endpoint_rejected") from None
    if (
        not stat.S_ISDIR(parent_metadata.st_mode)
        or stat.S_IMODE(parent_metadata.st_mode) != 0o750
        or parent_metadata.st_uid == effective_uid
        or parent_metadata.st_gid != primary_gid
        or not stat.S_ISSOCK(socket_metadata.st_mode)
        or stat.S_IMODE(socket_metadata.st_mode) != 0o660
        or socket_metadata.st_uid == effective_uid
        or socket_metadata.st_gid != primary_gid
    ):
        raise RunnerFailure("socket_endpoint_rejected")


def main(argv: list[str] | None = None) -> int:
    arguments = list(sys.argv[1:] if argv is None else argv)
    if arguments != ["--once"]:
        print("Usage: python3 tools/agents/run-space-agent-turn.py --once", file=sys.stderr)
        return 2
    try:
        result = run_once()
    except RunnerFailure as error:
        print(f"Space agent-turn runner failed: {error.code}", file=sys.stderr)
        return 1
    print(json.dumps(result, separators=(",", ":"), sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
